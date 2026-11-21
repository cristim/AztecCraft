//! `set_image_plane` and `remove_image_plane` (#459): the camera image plane of the live viewport
//! camera, session only. Both build the typed operations the native UI will dispatch.

use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::image_plane::model::{CoverageParts, ImagePlaneEdit, PlateDisplayMode, PlateFit};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FitArg {
    Fill,
    Best,
    Horizontal,
    Vertical,
    ToSize,
}

impl From<FitArg> for PlateFit {
    fn from(fit: FitArg) -> Self {
        match fit {
            FitArg::Fill => Self::Fill,
            FitArg::Best => Self::Best,
            FitArg::Horizontal => Self::Horizontal,
            FitArg::Vertical => Self::Vertical,
            FitArg::ToSize => Self::ToSize,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum DisplayModeArg {
    None,
    Outline,
    Rgb,
    Rgba,
    Luminance,
    Alpha,
}

impl From<DisplayModeArg> for PlateDisplayMode {
    fn from(mode: DisplayModeArg) -> Self {
        match mode {
            DisplayModeArg::None => Self::None,
            DisplayModeArg::Outline => Self::Outline,
            DisplayModeArg::Rgb => Self::Rgb,
            DisplayModeArg::Rgba => Self::Rgba,
            DisplayModeArg::Luminance => Self::Luminance,
            DisplayModeArg::Alpha => Self::Alpha,
        }
    }
}

/// Maya `imagePlane` attributes in Maya's units; every field is optional (omit or null leaves it
/// unchanged) and at least one must be set. The first call needs `image_name`.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetImagePlaneArgs {
    scene_token: String,
    /// The first frame's file (`imageName`): 1 to 4096 UTF-8 bytes, no NUL, ending in `.png`,
    /// `.jpg` or `.jpeg` (any case). Required to create the plane. Setting the same name again
    /// does not reload the file.
    #[schemars(length(min = 1, max = 4096))]
    image_name: Option<String>,
    /// Substitute the current time plus `frame_offset` into the last digit run of the file name
    /// (`useFrameExtension`).
    use_frame_extension: Option<bool>,
    /// Frames added to the current time (`frameOffset`), a whole number from -1000000 to 1000000.
    #[schemars(range(min = -1_000_000, max = 1_000_000))]
    frame_offset: Option<i32>,
    /// Frames kept in memory (`frameCache`), a whole number from 1 to 1000.
    #[schemars(range(min = 1, max = 1000))]
    frame_cache: Option<u32>,
    /// `fill`, `best`, `horizontal`, `vertical` or `to_size` (`fit`); the numbering is PROVISIONAL.
    fit: Option<FitArg>,
    /// Distance from the camera in centimetres (`depth`), greater than zero.
    depth: Option<f64>,
    /// Alpha multiplier (`alphaGain`), 0 to 1000.
    alpha_gain: Option<f64>,
    /// `none`, `outline`, `rgb`, `rgba`, `luminance` or `alpha` (`displayMode`).
    display_mode: Option<DisplayModeArg>,
    /// Width in source pixels of the part of the image used (`coverage`).
    coverage_x: Option<u32>,
    /// Height in source pixels of the part of the image used (`coverage`).
    coverage_y: Option<u32>,
    /// Image pixel column at the plane's lower left (`coverageOrigin`).
    coverage_origin_x: Option<u32>,
    /// Image pixel row at the plane's lower left (`coverageOrigin`).
    coverage_origin_y: Option<u32>,
    /// True restores the full image (clears the coverage); it excludes the four coverage numbers.
    #[serde(default)]
    reset_coverage: bool,
}

/// A finite number inside the f32 range, checked as sent so a value past it is refused rather
/// than rounded to infinity.
fn narrow(name: &str, value: f64) -> Result<f32, ErrorData> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(ErrorData::invalid_params(
            format!("{name} must be a finite number within the f32 range"),
            None,
        ));
    }
    Ok(value as f32)
}

fn parse_set_image_plane(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetImagePlaneArgs = decode(arguments)?;
    let parts = CoverageParts {
        origin_x: args.coverage_origin_x,
        origin_y: args.coverage_origin_y,
        width: args.coverage_x,
        height: args.coverage_y,
    };
    if args.reset_coverage && !parts.is_empty() {
        return Err(ErrorData::invalid_params(
            "reset_coverage excludes coverage_x, coverage_y, coverage_origin_x and coverage_origin_y",
            None,
        ));
    }
    let edit = ImagePlaneEdit {
        image_name: args.image_name.map(Into::into),
        use_frame_extension: args.use_frame_extension,
        frame_offset: args.frame_offset,
        frame_cache: args.frame_cache,
        fit: args.fit.map(PlateFit::from),
        depth: args.depth.map(|v| narrow("depth", v)).transpose()?,
        alpha_gain: args
            .alpha_gain
            .map(|v| narrow("alpha_gain", v))
            .transpose()?,
        display_mode: args.display_mode.map(PlateDisplayMode::from),
        coverage: args.reset_coverage.then_some(None),
        coverage_parts: parts,
    };
    if edit.is_empty() {
        return Err(ErrorData::invalid_params(
            "set_image_plane needs at least one field",
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetImagePlane(edit),
    ))
}

fn parse_remove_image_plane(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: MutationArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::RemoveImagePlane,
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_image_plane",
        description: "Create or edit the live camera's image plane (Maya imagePlane, a plate behind the scene for matchmove). The first call needs image_name, a .png, .jpg or .jpeg file (1 to 4096 UTF-8 bytes, no NUL; EXR, DPX and movies are not supported); its header is read at once, so a missing, unreadable, oversize (over 128 MiB or 8192 pixels a side) or non-image file is image_plane_error and nothing changes. Optional: use_frame_extension (substitute current time plus frame_offset into the last digit run of the file name; a frame outside the sequence has no file), frame_offset (whole frames, -1000000 to 1000000), frame_cache (frames kept, 1 to 1000), fit (fill, best, horizontal, vertical, to_size; the order is PROVISIONAL), depth (centimetres, greater than zero), alpha_gain (0 to 1000), display_mode (none, outline, rgb, rgba, luminance, alpha). Coverage: coverage_x and coverage_y (width and height in source pixels) and coverage_origin_x and coverage_origin_y name part of the image; the numbers you give replace those of the stored coverage (or of the full image when none is stored) and the result must lie inside the image; reset_coverage true restores the full image and excludes the four numbers. Every field is optional and at least one is required; only the named fields change, all or nothing: any invalid value (named in the error) leaves the plane untouched. A new image_name smaller than a stored coverage is refused with a message to reset the coverage in the same call. Setting the same image_name again is unchanged and does not reload the file; call remove_image_plane or change the name to reload. Integers must be whole numbers inside their range. Reports changed or unchanged, judged numerically. Session only, not saved with the scene, not undoable, cleared by New and Open; no reply or error carries the path. inspect_scene camera.image_plane reports the settings, image size, resolved file name and frame. PROVISIONAL: the Maya pages do not state the fit numbering, the occlusion at depth, what Outline shows, or the file-name numbering. The viewport does not draw the plate yet; once it does, the first pass of playback over a sequence is not real time and orthographic views show no plate (PROVISIONAL)",
        schema: schema::<SetImagePlaneArgs>,
        parse: parse_set_image_plane,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "remove_image_plane",
        description: "Remove the live camera's image plane. Reports unchanged when there is none. Session only; does not edit the scene. Set it again with set_image_plane (that reads the file again)",
        schema: schema::<MutationArgs>,
        parse: parse_remove_image_plane,
        idempotent: true,
        read_only: false,
    },
];
