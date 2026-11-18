//! Image sequence file naming (#459). The frame number replaces the last run of ASCII digits in
//! the file name stem (never the directory or the extension), keeping the template's zero padding. Maya's pages only
//! say "use the first file", so the numbering rule is PROVISIONAL application policy.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io;
use std::path::{Path, PathBuf};

use super::decode::PlateError;

/// Returns `template` with its last digit run in the file name stem replaced by `frame`, padded
/// with zeros to the run's width (the number grows past it). A negative frame or a name with
/// no digits is [`PlateError::Missing`]; a name that is not UTF-8 is `Io(InvalidInput)`.
pub fn frame_path(template: &Path, frame: i64) -> Result<PathBuf, PlateError> {
    let name = template
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(PlateError::Io(io::ErrorKind::InvalidInput))?;
    if frame < 0 {
        return Err(PlateError::Missing);
    }
    // The extension is excluded so `shot.0001.mp4` numbers the 0001, not the 4.
    let stem_len = name.rfind('.').filter(|&dot| dot > 0).unwrap_or(name.len());
    let bytes = &name.as_bytes()[..stem_len];
    let end = bytes
        .iter()
        .rposition(u8::is_ascii_digit)
        .ok_or(PlateError::Missing)?
        + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    let width = end - start;
    let numbered = format!("{}{frame:0width$}{}", &name[..start], &name[end..]);
    Ok(template.with_file_name(numbered))
}

#[cfg(test)]
mod tests;
