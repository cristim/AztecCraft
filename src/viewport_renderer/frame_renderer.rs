//! The wgpu [`FrameRenderer`] of the playblast (#460): the viewport camera through the window's
//! own `Submission` path, drawn offscreen on the window's device and read back as RGBA8.
use super::offscreen::{check_size, read_back};
use super::{
    GpuResources, RenderError, SelectionFeedback, ShadingMode, Submission, ViewportRenderer, wgpu,
};
use azteccraft::{
    playblast::{FrameRenderer, PlayblastError, SceneAt},
    viewport::Camera,
};

/// The readback is in the texture's channel order, so it must be RGBA, not the window's format.
const RENDER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// The window viewport clear colour (31, 31, 31).
const CLEAR_GREY: f64 = 31.0 / 255.0;

/// Draws a [`Submission`] into its own colour and depth textures on the window's device.
pub(super) struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    resources: GpuResources,
}

impl OffscreenRenderer {
    fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let resources = GpuResources::new(&device, RENDER_FORMAT);
        Self {
            device,
            queue,
            resources,
        }
    }

    fn render(&mut self, submission: &Submission, size: [u32; 2]) -> Result<Vec<u8>, RenderError> {
        check_size(&self.device.limits(), size)?;
        let [width, height] = size;
        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = |label, format, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let color = texture(
            "playblast color",
            RENDER_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let depth = texture(
            "playblast depth",
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        self.resources
            .upload(&self.device, &self.queue, submission)?;
        let (color_view, depth_view) = (
            color.create_view(&Default::default()),
            depth.create_view(&Default::default()),
        );
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("playblast frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: CLEAR_GREY,
                            g: CLEAR_GREY,
                            b: CLEAR_GREY,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.resources.draw(&mut pass, submission)?;
        }
        self.queue.submit([encoder.finish()]);
        read_back(&self.device, &self.queue, &color, size)
    }
}

/// Renders the viewport camera at each requested frame through the same [`Submission`] code
/// as the window, on the window's device and queue (no second wgpu device).
pub(crate) struct WgpuFrameRenderer {
    viewport: ViewportRenderer,
    offscreen: OffscreenRenderer,
    scene_at: SceneAt,
    camera: Camera,
    grid_visible: bool,
    shading: ShadingMode,
}

impl ViewportRenderer {
    /// `NoAdapter` when this renderer has no device (it was not built from a window).
    pub(crate) fn frame_renderer(
        &self,
        scene_at: SceneAt,
        camera: Camera,
        grid_visible: bool,
        shading: ShadingMode,
    ) -> Result<WgpuFrameRenderer, PlayblastError> {
        let (device, queue) = self.gpu.clone().ok_or(PlayblastError::NoAdapter)?;
        Ok(WgpuFrameRenderer {
            viewport: self.clone(),
            offscreen: OffscreenRenderer::new(device, queue),
            scene_at,
            camera,
            grid_visible,
            shading,
        })
    }
}

fn playblast_error(error: RenderError) -> PlayblastError {
    match error {
        RenderError::Uninitialized => PlayblastError::NoAdapter,
        RenderError::ResourceLimit => PlayblastError::TooLarge,
        RenderError::Camera(_) => PlayblastError::Render("invalid viewport camera"),
        RenderError::NonFiniteClip => PlayblastError::Render("viewport geometry is out of range"),
        RenderError::Backend(_) => PlayblastError::Render(BACKEND_FAILED),
    }
}

const BACKEND_FAILED: &str = "viewport GPU backend failed";

/// A latched uncaptured error or device loss from the shared window device.
fn backend_state(viewport: &ViewportRenderer) -> Result<(), PlayblastError> {
    match viewport.terminal_error() {
        Some(_) => Err(PlayblastError::Render(BACKEND_FAILED)),
        None => Ok(()),
    }
}

impl FrameRenderer for WgpuFrameRenderer {
    fn render(&mut self, frame: i64, width: u32, height: u32) -> Result<Vec<u8>, PlayblastError> {
        backend_state(&self.viewport)?;
        let scene = (self.scene_at)(frame)?;
        let submission = Submission::new(
            &scene,
            &self.camera,
            [width, height],
            SelectionFeedback::Object(None),
            self.grid_visible,
            self.shading,
            RENDER_FORMAT,
            self.viewport.max_buffer_size,
        )
        .map_err(playblast_error)?;
        let pixels = self
            .offscreen
            .render(&submission, [width, height])
            .map_err(playblast_error)?;
        backend_state(&self.viewport)?;
        Ok(pixels)
    }
}

#[cfg(test)]
#[path = "frame_renderer_tests.rs"]
mod tests;
