use azteccraft::{
    native_scene::{FaceTarget, NativeId},
    scene::NodeId,
    view_scene::{FACE_CENTER_MARKER_PIXELS, ViewFace, ViewInstance, ViewScene},
    viewport::{Camera, InvalidCameraInput},
};
use eframe::{
    egui::{self, Color32, Rect},
    egui_wgpu::{self, CallbackResources, CallbackTrait},
    wgpu,
};
use glam::{Mat4, Vec3, Vec4};
use std::{fmt, ops::Range, sync::Arc};

const VERTEX_BYTES: usize = 8 * size_of::<f32>();
// Charge snapshot, upload staging, retained vertices and a growth replacement per upload.
const MAX_UPLOAD_WORKING_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum SelectionFeedback<'a> {
    Object(Option<&'a [NativeId]>),
    Face {
        active: Option<(NodeId, &'a [NativeId])>,
        selected: Option<&'a FaceTarget>,
    },
}

impl SelectionFeedback<'_> {
    fn selected(self, instance: &ViewInstance, face: &ViewFace) -> bool {
        match self {
            Self::Object(path) => path.is_some_and(|path| instance.path().starts_with(path)),
            Self::Face { selected, .. } => selected.is_some_and(|target| {
                target.transform == instance.transform()
                    && target.shape == instance.shape()
                    && target.path == instance.path()
                    && target.face_index == face.source_index()
            }),
        }
    }

    fn centers(self, instance: &ViewInstance) -> bool {
        matches!(self, Self::Face { active: Some((owner, path)), .. }
            if instance.is_direct_shape_of(owner, path))
    }
}

fn check_buffer_bytes(bytes: u64, device_limit: u64) -> Result<(), RenderError> {
    if bytes > device_limit
        || bytes
            .checked_mul(4)
            .is_none_or(|charged| charged > MAX_UPLOAD_WORKING_BYTES)
    {
        return Err(RenderError::ResourceLimit);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenderError {
    Uninitialized,
    Camera(InvalidCameraInput),
    ResourceLimit,
    NonFiniteClip,
    Backend(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninitialized => f.write_str("viewport GPU is uninitialized"),
            Self::Camera(error) => error.fmt(f),
            Self::ResourceLimit => {
                f.write_str("viewport submission exceeds allocation or device limits")
            }
            Self::NonFiniteClip => {
                f.write_str("viewport clip coordinates exceed the finite render range")
            }
            Self::Backend(_) => f.write_str("viewport GPU unavailable: backend_failed"),
        }
    }
}

impl std::error::Error for RenderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Camera(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewportError {
    Uninitialized,
    BackendFailed,
    CameraInput,
    NonFiniteClip,
    ResourceLimit,
}

impl RenderError {
    pub fn category(&self) -> ViewportError {
        match self {
            Self::Uninitialized => ViewportError::Uninitialized,
            Self::Backend(_) => ViewportError::BackendFailed,
            Self::Camera(_) => ViewportError::CameraInput,
            Self::NonFiniteClip => ViewportError::NonFiniteClip,
            Self::ResourceLimit => ViewportError::ResourceLimit,
        }
    }
}

#[derive(Clone)]
pub struct ViewportRenderer {
    errors: BackendErrors,
    format: wgpu::TextureFormat,
    max_buffer_size: u64,
    #[cfg(test)]
    submission: Arc<egui::epaint::mutex::Mutex<Option<Submission>>>,
}

#[derive(Clone)]
struct BackendErrors {
    first: Arc<egui::epaint::mutex::Mutex<Option<String>>>,
    context: egui::Context,
}

impl BackendErrors {
    fn install(&self, device: &wgpu::Device) {
        let uncaptured = self.clone();
        device.on_uncaptured_error(Arc::new(move |error| {
            uncaptured.record(format!("{error}"));
        }));
        let lost = self.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.record(format!("device lost ({reason:?}): {message}"));
        });
    }

    fn record(&self, message: String) {
        let mut first = self.first.lock();
        if first.is_none() {
            *first = Some(message.clone());
            drop(first);
            eprintln!("viewport GPU: {message}");
            self.context.request_repaint();
        }
    }

    fn failed(&self) -> bool {
        self.first.lock().is_some()
    }
}

impl ViewportRenderer {
    pub fn initialize(context: &eframe::CreationContext<'_>) -> Result<Self, RenderError> {
        let state = context.wgpu_render_state.as_ref().ok_or_else(|| {
            RenderError::Backend("native startup did not provide the wgpu render state".into())
        })?;
        let errors = BackendErrors {
            first: Arc::default(),
            context: context.egui_ctx.clone(),
        };
        errors.install(&state.device);
        let resources = GpuResources::new(&state.device, state.target_format);
        state.renderer.write().callback_resources.insert(resources);
        let renderer = Self {
            errors,
            format: state.target_format,
            max_buffer_size: state.device.limits().max_buffer_size,
            #[cfg(test)]
            submission: Arc::default(),
        };
        if let Some(error) = renderer.terminal_error() {
            return Err(RenderError::Backend(error));
        }
        Ok(renderer)
    }

    pub fn terminal_error(&self) -> Option<String> {
        self.errors.first.lock().clone()
    }

    pub fn callback(
        &self,
        scene: &ViewScene,
        camera: &Camera,
        rect: Rect,
        physical_size: [u32; 2],
        selection: SelectionFeedback<'_>,
    ) -> Result<egui::epaint::PaintCallback, RenderError> {
        if let Some(error) = self.terminal_error() {
            return Err(RenderError::Backend(error));
        }
        if !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return Err(RenderError::Camera(InvalidCameraInput));
        }
        let submission = Submission::new(
            scene,
            camera,
            physical_size,
            selection,
            self.format,
            self.max_buffer_size,
        )?;
        #[cfg(test)]
        {
            *self.submission.lock() = Some(submission.clone());
        }
        Ok(egui_wgpu::Callback::new_paint_callback(
            rect,
            ViewportCallback {
                submission,
                errors: self.errors.clone(),
            },
        ))
    }
}

#[cfg_attr(test, derive(Clone))]
struct Submission {
    bytes: Vec<u8>,
    surfaces: Range<u32>,
    lines: Range<u32>,
    centers: Range<u32>,
}

impl Submission {
    fn new(
        scene: &ViewScene,
        camera: &Camera,
        physical_size: [u32; 2],
        selection: SelectionFeedback<'_>,
        format: wgpu::TextureFormat,
        max_buffer_size: u64,
    ) -> Result<Self, RenderError> {
        if physical_size.contains(&0) {
            return Err(RenderError::Camera(InvalidCameraInput));
        }
        let matrix = camera
            .projection_matrix(physical_size[0] as f32 / physical_size[1] as f32)
            .map_err(RenderError::Camera)?
            * camera.view_matrix();
        let mut vertices = 84_usize;
        for instance in scene.instances() {
            for face in instance.faces() {
                let surface_count = face.triangles().count() * 3;
                vertices = vertices
                    .checked_add(surface_count)
                    .and_then(|count| count.checked_add(face.corners().len() * 2))
                    .and_then(|count| {
                        count.checked_add(if selection.centers(instance) { 6 } else { 0 })
                    })
                    .ok_or(RenderError::ResourceLimit)?;
            }
        }
        let byte_count = vertices
            .checked_mul(VERTEX_BYTES)
            .ok_or(RenderError::ResourceLimit)?;
        if u32::try_from(vertices).is_err() {
            return Err(RenderError::ResourceLimit);
        }
        check_buffer_bytes(
            u64::try_from(byte_count).map_err(|_| RenderError::ResourceLimit)?,
            max_buffer_size,
        )?;
        let mut submission = Self {
            bytes: Vec::new(),
            surfaces: 0..0,
            lines: 0..0,
            centers: 0..0,
        };
        submission
            .bytes
            .try_reserve_exact(byte_count)
            .map_err(|_| RenderError::ResourceLimit)?;
        check_buffer_bytes(
            u64::try_from(submission.bytes.capacity()).map_err(|_| RenderError::ResourceLimit)?,
            u64::MAX,
        )?;
        for instance in scene.instances().iter().rev() {
            for face in instance.faces().iter().rev() {
                let fill = if selection.selected(instance, face) {
                    Color32::from_rgb(95, 110, 120)
                } else {
                    Color32::from_gray(83)
                };
                for triangle in face.triangles() {
                    for index in triangle {
                        submission.vertex(
                            matrix,
                            instance.world_vertices()[index],
                            fill,
                            format,
                        )?;
                    }
                }
            }
        }
        let surface_end = submission.vertex_count();
        for index in -10..=10 {
            let coordinate = index as f32;
            let color = Color32::from_gray(if index == 0 { 85 } else { 48 });
            for point in [
                Vec3::new(coordinate, 0.0, -10.0),
                Vec3::new(coordinate, 0.0, 10.0),
                Vec3::new(-10.0, 0.0, coordinate),
                Vec3::new(10.0, 0.0, coordinate),
            ] {
                submission.vertex(matrix, point, color, format)?;
            }
        }
        for selected_pass in [false, true] {
            for instance in scene.instances() {
                for face in instance.faces() {
                    let selected = selection.selected(instance, face);
                    if selected != selected_pass {
                        continue;
                    }
                    let color = if selected {
                        Color32::from_rgb(242, 163, 70)
                    } else {
                        Color32::from_gray(145)
                    };
                    let corners = face.corners();
                    for index in 0..corners.len() {
                        for vertex in [corners[index], corners[(index + 1) % corners.len()]] {
                            submission.vertex(
                                matrix,
                                instance.world_vertices()[vertex],
                                color,
                                format,
                            )?;
                        }
                    }
                }
            }
        }
        submission.surfaces = 0..surface_end;
        submission.lines = surface_end..submission.vertex_count();
        let center_start = submission.vertex_count();
        for instance in scene
            .instances()
            .iter()
            .filter(|instance| selection.centers(instance))
        {
            for face in instance.faces() {
                let center = face
                    .center(instance.world_vertices())
                    .map_err(|_| RenderError::NonFiniteClip)?;
                let clip = matrix * center.extend(1.0);
                if !clip.is_finite() {
                    return Err(RenderError::NonFiniteClip);
                }
                if clip.w <= 0.0
                    || clip.z < 0.0
                    || clip.z > clip.w
                    || clip.x.abs() > clip.w
                    || clip.y.abs() > clip.w
                {
                    continue;
                }
                let dx = FACE_CENTER_MARKER_PIXELS / physical_size[0] as f32 * clip.w;
                let dy = FACE_CENTER_MARKER_PIXELS / physical_size[1] as f32 * clip.w;
                for [x, y] in [
                    [-dx, -dy],
                    [dx, -dy],
                    [dx, dy],
                    [-dx, -dy],
                    [dx, dy],
                    [-dx, dy],
                ] {
                    submission.clip_vertex(
                        clip + Vec4::new(x, y, 0.0, 0.0),
                        Color32::from_rgb(242, 163, 70),
                        format,
                    )?;
                }
            }
        }
        submission.centers = center_start..submission.vertex_count();
        Ok(submission)
    }

    fn vertex_count(&self) -> u32 {
        (self.bytes.len() / VERTEX_BYTES) as u32
    }

    fn vertex(
        &mut self,
        matrix: Mat4,
        point: Vec3,
        color: Color32,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        self.clip_vertex(matrix * point.extend(1.0), color, format)
    }

    fn clip_vertex(
        &mut self,
        clip: Vec4,
        color: Color32,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        if !clip.is_finite() {
            return Err(RenderError::NonFiniteClip);
        }
        let color = if format.is_srgb() {
            egui::Rgba::from(color).to_array()
        } else {
            color.to_normalized_gamma_f32()
        };
        for value in clip.to_array().into_iter().chain(color) {
            self.bytes.extend_from_slice(&value.to_ne_bytes());
        }
        Ok(())
    }
}

struct GpuResources {
    surfaces: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    centers: wgpu::RenderPipeline,
    vertices: Option<wgpu::Buffer>,
}

impl GpuResources {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport flat geometry"),
            source: wgpu::ShaderSource::Wgsl(include_str!("viewport_shader.wgsl").into()),
        });
        let pipeline = |topology, depth_write_enabled| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("viewport geometry"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: VERTEX_BYTES as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(depth_write_enabled),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Self {
            surfaces: pipeline(wgpu::PrimitiveTopology::TriangleList, true),
            lines: pipeline(wgpu::PrimitiveTopology::LineList, false),
            centers: pipeline(wgpu::PrimitiveTopology::TriangleList, false),
            vertices: None,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submission: &Submission,
    ) -> Result<(), RenderError> {
        let payload =
            u64::try_from(submission.bytes.len()).map_err(|_| RenderError::ResourceLimit)?;
        let device_limit = device.limits().max_buffer_size;
        check_buffer_bytes(payload, device_limit)?;
        check_buffer_bytes(
            u64::try_from(submission.bytes.capacity()).map_err(|_| RenderError::ResourceLimit)?,
            u64::MAX,
        )?;
        if let Some(buffer) = &self.vertices {
            check_buffer_bytes(buffer.size(), device_limit)?;
        }
        if self
            .vertices
            .as_ref()
            .is_none_or(|buffer| buffer.size() < submission.bytes.len() as u64)
        {
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("viewport vertices"),
                size: payload,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            check_buffer_bytes(buffer.size(), device_limit)?;
            self.vertices = Some(buffer);
        }
        if let Some(buffer) = &self.vertices {
            queue.write_buffer(buffer, 0, &submission.bytes);
        }
        Ok(())
    }

    fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        submission: &Submission,
    ) -> Result<(), RenderError> {
        let vertices = self.vertices.as_ref().ok_or_else(|| {
            RenderError::Backend("viewport vertices have not been uploaded".into())
        })?;
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_pipeline(&self.surfaces);
        pass.draw(submission.surfaces.clone(), 0..1);
        pass.set_pipeline(&self.lines);
        pass.draw(submission.lines.clone(), 0..1);
        pass.set_pipeline(&self.centers);
        pass.draw(submission.centers.clone(), 0..1);
        Ok(())
    }
}

struct ViewportCallback {
    submission: Submission,
    errors: BackendErrors,
}

impl CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if !self.errors.failed() {
            if let Some(renderer) = resources.get_mut::<GpuResources>() {
                if let Err(error) = renderer.upload(device, queue, &self.submission) {
                    self.errors.record(error.to_string());
                }
            } else {
                self.errors
                    .record("viewport GPU resources were not initialized".into());
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        if !self.errors.failed() {
            if let Some(renderer) = resources.get::<GpuResources>() {
                if let Err(error) = renderer.draw(pass, &self.submission) {
                    self.errors.record(error.to_string());
                }
            } else {
                self.errors
                    .record("viewport GPU resources are unavailable during paint".into());
            }
        }
    }
}

#[cfg(test)]
#[path = "viewport_renderer/tests.rs"]
pub(crate) mod tests;
