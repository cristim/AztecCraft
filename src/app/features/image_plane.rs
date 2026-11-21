//! The image plane in the running editor (#459 P8): the frame stream behind the plane, the
//! per-tick request window, the frame state every surface reads (status text, `inspect_scene`
//! and the viewport), and the `PlateDraw` the renderer draws. Frame naming, decoding and the
//! cache are `azteccraft::image_plane`; nothing here re-implements them.
//!
//! PROVISIONAL (#5): the prefetch window (playing: the current frame and the following ones up
//! to the cache capacity; paused: the current frame and its two neighbours), holding the last
//! ready frame while the next one loads, and drawing nothing for a missing or failed frame.
//! The playing window does not wrap at the playback end: with looping on, the first frames are
//! not prefetched, so each wrap shows "loading" on the first frame until it decodes. Wrapping the
//! window is a later refinement.

use std::sync::Arc;

use azteccraft::{
    image_plane::{
        cache::FrameKey,
        decode::{PlateError, PlateFrame},
        fit::plate_quad,
        model::ImagePlane,
        sequence::frame_path,
        worker::{DecodeFn, FrameState, PlateSource, PlateStream, decode_from_disk},
    },
    viewport::Lens,
};

use crate::{app::Editor, editor_operations::Operation, viewport_renderer::PlateDraw};

/// What the editor last learned about the frame the current time resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) enum FrameStatus {
    Loading,
    Ready,
    Missing,
    /// A short stable code for the failure (`corrupt`, `truncated`, ...).
    Failed(&'static str),
}

impl FrameStatus {
    pub(in crate::app) fn name(&self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::Failed(_) => "failed",
        }
    }
}

fn error_code(error: &PlateError) -> &'static str {
    match error {
        PlateError::Io(_) => "io",
        PlateError::TooLarge => "too_large",
        PlateError::HeaderTooLarge => "header_too_large",
        PlateError::UnsupportedFormat { .. } => "unsupported_format",
        PlateError::Corrupt => "corrupt",
        PlateError::Truncated => "truncated",
        PlateError::DimensionMismatch => "dimension_mismatch",
        PlateError::Internal => "internal",
        PlateError::WorkerGone => "worker_gone",
        PlateError::Missing => "missing",
    }
}

struct Shown {
    key: FrameKey,
    frame: Arc<PlateFrame>,
}

/// The stream, created on first use (it needs the context to wake the UI), and the snapshot the
/// last tick took.
#[derive(Default)]
pub(in crate::app) struct PlateRuntime {
    stream: Option<PlateStream>,
    /// Replaces the file decoder (tests inject a blocking or counting one).
    decode: Option<DecodeFn>,
    source: Option<(PlateSource, u32)>,
    shown: Option<Shown>,
    /// The state of the frame the last tick resolved, keyed by source generation and frame.
    status: Option<(u64, i64, FrameStatus)>,
}

impl PlateRuntime {
    #[cfg(test)]
    pub(in crate::app) fn with_decode(decode: DecodeFn) -> Self {
        Self {
            decode: Some(decode),
            ..Self::default()
        }
    }

    /// The injected decoder, if any (the playblast decodes through the same function).
    pub(in crate::app) fn decoder(&self) -> Option<DecodeFn> {
        self.decode.clone()
    }

    /// Forgets the source and releases its frames (the plane was removed or replaced by New).
    fn clear(&mut self) {
        if self.source.take().is_some()
            && let Some(stream) = &mut self.stream
        {
            stream.remove_source();
        }
        self.shown = None;
        self.status = None;
    }

    /// One tick for `frame` (the key frame; 0 for a single image): follows the source, asks for
    /// the request window and snapshots the frame state. Returns whether a frame is outstanding.
    fn update(
        &mut self,
        ctx: &eframe::egui::Context,
        plane: &ImagePlane,
        frame: i64,
        playing: bool,
    ) -> bool {
        let stream = self.stream.get_or_insert_with(|| {
            let wake = ctx.clone();
            PlateStream::new(
                self.decode
                    .clone()
                    .unwrap_or_else(|| Arc::new(decode_from_disk)),
                Arc::new(move || wake.request_repaint()),
                plane.frame_cache as usize,
            )
        });
        let wanted_source = PlateSource {
            template: plane.image_name.clone(),
            sequence: plane.use_frame_extension,
        };
        if self.source.as_ref().map(|(s, c)| (s, *c)) != Some((&wanted_source, plane.frame_cache)) {
            if self.source.as_ref().map(|(s, _)| s) != Some(&wanted_source) {
                stream.set_source(wanted_source.clone());
                self.shown = None;
            }
            stream.set_frame_cache(plane.frame_cache as usize);
            self.source = Some((wanted_source, plane.frame_cache));
        }
        let window = if !plane.use_frame_extension {
            vec![0]
        } else if playing {
            (0..stream.cache().capacity() as i64)
                .map(|ahead| frame.saturating_add(ahead))
                .collect()
        } else {
            vec![frame, frame.saturating_add(1), frame.saturating_sub(1)]
        };
        let outstanding = stream.tick(&window);
        let generation = stream.generation();
        let status = match stream.state(frame) {
            FrameState::Ready(ready) => {
                self.shown = Some(Shown {
                    key: FrameKey { generation, frame },
                    frame: ready,
                });
                FrameStatus::Ready
            }
            // The previous ready frame stays on screen while this one loads.
            FrameState::Loading => FrameStatus::Loading,
            FrameState::Missing => {
                self.shown = None;
                FrameStatus::Missing
            }
            FrameState::Failed(error) => {
                self.shown = None;
                FrameStatus::Failed(error_code(&error))
            }
        };
        self.status = Some((generation, frame, status));
        if outstanding {
            ctx.request_repaint();
        }
        outstanding
    }

    /// The state of `frame` if the last tick resolved it for the current source, else `None`
    /// (nothing has been requested yet: the inspection says `not_loaded`).
    pub(in crate::app) fn status_of(&self, frame: i64) -> Option<&FrameStatus> {
        let generation = self.stream.as_ref()?.generation();
        match &self.status {
            Some((g, f, status)) if *g == generation && *f == frame && self.source.is_some() => {
                Some(status)
            }
            _ => None,
        }
    }

    /// Decoded frames held and their bytes, and the byte budget.
    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn cache_usage(&self) -> (usize, usize, usize) {
        self.stream.as_ref().map_or(
            (
                0,
                0,
                azteccraft::image_plane::cache::PLATE_CACHE_BUDGET_BYTES,
            ),
            |stream| {
                let cache = stream.cache();
                (cache.len(), cache.total_bytes(), cache.budget_bytes())
            },
        )
    }
}

/// What the editor knows about the frame the current time resolves to. The one source the
/// Attribute Editor row and `inspect_scene` read, so the UI and MCP cannot disagree.
pub(in crate::app) struct PlateFrameReport {
    /// Current time plus `frame_offset` for a sequence; none for a single image.
    pub(in crate::app) resolved_frame: Option<i64>,
    /// The file name (no directory); none when the frame has no file.
    pub(in crate::app) file_name: Option<String>,
    /// `not_loaded`, `loading`, `ready`, `missing` or `failed`.
    pub(in crate::app) state: &'static str,
    pub(in crate::app) error: Option<&'static str>,
}

impl Editor {
    pub(in crate::app) fn plate_frame_report(&self, plane: &ImagePlane) -> PlateFrameReport {
        let name = |path: &std::path::Path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        };
        let (resolved_frame, file_name) = if plane.use_frame_extension {
            let frame = self.plate_frame(plane);
            (
                Some(frame),
                frame_path(&plane.image_name, frame)
                    .ok()
                    .and_then(|path| name(&path)),
            )
        } else {
            (None, name(&plane.image_name))
        };
        let status = self.plate.status_of(self.plate_frame(plane));
        PlateFrameReport {
            state: status.map_or(
                if file_name.is_some() {
                    "not_loaded"
                } else {
                    "missing"
                },
                FrameStatus::name,
            ),
            error: match status {
                Some(FrameStatus::Failed(code)) => Some(*code),
                _ => None,
            },
            resolved_frame,
            file_name,
        }
    }
}

/// The key frame of `plane` at timeline `frame`: the frame plus the offset for a sequence, 0 for a
/// single image. The viewport and the playblast (#460) both use it.
pub(in crate::app) fn key_frame(plane: &ImagePlane, frame: i64) -> i64 {
    if plane.use_frame_extension {
        frame.saturating_add(i64::from(plane.frame_offset))
    } else {
        0
    }
}

impl Editor {
    /// The key frame of the plane at the current time: the timeline frame plus the offset for a
    /// sequence, 0 for a single image.
    pub(in crate::app) fn plate_frame(&self, plane: &ImagePlane) -> i64 {
        key_frame(plane, i64::from(self.timeline.current))
    }

    /// The per-tick update: reads `timeline.current`, asks the stream for the window and keeps
    /// the UI awake while a frame is outstanding (the return value says whether one is).
    pub(in crate::app) fn update_plate(&mut self, ctx: &eframe::egui::Context) -> bool {
        let Some(state) = &self.image_plane else {
            self.plate.clear();
            return false;
        };
        let frame = self.plate_frame(&state.plane);
        let plane = state.plane.clone();
        self.plate.update(ctx, &plane, frame, self.timeline.playing)
    }

    /// The text the status line shows for the plate; nothing while the frame is ready.
    pub(in crate::app) fn plate_status_text(&self) -> Option<String> {
        let plane = &self.image_plane.as_ref()?.plane;
        // The same report the Resolved Frame row and inspect_scene read.
        let report = self.plate_frame_report(plane);
        let label = match report.resolved_frame {
            Some(frame) => format!("Plate frame {frame}"),
            None => "Plate".to_owned(),
        };
        match (report.state, report.error) {
            ("not_loaded" | "ready", _) => None,
            ("failed", Some(code)) => Some(format!("{label}: failed: {code}")),
            (state, _) => Some(format!("{label}: {state}")),
        }
    }

    /// The plate the renderer draws this frame, or `None`: no plane, no ready frame, a lens
    /// without a film back, or geometry `plate_quad` refuses (orthographic, depth out of range).
    pub(in crate::app) fn plate_draw(&self) -> Option<PlateDraw> {
        let plane = &self.image_plane.as_ref()?.plane;
        let shown = self.plate.shown.as_ref()?;
        let Lens::Film(film) = self.camera.lens() else {
            return None;
        };
        let size = (shown.frame.width, shown.frame.height);
        let quad = plate_quad(plane, size, &film, self.aspect, &self.camera)?;
        Some(PlateDraw {
            key: shown.key,
            frame: shown.frame.clone(),
            quad,
            mode: plane.display_mode,
            alpha_gain: plane.alpha_gain,
        })
    }
}

/// Why the View > Image Plane > Remove row is disabled, when it is.
pub(in crate::app) const NO_PLANE_REASON: &str = "There is no image plane to remove";
/// The control and menu names the Import hint points at; the UI builds its widgets from these.
pub(in crate::app) const MENU_PATH: &str = "View > Image Plane";
pub(in crate::app) const ATTRIBUTES_ROW_LABEL: &str = "Image Plane Attributes";
pub(in crate::app) const RESET_COVERAGE_LABEL: &str = "Reset Coverage";

/// Why the View > Image Plane > Image Plane Attributes row is disabled without a plane.
pub(in crate::app) const NO_PLANE_FOR_TAB_REASON: &str =
    "There is no image plane: use Import Image first";

impl Editor {
    /// View > Image Plane > Import Image: asks for a PNG or JPEG and sets it as the plane's
    /// image (creating the plane if there is none). The dialog is the only untested part; the
    /// path then goes through [`Self::run_import_image`].
    pub(in crate::app) fn import_image_dialog(&mut self) {
        if self.files.is_modal() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Import Image")
            .add_filter("Image", &azteccraft::image_plane::ops::ALLOWED_EXTENSIONS)
            .pick_file()
        {
            self.run_import_image(path);
        }
    }

    /// The same typed operation the MCP `set_image_plane` dispatches, so the human guards, the
    /// pending-draft commit and the refusals are shared; a refusal shows as the status error.
    pub(in crate::app) fn run_import_image(&mut self, path: std::path::PathBuf) {
        use azteccraft::image_plane::{model::ImagePlaneError, ops::ImagePlaneOpError};
        let result = self.native_operation_result(Operation::SetImagePlane(
            azteccraft::image_plane::model::ImagePlaneEdit {
                image_name: Some(path),
                ..Default::default()
            },
        ));
        // The stored coverage no longer fits the new image: say what the UI can do about it.
        if let Err(crate::editor_operations::OperationError::ImagePlane(
            ImagePlaneOpError::Model(ImagePlaneError::CoverageOutsideImage { .. }),
        )) = result
        {
            self.status.push_str(&format!(
                " Use {RESET_COVERAGE_LABEL} in {MENU_PATH} > {ATTRIBUTES_ROW_LABEL}, then import again."
            ));
        }
    }
}
