use crate::scene::{Node, NodeId, Scene};
use crate::{
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
    view_scene::{
        EDGE_CLICK_PIXELS, FACE_CENTER_CLICK_PIXELS, VERTEX_CLICK_PIXELS, ViewError, ViewFace,
        ViewInstance, ViewScene,
    },
};
use glam::{DVec2, DVec4, Mat4, Vec2, Vec3, Vec4};
use std::fmt;

const MAX_FACE_CENTER_INTERSECTIONS: usize = 1_000_000;

/// Relative view-depth slack for a face center against its own surface hit (f32 rounding).
pub const FACE_CENTER_DEPTH_TOLERANCE: f32 = 1e-4;
/// Relative view-depth slack for a vertex, or a point on an edge, against the surface it lies
/// on. Vertex markers and the selected-edge strip are biased toward the eye by it and vertex and
/// edge picks test occlusion with it, so what is drawn and what is picked agree. Kept apart from
/// the face constant so loosening one never moves the other. A flat provisional policy value,
/// see `known-issues.md`.
pub const VERTEX_MARKER_DEPTH_TOLERANCE: f32 = 6e-3;

// Right-handed, Y-up; yaw zero looks from +Z. NDC uses X/Y [-1, 1], Y up, Z [0, 1].
// UI pixels map to NDC as (2*x/width - 1, 1 - 2*y/height).
#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    fov_y: f32,
    near: f32,
    far: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidCameraInput;

impl fmt::Display for InvalidCameraInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("camera inputs must be finite and within valid projection ranges")
    }
}
impl std::error::Error for InvalidCameraInput {}

/// Why `Camera::set_pose` refused a pose; names the field so callers can report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidCameraPose {
    TargetNotFinite,
    YawNotFinite,
    PitchNotFinite,
    PitchOutOfRange,
    DistanceNotFinite,
    DistanceNotPositive,
    /// Every field is valid alone but the eye or view matrix overflows.
    Unrepresentable,
}

impl fmt::Display for InvalidCameraPose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetNotFinite => f.write_str("camera target must be finite"),
            Self::YawNotFinite => f.write_str("camera yaw must be finite"),
            Self::PitchNotFinite => f.write_str("camera pitch must be finite"),
            Self::PitchOutOfRange => write!(
                f,
                "camera pitch must be within \u{b1}{} degrees",
                MAX_PITCH.to_degrees()
            ),
            Self::DistanceNotFinite => f.write_str("camera distance must be finite"),
            Self::DistanceNotPositive => f.write_str("camera distance must be greater than zero"),
            Self::Unrepresentable => f.write_str(
                "camera pose is not representable (distance too small or values too large)",
            ),
        }
    }
}
impl std::error::Error for InvalidCameraPose {}

/// Pitch limit shared by `new`, `set_pose` and `orbit`, just short of the poles so the view never flips.
pub const MAX_PITCH: f32 = std::f32::consts::FRAC_PI_2 - 0.001;

impl Default for Camera {
    fn default() -> Self {
        Self::new(
            Vec3::ZERO,
            0.6,
            0.4,
            8.0,
            45_f32.to_radians(),
            0.01,
            10_000.0,
        )
        .expect("default camera is valid")
    }
}

/// Wraps yaw into `[0, TAU)`. In f32 `rem_euclid` rounds a tiny negative input up to exactly
/// `TAU`, which would report 360 degrees and not round-trip through a pose.
fn wrap_yaw(yaw: f32) -> f32 {
    let wrapped = yaw.rem_euclid(std::f32::consts::TAU);
    if wrapped >= std::f32::consts::TAU {
        0.0
    } else {
        wrapped
    }
}

impl Camera {
    pub fn same_capture(&self, other: &Self) -> bool {
        self.target.to_array().map(f32::to_bits) == other.target.to_array().map(f32::to_bits)
            && [
                self.yaw,
                self.pitch,
                self.distance,
                self.fov_y,
                self.near,
                self.far,
            ]
            .map(f32::to_bits)
                == [
                    other.yaw,
                    other.pitch,
                    other.distance,
                    other.fov_y,
                    other.near,
                    other.far,
                ]
                .map(f32::to_bits)
    }

    /// Builds a camera in the same normal form `orbit` and `set_pose` keep: yaw wraps into
    /// `[0, TAU)` and a pitch beyond `MAX_PITCH` is rejected, not clamped.
    pub fn new(
        target: Vec3,
        yaw: f32,
        pitch: f32,
        distance: f32,
        fov_y: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, InvalidCameraInput> {
        let camera = Self {
            target,
            yaw: wrap_yaw(yaw),
            pitch,
            distance,
            fov_y,
            near,
            far,
        };
        camera.validate()?;
        Ok(camera)
    }

    fn validate(&self) -> Result<(), InvalidCameraInput> {
        if !self.target.is_finite()
            || !self.yaw.is_finite()
            || !self.pitch.is_finite()
            || self.pitch.abs() > MAX_PITCH
            || !self.distance.is_finite()
            || self.distance <= 0.0
            || !self.fov_y.is_finite()
            || self.fov_y <= 0.0
            || self.fov_y >= std::f32::consts::PI
            || !self.near.is_finite()
            || self.near <= 0.0
            || !self.far.is_finite()
            || self.far <= self.near
            || !self.eye().is_finite()
            || !self.view_matrix().is_finite()
        {
            return Err(InvalidCameraInput);
        }
        Ok(())
    }

    pub fn target(&self) -> Vec3 {
        self.target
    }
    pub fn distance(&self) -> f32 {
        self.distance
    }
    /// Orbit angle about world Y in radians; zero looks from +Z toward the target.
    pub fn yaw(&self) -> f32 {
        self.yaw
    }
    /// Elevation in radians, positive above the target, within ±`MAX_PITCH`.
    pub fn pitch(&self) -> f32 {
        self.pitch
    }
    /// Vertical field of view in radians.
    pub fn fov_y(&self) -> f32 {
        self.fov_y
    }
    pub fn near(&self) -> f32 {
        self.near
    }
    pub fn far(&self) -> f32 {
        self.far
    }
    pub fn eye(&self) -> Vec3 {
        self.target
            + self.distance
                * Vec3::new(
                    self.yaw.sin() * self.pitch.cos(),
                    self.pitch.sin(),
                    self.yaw.cos() * self.pitch.cos(),
                )
    }
    /// `point` moved toward the eye by `VERTEX_MARKER_DEPTH_TOLERANCE`, so a marker on a surface is not hidden by
    /// the neighbouring pixels of that surface. Depth is linear in `p - eye`, unlike NDC depth,
    /// so the shift scales with distance.
    pub fn marker_biased(&self, point: Vec3) -> Vec3 {
        let eye = self.eye();
        eye + (point - eye) * (1.0 - VERTEX_MARKER_DEPTH_TOLERANCE)
    }

    pub fn view_matrix(&self) -> Mat4 {
        let back = (self.eye() - self.target).normalize();
        let right = Vec3::Y.cross(back).normalize();
        let up = back.cross(right);
        let eye = self.eye();
        Mat4::from_rows(
            right.extend(-right.dot(eye)),
            up.extend(-up.dot(eye)),
            back.extend(-back.dot(eye)),
            Vec4::W,
        )
    }
    pub fn projection_matrix(&self, aspect: f32) -> Result<Mat4, InvalidCameraInput> {
        if !aspect.is_finite() || aspect <= 0.0 {
            return Err(InvalidCameraInput);
        }
        let y = 1.0 / (self.fov_y * 0.5).tan();
        let z = self.far / (self.near - self.far);
        let matrix = Mat4::from_cols(
            Vec4::new(y / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, y, 0.0, 0.0),
            Vec4::new(0.0, 0.0, z, -1.0),
            Vec4::new(0.0, 0.0, z * self.near, 0.0),
        );
        if !matrix.is_finite() {
            return Err(InvalidCameraInput);
        }
        Ok(matrix)
    }

    pub fn orbit(&mut self, radians: Vec2) -> Result<(), InvalidCameraInput> {
        if !radians.is_finite() {
            return Err(InvalidCameraInput);
        }
        let mut next = self.clone();
        next.yaw = wrap_yaw(next.yaw + radians.x);
        next.pitch = (next.pitch + radians.y).clamp(-MAX_PITCH, MAX_PITCH);
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Restores an absolute view. Yaw wraps like `orbit`; a pitch beyond orbit's limit is
    /// rejected rather than clamped so a pose that was read back never changes silently.
    /// fov and clip planes are untouched. The camera is unchanged on error.
    /// Changed/Unchanged is numeric: -0.0 equals 0.0, so a pose differing only in zero signs is
    /// no change although the stored bits may flip; `same_capture` compares bits.
    pub fn set_pose(
        &mut self,
        target: Vec3,
        distance: f32,
        yaw: f32,
        pitch: f32,
    ) -> Result<(), InvalidCameraPose> {
        if !target.is_finite() {
            return Err(InvalidCameraPose::TargetNotFinite);
        }
        if !distance.is_finite() {
            return Err(InvalidCameraPose::DistanceNotFinite);
        }
        if distance <= 0.0 {
            return Err(InvalidCameraPose::DistanceNotPositive);
        }
        if !yaw.is_finite() {
            return Err(InvalidCameraPose::YawNotFinite);
        }
        if !pitch.is_finite() {
            return Err(InvalidCameraPose::PitchNotFinite);
        }
        if pitch.abs() > MAX_PITCH {
            return Err(InvalidCameraPose::PitchOutOfRange);
        }
        let mut next = self.clone();
        next.target = target;
        next.distance = distance;
        next.yaw = wrap_yaw(yaw);
        next.pitch = pitch;
        next.validate()
            .map_err(|InvalidCameraInput| InvalidCameraPose::Unrepresentable)?;
        *self = next;
        Ok(())
    }

    pub fn pan(&mut self, world_units: Vec2) -> Result<(), InvalidCameraInput> {
        if !world_units.is_finite() {
            return Err(InvalidCameraInput);
        }
        let mut next = self.clone();
        let inverse_view = self.view_matrix().inverse();
        next.target += inverse_view.transform_vector3(Vec3::new(world_units.x, world_units.y, 0.0));
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn dolly(&mut self, factor: f32) -> Result<(), InvalidCameraInput> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(InvalidCameraInput);
        }
        let mut next = self.clone();
        next.distance *= factor;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn frame(&mut self, node: &Node, aspect: f32) -> Result<(), InvalidCameraInput> {
        self.frame_points(&node.world_vertices(), aspect)
    }

    pub fn frame_points(
        &mut self,
        vertices: &[Vec3],
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        self.frame_iter(vertices.iter().copied(), aspect)
    }

    pub fn frame_view(
        &mut self,
        scene: &ViewScene,
        prefix: Option<&[crate::native_scene::NativeId]>,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        self.frame_iter(
            scene
                .instances()
                .iter()
                .filter(move |instance| {
                    prefix.is_none_or(|prefix| instance.path().starts_with(prefix))
                })
                .flat_map(|instance| instance.world_vertices().iter().copied())
                .chain(
                    scene
                        .curve_instances()
                        .iter()
                        .filter(move |instance| {
                            prefix.is_none_or(|prefix| instance.path().starts_with(prefix))
                        })
                        .flat_map(|instance| {
                            let [low, high] = instance.bounds();
                            (0..8).map(move |corner| {
                                glam::DVec3::new(
                                    if corner & 1 == 0 { low.x } else { high.x },
                                    if corner & 2 == 0 { low.y } else { high.y },
                                    if corner & 4 == 0 { low.z } else { high.z },
                                )
                                .as_vec3()
                            })
                        }),
                ),
            aspect,
        )
    }

    pub fn frame_face(
        &mut self,
        scene: &ViewScene,
        target: &FaceTarget,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        let instance = component_instance(scene, target.transform, target.shape, &target.path)?;
        let face = instance
            .faces()
            .iter()
            .find(|face| face.source_index() == target.face_index)
            .ok_or(InvalidCameraInput)?;
        let vertices = instance.world_vertices();
        if face.corners().iter().any(|&index| index >= vertices.len()) {
            return Err(InvalidCameraInput);
        }
        self.frame_iter(
            face.corners()
                .iter()
                .filter_map(|&index| vertices.get(index).copied()),
            aspect,
        )
    }

    pub fn frame_vertex(
        &mut self,
        scene: &ViewScene,
        target: &VertexTarget,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        let instance = component_instance(scene, target.transform, target.shape, &target.path)?;
        let point = instance
            .world_vertices()
            .get(target.vertex_index)
            .copied()
            .ok_or(InvalidCameraInput)?;
        self.frame_points(&[point], aspect)
    }

    pub fn frame_edge(
        &mut self,
        scene: &ViewScene,
        target: &EdgeTarget,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        let instance = component_instance(scene, target.transform, target.shape, &target.path)?;
        let [a, b] = instance
            .edges()
            .get(target.edge_index)
            .copied()
            .ok_or(InvalidCameraInput)?;
        let vertices = instance.world_vertices();
        let ends = [
            vertices.get(a).copied().ok_or(InvalidCameraInput)?,
            vertices.get(b).copied().ok_or(InvalidCameraInput)?,
        ];
        self.frame_points(&ends, aspect)
    }

    fn frame_iter(
        &mut self,
        vertices: impl Iterator<Item = Vec3> + Clone,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        self.projection_matrix(aspect)?;
        let mut any = false;
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in vertices.clone() {
            if !vertex.is_finite() {
                return Err(InvalidCameraInput);
            }
            any = true;
            min = min.min(vertex);
            max = max.max(vertex);
        }
        if !any {
            return Err(InvalidCameraInput);
        }
        let target = min * 0.5 + max * 0.5;
        let radius = vertices.map(|v| v.distance(target)).fold(0.0_f32, f32::max);
        let half_angle = (self.fov_y * 0.5).min(((self.fov_y * 0.5).tan() * aspect).atan());
        let mut next = self.clone();
        next.target = target;
        next.distance = (radius / half_angle.sin())
            .max(self.near + radius)
            .max(self.near * 2.0)
            * 1.05;
        next.far = self.far.max(next.distance + radius + self.near);
        next.validate()?;
        *self = next;
        Ok(())
    }

    // Points outside depth clipping return None; lateral offscreen points retain their NDC coordinates.
    pub fn project(&self, world: Vec3, aspect: f32) -> Result<Option<Vec3>, InvalidCameraInput> {
        if !world.is_finite() {
            return Err(InvalidCameraInput);
        }
        let clip = self.projection_matrix(aspect)? * self.view_matrix() * world.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 || clip.z < 0.0 || clip.z > clip.w {
            return Ok(None);
        }
        let ndc = clip.truncate() / clip.w;
        if !ndc.is_finite() {
            return Err(InvalidCameraInput);
        }
        Ok(Some(ndc))
    }

    pub fn ray(&self, ndc: Vec2, aspect: f32) -> Result<Ray, InvalidCameraInput> {
        let projection = self.projection_matrix(aspect)?;
        if !ndc.is_finite() || ndc.abs().max_element() > 1.0 {
            return Err(InvalidCameraInput);
        }
        self.ray_from_projection(ndc, projection)
    }

    pub fn captured_ray(&self, ndc: Vec2, aspect: f32) -> Result<Ray, InvalidCameraInput> {
        let projection = self.projection_matrix(aspect)?;
        if !ndc.is_finite() {
            return Err(InvalidCameraInput);
        }
        self.ray_from_projection(ndc, projection)
    }

    fn ray_from_projection(&self, ndc: Vec2, projection: Mat4) -> Result<Ray, InvalidCameraInput> {
        let view_direction = Vec3::new(
            ndc.x / projection.x_axis.x,
            ndc.y / projection.y_axis.y,
            -1.0,
        );
        let view_length_squared = view_direction.length_squared();
        if !view_direction.is_finite()
            || !view_length_squared.is_finite()
            || view_length_squared <= 0.0
        {
            return Err(InvalidCameraInput);
        }
        let view_direction = view_direction.normalize();
        let direction = self
            .view_matrix()
            .inverse()
            .transform_vector3(view_direction);
        let direction_length_squared = direction.length_squared();
        if !direction.is_finite()
            || !direction_length_squared.is_finite()
            || direction_length_squared <= 0.0
        {
            return Err(InvalidCameraInput);
        }
        let direction = direction.normalize();
        if !direction.is_finite() {
            return Err(InvalidCameraInput);
        }
        Ok(Ray {
            origin: self.eye(),
            direction,
        })
    }

    pub fn pick(
        &self,
        scene: &Scene,
        ndc: Vec2,
        aspect: f32,
    ) -> Result<Option<NodeId>, InvalidCameraInput> {
        let ray = self.ray(ndc, aspect)?;
        let mut closest = None;
        for node in scene.nodes() {
            let vertices = node.world_vertices();
            let triangles = node.mesh().faces().iter().flat_map(|face| {
                [[face[0], face[1], face[2]], [face[0], face[2], face[3]]]
                    .map(|triangle| (node.id(), triangle.map(|index| vertices[index])))
            });
            self.closest_triangle(ray, triangles, &mut closest);
        }
        Ok(closest.map(|(_, id)| id))
    }

    /// The caller must consume this hit before any edit, or rebuild and repick its NDC.
    pub fn pick_view(
        &self,
        scene: &ViewScene,
        ndc: Vec2,
        aspect: f32,
    ) -> Result<Option<FaceTarget>, ViewError> {
        let ray = self.ray(ndc, aspect).map_err(ViewError::Camera)?;
        let mut closest = None;
        for instance in scene.instances() {
            let triangles = instance.faces().iter().flat_map(|face| {
                face.triangles().map(move |triangle| {
                    (
                        (instance, face.source_index()),
                        triangle.map(|index| instance.world_vertices()[index]),
                    )
                })
            });
            self.closest_triangle(ray, triangles, &mut closest);
        }
        let Some((_, (instance, face_index))) = closest else {
            return Ok(None);
        };
        face_target(instance, face_index).map(Some)
    }

    pub fn pick_face_center(
        &self,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
        pointer_points: Vec2,
        viewport_pixels: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Option<FaceTarget>, ViewError> {
        let frame = self.pick_frame(pointer_points, viewport_pixels, pixels_per_point)?;
        let pointer = frame.pointer;
        let Some((owner, path)) = active else {
            return Ok(None);
        };
        let candidate = |instance: &ViewInstance, face: &ViewFace| {
            if !instance.is_direct_shape_of(owner, path) {
                return Ok(None);
            }
            let center = face.center(instance.world_vertices())?;
            let Some((ndc, pixel)) = project_in_frustum(&frame, center)? else {
                return Ok(None);
            };
            Ok(
                ((pixel - pointer).abs().max_element() <= FACE_CENTER_CLICK_PIXELS * 0.5)
                    .then_some((center, ndc)),
            )
        };
        let mut eligible = 0usize;
        for instance in scene.instances() {
            for face in instance.faces() {
                if candidate(instance, face)?.is_some() {
                    eligible = eligible.checked_add(1).ok_or(ViewError::ResourceLimit)?;
                }
            }
        }
        admit_pick_work(scene, eligible)?;
        let mut selected: Option<(f32, &ViewInstance, usize)> = None;
        for instance in scene.instances() {
            for face in instance.faces() {
                let Some((center, ndc)) = candidate(instance, face)? else {
                    continue;
                };
                let Some(depth) =
                    self.unoccluded_depth(scene, &frame, center, ndc, FACE_CENTER_DEPTH_TOLERANCE)?
                else {
                    continue;
                };
                let key = (
                    instance.path(),
                    instance.transform().value(),
                    instance.shape(),
                    face.source_index(),
                );
                if selected.as_ref().is_none_or(|(previous, other, index)| {
                    depth < *previous
                        || (depth == *previous
                            && key
                                < (
                                    other.path(),
                                    other.transform().value(),
                                    other.shape(),
                                    *index,
                                ))
                }) {
                    selected = Some((depth, instance, face.source_index()));
                }
            }
        }
        selected
            .map(|(_, instance, index)| face_target(instance, index))
            .transpose()
    }

    pub fn pick_vertex(
        &self,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
        pointer_points: Vec2,
        viewport_pixels: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Option<VertexTarget>, ViewError> {
        let picked = self.pick_component(
            scene,
            active,
            pointer_points,
            viewport_pixels,
            pixels_per_point,
            |instance, frame, out| {
                for (index, &point) in instance.world_vertices().iter().enumerate() {
                    let Some((ndc, pixel)) = project_in_frustum(frame, point)? else {
                        continue;
                    };
                    let offset = pixel - frame.pointer;
                    if offset.abs().max_element() <= VERTEX_CLICK_PIXELS * 0.5 {
                        push_candidate(out, instance, index, point, ndc, offset.length())?;
                    }
                }
                Ok(())
            },
        )?;
        picked
            .map(|(instance, vertex_index)| {
                Ok(VertexTarget {
                    transform: instance.transform(),
                    shape: instance.shape(),
                    path: instance_path(instance)?,
                    vertex_index,
                })
            })
            .transpose()
    }

    pub fn pick_edge(
        &self,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
        pointer_points: Vec2,
        viewport_pixels: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Option<EdgeTarget>, ViewError> {
        let picked = self.pick_component(
            scene,
            active,
            pointer_points,
            viewport_pixels,
            pixels_per_point,
            |instance, frame, out| {
                let vertices = instance.world_vertices();
                for (index, &[a, b]) in instance.edges().iter().enumerate() {
                    let (Some(&a), Some(&b)) = (vertices.get(a), vertices.get(b)) else {
                        return Err(ViewError::InvalidGeometry);
                    };
                    if let Some(hit) = edge_candidate(frame, a, b)? {
                        push_candidate(out, instance, index, hit.point, hit.ndc, hit.pixels)?;
                    }
                }
                Ok(())
            },
        )?;
        picked
            .map(|(instance, edge_index)| {
                Ok(EdgeTarget {
                    transform: instance.transform(),
                    shape: instance.shape(),
                    path: instance_path(instance)?,
                    edge_index,
                })
            })
            .transpose()
    }

    /// Gathers click-window candidates from the active object's direct shapes, then returns the
    /// nearest to the pointer, then nearest to the eye, then lowest key, among the unoccluded.
    fn pick_component<'a>(
        &self,
        scene: &'a ViewScene,
        active: Option<(NodeId, &[NativeId])>,
        pointer_points: Vec2,
        viewport_pixels: [u32; 2],
        pixels_per_point: f32,
        gather: impl Fn(
            &'a ViewInstance,
            &PickFrame,
            &mut Vec<ScreenCandidate<'a>>,
        ) -> Result<(), ViewError>,
    ) -> Result<Option<(&'a ViewInstance, usize)>, ViewError> {
        let frame = self.pick_frame(pointer_points, viewport_pixels, pixels_per_point)?;
        let Some((owner, path)) = active else {
            return Ok(None);
        };
        let mut candidates = Vec::new();
        for instance in scene.instances() {
            if instance.is_direct_shape_of(owner, path) {
                gather(instance, &frame, &mut candidates)?;
            }
        }
        admit_pick_work(scene, candidates.len())?;
        let mut selected: Option<(f32, f32, &ViewInstance, usize)> = None;
        for candidate in candidates {
            let Some(depth) = self.unoccluded_depth(
                scene,
                &frame,
                candidate.point,
                candidate.ndc,
                VERTEX_MARKER_DEPTH_TOLERANCE,
            )?
            else {
                continue;
            };
            // Instances are sorted by path and indices ascend, so a strict comparison keeps the
            // lowest (path, index) among equal candidates.
            if selected.as_ref().is_none_or(|(pixels, nearest, _, _)| {
                (candidate.pixels, depth) < (*pixels, *nearest)
            }) {
                selected = Some((candidate.pixels, depth, candidate.instance, candidate.index));
            }
        }
        Ok(selected.map(|(_, _, instance, index)| (instance, index)))
    }

    /// Validates the pointer and viewport and builds the matrices every screen-space pick shares.
    fn pick_frame(
        &self,
        pointer_points: Vec2,
        viewport_pixels: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<PickFrame, ViewError> {
        let [width, height] = viewport_pixels;
        if width == 0 || height == 0 || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            return Err(ViewError::Camera(InvalidCameraInput));
        }
        let size = Vec2::new(width as f32, height as f32);
        let pointer = pointer_points * pixels_per_point;
        if !pointer.is_finite() || pointer.min_element() < 0.0 || pointer.cmpgt(size).any() {
            return Err(ViewError::Camera(InvalidCameraInput));
        }
        self.build_frame(size, pointer)
    }

    /// Matrices for occlusion tests outside picking (the renderer passes the physical viewport size).
    pub fn occlusion_frame(&self, size: Vec2) -> Result<PickFrame, ViewError> {
        self.build_frame(size, Vec2::ZERO)
    }

    fn build_frame(&self, size: Vec2, pointer: Vec2) -> Result<PickFrame, ViewError> {
        let aspect = size.x / size.y;
        let view = self.view_matrix();
        let matrix = self.projection_matrix(aspect).map_err(ViewError::Camera)? * view;
        Ok(PickFrame {
            size,
            pointer,
            aspect,
            view,
            matrix,
        })
    }

    /// Linear view depth of `point` when no surface hides it along the ray through `ndc`.
    pub fn unoccluded_depth(
        &self,
        scene: &ViewScene,
        frame: &PickFrame,
        point: Vec3,
        ndc: Vec3,
        tolerance: f32,
    ) -> Result<Option<f32>, ViewError> {
        let depth = -frame.view.transform_point3(point).z;
        let ray = self
            .ray(ndc.truncate(), frame.aspect)
            .map_err(ViewError::Camera)?;
        let mut closest = None;
        for occluder in scene.instances() {
            let triangles = occluder.faces().iter().flat_map(|surface| {
                surface
                    .triangles()
                    .map(|triangle| ((), triangle.map(|index| occluder.world_vertices()[index])))
            });
            self.closest_triangle(ray, triangles, &mut closest);
        }
        if let Some((distance, ())) = closest {
            // Compare linear view depth: the point lies on its own surface, so f32
            // rounding between the ray hit and the point must not self-occlude.
            let hit_depth = -frame
                .view
                .transform_point3(ray.origin + distance * ray.direction)
                .z;
            if hit_depth < depth * (1.0 - tolerance) {
                return Ok(None);
            }
        }
        Ok(Some(depth))
    }

    fn closest_triangle<T>(
        &self,
        ray: Ray,
        triangles: impl IntoIterator<Item = (T, [Vec3; 3])>,
        closest: &mut Option<(f32, T)>,
    ) {
        let view = self.view_matrix();
        for (target, [a, b, c]) in triangles {
            if let Some(distance) = intersect(ray, a, b, c) {
                let depth = -view
                    .transform_point3(ray.origin + distance * ray.direction)
                    .z;
                if depth >= self.near
                    && depth <= self.far
                    && closest
                        .as_ref()
                        .is_none_or(|(previous, _)| distance < *previous)
                {
                    *closest = Some((distance, target));
                }
            }
        }
    }
}

pub struct PickFrame {
    size: Vec2,
    pointer: Vec2,
    aspect: f32,
    view: Mat4,
    matrix: Mat4,
}

struct ScreenCandidate<'a> {
    instance: &'a ViewInstance,
    index: usize,
    point: Vec3,
    ndc: Vec3,
    pixels: f32,
}

fn push_candidate<'a>(
    out: &mut Vec<ScreenCandidate<'a>>,
    instance: &'a ViewInstance,
    index: usize,
    point: Vec3,
    ndc: Vec3,
    pixels: f32,
) -> Result<(), ViewError> {
    out.try_reserve(1).map_err(|_| ViewError::ResourceLimit)?;
    out.push(ScreenCandidate {
        instance,
        index,
        point,
        ndc,
        pixels,
    });
    Ok(())
}

struct EdgeHit {
    point: Vec3,
    ndc: Vec3,
    pixels: f32,
}

/// NDC and pixel position of `point`, or `None` outside the view frustum.
fn project_in_frustum(frame: &PickFrame, point: Vec3) -> Result<Option<(Vec3, Vec2)>, ViewError> {
    let clip = frame.matrix * point.extend(1.0);
    if !clip.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    if clip.w <= 0.0
        || clip.z < 0.0
        || clip.z > clip.w
        || clip.x.abs() > clip.w
        || clip.y.abs() > clip.w
    {
        return Ok(None);
    }
    let ndc = clip.truncate() / clip.w;
    if !ndc.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    Ok(Some((
        ndc,
        Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * frame.size,
    )))
}

/// Clip-space parameter interval of the segment `a..b` inside the frustum, if any.
fn clip_segment(a: DVec4, b: DVec4) -> Option<(f64, f64)> {
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for (f0, f1) in [
        (a.z, b.z),
        (a.w - a.z, b.w - b.z),
        (a.w + a.x, b.w + b.x),
        (a.w - a.x, b.w - b.x),
        (a.w + a.y, b.w + b.y),
        (a.w - a.y, b.w - b.y),
    ] {
        let slope = f1 - f0;
        if slope == 0.0 {
            if f0 < 0.0 {
                return None;
            }
        } else if slope > 0.0 {
            low = low.max(-f0 / slope);
        } else {
            high = high.min(-f0 / slope);
        }
    }
    (low <= high).then_some((low, high))
}

/// The point of edge `a..b` nearest the pointer on screen, when within `EDGE_CLICK_PIXELS`.
/// The screen segment is interpolated, then mapped back to the clip-space parameter so the
/// returned world point is perspective-correct.
fn edge_candidate(frame: &PickFrame, a: Vec3, b: Vec3) -> Result<Option<EdgeHit>, ViewError> {
    let (clip_a, clip_b) = (frame.matrix * a.extend(1.0), frame.matrix * b.extend(1.0));
    if !clip_a.is_finite() || !clip_b.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    let (clip_a, clip_b) = (clip_a.as_dvec4(), clip_b.as_dvec4());
    let Some((low, high)) = clip_segment(clip_a, clip_b) else {
        return Ok(None);
    };
    let at = |t: f64| clip_a + (clip_b - clip_a) * t;
    let (start, end) = (at(low), at(high));
    if start.w <= 0.0 || end.w <= 0.0 {
        return Ok(None);
    }
    let size = frame.size.as_dvec2();
    let pixel = |clip: DVec4| DVec2::new(clip.x / clip.w + 1.0, 1.0 - clip.y / clip.w) * 0.5 * size;
    let (from, to) = (pixel(start), pixel(end));
    let pointer = frame.pointer.as_dvec2();
    let along = to - from;
    let length_squared = along.length_squared();
    let mut s = if length_squared > 0.0 {
        ((pointer - from).dot(along) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if !s.is_finite() {
        s = 0.0;
    }
    let pixels = (from + along * s - pointer).length();
    if !pixels.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    if pixels > f64::from(EDGE_CLICK_PIXELS) {
        return Ok(None);
    }
    let t = low + (high - low) * (s * start.w / (end.w + s * (start.w - end.w)));
    let clip = at(t);
    let point = a.as_dvec3() + (b.as_dvec3() - a.as_dvec3()) * t;
    let ndc = glam::DVec3::new(clip.x, clip.y, clip.z) / clip.w;
    let ndc = ndc.as_vec3();
    if !point.is_finite() || !ndc.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    Ok(Some(EdgeHit {
        point: point.as_vec3(),
        ndc,
        pixels: pixels as f32,
    }))
}

/// Refuses a pick whose occlusion work (candidates x scene triangles) exceeds the budget.
fn admit_pick_work(scene: &ViewScene, eligible: usize) -> Result<(), ViewError> {
    let mut triangles = 0usize;
    for instance in scene.instances() {
        for face in instance.faces() {
            triangles = triangles
                .checked_add(face.triangles().count())
                .ok_or(ViewError::ResourceLimit)?;
        }
    }
    face_center_work(eligible, triangles)
}

fn instance_path(instance: &ViewInstance) -> Result<Vec<NativeId>, ViewError> {
    let mut path = Vec::new();
    path.try_reserve_exact(instance.path().len())
        .map_err(|_| ViewError::ResourceLimit)?;
    path.extend_from_slice(instance.path());
    Ok(path)
}

pub fn face_center_work(eligible: usize, triangles: usize) -> Result<(), ViewError> {
    match eligible.checked_mul(triangles) {
        Some(checks) if checks <= MAX_FACE_CENTER_INTERSECTIONS => Ok(()),
        _ => Err(ViewError::ResourceLimit),
    }
}

fn component_instance<'a>(
    scene: &'a ViewScene,
    transform: NodeId,
    shape: NativeId,
    path: &[NativeId],
) -> Result<&'a ViewInstance, InvalidCameraInput> {
    scene
        .instances()
        .iter()
        .find(|instance| {
            instance.transform() == transform
                && instance.shape() == shape
                && instance.path() == path
        })
        .ok_or(InvalidCameraInput)
}

fn face_target(instance: &ViewInstance, face_index: usize) -> Result<FaceTarget, ViewError> {
    Ok(FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance_path(instance)?,
        face_index,
    })
}

fn intersect(ray: Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    // Double precision avoids a fixed world-unit epsilon rejecting very small valid triangles.
    let edge1 = b.as_dvec3() - a.as_dvec3();
    let edge2 = c.as_dvec3() - a.as_dvec3();
    let direction = ray.direction.as_dvec3();
    let p = direction.cross(edge2);
    let determinant = edge1.dot(p);
    if determinant == 0.0 {
        return None;
    }
    let t = ray.origin.as_dvec3() - a.as_dvec3();
    let u = t.dot(p) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = t.cross(edge1);
    let v = direction.dot(q) / determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = (edge2.dot(q) / determinant) as f32;
    (distance.is_finite() && distance >= 0.0).then_some(distance)
}

#[cfg(test)]
#[path = "viewport/tests.rs"]
mod tests;
