//! The camera image plane in a playblast (#460): which plate frame each playblast frame shows
//! and the `PlateDraw` it becomes. Quad geometry, display modes and the draw order are the
//! viewport's (`image_plane::fit::plate_quad`, `viewport_renderer::plate`); nothing is
//! re-implemented.
//!
//! PROVISIONAL decisions (no Maya comparison exists; no Maya page is cited for them):
//! - The plate is part of the scene, not an ornament: `show_ornaments` does not affect it.
//! - The playblast frame is the timeline frame, so a sequence shows `frame + frame_offset`, a
//!   single image shows itself, exactly like the viewport at that time.
//! - The plate is placed by `plate_quad` with the aspect of the requested output size, as the
//!   viewport does with its own: the film gate is fitted into the output, and the plate stays
//!   inside the gate (crop, coverage and fit rules unchanged). The area outside the gate keeps
//!   the clear colour, as in the viewport.
//! - A frame with no file (or a negative key frame) renders without a plate, like the viewport,
//!   and the job continues. Any other failure (unreadable, corrupt, too large) stops the job
//!   with `PlayblastError::Plate`, because a deliverable must not silently lose its footage.
//! - Frames are decoded synchronously on the UI tick, one per render, so none is skipped; the
//!   previous key frame is reused (a single image decodes once).

use std::sync::Arc;

use azteccraft::{
    image_plane::{
        cache::FrameKey,
        decode::{PlateError, PlateFrame},
        fit::plate_quad,
        model::{ImagePlane, PlateDisplayMode},
        worker::{DecodeFn, PlateSource},
    },
    playblast::PlayblastError,
    viewport::{Camera, Lens},
};

use super::image_plane::key_frame;
use crate::viewport_renderer::{PlateAt, PlateDraw};

/// A resolver for `plane` seen through `camera`, both snapshots taken at the start of the job.
pub(in crate::app) fn plate_at(plane: ImagePlane, camera: Camera, decode: DecodeFn) -> PlateAt {
    let source = PlateSource {
        template: plane.image_name.clone(),
        sequence: plane.use_frame_extension,
    };
    let mut held: Option<(i64, Arc<PlateFrame>)> = None;
    Box::new(move |frame, [width, height]| {
        let Lens::Film(film) = camera.lens() else {
            return Ok(None);
        };
        if plane.display_mode == PlateDisplayMode::None {
            return Ok(None);
        }
        let key = key_frame(&plane, frame);
        if held.as_ref().is_none_or(|(held_key, _)| *held_key != key) {
            held = None;
            match decode(&source, key) {
                Ok(decoded) => held = Some((key, Arc::new(decoded))),
                Err(PlateError::Missing) => return Ok(None),
                Err(_) => return Err(PlayblastError::Plate { frame }),
            }
        }
        let Some((_, shown)) = &held else {
            return Ok(None);
        };
        let size = (shown.width, shown.height);
        let aspect = width as f32 / height as f32;
        let Some(quad) = plate_quad(&plane, size, &film, aspect, &camera) else {
            return Ok(None);
        };
        Ok(Some(PlateDraw {
            key: FrameKey {
                generation: 0,
                frame: key,
            },
            frame: Arc::clone(shown),
            quad,
            mode: plane.display_mode,
            alpha_gain: plane.alpha_gain,
        }))
    })
}
