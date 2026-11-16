//! Byte-bounded LRU of decoded plate frames (#459). Keys carry the source generation so a
//! frame decoded for an older source can never be shown for a newer one.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::sync::Arc;

use super::decode::{PlateError, PlateFrame};

/// Fixed cache cap (PROVISIONAL policy; Maya's default is 10% of available memory).
pub const PLATE_CACHE_BUDGET_BYTES: usize = 512 * 1024 * 1024;

/// A decoded frame's identity: the source generation it was requested for and the frame number
/// after the offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FrameKey {
    pub generation: u64,
    pub frame: i64,
}

/// How many frames the cache keeps: `min(frame_cache, max(1, budget / frame_bytes))`. Hand
/// values: 4096x2160 RGBA8 is 35,389,440 bytes, so 512 MiB holds 15; 2K holds 24 at the default
/// `frame_cache` of 24.
pub fn effective_frames(frame_cache: usize, budget_bytes: usize, frame_bytes: usize) -> usize {
    let by_budget = budget_bytes.checked_div(frame_bytes).unwrap_or(frame_cache);
    frame_cache.min(by_budget.max(1)).max(1)
}

struct Entry {
    frame: Arc<PlateFrame>,
    stamp: u64,
}

pub struct FrameCache {
    entries: HashMap<FrameKey, Entry>,
    clock: u64,
    bytes: usize,
    budget_bytes: usize,
    frame_cache: usize,
    /// Size of the last frame stored; 0 until the first one.
    frame_bytes: usize,
}

impl FrameCache {
    pub fn new(budget_bytes: usize, frame_cache: usize) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            bytes: 0,
            budget_bytes,
            frame_cache,
            frame_bytes: 0,
        }
    }

    /// Frames the cache will hold: `frame_cache` until the first frame fixes the frame size,
    /// then [`effective_frames`].
    pub fn capacity(&self) -> usize {
        if self.frame_bytes == 0 {
            self.frame_cache.max(1)
        } else {
            effective_frames(self.frame_cache, self.budget_bytes, self.frame_bytes)
        }
    }

    /// Whether a stored frame has fixed the frame size, and with it [`Self::capacity`].
    pub fn knows_frame_size(&self) -> bool {
        self.frame_bytes != 0
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.bytes
    }

    pub fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    pub fn contains(&self, key: FrameKey) -> bool {
        self.entries.contains_key(&key)
    }

    /// Returns the frame and marks it most recently used.
    pub fn get(&mut self, key: FrameKey) -> Option<Arc<PlateFrame>> {
        self.clock += 1;
        let stamp = self.clock;
        let entry = self.entries.get_mut(&key)?;
        entry.stamp = stamp;
        Some(Arc::clone(&entry.frame))
    }

    /// Stores a frame, evicting the least recently used unpinned entries until the frame count
    /// and the byte budget hold. A frame larger than the whole budget is refused, and so is any
    /// frame that cannot fit without evicting a pinned entry: the cache never goes over its
    /// budget or frame count, and the refused insert leaves it unchanged.
    pub fn insert(
        &mut self,
        key: FrameKey,
        frame: Arc<PlateFrame>,
        pinned: &[FrameKey],
    ) -> Result<(), PlateError> {
        let size = frame.rgba.len();
        if size > self.budget_bytes {
            return Err(PlateError::TooLarge);
        }
        let replaced = self.entries.remove(&key);
        if let Some(old) = &replaced {
            self.bytes -= old.frame.rgba.len();
        }
        let capacity = effective_frames(self.frame_cache, self.budget_bytes, size);
        while self.entries.len() + 1 > capacity || self.bytes + size > self.budget_bytes {
            let victim = self
                .entries
                .iter()
                .filter(|(candidate, _)| !pinned.contains(candidate))
                .min_by_key(|(_, entry)| entry.stamp)
                .map(|(candidate, _)| *candidate);
            let Some(victim) = victim else {
                if let Some(old) = replaced {
                    self.bytes += old.frame.rgba.len();
                    self.entries.insert(key, old);
                }
                return Err(PlateError::TooLarge);
            };
            if let Some(gone) = self.entries.remove(&victim) {
                self.bytes -= gone.frame.rgba.len();
            }
        }
        self.clock += 1;
        self.bytes += size;
        self.frame_bytes = size;
        self.entries.insert(
            key,
            Entry {
                frame,
                stamp: self.clock,
            },
        );
        Ok(())
    }

    /// Drops every entry that does not belong to `generation`. Once nothing is left the frame
    /// size is unknown again, so a new source with other frames is not sized by the old ones.
    pub fn retain_generation(&mut self, generation: u64) {
        self.entries.retain(|key, entry| {
            let keep = key.generation == generation;
            if !keep {
                self.bytes -= entry.frame.rgba.len();
            }
            keep
        });
        if self.entries.is_empty() {
            self.frame_bytes = 0;
        }
    }

    /// Changes the frame-count cap; the next insert evicts down to it.
    pub fn set_frame_cache(&mut self, frame_cache: usize) {
        self.frame_cache = frame_cache;
    }
}

#[cfg(test)]
mod tests;
