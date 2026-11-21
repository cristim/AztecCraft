//! Staged PNG installation. `craft_atomic::write_new` resolves symlinks and then checks
//! `exists()`, so it would write through a dangling symlink at the target; this stages the file
//! itself and installs it with `hard_link`, which never follows a destination symlink and fails
//! when any entry (a dangling symlink included) is there.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
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
/// `cancelled()` says stop (`Interrupted`). The staged file is removed on every path.
pub fn install_frame(target: &Path, bytes: &[u8], cancelled: &dyn Fn() -> bool) -> io::Result<()> {
    let dir = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
    let staged = dir.join(format!(
        ".playblast-{}-{}.pb_tmp",
        std::process::id(),
        STAGED.fetch_add(1, Ordering::Relaxed)
    ));
    // create_new: a file that is not ours is never opened, so never removed below.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staged)?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            if cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            fs::hard_link(&staged, target)
        });
    drop(file);
    let removed = fs::remove_file(&staged);
    result?;
    // The frame is in place; a failed unlink only leaves a stray staged name.
    removed
}
