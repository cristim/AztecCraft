//! The playblast job: one frame is rendered per [`PlayblastJob::step`] and handed to a writer
//! thread through a bounded queue, so the caller only blocks on rendering. `done` counts files
//! that are installed, not frames queued; `Finished` means every file is in place and the
//! writer thread has been joined.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, channel, sync_channel};
use std::thread::{self, JoinHandle};

use super::writer::{encode_png, install_frame};
use super::{PlayblastError, PlayblastPlan};

/// Queue depth between render and write; PROVISIONAL.
const QUEUE: usize = 2;

/// Renders one frame as tightly packed RGBA8 at `width` x `height`. `Send` so a job can be
/// stepped from whichever thread owns it.
pub trait FrameRenderer: Send {
    fn render(&mut self, frame: i64, width: u32, height: u32) -> Result<Vec<u8>, PlayblastError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobProgress {
    Running { done: u32, total: u32 },
    Finished { total: u32 },
    Failed(PlayblastError),
    Cancelled { done: u32 },
}

struct Work {
    frame: i64,
    path: PathBuf,
    rgba: Vec<u8>,
}

type Ack = Result<(), PlayblastError>;

pub struct PlayblastJob {
    plan: PlayblastPlan,
    next: i64,
    done: u32,
    cancel: Arc<AtomicBool>,
    work: Option<SyncSender<Work>>,
    acks: Receiver<Ack>,
    writer: Option<JoinHandle<()>>,
    terminal: Option<JobProgress>,
}

/// Raw OS codes that mean "no hard links here": Linux EPERM from vfat and similar, Windows
/// ERROR_INVALID_FUNCTION (1) and ERROR_NOT_SUPPORTED (50).
pub(super) fn no_hard_links(raw: i32, os: &str) -> bool {
    match os {
        "linux" => raw == 1,
        "windows" => raw == 1 || raw == 50,
        _ => false,
    }
}

pub(super) fn install_error(error: &io::Error, frame: i64) -> PlayblastError {
    let unsupported = error.kind() == io::ErrorKind::Unsupported
        || error
            .raw_os_error()
            .is_some_and(|raw| no_hard_links(raw, std::env::consts::OS));
    match error.kind() {
        io::ErrorKind::AlreadyExists => PlayblastError::DestinationExists { frame },
        _ if unsupported => PlayblastError::Unsupported("hard links are not available here"),
        kind => PlayblastError::Io(kind),
    }
}

fn write_loop(
    work: Receiver<Work>,
    acks: std::sync::mpsc::Sender<Ack>,
    cancel: Arc<AtomicBool>,
    size: (u32, u32),
) {
    let cancelled = || cancel.load(Ordering::SeqCst);
    for item in work {
        if cancelled() {
            return;
        }
        let png = match encode_png(&item.rgba, size.0, size.1) {
            Ok(png) => png,
            Err(error) => {
                let _ = acks.send(Err(error));
                return;
            }
        };
        let result = match install_frame(&item.path, &png, &cancelled) {
            Ok(()) => Ok(()),
            // An interrupted install is a cancel; the job reports it, not the writer.
            Err(error) if error.kind() == io::ErrorKind::Interrupted => return,
            Err(error) => Err(install_error(&error, item.frame)),
        };
        // An install that finished is acked even when a cancel arrived meanwhile.
        let failed = result.is_err();
        if acks.send(result).is_err() || failed {
            return;
        }
    }
}

impl PlayblastJob {
    /// Refuses an occupied destination before any frame is rendered, then starts the writer.
    pub fn start(plan: PlayblastPlan) -> Result<Self, PlayblastError> {
        // Ancestor symlinks are allowed; only the final component is checked, here again because
        // it can change between planning and starting. Later swaps are caught by hard_link.
        super::check_directory(&plan.directory)?;
        plan.check_no_collisions()?;
        let (work, queue) = sync_channel(QUEUE);
        let (ack_tx, acks) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (flag, size) = (Arc::clone(&cancel), plan.size());
        let writer = thread::Builder::new()
            .name("playblast-writer".into())
            .spawn(move || write_loop(queue, ack_tx, flag, size))
            .map_err(|error| PlayblastError::Io(error.kind()))?;
        Ok(Self {
            next: *plan.frames().start(),
            plan,
            done: 0,
            cancel,
            work: Some(work),
            acks,
            writer: Some(writer),
            terminal: None,
        })
    }

    pub fn step(&mut self, renderer: &mut dyn FrameRenderer) -> JobProgress {
        if let Some(state) = &self.terminal {
            return state.clone();
        }
        if let Some(error) = self.collect() {
            return self.fail(error);
        }
        let total = self.plan.frame_count();
        if self.next > *self.plan.frames().end() {
            let error = self.close(false);
            let state = match error {
                Some(error) => JobProgress::Failed(error),
                None if self.done == total => JobProgress::Finished { total },
                None => JobProgress::Failed(PlayblastError::Io(io::ErrorKind::Other)),
            };
            return self.settle(state);
        }
        let (width, height) = self.plan.size();
        let frame = self.next;
        let rgba = match renderer.render(frame, width, height) {
            Ok(rgba) => rgba,
            Err(error) => return self.fail(error),
        };
        let path = match self.plan.frame_file(frame) {
            Ok(path) => path,
            Err(error) => return self.fail(error),
        };
        let sent = self
            .work
            .as_ref()
            .map(|queue| queue.send(Work { frame, path, rgba }));
        if !matches!(sent, Some(Ok(()))) {
            // The writer stopped early: its ack names the reason.
            let error = self
                .close(true)
                .unwrap_or(PlayblastError::Io(io::ErrorKind::BrokenPipe));
            return self.fail(error);
        }
        self.next += 1;
        JobProgress::Running {
            done: self.done,
            total,
        }
    }

    /// Stops after the file in flight, removes its staged copy, joins the writer and keeps the
    /// frames already installed. A finished or failed job keeps its state.
    pub fn cancel(&mut self) -> JobProgress {
        if self.terminal.is_none() {
            self.close(true);
            let done = self.done;
            self.settle(JobProgress::Cancelled { done });
        }
        self.terminal
            .clone()
            .unwrap_or(JobProgress::Cancelled { done: self.done })
    }

    fn settle(&mut self, state: JobProgress) -> JobProgress {
        self.terminal = Some(state.clone());
        state
    }

    fn fail(&mut self, error: PlayblastError) -> JobProgress {
        self.close(true);
        self.settle(JobProgress::Failed(error))
    }

    fn collect(&mut self) -> Option<PlayblastError> {
        let mut first = None;
        for ack in self.acks.try_iter() {
            match ack {
                Ok(()) => self.done += 1,
                Err(error) => {
                    first.get_or_insert(error);
                }
            }
        }
        first
    }

    /// Drops the queue and joins the writer; `abort` makes it skip what is still queued.
    fn close(&mut self, abort: bool) -> Option<PlayblastError> {
        if abort {
            self.cancel.store(true, Ordering::SeqCst);
        }
        self.work = None;
        let panicked = self
            .writer
            .take()
            .is_some_and(|writer| writer.join().is_err());
        let error = self.collect();
        error.or(panicked.then_some(PlayblastError::WriterPanicked))
    }
}

impl Drop for PlayblastJob {
    fn drop(&mut self) {
        if self.terminal.is_none() {
            self.close(true);
        }
    }
}
