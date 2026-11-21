//! `Operation::Playblast` and `CancelPlayblast` (#460): a job renders a clone of the scene, one
//! frame per tick, and the authoritative scene, selection and history are never written. The
//! clone is advanced frame by frame (strictly ascending), and every keyed channel is rewritten
//! at each frame, so no value carries over from an earlier one.

use super::playblast_plate::plate_at;
use crate::app::Editor;
use crate::editor_operations::{
    OperationError, OperationReply, PlayblastFormat, PlayblastInspection, PlayblastRequest,
    ShadingMode,
};
use crate::viewport_renderer::PlateAt;
use azteccraft::image_plane::worker::decode_from_disk;
use azteccraft::playblast::{
    FrameRenderer, JobProgress, PlayblastError, PlayblastJob, PlayblastPlan, SceneAt,
};
use azteccraft::view_scene::{VIEW_BUDGET, ViewScene};
use azteccraft::viewport::Camera;
use eframe::egui;
use std::sync::Arc;

/// Builds the renderer of one job from its scene source, the plate source (`None` without an
/// image plane), the viewport camera, the grid flag and the shading mode. Tests install a fake here; with `None` the window's renderer is used.
pub(in crate::app) type RendererFactory = Box<
    dyn FnMut(
        SceneAt,
        Option<PlateAt>,
        &Camera,
        bool,
        ShadingMode,
    ) -> Result<Box<dyn FrameRenderer>, PlayblastError>,
>;

/// What the inspection reports about the latest job besides its progress.
struct Target {
    directory_name: String,
    width: u32,
    height: u32,
    total: u32,
    done: u32,
}

pub(in crate::app) struct Run {
    pub(in crate::app) job: PlayblastJob,
    pub(in crate::app) renderer: Box<dyn FrameRenderer>,
}

#[derive(Default)]
pub(in crate::app) struct Playblast {
    pub(in crate::app) run: Option<Run>,
    /// The latest progress, kept after the job ends; cleared when a document replaces the scene.
    pub(in crate::app) last: Option<JobProgress>,
    target: Option<Target>,
    pub(in crate::app) factory: Option<RendererFactory>,
}

/// Renders `scene` at each requested frame; an unevaluable frame stops the job.
fn scene_at(mut scene: azteccraft::native_scene::NativeScene) -> SceneAt {
    Box::new(move |frame| {
        let mut evaluate = || {
            let at = i32::try_from(frame).ok()?;
            let writes = scene.evaluated_writes(at).ok()?;
            scene.apply_evaluated(writes);
            ViewScene::from_native(&scene, VIEW_BUDGET).ok()
        };
        evaluate().ok_or(PlayblastError::Evaluate { frame })
    })
}

impl Editor {
    fn playblast_renderer(
        &mut self,
        scene: SceneAt,
        grid: bool,
    ) -> Result<Box<dyn FrameRenderer>, PlayblastError> {
        let (camera, shading) = (self.camera.clone(), self.shading);
        let plate = self.image_plane.as_ref().map(|state| {
            let decode = self
                .plate
                .decoder()
                .unwrap_or_else(|| Arc::new(decode_from_disk));
            plate_at(state.plane.clone(), camera.clone(), decode)
        });
        if let Some(factory) = &mut self.playblast.factory {
            return factory(scene, plate, &camera, grid, shading);
        }
        if let Some(renderer) = &self.renderer {
            return renderer
                .frame_renderer(scene, plate, camera, grid, shading)
                .map(|renderer| Box::new(renderer) as Box<dyn FrameRenderer>);
        }
        Err(PlayblastError::NoAdapter)
    }

    pub(in crate::app) fn start_playblast(
        &mut self,
        request: PlayblastRequest,
    ) -> Result<OperationReply, OperationError> {
        let refuse = OperationError::Playblast;
        if self.playblast.run.is_some() {
            return Err(refuse(PlayblastError::Busy));
        }
        if !request.offscreen {
            return Err(refuse(PlayblastError::Unsupported(
                "on-screen capture is not implemented",
            )));
        }
        let PlayblastFormat::Png = request.format;
        let (Some(width), Some(height)) = (request.width, request.height) else {
            return Err(refuse(PlayblastError::InvalidSize));
        };
        self.history
            .scene()
            .check_time_unit()
            .map_err(OperationError::Scene)?;
        let start = request.start.unwrap_or(f64::from(self.timeline.start));
        let end = request.end.unwrap_or(f64::from(self.timeline.end));
        let plan = PlayblastPlan::new(
            &request.directory,
            &request.name,
            start,
            end,
            width,
            height,
            request.percent,
        )
        .map_err(refuse)?;
        let (frames, (width, height)) = (plan.frame_count(), plan.size());
        let source = scene_at(self.history.scene().clone());
        let renderer = self
            .playblast_renderer(source, self.grid_visible && request.show_ornaments)
            .map_err(refuse)?;
        let job = PlayblastJob::start(plan).map_err(refuse)?;
        self.playblast.last = Some(JobProgress::Running {
            done: 0,
            total: frames,
        });
        self.playblast.target = Some(Target {
            directory_name: request
                .directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            width,
            height,
            total: frames,
            done: 0,
        });
        self.playblast.run = Some(Run { job, renderer });
        Ok(OperationReply::PlayblastStarted {
            frames,
            width,
            height,
        })
    }

    /// Stops the job after the file in flight and joins its writer thread.
    pub(in crate::app) fn cancel_playblast(&mut self) -> OperationReply {
        match self.playblast.run.take() {
            Some(mut run) => {
                let progress = run.job.cancel();
                self.record_playblast(progress);
                OperationReply::Changed
            }
            None => OperationReply::Unchanged,
        }
    }

    /// A new or opened document ends the job and forgets its report.
    pub(in crate::app) fn reset_playblast(&mut self) {
        self.cancel_playblast();
        self.playblast.last = None;
        self.playblast.target = None;
    }

    fn record_playblast(&mut self, progress: JobProgress) {
        if let Some(target) = &mut self.playblast.target {
            match &progress {
                JobProgress::Running { done, .. } | JobProgress::Cancelled { done } => {
                    target.done = *done;
                }
                JobProgress::Finished { total } => target.done = *total,
                JobProgress::Failed(_) => {}
            }
        }
        self.playblast.last = Some(progress);
    }

    pub(in crate::app) fn inspect_playblast(&self) -> PlayblastInspection {
        let (state, error) = match &self.playblast.last {
            None => ("idle", None),
            Some(JobProgress::Running { .. }) => ("running", None),
            Some(JobProgress::Finished { .. }) => ("finished", None),
            Some(JobProgress::Cancelled { .. }) => ("cancelled", None),
            Some(JobProgress::Failed(error)) => ("failed", Some(error.to_string())),
        };
        let target = self.playblast.target.as_ref();
        PlayblastInspection {
            state,
            frames_done: target.map_or(0, |target| target.done),
            frames_total: target.map_or(0, |target| target.total),
            width: target.map_or(0, |target| target.width),
            height: target.map_or(0, |target| target.height),
            directory_name: target.map(|target| target.directory_name.clone()),
            last_error: error,
        }
    }

    /// Renders one frame of the running job. The job owns its scene snapshot, so a modal, a
    /// pending close or a draft only stall it; it resumes afterwards.
    pub(in crate::app) fn tick_playblast(&mut self, ctx: &egui::Context) {
        if self.files.is_modal() || self.files.is_closing() || self.has_draft() {
            return;
        }
        let Some(run) = &mut self.playblast.run else {
            return;
        };
        let progress = run.job.step(run.renderer.as_mut());
        if matches!(progress, JobProgress::Running { .. }) {
            ctx.request_repaint();
        } else {
            self.playblast.run = None;
        }
        self.record_playblast(progress);
    }
}

/// A renderer that draws no GPU pixels: each frame is `[frame, x, y, 255]` per pixel, and the
/// world vertices of the first instance it was handed are recorded, so tests see what the job
/// evaluated at each frame.
#[cfg(test)]
pub(in crate::app) mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub(in crate::app) type Seen = Arc<Mutex<Vec<(i64, Vec<glam::Vec3>)>>>;

    /// What the job asked the renderer for: the grid flag of each renderer built and the
    /// `(width, height)` of each frame drawn.
    #[derive(Default)]
    pub(in crate::app) struct Asked {
        pub(in crate::app) grid: Vec<bool>,
        pub(in crate::app) sizes: Vec<(u32, u32)>,
        /// Per rendered frame: the key frame and first texel of the plate it resolved (`None`:
        /// no plate drawn). Empty without a plate source.
        pub(in crate::app) plates: Vec<PlateSeen>,
        /// Whether each renderer was built with a plate source.
        pub(in crate::app) had_plate_source: Vec<bool>,
    }

    /// A rendered frame and the `(key frame, first texel)` of the plate drawn for it, if any.
    pub(in crate::app) type PlateSeen = (i64, Option<(i64, [u8; 4])>);

    pub(in crate::app) type AskedLog = Arc<Mutex<Asked>>;

    struct FakeRenderer {
        scene: SceneAt,
        plate: Option<PlateAt>,
        seen: Seen,
        asked: Option<AskedLog>,
    }

    impl FrameRenderer for FakeRenderer {
        fn render(
            &mut self,
            frame: i64,
            width: u32,
            height: u32,
        ) -> Result<Vec<u8>, PlayblastError> {
            if let Some(asked) = &self.asked {
                asked.lock().unwrap().sizes.push((width, height));
            }
            let view = (self.scene)(frame)?;
            if let (Some(plate), Some(asked)) = (&mut self.plate, &self.asked) {
                let drawn = plate(frame, [width, height])?.map(|draw| {
                    let texel = draw.frame.rgba[..4].try_into().unwrap_or_default();
                    (draw.key.frame, texel)
                });
                asked.lock().unwrap().plates.push((frame, drawn));
            }
            let vertices = view
                .instances()
                .first()
                .map(|instance| instance.world_vertices().to_vec())
                .unwrap_or_default();
            self.seen.lock().unwrap().push((frame, vertices));
            Ok((0..height)
                .flat_map(|y| (0..width).flat_map(move |x| [frame as u8, x as u8, y as u8, 255]))
                .collect())
        }
    }

    pub(in crate::app) fn factory(seen: Seen) -> RendererFactory {
        build(seen, None)
    }

    /// Like `factory`, and records the grid flag and frame sizes it is asked for.
    pub(in crate::app) fn recording_factory(seen: Seen, asked: AskedLog) -> RendererFactory {
        build(seen, Some(asked))
    }

    fn build(seen: Seen, asked: Option<AskedLog>) -> RendererFactory {
        Box::new(move |scene, plate, _, grid, _| {
            if let Some(asked) = &asked {
                let mut asked = asked.lock().unwrap();
                asked.grid.push(grid);
                asked.had_plate_source.push(plate.is_some());
            }
            Ok(Box::new(FakeRenderer {
                scene,
                plate,
                seen: Arc::clone(&seen),
                asked: asked.clone(),
            }))
        })
    }
}

#[cfg(test)]
#[path = "playblast_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "playblast_gpu_tests.rs"]
mod gpu_tests;
