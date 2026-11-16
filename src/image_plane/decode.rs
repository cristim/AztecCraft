//! Plate image decoding with hard limits (#459). Image files are untrusted input.
//!
//! Order of defence: a bounded header probe, the header parse, [`check_dimensions`] and only
//! then the single `width * height * 4` allocation. The decoders' own limits do not count the
//! caller's output buffer (`png::Limits` bound intermediate buffers only), so the check in this
//! module is the real decompression-bomb defence. Every decode runs inside `catch_unwind`; a
//! decoder panic is a typed [`PlateError::Internal`]. All limits are PROVISIONAL application
//! policy (Maya documents none).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{self, Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use zune_jpeg::JpegDecoder;
use zune_jpeg::errors::DecodeErrors;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use crate::document::open_regular_file;

/// Largest plate file read for a decode.
pub const MAX_PLATE_FILE_BYTES: usize = 128 * 1024 * 1024;
/// Largest prefix read to parse a header without decoding.
pub const PROBE_BYTES: usize = 1024 * 1024;
/// Largest width or height; also the default wgpu `max_texture_dimension_2d`.
pub const MAX_PLATE_DIMENSION: u32 = 8192;
/// Largest pixel count (128 MiB of RGBA8).
pub const MAX_PLATE_PIXELS: u64 = 33_554_432;
/// Largest pixel count of a progressive JPEG (4096 x 4096): zune holds the coefficients of the
/// whole image, about 6 bytes per pixel, on top of the RGBA output.
pub const MAX_PROGRESSIVE_JPEG_PIXELS: u64 = 4096 * 4096;
/// Bytes the PNG decoder may allocate for its own intermediate buffers.
const PNG_DECODER_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// A file format recognised by its magic bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateFormat {
    Png,
    Jpeg,
    Exr,
    Dpx,
    Tiff,
    /// A CMYK or YCCK JPEG, which has no plate colour path.
    JpegCmyk,
    Unknown,
}

impl PlateFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Exr => "EXR",
            Self::Dpx => "DPX",
            Self::Tiff => "TIFF",
            Self::JpegCmyk => "CMYK JPEG",
            Self::Unknown => "unrecognised",
        }
    }
}

/// A typed plate failure. Display text never contains a path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlateError {
    Io(io::ErrorKind),
    /// A file, dimension or pixel count above the limits.
    TooLarge,
    /// The header does not fit in [`PROBE_BYTES`].
    HeaderTooLarge,
    UnsupportedFormat {
        format: PlateFormat,
    },
    Corrupt,
    Truncated,
    DimensionMismatch,
    Internal,
    WorkerGone,
    /// The frame has no file: a gap, a negative frame or a name with no digits.
    Missing,
}

impl std::fmt::Display for PlateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(kind) => write!(f, "image could not be read ({kind})"),
            Self::TooLarge => f.write_str("image is over the plate size limits"),
            Self::HeaderTooLarge => f.write_str("image header is too large to read"),
            Self::UnsupportedFormat { format } => {
                write!(f, "{} images are not supported as plates", format.name())
            }
            Self::Corrupt => f.write_str("image data is corrupt"),
            Self::Truncated => f.write_str("image data is truncated"),
            Self::DimensionMismatch => {
                f.write_str("image size differs from the first frame of the sequence")
            }
            Self::Internal => f.write_str("image decoder failed internally"),
            Self::WorkerGone => f.write_str("plate decoder is not running"),
            Self::Missing => f.write_str("no image for this frame"),
        }
    }
}

impl std::error::Error for PlateError {}

/// What a header says about a plate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlateInfo {
    pub width: u32,
    pub height: u32,
    pub format: PlateFormat,
}

/// A decoded plate frame: straight-alpha RGBA8, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlateFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Identifies the format from the first bytes, or `None` when nothing matches.
pub fn sniff(bytes: &[u8]) -> Option<PlateFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(PlateFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(PlateFormat::Jpeg)
    } else if bytes.starts_with(&[0x76, 0x2f, 0x31, 0x01]) {
        Some(PlateFormat::Exr)
    } else if bytes.starts_with(b"SDPX") || bytes.starts_with(b"XPDS") {
        Some(PlateFormat::Dpx)
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        Some(PlateFormat::Tiff)
    } else {
        None
    }
}

/// Validates a header size and returns the RGBA8 byte length. Pure and checked: zero, over
/// `MAX_PLATE_DIMENSION` on either side, over `MAX_PLATE_PIXELS` or an overflow all refuse.
pub fn check_dimensions(width: u32, height: u32) -> Result<usize, PlateError> {
    if width == 0 || height == 0 {
        return Err(PlateError::Corrupt);
    }
    if width > MAX_PLATE_DIMENSION || height > MAX_PLATE_DIMENSION {
        return Err(PlateError::TooLarge);
    }
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(PlateError::TooLarge)?;
    if pixels > MAX_PLATE_PIXELS {
        return Err(PlateError::TooLarge);
    }
    pixels
        .checked_mul(4)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(PlateError::TooLarge)
}

fn unsupported(bytes: &[u8]) -> PlateError {
    match sniff(bytes) {
        Some(format) => PlateError::UnsupportedFormat { format },
        None if bytes.len() < 8 => PlateError::Truncated,
        None => PlateError::UnsupportedFormat {
            format: PlateFormat::Unknown,
        },
    }
}

fn guarded<T>(work: impl FnOnce() -> Result<T, PlateError>) -> Result<T, PlateError> {
    catch_unwind(AssertUnwindSafe(work)).unwrap_or(Err(PlateError::Internal))
}

/// Parses the size from a prefix of at most [`PROBE_BYTES`]. A header that needs more than the
/// prefix gives [`PlateError::HeaderTooLarge`] when the prefix is full, else `Truncated`.
pub fn probe_bytes(prefix: &[u8]) -> Result<PlateInfo, PlateError> {
    let result = guarded(|| match sniff(prefix) {
        Some(PlateFormat::Png) => probe_png(prefix),
        Some(PlateFormat::Jpeg) => probe_jpeg(prefix),
        _ => Err(unsupported(prefix)),
    });
    match result {
        Err(PlateError::Truncated) if prefix.len() >= PROBE_BYTES => {
            Err(PlateError::HeaderTooLarge)
        }
        other => other,
    }
}

/// Reads at most [`PROBE_BYTES`] from `reader` and probes them.
pub fn probe_reader(reader: impl Read) -> Result<PlateInfo, PlateError> {
    let mut prefix = Vec::new();
    reader
        .take(PROBE_BYTES as u64)
        .read_to_end(&mut prefix)
        .map_err(|error| PlateError::Io(error.kind()))?;
    probe_bytes(&prefix)
}

/// Opens a regular file and probes its header; never reads more than [`PROBE_BYTES`].
pub fn probe_file(path: &Path) -> Result<PlateInfo, PlateError> {
    let (file, _) = open_regular_file(path).map_err(|error| PlateError::Io(error.kind()))?;
    probe_reader(file)
}

/// Reads a whole plate file through the regular-file open, never more than
/// [`MAX_PLATE_FILE_BYTES`]; a file over the limit is refused before and while reading.
pub fn read_plate_file(path: &Path) -> Result<Vec<u8>, PlateError> {
    let (file, metadata) = open_regular_file(path).map_err(|error| PlateError::Io(error.kind()))?;
    if metadata.len() > MAX_PLATE_FILE_BYTES as u64 {
        return Err(PlateError::TooLarge);
    }
    let wanted = metadata.len().min(MAX_PLATE_FILE_BYTES as u64 + 1);
    let mut bytes = Vec::with_capacity(usize::try_from(wanted).unwrap_or(0));
    file.take(MAX_PLATE_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| PlateError::Io(error.kind()))?;
    if bytes.len() > MAX_PLATE_FILE_BYTES {
        return Err(PlateError::TooLarge);
    }
    Ok(bytes)
}

/// Decodes a whole plate file image to RGBA8 within the limits.
pub fn decode_plate(bytes: &[u8]) -> Result<PlateFrame, PlateError> {
    if bytes.len() > MAX_PLATE_FILE_BYTES {
        return Err(PlateError::TooLarge);
    }
    guarded(|| match sniff(bytes) {
        Some(PlateFormat::Png) => decode_png(bytes),
        Some(PlateFormat::Jpeg) => decode_jpeg(bytes),
        _ => Err(unsupported(bytes)),
    })
}

fn png_error(error: png::DecodingError) -> PlateError {
    match error {
        png::DecodingError::IoError(io) if io.kind() == io::ErrorKind::UnexpectedEof => {
            PlateError::Truncated
        }
        png::DecodingError::IoError(io) => PlateError::Io(io.kind()),
        png::DecodingError::LimitsExceeded => PlateError::TooLarge,
        png::DecodingError::Parameter(_) => PlateError::Internal,
        _ => PlateError::Corrupt,
    }
}

fn png_reader(bytes: &[u8]) -> Result<png::Reader<Cursor<&[u8]>>, PlateError> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: PNG_DECODER_LIMIT_BYTES,
        },
    );
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.read_info().map_err(png_error)
}

fn probe_png(prefix: &[u8]) -> Result<PlateInfo, PlateError> {
    let reader = png_reader(prefix)?;
    let (width, height) = reader.info().size();
    check_dimensions(width, height)?;
    Ok(PlateInfo {
        width,
        height,
        format: PlateFormat::Png,
    })
}

fn decode_png(bytes: &[u8]) -> Result<PlateFrame, PlateError> {
    let mut reader = png_reader(bytes)?;
    let (width, height) = reader.info().size();
    let total = check_dimensions(width, height)?;
    let native = reader.output_buffer_size().ok_or(PlateError::TooLarge)?;
    let (color, depth) = reader.output_color_type();
    let channels = match (color, depth) {
        (png::ColorType::Grayscale, png::BitDepth::Eight) => 1,
        (png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => 2,
        (png::ColorType::Rgb, png::BitDepth::Eight) => 3,
        (png::ColorType::Rgba, png::BitDepth::Eight) => 4,
        _ => return Err(PlateError::Internal),
    };
    if native > total {
        return Err(PlateError::Internal);
    }
    let mut rgba = vec![0u8; total];
    let native_bytes = rgba.get_mut(..native).ok_or(PlateError::Internal)?;
    reader.next_frame(native_bytes).map_err(png_error)?;
    expand_to_rgba(&mut rgba, channels);
    Ok(PlateFrame {
        width,
        height,
        rgba,
    })
}

/// Expands `channels`-per-pixel data held at the front of `buf` to RGBA in place, walking
/// backwards so no source pixel is overwritten before it is read.
fn expand_to_rgba(buf: &mut [u8], channels: usize) {
    if channels == 4 {
        return;
    }
    for pixel in (0..buf.len() / 4).rev() {
        let source = pixel * channels;
        let Some(src) = buf.get(source..source + channels) else {
            continue;
        };
        let rgba = match *src {
            [g] => [g, g, g, 255],
            [g, a] => [g, g, g, a],
            [r, g, b] => [r, g, b, 255],
            _ => continue,
        };
        if let Some(dst) = buf.get_mut(pixel * 4..pixel * 4 + 4) {
            dst.copy_from_slice(&rgba);
        }
    }
}

fn jpeg_error(error: DecodeErrors) -> PlateError {
    match error {
        DecodeErrors::ExhaustedData | DecodeErrors::IoErrors(_) => PlateError::Truncated,
        DecodeErrors::LargeDimensions(_) => PlateError::TooLarge,
        DecodeErrors::Unsupported(_) => PlateError::UnsupportedFormat {
            format: PlateFormat::Jpeg,
        },
        _ => PlateError::Corrupt,
    }
}

/// Strict so truncated or damaged scans fail instead of returning a part-grey frame. zune's own
/// size limit (16384) stays at its default: it is only a second line behind
/// [`check_dimensions`], which is what refuses anything over the plate limits.
fn jpeg_decoder(bytes: &[u8]) -> JpegDecoder<ZCursor<&[u8]>> {
    let options = DecoderOptions::default()
        .set_strict_mode(true)
        .jpeg_set_out_colorspace(ColorSpace::RGBA);
    JpegDecoder::new_with_options(ZCursor::new(bytes), options)
}

/// Parses the headers and returns the checked size and RGBA byte length.
fn jpeg_header(decoder: &mut JpegDecoder<ZCursor<&[u8]>>) -> Result<(u32, u32, usize), PlateError> {
    decoder.decode_headers().map_err(jpeg_error)?;
    if matches!(
        decoder.input_colorspace(),
        Some(ColorSpace::CMYK | ColorSpace::YCCK)
    ) {
        return Err(PlateError::UnsupportedFormat {
            format: PlateFormat::JpegCmyk,
        });
    }
    let info = decoder.info().ok_or(PlateError::Internal)?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let total = check_dimensions(width, height)?;
    if info.sof.is_progressive()
        && u64::from(width) * u64::from(height) > MAX_PROGRESSIVE_JPEG_PIXELS
    {
        return Err(PlateError::TooLarge);
    }
    Ok((width, height, total))
}

fn probe_jpeg(prefix: &[u8]) -> Result<PlateInfo, PlateError> {
    let (width, height, _) = jpeg_header(&mut jpeg_decoder(prefix))?;
    Ok(PlateInfo {
        width,
        height,
        format: PlateFormat::Jpeg,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Result<PlateFrame, PlateError> {
    let mut decoder = jpeg_decoder(bytes);
    let (width, height, total) = jpeg_header(&mut decoder)?;
    if decoder.output_buffer_size() != Some(total) {
        return Err(PlateError::Internal);
    }
    let mut rgba = vec![0u8; total];
    decoder.decode_into(&mut rgba).map_err(jpeg_error)?;
    Ok(PlateFrame {
        width,
        height,
        rgba,
    })
}
