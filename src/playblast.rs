//! Playblast request validation, sizes and output file names (#460), pure policy. Caps, name
//! charset, `<name>.NNNN.png` numbering, `percent` rounding and the inclusive range end are
//! PROVISIONAL: no comparison with Maya's playblast output exists.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt;
use std::io;
use std::ops::RangeInclusive;
use std::path::{Component, Path, PathBuf};

use crate::image_plane::decode::PlateError;
use crate::image_plane::sequence::frame_path;

/// PROVISIONAL: largest frame count of one playblast.
pub const MAX_FRAMES: u64 = 100_000;
/// PROVISIONAL: largest width or height after and before `percent`.
pub const MAX_DIMENSION: u32 = 8192;
/// PROVISIONAL: largest pixel count of one frame.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// PROVISIONAL: largest total decoded size of the whole sequence (frames * w * h * 4).
pub const MAX_TOTAL_BYTES: u64 = 4 << 30;
/// PROVISIONAL: longest name in bytes.
pub const MAX_NAME_BYTES: usize = 64;
/// First frame number never reached by a request; keeps the `f64` to `i64` cast exact.
const FRAME_LIMIT: f64 = 1e9;
const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

/// A typed playblast failure. Display text never contains a path or file name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayblastError {
    InvalidName,
    InvalidDirectory,
    InvalidRange,
    InvalidSize,
    TooManyFrames,
    TooLarge,
    /// The output of this frame already exists (a symlink counts, dangling or not).
    DestinationExists {
        frame: i64,
    },
    Cancelled {
        done: u32,
    },
    Io(io::ErrorKind),
    Unsupported(&'static str),
    Render(&'static str),
    WriterPanicked,
}

impl fmt::Display for PlayblastError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName => {
                f.write_str("name must be 1-64 of A-Z a-z 0-9 _ - and no device name")
            }
            Self::InvalidDirectory => f.write_str("directory must be an existing real directory"),
            Self::InvalidRange => f.write_str("range must be whole frames, 0 <= start <= end"),
            Self::InvalidSize => f.write_str("width, height 1-8192 and percent 1-100 required"),
            Self::TooManyFrames => write!(f, "limited to {MAX_FRAMES} frames"),
            Self::TooLarge => f.write_str("frame or total size is above the limit"),
            Self::DestinationExists { frame } => write!(f, "output of frame {frame} exists"),
            Self::Cancelled { done } => write!(f, "cancelled after {done} files"),
            Self::Io(kind) => write!(f, "I/O failed: {kind}"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Render(what) => write!(f, "render failed: {what}"),
            Self::WriterPanicked => f.write_str("the writer thread panicked"),
        }
    }
}

impl std::error::Error for PlayblastError {}

/// A validated playblast: directory, name, frames `start..=end` (inclusive) and output size.
#[derive(Clone, Debug)]
pub struct PlayblastPlan {
    directory: PathBuf,
    name: String,
    frames: RangeInclusive<i64>,
    width: u32,
    height: u32,
}

impl PlayblastPlan {
    /// `width` and `height` are the 100% size; the output is `percent` of it, rounded half up,
    /// at least 1. Frames are whole numbers, `start` is at least 0.
    pub fn new(
        directory: &Path,
        name: &str,
        start: f64,
        end: f64,
        width: u32,
        height: u32,
        percent: u32,
    ) -> Result<Self, PlayblastError> {
        check_name(name)?;
        check_directory(directory)?;
        let (start, end) = (whole_frame(start)?, whole_frame(end)?);
        if start > end {
            return Err(PlayblastError::InvalidRange);
        }
        let count = (end - start) as u64 + 1;
        if count > MAX_FRAMES {
            return Err(PlayblastError::TooManyFrames);
        }
        if width == 0 || height == 0 || !(1..=100).contains(&percent) {
            return Err(PlayblastError::InvalidSize);
        }
        if width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(PlayblastError::TooLarge);
        }
        let scale = |size: u32| ((u64::from(size) * u64::from(percent) + 50) / 100).max(1) as u32;
        let (width, height) = (scale(width), scale(height));
        let pixels = u64::from(width) * u64::from(height);
        // pixels and count are capped above, so the product cannot overflow u64.
        if pixels > MAX_PIXELS || pixels * 4 * count > MAX_TOTAL_BYTES {
            return Err(PlayblastError::TooLarge);
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            name: name.to_owned(),
            frames: start..=end,
            width,
            height,
        })
    }

    pub fn frames(&self) -> RangeInclusive<i64> {
        self.frames.clone()
    }

    pub fn frame_count(&self) -> u32 {
        (self.frames.end() - self.frames.start() + 1) as u32
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// `directory/<name>.NNNN.png`, numbered by [`frame_path`] (the one numbering function).
    pub fn frame_file(&self, frame: i64) -> Result<PathBuf, PlayblastError> {
        let template = self.directory.join(format!("{}.0001.png", self.name));
        frame_path(&template, frame).map_err(|error| match error {
            PlateError::Missing => PlayblastError::InvalidRange,
            PlateError::Io(kind) => PlayblastError::Io(kind),
            _ => PlayblastError::Io(io::ErrorKind::Other),
        })
    }

    /// Early, friendly refusal only: a symlink, even a dangling one, counts as existing. The
    /// install itself ([`install_frame`]) is what actually refuses to replace anything.
    pub fn check_no_collisions(&self) -> Result<(), PlayblastError> {
        for frame in self.frames() {
            match std::fs::symlink_metadata(self.frame_file(frame)?) {
                Ok(_) => return Err(PlayblastError::DestinationExists { frame }),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(PlayblastError::Io(error.kind())),
            }
        }
        Ok(())
    }
}

fn check_name(name: &str) -> Result<(), PlayblastError> {
    let stem = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let reserved = RESERVED
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device))
        || ["com", "lpt"].iter().any(|prefix| {
            let lower = stem.to_ascii_lowercase();
            lower
                .strip_prefix(prefix)
                .is_some_and(|n| matches!(n.as_bytes(), [b'1'..=b'9']))
        });
    let charset = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if reserved || !charset || name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(PlayblastError::InvalidName);
    }
    Ok(())
}

fn check_directory(directory: &Path) -> Result<(), PlayblastError> {
    let text = directory
        .to_str()
        .filter(|t| !t.is_empty() && t.len() <= 4096 && !t.contains('\0'));
    if text.is_none() || directory.components().any(|c| c == Component::ParentDir) {
        return Err(PlayblastError::InvalidDirectory);
    }
    // Output must not depend on the process working directory.
    if !directory.is_absolute() {
        return Err(PlayblastError::InvalidDirectory);
    }
    // symlink_metadata: a symlinked final component is refused, not followed. Symlinks in
    // ancestor components are allowed and followed.
    match std::fs::symlink_metadata(directory) {
        Ok(meta) if meta.is_dir() => Ok(()),
        _ => Err(PlayblastError::InvalidDirectory),
    }
}

fn whole_frame(value: f64) -> Result<i64, PlayblastError> {
    if !value.is_finite() || value.fract() != 0.0 || !(0.0..FRAME_LIMIT).contains(&value) {
        return Err(PlayblastError::InvalidRange);
    }
    Ok(value as i64)
}

mod job;
mod writer;
pub use job::{FrameRenderer, JobProgress, PlayblastJob};
pub use writer::{encode_png, install_frame};

#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod writer_tests;
