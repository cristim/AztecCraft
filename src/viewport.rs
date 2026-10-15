use crate::scene::{Node, NodeId, Scene};
use crate::{
    native_scene::{FaceTarget, NativeId},
    view_scene::{FACE_CENTER_CLICK_PIXELS, ViewError, ViewFace, ViewInstance, ViewScene},
};
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::fmt;

const MAX_FACE_CENTER_INTERSECTIONS: usize = 1_000_000;

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

impl Camera {
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
            yaw,
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
            || self.pitch.abs() >= std::f32::consts::FRAC_PI_2
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
    pub fn eye(&self) -> Vec3 {
        self.target
            + self.distance
                * Vec3::new(
                    self.yaw.sin() * self.pitch.cos(),
                    self.pitch.sin(),
                    self.yaw.cos() * self.pitch.cos(),
                )
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
        next.yaw = (next.yaw + radians.x).rem_euclid(std::f32::consts::TAU);
        let limit = std::f32::consts::FRAC_PI_2 - 0.001;
        next.pitch = (next.pitch + radians.y).clamp(-limit, limit);
        next.validate()?;
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
                .flat_map(|instance| instance.world_vertices().iter().copied()),
            aspect,
        )
    }

    pub fn frame_face(
        &mut self,
        scene: &ViewScene,
        target: &FaceTarget,
        aspect: f32,
    ) -> Result<(), InvalidCameraInput> {
        let instance = scene
            .instances()
            .iter()
            .find(|instance| {
                instance.transform() == target.transform
                    && instance.shape() == target.shape
                    && instance.path() == target.path
            })
            .ok_or(InvalidCameraInput)?;
        let face = instance
            .faces()
            .iter()
            .find(|face| face.source_index() == target.face_index)
            .ok_or(InvalidCameraInput)?;
        let mut corners = [Vec3::ZERO; 4];
        for (slot, &index) in face.corners().iter().enumerate() {
            *corners.get_mut(slot).ok_or(InvalidCameraInput)? = *instance
                .world_vertices()
                .get(index)
                .ok_or(InvalidCameraInput)?;
        }
        self.frame_points(
            corners
                .get(..face.corners().len())
                .ok_or(InvalidCameraInput)?,
            aspect,
        )
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
        let view_direction = Vec3::new(
            ndc.x / projection.x_axis.x,
            ndc.y / projection.y_axis.y,
            -1.0,
        )
        .normalize();
        let direction = self
            .view_matrix()
            .inverse()
            .transform_vector3(view_direction)
            .normalize();
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
        let [width, height] = viewport_pixels;
        if width == 0 || height == 0 || !pixels_per_point.is_finite() || pixels_per_point <= 0.0 {
            return Err(ViewError::Camera(InvalidCameraInput));
        }
        let size = Vec2::new(width as f32, height as f32);
        let pointer = pointer_points * pixels_per_point;
        if !pointer.is_finite() || pointer.min_element() < 0.0 || pointer.cmpgt(size).any() {
            return Err(ViewError::Camera(InvalidCameraInput));
        }
        let aspect = size.x / size.y;
        let matrix =
            self.projection_matrix(aspect).map_err(ViewError::Camera)? * self.view_matrix();
        let Some((owner, path)) = active else {
            return Ok(None);
        };
        let candidate = |instance: &ViewInstance, face: &ViewFace| {
            if !instance.is_direct_shape_of(owner, path) {
                return Ok(None);
            }
            let center = face.center(instance.world_vertices())?;
            let clip = matrix * center.extend(1.0);
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
            let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size;
            Ok(
                ((pixel - pointer).abs().max_element() <= FACE_CENTER_CLICK_PIXELS * 0.5)
                    .then_some((center, ndc)),
            )
        };
        let mut eligible = 0usize;
        let mut triangles = 0usize;
        for instance in scene.instances() {
            for face in instance.faces() {
                triangles = triangles
                    .checked_add(face.triangles().count())
                    .ok_or(ViewError::ResourceLimit)?;
                if candidate(instance, face)?.is_some() {
                    eligible = eligible.checked_add(1).ok_or(ViewError::ResourceLimit)?;
                }
            }
        }
        face_center_work(eligible, triangles)?;
        let mut selected: Option<(f32, &ViewInstance, usize)> = None;
        for instance in scene.instances() {
            for face in instance.faces() {
                let Some((center, ndc)) = candidate(instance, face)? else {
                    continue;
                };
                let ray = self
                    .ray(ndc.truncate(), aspect)
                    .map_err(ViewError::Camera)?;
                let mut closest = None;
                for occluder in scene.instances() {
                    let triangles = occluder.faces().iter().flat_map(|surface| {
                        surface.triangles().map(|triangle| {
                            ((), triangle.map(|index| occluder.world_vertices()[index]))
                        })
                    });
                    self.closest_triangle(ray, triangles, &mut closest);
                }
                if let Some((distance, ())) = closest {
                    let hit = matrix * (ray.origin + distance * ray.direction).extend(1.0);
                    if hit.z / hit.w < ndc.z {
                        continue;
                    }
                }
                let depth = -self.view_matrix().transform_point3(center).z;
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

pub(crate) fn face_center_work(eligible: usize, triangles: usize) -> Result<(), ViewError> {
    match eligible.checked_mul(triangles) {
        Some(checks) if checks <= MAX_FACE_CENTER_INTERSECTIONS => Ok(()),
        _ => Err(ViewError::ResourceLimit),
    }
}

fn face_target(instance: &ViewInstance, face_index: usize) -> Result<FaceTarget, ViewError> {
    let mut path = Vec::new();
    path.try_reserve_exact(instance.path().len())
        .map_err(|_| ViewError::ResourceLimit)?;
    path.extend_from_slice(instance.path());
    Ok(FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path,
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
