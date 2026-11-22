//! The image plane operation core (#459): the rules `Operation::SetImagePlane` and
//! `RemoveImagePlane` share between the native UI and MCP. It owns the session state (a plane and
//! its probed first-frame size) and decides, before any mutation, whether an edit is allowed.
//!
//! Nothing here carries a path in an error or in `Debug` output.

use std::fmt;
use std::path::Path;

use super::decode::{PlateError, probe_file};
use super::model::{Coverage, ImagePlane, ImagePlaneEdit, ImagePlaneError, PlateChange};

/// The file-tool path rule (`mcp.md`): 1 to 4096 UTF-8 bytes, no NUL.
pub const MAX_IMAGE_NAME_BYTES: usize = 4096;
/// PROVISIONAL (#459): the formats the decoder supports. Checked before any open.
pub const ALLOWED_EXTENSIONS: [&str; 3] = ["png", "jpg", "jpeg"];

/// The plane and the size of its first frame (probed when the name last changed).
#[derive(Clone, Debug, PartialEq)]
pub struct ImagePlaneState {
    pub plane: ImagePlane,
    pub image_size: (u32, u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImagePlaneOpError {
    /// An edit that names no field.
    EmptyEdit,
    /// `coverage` and `coverage_parts` in one edit.
    CoverageConflict,
    NameNotUtf8,
    NameHasNul,
    NameLength,
    NameExtension,
    /// The first frame could not be probed. Its text never carries a path.
    Plate(PlateError),
    Model(ImagePlaneError),
}

impl fmt::Display for ImagePlaneOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyEdit => f.write_str("name at least one image plane field to set"),
            Self::CoverageConflict => {
                f.write_str("name a whole coverage or some of its numbers, not both")
            }
            Self::NameNotUtf8 => f.write_str("image_name must be valid UTF-8"),
            Self::NameHasNul => f.write_str("image_name must not contain a NUL character"),
            Self::NameLength => write!(
                f,
                "image_name must contain 1 to {MAX_IMAGE_NAME_BYTES} UTF-8 bytes"
            ),
            Self::NameExtension => {
                f.write_str("image_name must end in .png, .jpg or .jpeg (any case)")
            }
            Self::Plate(error) => error.fmt(f),
            Self::Model(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ImagePlaneOpError {}

impl From<ImagePlaneError> for ImagePlaneOpError {
    fn from(error: ImagePlaneError) -> Self {
        Self::Model(error)
    }
}

impl From<PlateError> for ImagePlaneOpError {
    fn from(error: PlateError) -> Self {
        Self::Plate(error)
    }
}

/// The path rules, all decided from the text alone so a refused name never touches the disk.
pub fn check_image_name(name: &Path) -> Result<(), ImagePlaneOpError> {
    let text = name.to_str().ok_or(ImagePlaneOpError::NameNotUtf8)?;
    if text.contains('\0') {
        return Err(ImagePlaneOpError::NameHasNul);
    }
    if text.is_empty() || text.len() > MAX_IMAGE_NAME_BYTES {
        return Err(ImagePlaneOpError::NameLength);
    }
    let allowed = name
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ALLOWED_EXTENSIONS.iter().any(|a| e.eq_ignore_ascii_case(a)));
    if allowed {
        Ok(())
    } else {
        Err(ImagePlaneOpError::NameExtension)
    }
}

/// Creates the plane (when `state` is `None`; `image_name` is required) or edits it, all or
/// nothing: any error leaves `state` as it was. An empty edit is refused.
///
/// The first frame's header is read (at most 1 MiB) when the name is new. PROVISIONAL: setting
/// the same `image_name` again is Unchanged and does not reload the file; remove the plane or
/// change the name to pick up a file that changed on disk.
pub fn set_image_plane(
    state: &mut Option<ImagePlaneState>,
    edit: &ImagePlaneEdit,
) -> Result<PlateChange, ImagePlaneOpError> {
    if edit.is_empty() {
        return Err(ImagePlaneOpError::EmptyEdit);
    }
    if let Some(name) = &edit.image_name {
        check_image_name(name)?;
    }
    if !edit.coverage_parts.is_empty() && edit.coverage.is_some() {
        return Err(ImagePlaneOpError::CoverageConflict);
    }
    match state {
        None => {
            let name = edit
                .image_name
                .as_deref()
                .ok_or(ImagePlaneError::ImageNameMissing)?;
            let info = probe_file(name)?;
            let image_size = (info.width, info.height);
            let edit = merge_coverage(edit, None, image_size);
            let plane = ImagePlane::create(&edit, image_size)?;
            *state = Some(ImagePlaneState { plane, image_size });
            Ok(PlateChange::Changed)
        }
        Some(current) => {
            let new_name = edit
                .image_name
                .as_deref()
                .filter(|name| *name != current.plane.image_name);
            let image_size = match new_name {
                Some(name) => {
                    let info = probe_file(name)?;
                    (info.width, info.height)
                }
                None => current.image_size,
            };
            let mut plane = current.plane.clone();
            let edit = merge_coverage(edit, current.plane.coverage, image_size);
            let change = plane.apply(&edit, image_size)?;
            if change == PlateChange::Changed {
                *current = ImagePlaneState { plane, image_size };
            }
            Ok(change)
        }
    }
}

/// Turns `coverage_parts` into a whole coverage: the stored coverage (or the full image when none
/// is stored) with the named numbers replaced. The model then checks the result against the image.
fn merge_coverage(
    edit: &ImagePlaneEdit,
    stored: Option<Coverage>,
    image_size: (u32, u32),
) -> ImagePlaneEdit {
    let parts = edit.coverage_parts;
    let mut merged = edit.clone();
    merged.coverage_parts = Default::default();
    if !parts.is_empty() {
        let base = stored.unwrap_or(Coverage {
            origin_x: 0,
            origin_y: 0,
            width: image_size.0,
            height: image_size.1,
        });
        merged.coverage = Some(Some(Coverage {
            origin_x: parts.origin_x.unwrap_or(base.origin_x),
            origin_y: parts.origin_y.unwrap_or(base.origin_y),
            width: parts.width.unwrap_or(base.width),
            height: parts.height.unwrap_or(base.height),
        }));
    }
    merged
}

/// Removes the plane; Unchanged when there was none.
pub fn remove_image_plane(state: &mut Option<ImagePlaneState>) -> PlateChange {
    if state.take().is_some() {
        PlateChange::Changed
    } else {
        PlateChange::Unchanged
    }
}

#[cfg(test)]
mod tests;
