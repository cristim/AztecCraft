use crate::editor_operations::ShadingMode;
use azteccraft::{
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
    scene::NodeId,
    shading::shade,
    view_scene::{
        FACE_CENTER_MARKER_PIXELS, SELECTED_EDGE_PIXELS, VERTEX_MARKER_PIXELS, ViewFace,
        ViewInstance, ViewJoint, ViewScene,
    },
    viewport::{Camera, InvalidCameraInput},
};
use eframe::{
    egui::{self, Color32, Rect},
    egui_wgpu::{self, CallbackResources, CallbackTrait},
    wgpu,
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::{fmt, ops::Range, sync::Arc};

/// Selection tint, averaged with a face's base colour. On the grey base fill (83) the result is
/// the historical selected fill (95, 110, 120).
const SELECTION_TINT_RGB: [u8; 3] = [107, 137, 157];
/// Tint and edge colour of the lead (key) object while several objects are selected. Maya draws
/// the last selected object in a different colour but its values are not documented, so these
/// are our own (PROVISIONAL, development.md row 33).
const LEAD_TINT_RGB: [u8; 3] = [117, 167, 117];
const SELECTED_EDGE_RGB_OBJECT: [u8; 3] = [242, 163, 70];
const LEAD_EDGE_RGB: [u8; 3] = [140, 240, 140];

/// Smallest per-channel gap between a selected and an unselected base on at least one channel.
/// `shade()` scales by at least `AMBIENT_FLOOR` (0.4) and rounds, so a gap of 3 (3 * 0.4 > 1)
/// keeps the two colours different on screen at the floor, obliquely and front-on.
const MIN_SELECTION_DELTA: u8 = 3;

/// A base whose selected colour differs from it by at least `MIN_SELECTION_DELTA` on some
/// channel, so the pixels differ after lighting. The tint average comes within that gap only for
/// bases near the tint; those use the average with their complement (about 127 per channel),
/// which is far from every such base.
fn selected_base(base: [u8; 3]) -> [u8; 3] {
    tinted_base(base, SELECTION_TINT_RGB)
}

fn lead_base(base: [u8; 3]) -> [u8; 3] {
    tinted_base(base, LEAD_TINT_RGB)
}

fn tinted_base(base: [u8; 3], tint: [u8; 3]) -> [u8; 3] {
    let blend = |other: [u8; 3]| -> [u8; 3] {
        std::array::from_fn(|channel| {
            ((u16::from(base[channel]) + u16::from(other[channel])) / 2) as u8
        })
    };
    let tinted = blend(tint);
    let gap = |color: [u8; 3]| {
        (0..3)
            .map(|c| color[c].abs_diff(base[c]))
            .max()
            .unwrap_or(0)
    };
    if gap(tinted) < MIN_SELECTION_DELTA {
        blend(base.map(|channel| 255 - channel))
    } else {
        tinted
    }
}

/// Segments in each of the three wire circles standing in for Maya's joint drawing (PROVISIONAL).
const JOINT_CIRCLE_SEGMENTS: usize = 16;
const JOINT_VERTICES: usize = 3 * JOINT_CIRCLE_SEGMENTS * 2;

/// The three world-aligned circles of one joint.
fn joint_circle_segments(joint: &ViewJoint) -> impl Iterator<Item = [Vec3; 2]> + '_ {
    let point = move |plane: usize, step: usize| {
        let (sin, cos) =
            (step as f32 * std::f32::consts::TAU / JOINT_CIRCLE_SEGMENTS as f32).sin_cos();
        let radial = match plane {
            0 => Vec3::new(cos, sin, 0.0),
            1 => Vec3::new(cos, 0.0, sin),
            _ => Vec3::new(0.0, cos, sin),
        };
        joint.center() + radial * joint.display_radius()
    };
    (0..3).flat_map(move |plane| {
        (0..JOINT_CIRCLE_SEGMENTS).map(move |step| [point(plane, step), point(plane, step + 1)])
    })
}

fn highlight_edge_color(highlight: Highlight) -> Color32 {
    let [red, green, blue] = match highlight {
        Highlight::None => return Color32::from_gray(145),
        Highlight::Member => SELECTED_EDGE_RGB_OBJECT,
        Highlight::Lead => LEAD_EDGE_RGB,
    };
    Color32::from_rgb(red, green, blue)
}

// Provisional original colours that follow the Maya 2026 display-colour words only (purple-ish
// vertex, highlighted when selected, light-blue edge); see `known-issues.md`.
const VERTEX_RGB: [u8; 3] = [150, 110, 200];
const SELECTED_VERTEX_RGB: [u8; 3] = [255, 232, 64];
const EDGE_RGB: [u8; 3] = [120, 180, 235];
const SELECTED_EDGE_RGB: [u8; 3] = [255, 178, 96];

// Maya 2025.1's default grid (`grid -q`): size 12, major lines every 5 units and 5 divisions
// between them, so a line every unit out to +/-12 on the y = 0 plane (#160). Reading size as
// the half-length, and these greys, are PROVISIONAL; Maya also draws the axes bold, which
// one-pixel lines cannot.
const GRID_SIZE: i32 = 12;
const GRID_MAJOR_EVERY: i32 = 5;
const GRID_AXIS_GRAY: u8 = 85;
const GRID_MAJOR_GRAY: u8 = 64;
const GRID_DIVISION_GRAY: u8 = 48;
/// Two lines, each two vertices, per grid coordinate.
pub(crate) const GRID_VERTICES: usize = (2 * GRID_SIZE as usize + 1) * 4;

/// The grey of the grid lines `index` units from the origin.
fn grid_gray(index: i32) -> u8 {
    if index == 0 {
        GRID_AXIS_GRAY
    } else if index % GRID_MAJOR_EVERY == 0 {
        GRID_MAJOR_GRAY
    } else {
        GRID_DIVISION_GRAY
    }
}

const VERTEX_BYTES: usize = 8 * size_of::<f32>();
// Charge snapshot, upload staging, retained vertices and a growth replacement per upload.
const MAX_UPLOAD_WORKING_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy)]
pub enum SelectionFeedback<'a> {
    /// No selection or one selected object, drawn in the member colours.
    Object(Option<&'a [NativeId]>),
    /// Several selected objects in selection order; the last is the lead and has its own colours.
    Multi(&'a [azteccraft::native_scene::TransformTarget]),
    Face {
        active: Option<(NodeId, &'a [NativeId])>,
        selected: Option<&'a FaceTarget>,
    },
    Vertex {
        active: Option<(NodeId, &'a [NativeId])>,
        selected: Option<&'a VertexTarget>,
    },
    Edge {
        active: Option<(NodeId, &'a [NativeId])>,
        selected: Option<&'a EdgeTarget>,
    },
}

/// How an object is drawn; passes run in this order so the lead lands on top.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Highlight {
    None,
    Member,
    Lead,
}

impl SelectionFeedback<'_> {
    fn object_highlight(self, path: &[NativeId]) -> Highlight {
        match self {
            Self::Object(Some(selected)) if path.starts_with(selected) => Highlight::Member,
            Self::Multi(members) => {
                if members
                    .last()
                    .is_some_and(|lead| path.starts_with(&lead.path))
                {
                    Highlight::Lead
                } else if members.iter().any(|member| path.starts_with(&member.path)) {
                    Highlight::Member
                } else {
                    Highlight::None
                }
            }
            _ => Highlight::None,
        }
    }

    /// Curves are object-selectable only; Face mode never highlights or marks them.
    fn curve_highlight(self, instance: &azteccraft::view_scene::ViewCurveInstance) -> Highlight {
        self.object_highlight(instance.path())
    }

    fn highlight(self, instance: &ViewInstance, face: &ViewFace) -> Highlight {
        match self {
            Self::Object(_) | Self::Multi(_) => self.object_highlight(instance.path()),
            Self::Face { selected, .. }
                if selected.is_some_and(|target| {
                    target.transform == instance.transform()
                        && target.shape == instance.shape()
                        && target.path == instance.path()
                        && target.face_index == face.source_index()
                }) =>
            {
                Highlight::Member
            }
            Self::Face { .. } | Self::Vertex { .. } | Self::Edge { .. } => Highlight::None,
        }
    }

    fn centers(self, instance: &ViewInstance) -> bool {
        matches!(self, Self::Face { active: Some((owner, path)), .. }
            if instance.is_direct_shape_of(owner, path))
    }

    fn vertex_markers(self, instance: &ViewInstance) -> bool {
        matches!(self, Self::Vertex { active: Some((owner, path)), .. }
            if instance.is_direct_shape_of(owner, path))
    }

    /// Edge mode replaces the active object's face outlines with its full edge list.
    fn edges_shown(self, instance: &ViewInstance) -> bool {
        matches!(self, Self::Edge { active: Some((owner, path)), .. }
            if instance.is_direct_shape_of(owner, path))
    }

    fn selected_vertex(self, instance: &ViewInstance) -> Option<usize> {
        match self {
            Self::Vertex {
                selected: Some(target),
                ..
            } if target.transform == instance.transform()
                && target.shape == instance.shape()
                && target.path == instance.path() =>
            {
                Some(target.vertex_index)
            }
            _ => None,
        }
    }

    fn selected_edge(self, instance: &ViewInstance) -> Option<usize> {
        match self {
            Self::Edge {
                selected: Some(target),
                ..
            } if self.edges_shown(instance)
                && target.transform == instance.transform()
                && target.shape == instance.shape()
                && target.path == instance.path() =>
            {
                Some(target.edge_index)
            }
            _ => None,
        }
    }

    /// Markers the Wireframe CPU occlusion filter would test on `instance`.
    fn filtered_markers(self, instance: &ViewInstance) -> usize {
        if self.centers(instance) {
            instance.faces().len()
        } else if self.vertex_markers(instance) {
            instance.world_vertices().len()
        } else {
            0
        }
    }
}

/// Whether a clip-space point lies inside the view frustum.
fn clip_inside(clip: Vec4) -> bool {
    clip.w > 0.0
        && clip.z >= 0.0
        && clip.z <= clip.w
        && clip.x.abs() <= clip.w
        && clip.y.abs() <= clip.w
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

    #[expect(
        clippy::too_many_arguments,
        reason = "display state is a flat argument list"
    )]
    pub fn callback(
        &self,
        scene: &ViewScene,
        camera: &Camera,
        rect: Rect,
        physical_size: [u32; 2],
        selection: SelectionFeedback<'_>,
        grid_visible: bool,
        shading: ShadingMode,
    ) -> Result<(egui::epaint::PaintCallback, bool), RenderError> {
        if let Some(error) = self.terminal_error() {
            return Err(RenderError::Backend(error));
        }
        if !rect.is_finite()
            || rect.width() <= 0.0
            || rect.height() <= 0.0
            || physical_size.contains(&0)
        {
            return Err(RenderError::Camera(InvalidCameraInput));
        }
        let submission = Submission::new(
            scene,
            camera,
            physical_size,
            selection,
            grid_visible,
            shading,
            self.format,
            self.max_buffer_size,
        )?;
        #[cfg(test)]
        {
            *self.submission.lock() = Some(submission.clone());
        }
        let degraded = submission.centers_degraded;
        Ok((
            egui_wgpu::Callback::new_paint_callback(
                rect,
                ViewportCallback {
                    submission,
                    errors: self.errors.clone(),
                },
            ),
            degraded,
        ))
    }
}

#[cfg_attr(test, derive(Clone))]
struct Submission {
    bytes: Vec<u8>,
    surfaces: Range<u32>,
    lines: Range<u32>,
    markers: Range<u32>,
    /// Wireframe skipped its occlusion-filtered markers (face centres in Face mode, vertices in
    /// Vertex mode) because the check exceeded its work budget.
    centers_degraded: bool,
}

impl Submission {
    #[expect(
        clippy::too_many_arguments,
        reason = "display state is a flat argument list"
    )]
    fn new(
        scene: &ViewScene,
        camera: &Camera,
        physical_size: [u32; 2],
        selection: SelectionFeedback<'_>,
        grid_visible: bool,
        shading: ShadingMode,
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
        let mut vertices = if grid_visible { GRID_VERTICES } else { 0 };
        for instance in scene.instances() {
            let outline_corners = if selection.edges_shown(instance) {
                0
            } else {
                2
            };
            for face in instance.faces() {
                let surface_count = if shading == ShadingMode::Wireframe {
                    0
                } else {
                    face.triangles().count() * 3
                };
                vertices = vertices
                    .checked_add(surface_count)
                    .and_then(|count| count.checked_add(face.corners().len() * outline_corners))
                    .and_then(|count| {
                        count.checked_add(if selection.centers(instance) { 6 } else { 0 })
                    })
                    .ok_or(RenderError::ResourceLimit)?;
            }
            let component_vertices = if selection.edges_shown(instance) {
                instance.edges().len().checked_mul(2).and_then(|count| {
                    count.checked_add(if selection.selected_edge(instance).is_some() {
                        6
                    } else {
                        0
                    })
                })
            } else if selection.vertex_markers(instance) {
                instance.world_vertices().len().checked_mul(6)
            } else {
                Some(0)
            };
            vertices = component_vertices
                .and_then(|count| vertices.checked_add(count))
                .ok_or(RenderError::ResourceLimit)?;
        }
        for instance in scene.curve_instances() {
            vertices = vertices
                .checked_add(
                    instance
                        .segments()
                        .len()
                        .checked_mul(2)
                        .ok_or(RenderError::ResourceLimit)?,
                )
                .ok_or(RenderError::ResourceLimit)?;
        }
        for joint in scene.joints() {
            let bone = if joint.parent_center().is_some() {
                2
            } else {
                0
            };
            vertices = vertices
                .checked_add(JOINT_VERTICES + bone)
                .ok_or(RenderError::ResourceLimit)?;
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
            markers: 0..0,
            centers_degraded: false,
        };
        submission
            .bytes
            .try_reserve_exact(byte_count)
            .map_err(|_| RenderError::ResourceLimit)?;
        check_buffer_bytes(
            u64::try_from(submission.bytes.capacity()).map_err(|_| RenderError::ResourceLimit)?,
            u64::MAX,
        )?;
        let eye = camera.eye();
        let surface_instances: &[ViewInstance] = if shading == ShadingMode::Wireframe {
            &[]
        } else {
            scene.instances()
        };
        for instance in surface_instances.iter().rev() {
            for face in instance.faces().iter().rev() {
                let base = match selection.highlight(instance, face) {
                    Highlight::None => instance.base_rgb(),
                    Highlight::Member => selected_base(instance.base_rgb()),
                    Highlight::Lead => lead_base(instance.base_rgb()),
                };
                let center = face
                    .center(instance.world_vertices())
                    .map_err(|_| RenderError::NonFiniteClip)?;
                let view_dir = (eye - center).normalize_or_zero();
                let [red, green, blue] = shade(
                    instance.world_normal(face).to_array(),
                    view_dir.to_array(),
                    base,
                );
                let fill = Color32::from_rgb(red, green, blue);
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
        if grid_visible {
            let extent = GRID_SIZE as f32;
            for index in -GRID_SIZE..=GRID_SIZE {
                let coordinate = index as f32;
                let color = Color32::from_gray(grid_gray(index));
                for point in [
                    Vec3::new(coordinate, 0.0, -extent),
                    Vec3::new(coordinate, 0.0, extent),
                    Vec3::new(-extent, 0.0, coordinate),
                    Vec3::new(extent, 0.0, coordinate),
                ] {
                    submission.vertex(matrix, point, color, format)?;
                }
            }
        }
        for pass in [Highlight::None, Highlight::Member, Highlight::Lead] {
            for instance in scene.instances() {
                if selection.edges_shown(instance) {
                    continue;
                }
                for face in instance.faces() {
                    let highlight = selection.highlight(instance, face);
                    if highlight != pass {
                        continue;
                    }
                    let color = highlight_edge_color(highlight);
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
            if pass == Highlight::None {
                for instance in scene
                    .instances()
                    .iter()
                    .filter(|instance| selection.edges_shown(instance))
                {
                    for &[a, b] in instance.edges() {
                        for vertex in [a, b] {
                            submission.vertex(
                                matrix,
                                instance.world_vertices()[vertex],
                                Color32::from_rgb(EDGE_RGB[0], EDGE_RGB[1], EDGE_RGB[2]),
                                format,
                            )?;
                        }
                    }
                }
            }
            for instance in scene.curve_instances() {
                let highlight = selection.curve_highlight(instance);
                if highlight != pass {
                    continue;
                }
                let color = highlight_edge_color(highlight);
                for segment in instance.segments() {
                    for point in segment {
                        submission.vertex(matrix, *point, color, format)?;
                    }
                }
            }
            for joint in scene.joints() {
                let highlight = selection.object_highlight(joint.path());
                if highlight != pass {
                    continue;
                }
                let color = highlight_edge_color(highlight);
                for [a, b] in joint_circle_segments(joint) {
                    submission.vertex(matrix, a, color, format)?;
                    submission.vertex(matrix, b, color, format)?;
                }
                if let Some(parent) = joint.parent_center() {
                    submission.vertex(matrix, parent, color, format)?;
                    submission.vertex(matrix, joint.center(), color, format)?;
                }
            }
        }
        submission.surfaces = 0..surface_end;
        submission.lines = surface_end..submission.vertex_count();
        let marker_start = submission.vertex_count();
        if shading == ShadingMode::Wireframe {
            // No surface depth exists, so centres need the CPU occlusion filter. Over budget the
            // frame hides them (flagged to the user) instead of failing; edges and grid still draw.
            let mut eligible = 0_usize;
            let mut triangles = 0_usize;
            for instance in scene.instances() {
                for face in instance.faces() {
                    triangles = triangles
                        .checked_add(face.triangles().count())
                        .ok_or(RenderError::ResourceLimit)?;
                }
                eligible = eligible
                    .checked_add(selection.filtered_markers(instance))
                    .ok_or(RenderError::ResourceLimit)?;
            }
            submission.centers_degraded =
                azteccraft::viewport::face_center_work(eligible, triangles).is_err();
        }
        let frame = camera
            .occlusion_frame(Vec2::new(physical_size[0] as f32, physical_size[1] as f32))
            .map_err(|_| RenderError::NonFiniteClip)?;
        let draw_centers = !submission.centers_degraded;
        for instance in scene
            .instances()
            .iter()
            .filter(|instance| draw_centers && selection.centers(instance))
        {
            for face in instance.faces() {
                let center = face
                    .center(instance.world_vertices())
                    .map_err(|_| RenderError::NonFiniteClip)?;
                let clip = matrix * center.extend(1.0);
                if !clip.is_finite() {
                    return Err(RenderError::NonFiniteClip);
                }
                if !clip_inside(clip) {
                    continue;
                }
                if shading == ShadingMode::Wireframe
                    && camera
                        .unoccluded_depth(
                            scene,
                            &frame,
                            center,
                            clip.truncate() / clip.w,
                            azteccraft::viewport::FACE_CENTER_DEPTH_TOLERANCE,
                        )
                        .map_err(|_| RenderError::NonFiniteClip)?
                        .is_none()
                {
                    continue;
                }
                submission.square(
                    clip,
                    FACE_CENTER_MARKER_PIXELS,
                    physical_size,
                    Color32::from_rgb(242, 163, 70),
                    format,
                )?;
            }
        }
        for instance in scene
            .instances()
            .iter()
            .filter(|instance| draw_centers && selection.vertex_markers(instance))
        {
            let selected = selection.selected_vertex(instance);
            // The selected marker goes last so it stays on top at the shared depth.
            for selected_pass in [false, true] {
                for (index, &point) in instance.world_vertices().iter().enumerate() {
                    if (selected == Some(index)) != selected_pass {
                        continue;
                    }
                    let clip = matrix * camera.marker_biased(point).extend(1.0);
                    if !clip.is_finite() {
                        return Err(RenderError::NonFiniteClip);
                    }
                    if !clip_inside(clip) {
                        continue;
                    }
                    if shading == ShadingMode::Wireframe
                        && camera
                            .unoccluded_depth(
                                scene,
                                &frame,
                                point,
                                clip.truncate() / clip.w,
                                azteccraft::viewport::VERTEX_MARKER_DEPTH_TOLERANCE,
                            )
                            .map_err(|_| RenderError::NonFiniteClip)?
                            .is_none()
                    {
                        continue;
                    }
                    let [red, green, blue] = if selected_pass {
                        SELECTED_VERTEX_RGB
                    } else {
                        VERTEX_RGB
                    };
                    submission.square(
                        clip,
                        VERTEX_MARKER_PIXELS,
                        physical_size,
                        Color32::from_rgb(red, green, blue),
                        format,
                    )?;
                }
            }
        }
        for instance in scene.instances() {
            let Some(&[a, b]) = selection
                .selected_edge(instance)
                .and_then(|index| instance.edges().get(index))
            else {
                continue;
            };
            let [red, green, blue] = SELECTED_EDGE_RGB;
            submission.edge_quad(
                matrix,
                [a, b].map(|index| camera.marker_biased(instance.world_vertices()[index])),
                physical_size,
                Color32::from_rgb(red, green, blue),
                format,
            )?;
        }
        submission.markers = marker_start..submission.vertex_count();
        Ok(submission)
    }

    /// A `pixels` wide square centred on `clip`, as two triangles.
    fn square(
        &mut self,
        clip: Vec4,
        pixels: f32,
        physical_size: [u32; 2],
        color: Color32,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        let dx = pixels / physical_size[0] as f32 * clip.w;
        let dy = pixels / physical_size[1] as f32 * clip.w;
        for [x, y] in [
            [-dx, -dy],
            [dx, -dy],
            [dx, dy],
            [-dx, -dy],
            [dx, dy],
            [-dx, dy],
        ] {
            self.clip_vertex(clip + Vec4::new(x, y, 0.0, 0.0), color, format)?;
        }
        Ok(())
    }

    /// A `SELECTED_EDGE_PIXELS` wide screen-space strip along the world segment `ends`, cut at the
    /// near plane. Skipped when behind the eye or when it projects to no length.
    fn edge_quad(
        &mut self,
        matrix: Mat4,
        ends: [Vec3; 2],
        physical_size: [u32; 2],
        color: Color32,
        format: wgpu::TextureFormat,
    ) -> Result<(), RenderError> {
        let [mut a, mut b] = ends.map(|point| matrix * point.extend(1.0));
        if !a.is_finite() || !b.is_finite() {
            return Err(RenderError::NonFiniteClip);
        }
        match (a.z >= 0.0, b.z >= 0.0) {
            (false, false) => return Ok(()),
            (true, false) => b = a + (b - a) * (a.z / (a.z - b.z)),
            (false, true) => a = b + (a - b) * (b.z / (b.z - a.z)),
            (true, true) => {}
        }
        if a.w <= 0.0 || b.w <= 0.0 {
            return Ok(());
        }
        let half_size = Vec2::new(physical_size[0] as f32, physical_size[1] as f32) * 0.5;
        let along = (b.truncate().truncate() / b.w - a.truncate().truncate() / a.w) * half_size;
        let length = along.length();
        if !length.is_finite() {
            return Err(RenderError::NonFiniteClip);
        }
        if length <= f32::EPSILON {
            return Ok(());
        }
        let side = Vec2::new(-along.y, along.x) / length * (SELECTED_EDGE_PIXELS * 0.5) / half_size;
        let corner = |clip: Vec4, sign: f32| clip + (side * sign * clip.w).extend(0.0).extend(0.0);
        for (clip, sign) in [
            (a, -1.0),
            (b, -1.0),
            (b, 1.0),
            (a, -1.0),
            (b, 1.0),
            (a, 1.0),
        ] {
            self.clip_vertex(corner(clip, sign), color, format)?;
        }
        Ok(())
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
    markers: wgpu::RenderPipeline,
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
            markers: pipeline(wgpu::PrimitiveTopology::TriangleList, false),
            vertices: None,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        submission: &Submission,
    ) -> Result<(), RenderError> {
        if submission.bytes.is_empty() {
            return Ok(());
        }
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
        if submission.bytes.is_empty() {
            return Ok(());
        }
        let vertices = self.vertices.as_ref().ok_or_else(|| {
            RenderError::Backend("viewport vertices have not been uploaded".into())
        })?;
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.set_pipeline(&self.surfaces);
        pass.draw(submission.surfaces.clone(), 0..1);
        pass.set_pipeline(&self.lines);
        pass.draw(submission.lines.clone(), 0..1);
        pass.set_pipeline(&self.markers);
        pass.draw(submission.markers.clone(), 0..1);
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
