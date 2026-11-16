//! The plate prefetch worker and the UI-side stream that drives it (#459).
//!
//! One long-lived `plate-decode` thread decodes frames for the UI. The UI never decodes, never
//! blocks and never joins the thread. The UI states the window of frames it wants through a
//! latest-wins `Mutex` + `Condvar` slot; results come back over a `sync_channel(1)`. A new source
//! reaches the same thread through the slot (a new source generation), and a thread is only
//! respawned when the previous one is gone.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};

use super::cache::{FrameCache, FrameKey, PLATE_CACHE_BUDGET_BYTES};
use super::decode::{PlateError, PlateFrame, decode_plate, read_plate_file};
use super::sequence::frame_path;

/// Most results the UI takes from the channel in one tick.
const MAX_RESULTS_PER_TICK: usize = 8;
/// Failed or missing frames remembered per source before the table is reset.
const MAX_REMEMBERED_FAILURES: usize = 4096;

/// Where frames come from: a file, or a numbered template when `sequence` is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlateSource {
    pub template: PathBuf,
    pub sequence: bool,
}

/// Decodes one frame of a source. Replaceable so tests can block, count and panic.
pub type DecodeFn = Arc<dyn Fn(&PlateSource, i64) -> Result<PlateFrame, PlateError> + Send + Sync>;
/// Called by the worker after every result so the UI repaints; never called on a timer.
pub type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Reads and decodes the frame's file. A file that does not exist is `Missing`.
pub fn decode_from_disk(source: &PlateSource, frame: i64) -> Result<PlateFrame, PlateError> {
    let path = if source.sequence {
        frame_path(&source.template, frame)?
    } else {
        source.template.clone()
    };
    let bytes = read_plate_file(&path).map_err(|error| match error {
        PlateError::Io(std::io::ErrorKind::NotFound) => PlateError::Missing,
        other => other,
    })?;
    decode_plate(&bytes)
}

/// What the UI knows about one frame of the current source.
#[derive(Clone, Debug)]
pub enum FrameState {
    Loading,
    Ready(Arc<PlateFrame>),
    Missing,
    Failed(PlateError),
}

#[derive(Default)]
struct Slot {
    /// Bumped on every write; the worker parks until it changes.
    version: u64,
    generation: u64,
    source: Option<Arc<PlateSource>>,
    window: Vec<i64>,
    cancel: bool,
}

struct Shared {
    slot: Mutex<Slot>,
    wake: Condvar,
}

fn lock(shared: &Shared) -> Option<MutexGuard<'_, Slot>> {
    shared.slot.lock().ok()
}

type Outcome = (FrameKey, Result<PlateFrame, PlateError>);

fn run_worker(
    shared: Arc<Shared>,
    results: SyncSender<Outcome>,
    decode: DecodeFn,
    wake: WakeFn,
    _alive: Arc<()>,
) {
    let mut seen = 0;
    loop {
        let (version, generation, source, window) = {
            let Some(guard) = lock(&shared) else { return };
            let Ok(guard) = shared
                .wake
                .wait_while(guard, |slot| slot.version == seen && !slot.cancel)
            else {
                return;
            };
            if guard.cancel {
                return;
            }
            seen = guard.version;
            (
                guard.version,
                guard.generation,
                guard.source.clone(),
                guard.window.clone(),
            )
        };
        let Some(source) = source else { continue };
        for frame in window {
            let still_current = lock(&shared).is_some_and(|slot| slot.version == version);
            if !still_current {
                break;
            }
            let outcome = catch_unwind(AssertUnwindSafe(|| decode(&source, frame)))
                .unwrap_or(Err(PlateError::Internal));
            let key = FrameKey { generation, frame };
            if results.send((key, outcome)).is_err() {
                return;
            }
            wake();
        }
    }
}

/// The UI-side owner of the worker, the cache and the per-frame outcomes.
pub struct PlateStream {
    shared: Arc<Shared>,
    decode: DecodeFn,
    wake: WakeFn,
    results: Option<Receiver<Outcome>>,
    alive: Weak<()>,
    spawned: Arc<AtomicUsize>,
    cache: FrameCache,
    failed: HashMap<FrameKey, PlateError>,
    generation: u64,
    has_source: bool,
    /// Size of the first frame decoded for the current source; later frames must match it.
    frame_size: Option<(u32, u32)>,
}

impl PlateStream {
    pub fn new(decode: DecodeFn, wake: WakeFn, frame_cache: usize) -> Self {
        Self::with_budget(decode, wake, frame_cache, PLATE_CACHE_BUDGET_BYTES)
    }

    pub fn with_budget(
        decode: DecodeFn,
        wake: WakeFn,
        frame_cache: usize,
        budget_bytes: usize,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                slot: Mutex::new(Slot::default()),
                wake: Condvar::new(),
            }),
            decode,
            wake,
            results: None,
            alive: Weak::new(),
            spawned: Arc::new(AtomicUsize::new(0)),
            cache: FrameCache::new(budget_bytes, frame_cache),
            failed: HashMap::new(),
            generation: 0,
            has_source: false,
            frame_size: None,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn cache(&self) -> &FrameCache {
        &self.cache
    }

    /// Threads spawned over the stream's life (a test and diagnostics counter).
    pub fn spawned_workers(&self) -> usize {
        self.spawned.load(Ordering::SeqCst)
    }

    pub fn worker_alive(&self) -> bool {
        self.alive.strong_count() > 0
    }

    /// Starts a new source generation: older results are dropped on arrival and the cache and
    /// failures of the old source are cleared. Respawns the worker only when it is gone.
    pub fn set_source(&mut self, source: PlateSource) {
        self.generation += 1;
        self.has_source = true;
        self.cache.retain_generation(self.generation);
        self.failed.clear();
        self.frame_size = None;
        self.write_slot(Some(Arc::new(source)), Vec::new());
        self.ensure_worker();
    }

    /// Ends the source; the worker parks and every pending frame is forgotten.
    pub fn remove_source(&mut self) {
        self.generation += 1;
        self.has_source = false;
        self.cache.retain_generation(self.generation);
        self.failed.clear();
        self.frame_size = None;
        self.write_slot(None, Vec::new());
    }

    pub fn set_frame_cache(&mut self, frame_cache: usize) {
        self.cache.set_frame_cache(frame_cache);
    }

    /// The state of one frame of the current source.
    pub fn state(&mut self, frame: i64) -> FrameState {
        let key = FrameKey {
            generation: self.generation,
            frame,
        };
        if let Some(found) = self.cache.get(key) {
            return FrameState::Ready(found);
        }
        match self.failed.get(&key) {
            Some(PlateError::Missing) => FrameState::Missing,
            Some(error) => FrameState::Failed(error.clone()),
            None => FrameState::Loading,
        }
    }

    /// One UI tick: drains results (never blocks), then asks the worker for the frames of
    /// `wanted` that are neither cached nor failed. The slot is written only when that list
    /// differs from what the worker is still working through, so once every wanted frame is
    /// cached or failed the worker parks and nothing is re-requested. Returns whether a wanted
    /// frame is still outstanding (the caller then repaints; nothing repaints on a timer).
    pub fn tick(&mut self, wanted: &[i64]) -> bool {
        self.drain(wanted);
        if !self.has_source {
            return false;
        }
        let generation = self.generation;
        let wanted = &wanted[..wanted.len().min(self.cache.capacity())];
        let pending: Vec<i64> = wanted
            .iter()
            .copied()
            .filter(|&frame| {
                let key = FrameKey { generation, frame };
                !self.cache.contains(key) && !self.failed.contains_key(&key)
            })
            .collect();
        if !pending.is_empty() && !self.worker_alive() {
            for frame in pending {
                self.remember_failure(FrameKey { generation, frame }, PlateError::WorkerGone);
            }
            return false;
        }
        let needs_write = lock(&self.shared).is_none_or(|slot| {
            let outstanding: Vec<i64> = slot
                .window
                .iter()
                .copied()
                .filter(|&frame| {
                    let key = FrameKey { generation, frame };
                    !self.cache.contains(key) && !self.failed.contains_key(&key)
                })
                .collect();
            outstanding != pending
        });
        if needs_write {
            let source = lock(&self.shared).and_then(|slot| slot.source.clone());
            self.write_slot(source, pending.clone());
        }
        !pending.is_empty()
    }

    fn remember_failure(&mut self, key: FrameKey, error: PlateError) {
        if self.failed.len() >= MAX_REMEMBERED_FAILURES {
            self.failed.clear();
        }
        self.failed.insert(key, error);
    }

    /// Takes results and stores the ones still wanted. Pinned are exactly the wanted frames that
    /// fit the cache capacity (the frame on screen first), so eviction always has a victim.
    fn drain(&mut self, wanted: &[i64]) {
        for _ in 0..MAX_RESULTS_PER_TICK {
            let Some(results) = &self.results else { return };
            let (key, outcome) = match results.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return,
            };
            if key.generation != self.generation {
                continue;
            }
            let generation = self.generation;
            let pinned: Vec<FrameKey> = wanted
                .iter()
                .take(self.cache.capacity())
                .map(|&frame| FrameKey { generation, frame })
                .collect();
            if !pinned.contains(&key) {
                continue;
            }
            match outcome {
                Ok(frame) => {
                    let size = (frame.width, frame.height);
                    if *self.frame_size.get_or_insert(size) != size {
                        self.remember_failure(key, PlateError::DimensionMismatch);
                        continue;
                    }
                    if let Err(error) = self.cache.insert(key, Arc::new(frame), &pinned) {
                        self.remember_failure(key, error);
                    }
                }
                Err(error) => self.remember_failure(key, error),
            }
        }
    }

    fn write_slot(&self, source: Option<Arc<PlateSource>>, window: Vec<i64>) {
        if let Some(mut slot) = lock(&self.shared) {
            slot.version += 1;
            slot.generation = self.generation;
            slot.source = source;
            slot.window = window;
        }
        self.shared.wake.notify_one();
    }

    fn ensure_worker(&mut self) {
        if self.worker_alive() {
            return;
        }
        let token = Arc::new(());
        self.alive = Arc::downgrade(&token);
        let (sender, receiver) = sync_channel(1);
        self.results = Some(receiver);
        let (shared, decode, wake) = (
            Arc::clone(&self.shared),
            Arc::clone(&self.decode),
            Arc::clone(&self.wake),
        );
        let spawned = std::thread::Builder::new()
            .name("plate-decode".into())
            .spawn(move || run_worker(shared, sender, decode, wake, token));
        if spawned.is_ok() {
            self.spawned.fetch_add(1, Ordering::SeqCst);
        } else {
            self.alive = Weak::new();
        }
    }
}

impl Drop for PlateStream {
    fn drop(&mut self) {
        if let Some(mut slot) = lock(&self.shared) {
            slot.cancel = true;
        }
        self.shared.wake.notify_one();
        self.results = None;
    }
}

#[cfg(test)]
mod tests;
