//! The image plane model (#459): one plate attached to the live camera, its defaults, validation
//! and the all-or-nothing edit. Pure data: no files, decoding, editor or renderer state. The image
//! size comes from the caller (the header probe), so coverage can be checked against it.
//!
//! Defaults follow the Maya 2027 imagePlane node page where it states them (depth 100 cm, alpha
//! gain 1, frame offset 0, frame cache 24, fit Best, display mode RGBA). Everything below marked
//! PROVISIONAL is our reading or our own safety bound, pending the #5 comparison; none of it is a
//! parity claim.

use std::fmt;
use std::path::PathBuf;

/// Node page default (cm).
pub const DEFAULT_DEPTH_CM: f32 = 100.0;
pub const DEFAULT_ALPHA_GAIN: f32 = 1.0;
pub const DEFAULT_FRAME_OFFSET: i32 = 0;
/// Node page default (frames kept in memory); the attribute editor page says "10% of memory".
pub const DEFAULT_FRAME_CACHE: u32 = 24;

/// PROVISIONAL (#5) safety bounds, not Maya limits.
pub const MIN_FRAME_CACHE: u32 = 1;
pub const MAX_FRAME_CACHE: u32 = 1000;
pub const MAX_FRAME_OFFSET: i32 = 1_000_000;
pub const MAX_ALPHA_GAIN: f32 = 1e3;

/// How the image is scaled into the film gate. The pages name the five modes but no indices;
/// PROVISIONAL: the order below is our reading of the list order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlateFit {
    Fill,
    #[default]
    Best,
    Horizontal,
    Vertical,
    ToSize,
}

/// What the viewport shows of the plate. PROVISIONAL: the pages do not say what Outline draws in
/// the viewport (the node page says it still renders as RGB).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlateDisplayMode {
    None,
    Outline,
    Rgb,
    #[default]
    Rgba,
    Luminance,
    Alpha,
}

/// The source pixels used, in image pixels. The origin is the image pixel at the plane's lower
/// left (node page). `None` on the plane means the full image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coverage {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, PartialEq)]
pub struct ImagePlane {
    /// The first frame's path; with `use_frame_extension` it is the template for the sequence.
    pub image_name: PathBuf,
    pub use_frame_extension: bool,
    pub frame_offset: i32,
    pub frame_cache: u32,
    pub fit: PlateFit,
    /// Distance from the camera in centimetres.
    pub depth: f32,
    pub alpha_gain: f32,
    pub display_mode: PlateDisplayMode,
    pub coverage: Option<Coverage>,
}

/// Every field optional. Creating a plane requires `image_name`. `coverage` is
/// `Some(None)` to go back to the full image.
#[derive(Clone, Default, PartialEq)]
pub struct ImagePlaneEdit {
    pub image_name: Option<PathBuf>,
    pub use_frame_extension: Option<bool>,
    pub frame_offset: Option<i32>,
    pub frame_cache: Option<u32>,
    pub fit: Option<PlateFit>,
    pub depth: Option<f32>,
    pub alpha_gain: Option<f32>,
    pub display_mode: Option<PlateDisplayMode>,
    pub coverage: Option<Option<Coverage>>,
}

// The image name is a user path: Debug prints its length only, so logs and panics never carry it.
impl fmt::Debug for ImagePlane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImagePlane")
            .field("image_name_len", &self.image_name.as_os_str().len())
            .field("use_frame_extension", &self.use_frame_extension)
            .field("frame_offset", &self.frame_offset)
            .field("frame_cache", &self.frame_cache)
            .field("fit", &self.fit)
            .field("depth", &self.depth)
            .field("alpha_gain", &self.alpha_gain)
            .field("display_mode", &self.display_mode)
            .field("coverage", &self.coverage)
            .finish()
    }
}

impl fmt::Debug for ImagePlaneEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImagePlaneEdit")
            .field(
                "image_name_len",
                &self.image_name.as_ref().map(|n| n.as_os_str().len()),
            )
            .field("use_frame_extension", &self.use_frame_extension)
            .field("frame_offset", &self.frame_offset)
            .field("frame_cache", &self.frame_cache)
            .field("fit", &self.fit)
            .field("depth", &self.depth)
            .field("alpha_gain", &self.alpha_gain)
            .field("display_mode", &self.display_mode)
            .field("coverage", &self.coverage)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlateChange {
    Changed,
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImagePlaneError {
    ImageNameMissing,
    ImageNameEmpty,
    NotFinite {
        field: &'static str,
    },
    OutOfRange {
        field: &'static str,
        min: f64,
        max: f64,
    },
    DepthOutOfRange,
    ImageSizeZero {
        field: &'static str,
    },
    CoverageEmpty,
    CoverageOutsideImage {
        image_width: u32,
        image_height: u32,
    },
}

impl fmt::Display for ImagePlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImageNameMissing => write!(f, "image_name is required to create an image plane"),
            Self::ImageNameEmpty => write!(f, "image_name must not be empty"),
            Self::NotFinite { field } => write!(f, "{field} must be a finite number"),
            Self::OutOfRange { field, min, max } => {
                write!(f, "{field} must be between {min} and {max}")
            }
            Self::DepthOutOfRange => {
                write!(f, "depth must be greater than 0 and at most {:e}", f32::MAX)
            }
            Self::ImageSizeZero { field } => write!(f, "{field} must be at least 1"),
            Self::CoverageEmpty => write!(f, "coverage width and height must be at least 1"),
            Self::CoverageOutsideImage {
                image_width,
                image_height,
            } => write!(
                f,
                "coverage must lie inside the {image_width}x{image_height} image; \
                 reset it in the same edit (coverage = full image) when the image changes"
            ),
        }
    }
}

impl std::error::Error for ImagePlaneError {}

impl ImagePlane {
    /// A plane with the documented defaults for `image_name`. Not validated: use [`Self::create`].
    fn with_defaults(image_name: PathBuf) -> Self {
        Self {
            image_name,
            use_frame_extension: false,
            frame_offset: DEFAULT_FRAME_OFFSET,
            frame_cache: DEFAULT_FRAME_CACHE,
            fit: PlateFit::default(),
            depth: DEFAULT_DEPTH_CM,
            alpha_gain: DEFAULT_ALPHA_GAIN,
            display_mode: PlateDisplayMode::default(),
            coverage: None,
        }
    }

    /// Creates a plane from defaults plus `edit`; `edit.image_name` is required. `image_size` is
    /// the probed size of the first frame.
    pub fn create(edit: &ImagePlaneEdit, image_size: (u32, u32)) -> Result<Self, ImagePlaneError> {
        let name = edit
            .image_name
            .clone()
            .ok_or(ImagePlaneError::ImageNameMissing)?;
        let mut plane = Self::with_defaults(name);
        plane.set_fields(edit);
        plane.validate(image_size)?;
        Ok(plane)
    }

    /// Applies `edit` as a whole: the edited copy is validated, and only then replaces `self`, so
    /// a refusal leaves the plane untouched. `image_size` is the size of the image after the edit
    /// (the caller probes a new `image_name` first). Equality is numeric, so `-0.0` equals `0.0`.
    pub fn apply(
        &mut self,
        edit: &ImagePlaneEdit,
        image_size: (u32, u32),
    ) -> Result<PlateChange, ImagePlaneError> {
        let mut candidate = self.clone();
        candidate.set_fields(edit);
        candidate.validate(image_size)?;
        if candidate == *self {
            return Ok(PlateChange::Unchanged);
        }
        *self = candidate;
        Ok(PlateChange::Changed)
    }

    fn set_fields(&mut self, edit: &ImagePlaneEdit) {
        if let Some(v) = &edit.image_name {
            self.image_name = v.clone();
        }
        if let Some(v) = edit.use_frame_extension {
            self.use_frame_extension = v;
        }
        if let Some(v) = edit.frame_offset {
            self.frame_offset = v;
        }
        if let Some(v) = edit.frame_cache {
            self.frame_cache = v;
        }
        if let Some(v) = edit.fit {
            self.fit = v;
        }
        if let Some(v) = edit.depth {
            self.depth = v;
        }
        if let Some(v) = edit.alpha_gain {
            // Adding 0.0 turns -0.0 into +0.0, so one zero is stored.
            self.alpha_gain = v + 0.0;
        }
        if let Some(v) = edit.display_mode {
            self.display_mode = v;
        }
        if let Some(v) = edit.coverage {
            self.coverage = v;
        }
    }

    fn validate(&self, image_size: (u32, u32)) -> Result<(), ImagePlaneError> {
        if image_size.0 == 0 {
            return Err(ImagePlaneError::ImageSizeZero {
                field: "image_width",
            });
        }
        if image_size.1 == 0 {
            return Err(ImagePlaneError::ImageSizeZero {
                field: "image_height",
            });
        }
        if self.image_name.as_os_str().is_empty() {
            return Err(ImagePlaneError::ImageNameEmpty);
        }
        if !self.depth.is_finite() {
            return Err(ImagePlaneError::NotFinite { field: "depth" });
        }
        if self.depth <= 0.0 {
            return Err(ImagePlaneError::DepthOutOfRange);
        }
        if !self.alpha_gain.is_finite() {
            return Err(ImagePlaneError::NotFinite {
                field: "alpha_gain",
            });
        }
        if !(0.0..=MAX_ALPHA_GAIN).contains(&self.alpha_gain) {
            return Err(ImagePlaneError::OutOfRange {
                field: "alpha_gain",
                min: 0.0,
                max: f64::from(MAX_ALPHA_GAIN),
            });
        }
        if self.frame_offset.unsigned_abs() > MAX_FRAME_OFFSET.unsigned_abs() {
            return Err(ImagePlaneError::OutOfRange {
                field: "frame_offset",
                min: -f64::from(MAX_FRAME_OFFSET),
                max: f64::from(MAX_FRAME_OFFSET),
            });
        }
        if !(MIN_FRAME_CACHE..=MAX_FRAME_CACHE).contains(&self.frame_cache) {
            return Err(ImagePlaneError::OutOfRange {
                field: "frame_cache",
                min: f64::from(MIN_FRAME_CACHE),
                max: f64::from(MAX_FRAME_CACHE),
            });
        }
        if let Some(c) = self.coverage {
            if c.width == 0 || c.height == 0 {
                return Err(ImagePlaneError::CoverageEmpty);
            }
            let (w, h) = image_size;
            let inside = c.origin_x.checked_add(c.width).is_some_and(|x| x <= w)
                && c.origin_y.checked_add(c.height).is_some_and(|y| y <= h);
            if !inside {
                return Err(ImagePlaneError::CoverageOutsideImage {
                    image_width: w,
                    image_height: h,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
