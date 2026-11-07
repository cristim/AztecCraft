//! Marquee geometry: a normalised device rectangle, the overlap test of a projected triangle
//! against it, and `Camera::pick_rect`, which applies both to a `ViewScene`.

use super::{Camera, InvalidCameraInput};
use crate::view_scene::{ViewError, ViewScene};
use glam::{Vec2, Vec4};
use std::fmt;

/// An axis-aligned rectangle in normalised device coordinates (x right, y up, both within
/// `[-1, 1]`), with `min <= max` on both axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NdcRect {
    min: Vec2,
    max: Vec2,
}

/// A rectangle corner was not finite, or lay outside the viewport where that is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidRect;

impl fmt::Display for InvalidRect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("rectangle corners must be finite and within -1 to 1 on both axes")
    }
}

impl std::error::Error for InvalidRect {}

impl NdcRect {
    /// The rectangle spanned by two opposite corners, in either order. Refuses a corner that is
    /// not finite or is outside `[-1, 1]`.
    pub fn new(a: Vec2, b: Vec2) -> Result<Self, InvalidRect> {
        if !a.is_finite()
            || !b.is_finite()
            || a.abs().max_element() > 1.0
            || b.abs().max_element() > 1.0
        {
            return Err(InvalidRect);
        }
        Ok(Self {
            min: a.min(b),
            max: a.max(b),
        })
    }

    /// Like `new`, but clamps corners to the viewport first, for a drag that ends outside it.
    /// Still refuses a corner that is not finite.
    pub fn clamped(a: Vec2, b: Vec2) -> Result<Self, InvalidRect> {
        if !a.is_finite() || !b.is_finite() {
            return Err(InvalidRect);
        }
        Self::new(
            a.clamp(Vec2::NEG_ONE, Vec2::ONE),
            b.clamp(Vec2::NEG_ONE, Vec2::ONE),
        )
    }

    pub fn min(&self) -> Vec2 {
        self.min
    }

    pub fn max(&self) -> Vec2 {
        self.max
    }

    fn corners(&self) -> [Vec2; 4] {
        [
            self.min,
            Vec2::new(self.max.x, self.min.y),
            self.max,
            Vec2::new(self.min.x, self.max.y),
        ]
    }
}

impl Camera {
    /// Indices into `scene.instances()`, in that order and without duplicates, of every instance
    /// with a triangle whose projection touches `rect`. Any overlap counts (PROVISIONAL); there is
    /// no backface culling and no occlusion test. Triangles are clipped to the depth range of
    /// `project` first.
    pub fn pick_rect(
        &self,
        scene: &ViewScene,
        rect: NdcRect,
        aspect: f32,
    ) -> Result<Vec<usize>, ViewError> {
        let camera_error = || ViewError::Camera(InvalidCameraInput);
        let matrix =
            self.projection_matrix(aspect).map_err(ViewError::Camera)? * self.view_matrix();
        if !matrix.is_finite() {
            return Err(camera_error());
        }
        let mut hits = Vec::new();
        let (mut scratch, mut clipped, mut polygon) = (Vec::new(), Vec::new(), Vec::new());
        for (index, instance) in scene.instances().iter().enumerate() {
            let clip: Vec<Vec4> = instance
                .world_vertices()
                .iter()
                .map(|vertex| matrix * vertex.extend(1.0))
                .collect();
            if clip.iter().any(|point| !point.is_finite()) {
                return Err(camera_error());
            }
            'faces: for face in instance.faces() {
                for triangle in face.triangles() {
                    let [a, b, c] = triangle.map(|corner| clip.get(corner).copied());
                    let (Some(a), Some(b), Some(c)) = (a, b, c) else {
                        return Err(ViewError::InvalidGeometry);
                    };
                    clip_triangle_depth([a, b, c], &mut scratch, &mut clipped);
                    polygon.clear();
                    for point in &clipped {
                        let ndc = Vec2::new(point.x, point.y) / point.w;
                        if !ndc.is_finite() {
                            return Err(camera_error());
                        }
                        polygon.push(ndc);
                    }
                    if polygon_overlaps_rect(&polygon, rect) {
                        hits.push(index);
                        break 'faces;
                    }
                }
            }
        }
        Ok(hits)
    }
}

/// Clips a clip-space triangle to the depth range `0 <= z <= w` that `Camera::project` accepts.
/// The convex polygon (empty when nothing is inside the range) is left in `out`; `scratch` holds
/// the intermediate polygon. Both are cleared first, so callers reuse them across triangles.
fn clip_triangle_depth(triangle: [Vec4; 3], scratch: &mut Vec<Vec4>, out: &mut Vec<Vec4>) {
    clip_against(
        &triangle,
        scratch,
        |p| p.z,
        |p| Vec4::new(p.x, p.y, 0.0, p.w),
    );
    clip_against(
        scratch,
        out,
        |p| p.w - p.z,
        |p| Vec4::new(p.x, p.y, p.w, p.w),
    );
}

/// One Sutherland-Hodgman pass into `out`, keeping the points where `distance` is not negative.
/// `snap` puts a computed boundary point exactly on the plane.
fn clip_against(
    polygon: &[Vec4],
    out: &mut Vec<Vec4>,
    distance: impl Fn(Vec4) -> f32,
    snap: impl Fn(Vec4) -> Vec4,
) {
    out.clear();
    for (index, &current) in polygon.iter().enumerate() {
        let previous = polygon[(index + polygon.len() - 1) % polygon.len()];
        let (d_current, d_previous) = (distance(current), distance(previous));
        if (d_current >= 0.0) != (d_previous >= 0.0) {
            let t = d_previous / (d_previous - d_current);
            out.push(snap(previous + (current - previous) * t));
        }
        if d_current >= 0.0 {
            out.push(current);
        }
    }
}

/// Whether a convex polygon in NDC and the closed rectangle share a point (touching counts).
/// A polygon with fewer than three vertices or zero area never overlaps.
pub(super) fn polygon_overlaps_rect(polygon: &[Vec2], rect: NdcRect) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut twice_area = 0.0_f32;
    let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
    for (index, &p) in polygon.iter().enumerate() {
        let next = polygon[(index + 1) % polygon.len()];
        twice_area += p.x * next.y - next.x * p.y;
        lo = lo.min(p);
        hi = hi.max(p);
    }
    if twice_area == 0.0 {
        return false;
    }
    if hi.x < rect.min.x || lo.x > rect.max.x || hi.y < rect.min.y || lo.y > rect.max.y {
        return false;
    }
    // Separating axes along each polygon edge's outward normal; the rectangle's own axes were
    // tested above.
    let outward = twice_area.signum();
    let corners = rect.corners();
    for (index, &a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let edge = b - a;
        let normal = Vec2::new(edge.y, -edge.x) * outward;
        let polygon_max = normal.dot(a);
        if corners.iter().all(|c| normal.dot(*c) > polygon_max) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests;
