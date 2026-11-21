//! Staged PNG installation. `craft_atomic::write_new` resolves symlinks and then checks
//! `exists()`, so it would write through a dangling symlink at the target; this stages the file
//! itself and installs it with `hard_link`, which never follows a destination symlink and fails
//! when any entry (a dangling symlink included) is there.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::PlayblastError;

static STAGED: AtomicU64 = AtomicU64::new(0);

/// Encodes tightly packed RGBA8 as a non-interlaced PNG.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, PlayblastError> {
    let expected = u64::from(width) * u64::from(height) * 4;
    if rgba.len() as u64 != expected {
        return Err(PlayblastError::Render("frame size does not match the plan"));
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let invalid = |_| PlayblastError::Io(io::ErrorKind::InvalidData);
    let mut writer = encoder.write_header().map_err(invalid)?;
    writer.write_image_data(rgba).map_err(invalid)?;
    writer.finish().map_err(invalid)?;
    Ok(out)
}

/// Writes `bytes` beside `target` under a staged name, then links it into place unless
/// `cancelled()` says stop (`Interrupted`). The staged name is removed on every path, panics
/// included. Not covered: no directory fsync follows `hard_link`, so a power loss right after
/// can lose the new entry, and a killed process leaves its `.pb_tmp` behind.
pub fn install_frame(target: &Path, bytes: &[u8], cancelled: &dyn Fn() -> bool) -> io::Result<()> {
    install_with(
        target,
        bytes,
        cancelled,
        &|| STAGED.fetch_add(1, Ordering::Relaxed),
        &|path: &Path| fs::remove_file(path),
    )
}

/// A staged name that already exists (a leftover, or a reused PID) is skipped, never opened.
const NAME_TRIES: u32 = 8;

/// Removes the staged name when dropped; an unlink failure is ignored because after a
/// successful install the frame is in place and the name is only a stray extra link.
struct Staged<'a> {
    path: PathBuf,
    remove: &'a dyn Fn(&Path) -> io::Result<()>,
}

impl Drop for Staged<'_> {
    fn drop(&mut self) {
        let _ = (self.remove)(&self.path);
    }
}

pub(super) fn install_with(
    target: &Path,
    bytes: &[u8],
    cancelled: &dyn Fn() -> bool,
    next_id: &dyn Fn() -> u64,
    remove: &dyn Fn(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let dir = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
    // create_new: a file that is not ours is never opened, so never removed by the guard.
    let (mut file, path) = (0..NAME_TRIES)
        .find_map(|_| {
            let path = dir.join(format!(
                ".playblast-{}-{}.pb_tmp",
                std::process::id(),
                next_id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                opened => Some(opened.map(|file| (file, path))),
            }
        })
        .unwrap_or(Err(io::ErrorKind::Other.into()))?;
    let staged = Staged { path, remove };
    file.write_all(bytes)?;
    file.sync_all()?;
    if cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    fs::hard_link(&staged.path, target)
}
