//! The renderer's view of a plate (#459 P7b), kept free of wgpu so the byte layouts and the
//! validation rules are unit-testable: what the display mode means to the shader, the uniform
//! and vertex bytes, and the checks a frame must pass before any GPU allocation.
//!
//! The plate shader (`src/plate_shader.wgsl`) reads exactly these layouts. Everything the Maya
//! pages do not state is PROVISIONAL (#5): what Outline shows, that Luminance and RGB ignore
//! alpha, and that transparent pixels still write depth.

use std::fmt;

use super::fit::PlateQuad;
use super::model::{MAX_ALPHA_GAIN, PlateDisplayMode};

/// How the fragment shader turns a sampled texel into a colour. The numbers are the shader's
/// `mode` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum PlateShading {
    /// Straight alpha times the gain.
    Rgba = 0,
    /// The colour with alpha 1 (RGB ignores alpha, per the Image Plane Attributes page).
    Rgb = 1,
    /// Rec. 709 weights on the display-encoded values, alpha 1 (PROVISIONAL).
    Luminance = 2,
    /// The alpha (times the gain) as an opaque grey.
    Alpha = 3,
}

/// What a display mode asks the renderer to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateStyle {
    /// Draw nothing: no quad and no depth write.
    Skip,
    /// Only the quad's four edges, no texture and no plate depth (PROVISIONAL: the pages do not
    /// say what the viewport shows for Outline).
    Outline,
    Textured(PlateShading),
}

impl PlateStyle {
    pub fn of(mode: PlateDisplayMode) -> Self {
        match mode {
            PlateDisplayMode::None => Self::Skip,
            PlateDisplayMode::Outline => Self::Outline,
            PlateDisplayMode::Rgb => Self::Textured(PlateShading::Rgb),
            PlateDisplayMode::Rgba => Self::Textured(PlateShading::Rgba),
            PlateDisplayMode::Luminance => Self::Textured(PlateShading::Luminance),
            PlateDisplayMode::Alpha => Self::Textured(PlateShading::Alpha),
        }
    }
}

/// Why a plate could not be uploaded or drawn. Never carries a path or pixel data. A plate
/// failure is non-terminal: the viewport keeps drawing without the plate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateUploadError {
    /// A frame with no pixels.
    ZeroSize,
    /// A side above what the device can hold as a 2D texture.
    OverDeviceLimit { limit: u32 },
    /// The pixel bytes are not `width * height * 4`.
    ByteCountMismatch,
    /// A non-finite alpha gain or quad coordinate.
    NonFinite,
    /// The GPU ran out of memory creating the texture.
    GpuMemory,
}

impl fmt::Display for PlateUploadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSize => f.write_str("the plate frame has no pixels"),
            Self::OverDeviceLimit { limit } => {
                write!(
                    f,
                    "the plate frame is larger than the device limit of {limit} pixels a side"
                )
            }
            Self::ByteCountMismatch => {
                f.write_str("the plate frame bytes do not match its width and height")
            }
            Self::NonFinite => f.write_str("the plate has a non-finite gain or corner"),
            Self::GpuMemory => f.write_str("the GPU ran out of memory for the plate"),
        }
    }
}

impl std::error::Error for PlateUploadError {}

/// Checks a frame before any allocation and returns its byte count: both sides at least 1 and at
/// most `max_texture_dimension`, and `bytes == width * height * 4` in checked arithmetic.
pub fn check_frame(
    width: u32,
    height: u32,
    bytes: usize,
    max_texture_dimension: u32,
) -> Result<usize, PlateUploadError> {
    if width == 0 || height == 0 {
        return Err(PlateUploadError::ZeroSize);
    }
    if width > max_texture_dimension || height > max_texture_dimension {
        return Err(PlateUploadError::OverDeviceLimit {
            limit: max_texture_dimension,
        });
    }
    let expected = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(usize::try_from(height).ok()?))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(PlateUploadError::ByteCountMismatch)?;
    if expected != bytes {
        return Err(PlateUploadError::ByteCountMismatch);
    }
    Ok(expected)
}

/// Bytes of the shader's `PlateParams` uniform: `mode: u32`, `output_srgb: u32`,
/// `alpha_gain: f32` and one padding word, little-endian native order, 16 bytes.
pub const UNIFORM_BYTES: usize = 16;

/// The uniform for a textured plate. `output_srgb` says the render target is an sRGB format, so
/// the shader writes linear values and the target re-encodes them to the stored bytes
/// (the colour handling of section 2d: bytes are display-referred and pass through unchanged;
/// no OCIO, #49; PROVISIONAL: filtering then mixes encoded values, unlike an sRGB texture's linear
/// filtering). The gain is clamped to 0..=MAX_ALPHA_GAIN.
pub fn uniform_bytes(
    shading: PlateShading,
    alpha_gain: f32,
    output_srgb: bool,
) -> Result<[u8; UNIFORM_BYTES], PlateUploadError> {
    if !alpha_gain.is_finite() {
        return Err(PlateUploadError::NonFinite);
    }
    let gain = alpha_gain.clamp(0.0, MAX_ALPHA_GAIN);
    let words = [
        (shading as u32).to_ne_bytes(),
        u32::from(output_srgb).to_ne_bytes(),
        gain.to_ne_bytes(),
        0_u32.to_ne_bytes(),
    ];
    let mut out = [0; UNIFORM_BYTES];
    for (i, word) in words.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(word);
    }
    Ok(out)
}

/// Floats per vertex: clip position (x, y, z, w) then texture coordinates (u, v).
pub const VERTEX_FLOATS: usize = 6;
/// Two triangles for the textured fill, then the four edges as a line list.
pub const FILL_VERTICES: u32 = 6;
pub const OUTLINE_VERTICES: u32 = 8;
pub const QUAD_VERTICES: usize = (FILL_VERTICES + OUTLINE_VERTICES) as usize;
pub const QUAD_BYTES: usize = QUAD_VERTICES * VERTEX_FLOATS * 4;

/// The quad as vertex bytes: vertices 0..6 are the fill (corners 0,1,2 and 0,2,3, so lower-left,
/// lower-right, upper-right, then lower-left, upper-right, upper-left), vertices 6..14 the
/// outline (0-1, 1-2, 2-3, 3-0). The position is (ndc.x, ndc.y, z_ndc, 1); uv passes through
/// with v down, so the shader samples the frame as uploaded, top row first.
pub fn quad_vertex_bytes(quad: &PlateQuad) -> Result<[u8; QUAD_BYTES], PlateUploadError> {
    let corner = |i: usize| {
        [
            quad.ndc[i].x,
            quad.ndc[i].y,
            quad.z_ndc,
            1.0,
            quad.uv[i].x,
            quad.uv[i].y,
        ]
    };
    let order = [0, 1, 2, 0, 2, 3, 0, 1, 1, 2, 2, 3, 3, 0];
    let mut out = [0; QUAD_BYTES];
    for (slot, index) in order.into_iter().enumerate() {
        for (component, value) in corner(index).into_iter().enumerate() {
            if !value.is_finite() {
                return Err(PlateUploadError::NonFinite);
            }
            let at = (slot * VERTEX_FLOATS + component) * 4;
            out[at..at + 4].copy_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
