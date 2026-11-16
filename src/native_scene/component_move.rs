//! The frame a component Move (#41) turns a world drag into mesh-local point edits with: the
//! instance's world matrix, its linear part's inverse, and the handle directions and centre.

use super::move_translation::{MAX_PARENT_CONDITION, infinity_norm};
use super::*;
use std::collections::BTreeSet;

/// Most vertices one component Move edits. Bounded by the 64 KiB MCP ingress line: a request of
/// this many indices (up to about 6 bytes each) still fits with room for the envelope.
pub const MAX_MOVED_VERTICES: usize = 8192;

/// An instance's world matrix `W` (mesh-local centimetres to world centimetres) and the inverse of
/// its linear part `L`. A world displacement `v` moves a mesh point by `L^-1 v`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComponentMoveFrame {
    world: DMat4,
    inverse_linear: DMat3,
}

impl ComponentMoveFrame {
    /// `None` when `world` is not finite, or its linear part is singular or too ill-conditioned
    /// to invert accurately.
    fn new(world: DMat4) -> Option<Self> {
        let linear = DMat3::from_mat4(world);
        let determinant = linear.determinant();
        if !world.is_finite() || !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        let inverse_linear = linear.inverse();
        let condition = infinity_norm(linear) * infinity_norm(inverse_linear);
        (inverse_linear.is_finite() && condition.is_finite() && condition <= MAX_PARENT_CONDITION)
            .then_some(Self {
                world,
                inverse_linear,
            })
    }

    /// The mesh-local delta, in centimetres, that displaces every point by `world_vector_cm` in the
    /// world. Fails with `NonFinite` when the vector or the result is not finite.
    pub fn local_delta_cm(&self, world_vector_cm: DVec3) -> Result<[f64; 3], NativeError> {
        let local = self.inverse_linear * world_vector_cm;
        if !world_vector_cm.is_finite() || !local.is_finite() {
            return Err(NativeError::NonFinite);
        }
        Ok(local.to_array())
    }

    /// The world position of a mesh-local point.
    pub fn world_point_cm(&self, local_cm: [f64; 3]) -> DVec3 {
        self.world.transform_point3(DVec3::from_array(local_cm))
    }

    /// The centre of the world-space bounding box of `points_cm` (mesh-local centimetres), the
    /// Move handle origin. PROVISIONAL: Maya's docs say only "the centre of the selected components".
    pub fn bbox_center_world_cm(&self, points_cm: &[[f64; 3]]) -> Result<DVec3, NativeError> {
        if points_cm.is_empty() {
            return Err(NativeError::InvalidTopology);
        }
        let (min, max) = points_cm.iter().fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(min, max), point| {
                let world = self.world_point_cm(*point);
                (min.min(world), max.max(world))
            },
        );
        let centre = (min + max) * 0.5;
        if !centre.is_finite() {
            return Err(NativeError::NonFinite);
        }
        Ok(centre)
    }
}

impl NativeScene {
    /// A vector in the scene's declared linear unit as centimetres, the one conversion every
    /// declared-unit caller of a component move shares. The result is not checked for finiteness.
    pub fn declared_vector_to_cm(&self, declared: [f64; 3]) -> Result<[f64; 3], NativeError> {
        let factor = self.units.linear_factor()?;
        Ok(declared.map(|v| v * factor))
    }

    /// The frame of the instance at `target`. Refuses a singular, non-finite or ill-conditioned
    /// world matrix with the parent-transform reason the Move handles use.
    pub fn component_move_frame(
        &self,
        target: &TransformTarget,
    ) -> Result<ComponentMoveFrame, NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        self.resolve_transform_path(target.node, &target.path)?;
        ComponentMoveFrame::new(self.path_world(&target.path)?).ok_or(NativeError::UnsupportedMove(
            WorldMoveUnsupported::ParentTransform,
        ))
    }

    /// The unit world directions of the node's own X, Y and Z (Move Axis Orientation Object), from
    /// `manipulator_frame` with its refusals. A zero-length axis refuses.
    pub fn component_move_object_axes(
        &self,
        target: &TransformTarget,
    ) -> Result<[DVec3; 3], NativeError> {
        let axes = self.manipulator_frame(target)?.object_axes;
        let mut unit = [DVec3::ZERO; 3];
        for (out, axis) in unit.iter_mut().zip(axes) {
            let length = axis.length();
            *out = axis / length;
            if !length.is_finite() || length == 0.0 || !out.is_finite() {
                return Err(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::ParentTransform,
                ));
            }
        }
        Ok(unit)
    }
}

/// The stored points of one shape's moved vertices, captured before a drag. A preview writes
/// `baseline + delta` from it every time, so the last preview and the commit agree bit for bit.
#[derive(Clone, Debug, PartialEq)]
pub struct VertexBaseline {
    shape: NativeId,
    vertices: Vec<usize>,
    points: Points,
}

impl VertexBaseline {
    pub fn shape(&self) -> NativeId {
        self.shape
    }
    pub fn vertices(&self) -> &[usize] {
        &self.vertices
    }
}

impl NativeScene {
    /// The unique vertices of `edges` (edge indices into the shape's mesh), sorted. An out-of-range
    /// edge refuses, and so does a result over `MAX_MOVED_VERTICES`.
    pub fn vertices_of_edges(
        &self,
        shape: NativeId,
        edges: &[usize],
    ) -> Result<Vec<usize>, NativeError> {
        let mesh = self.shape(shape)?.mesh();
        let mut vertices = BTreeSet::new();
        for edge in edges {
            let edge = mesh
                .edges()
                .get(*edge)
                .ok_or(NativeError::InvalidTopology)?;
            vertices.extend(edge.endpoints());
            if vertices.len() > MAX_MOVED_VERTICES {
                return Err(NativeError::ResourceLimit);
            }
        }
        Ok(vertices.into_iter().collect())
    }

    /// The unique vertices of every loop (outer and holes) of `faces`, sorted. An out-of-range face
    /// refuses, and so does a result over `MAX_MOVED_VERTICES`.
    pub fn vertices_of_faces(
        &self,
        shape: NativeId,
        faces: &[usize],
    ) -> Result<Vec<usize>, NativeError> {
        let mesh = self.shape(shape)?.mesh();
        let mut vertices = BTreeSet::new();
        for face in faces {
            let face = mesh
                .faces()
                .get(*face)
                .ok_or(NativeError::InvalidTopology)?;
            for directed in face.outer().iter().chain(face.holes().iter().flatten()) {
                let edge = mesh
                    .edges()
                    .get(directed.edge)
                    .ok_or(NativeError::InvalidTopology)?;
                vertices.extend(edge.endpoints());
            }
            if vertices.len() > MAX_MOVED_VERTICES {
                return Err(NativeError::ResourceLimit);
            }
        }
        Ok(vertices.into_iter().collect())
    }

    /// Why moving `vertices` of `shape` would be refused, without changing anything; the position
    /// of the shape on success. Source-bound scenes, shapes with effects or per-point offsets,
    /// and missing or duplicate vertices refuse.
    pub(super) fn check_vertex_move(
        &self,
        shape: NativeId,
        vertices: &[usize],
    ) -> Result<usize, NativeError> {
        self.structural_edit()?;
        let index = self
            .shapes
            .iter()
            .position(|candidate| candidate.id == shape)
            .ok_or(NativeError::MissingShape(shape))?;
        self.check_effects(shape)?;
        self.shapes[index].mesh.check_points(vertices)?;
        Ok(index)
    }

    /// `check_vertex_move` as a public, non-mutating question.
    pub fn vertex_move_refusal(&self, shape: NativeId, vertices: &[usize]) -> Option<NativeError> {
        self.check_vertex_move(shape, vertices).err()
    }

    /// Captures the stored points of `vertices` for `preview_vertices` and `restore_vertices`.
    pub fn capture_vertices(
        &self,
        shape: NativeId,
        vertices: &[usize],
    ) -> Result<VertexBaseline, NativeError> {
        let index = self.check_vertex_move(shape, vertices)?;
        Ok(VertexBaseline {
            shape,
            vertices: vertices.to_vec(),
            points: self.shapes[index].mesh.capture_points(vertices)?,
        })
    }

    /// Sets the baseline vertices to their baseline plus `delta_cm` (centimetres, mesh-local). Not
    /// cumulative. Unchanged on error.
    pub fn preview_vertices(
        &mut self,
        baseline: &VertexBaseline,
        delta_cm: [f64; 3],
    ) -> Result<(), NativeError> {
        let shape = self
            .shapes
            .iter_mut()
            .find(|shape| shape.id == baseline.shape)
            .ok_or(NativeError::MissingShape(baseline.shape))?;
        shape
            .mesh
            .displace_from(&baseline.vertices, &baseline.points, delta_cm)
    }

    /// Whether any baseline vertex now differs from its captured bytes.
    pub fn vertices_differ_from(&self, baseline: &VertexBaseline) -> Result<bool, NativeError> {
        let current = self
            .shape(baseline.shape)?
            .mesh
            .capture_points(&baseline.vertices)?;
        Ok(current != baseline.points)
    }

    /// Puts the baseline vertices back exactly as captured.
    pub fn restore_vertices(&mut self, baseline: &VertexBaseline) -> Result<(), NativeError> {
        self.preview_vertices(baseline, [0.0; 3])
    }
}

#[cfg(test)]
mod tests;
