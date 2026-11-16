//! Mesh > Reduce core (issue #462, Maya `polyReduce`): quadric-error edge-collapse decimation.
//!
//! The Maya 2027 page states no algorithm, so the choices below are PROVISIONAL:
//! - every face is triangulated first (a quad by its shorter usable diagonal, a larger polygon by
//!   a fan from the first corner that keeps every triangle on the face's side);
//! - collapses are taken cheapest first by the summed area-weighted plane quadrics of the two
//!   endpoints, ties broken by vertex index, so the result is deterministic;
//! - an interior edge collapses to the endpoint or midpoint of least error; an edge with one
//!   border endpoint collapses onto that endpoint, and an edge with two border endpoints never
//!   collapses, so every border vertex keeps its place (Maya's "keep borders", always on);
//! - a collapse is refused when it breaks the link condition (manifold topology), flips or
//!   collapses a surviving triangle, or would leave fewer triangles than a closed mesh needs.
//!
//! A target the collapses cannot reach is refused, not approximated.

mod engine;
use super::{NativeMesh, Points};
use crate::native_scene::{NativeError, NativeId, NativeScene};
#[cfg(test)]
pub(super) use engine::triangulate;
use engine::{Reducer, triangulate as split_faces};
use glam::DVec3;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Most triangles one reduce accepts (PROVISIONAL). Each collapse rescans vertex stars and sets, so
/// the cost grows with the mesh; a larger one is refused with `ReduceRefusal::TooLarge`.
pub const MAX_REDUCE_TRIANGLES: usize = 200_000;

/// How far to reduce: by a share of the vertices, or to a triangle or vertex count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReduceTarget {
    /// Remove this percentage (above 0, below 100) of the original vertices, as the `polyReduce`
    /// `-percentage` flag says. The count removed is rounded down, so never more than the
    /// percentage: 30% of 81 vertices removes 24 and keeps 57. A percentage that removes no
    /// whole vertex is `NothingToReduce`.
    Percent(f64),
    /// Stop at or below this many triangles (a collapse removes one or two).
    Triangles(usize),
    /// Stop at this many vertices (a collapse removes exactly one).
    Vertices(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReduceRefusal {
    /// The percentage is not above 0 and below 100, or a count is below the least a mesh holds.
    InvalidTarget,
    /// The target does not ask for fewer triangles or vertices than the mesh has.
    NothingToReduce,
    /// The mesh is not an oriented manifold (an edge in three faces, a pinched vertex, or
    /// inconsistent winding).
    NotManifold,
    /// The mesh has vertices or edges no face uses.
    LooseGeometry,
    /// A polygon cannot be split into triangles that all face its own way.
    NotTriangulable,
    /// The mesh has more than `MAX_REDUCE_TRIANGLES` triangles once triangulated.
    TooLarge,
    /// The collapses ran out before the target; `triangles` and `vertices` are the best reachable.
    Unreachable { triangles: usize, vertices: usize },
}

impl fmt::Display for ReduceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTarget => f.write_str(
                "reduce needs a percentage above 0 and below 100, or a count of at least 1 triangle or 3 vertices",
            ),
            Self::NothingToReduce => {
                f.write_str("the target does not remove any triangle or vertex")
            }
            Self::NotManifold => f.write_str("reduce needs an oriented manifold mesh"),
            Self::LooseGeometry => f.write_str("reduce refuses vertices or edges that no face uses"),
            Self::NotTriangulable => {
                f.write_str("a polygon cannot be triangulated without flipping a triangle")
            }
            Self::TooLarge => write!(
                f,
                "reduce accepts at most {MAX_REDUCE_TRIANGLES} triangles"
            ),
            Self::Unreachable {
                triangles,
                vertices,
            } => write!(
                f,
                "the target is out of reach: borders and manifold limits stop at {triangles} triangles and {vertices} vertices"
            ),
        }
    }
}

impl From<ReduceRefusal> for NativeError {
    fn from(value: ReduceRefusal) -> Self {
        Self::Reduce(value)
    }
}

/// Refuses a triangle count above `MAX_REDUCE_TRIANGLES`.
pub(in crate::native_scene::mesh) fn check_size(triangles: usize) -> Result<(), ReduceRefusal> {
    if triangles > MAX_REDUCE_TRIANGLES {
        return Err(ReduceRefusal::TooLarge);
    }
    Ok(())
}

impl NativeMesh {
    /// Edge-collapse decimation to `target`; the mesh is unchanged on error. The result is all
    /// triangles with hard edges, and the vertex order of the survivors is kept.
    pub(in crate::native_scene) fn reduced(
        &self,
        target: ReduceTarget,
    ) -> Result<Self, NativeError> {
        if !self.unsupported_channels.is_empty()
            || !self.point_offsets.is_empty()
            || self.faces.iter().any(|f| !f.holes.is_empty())
            || self.edges.iter().any(|e| e.smooth != Some(false))
        {
            return Err(NativeError::UnsupportedEdit);
        }
        let used: BTreeSet<usize> = self
            .faces
            .iter()
            .flat_map(|f| &f.outer)
            .map(|e| e.edge)
            .collect();
        if used.len() != self.edges.len() {
            return Err(ReduceRefusal::LooseGeometry.into());
        }
        let triangles: usize = self
            .faces
            .iter()
            .map(|f| f.outer.len().saturating_sub(2))
            .sum();
        check_size(triangles)?;
        let positions = self.points.values();
        let mut tris = Vec::new();
        for face in &self.faces {
            let corners = self.loop_vertices(&face.outer)?;
            tris.extend(split_faces(&corners, &positions).ok_or(ReduceRefusal::NotTriangulable)?);
        }
        let mut reducer = Reducer::new(tris, positions)?;
        let vertices = reducer.live_verts;
        let (max_triangles, max_vertices) = match target {
            ReduceTarget::Percent(p) if p > 0.0 && p < 100.0 => (
                usize::MAX,
                vertices - (vertices as f64 * p / 100.0).floor() as usize,
            ),
            ReduceTarget::Triangles(n) if n >= 1 => (n, usize::MAX),
            ReduceTarget::Vertices(n) if n >= 3 => (usize::MAX, n),
            _ => return Err(ReduceRefusal::InvalidTarget.into()),
        };
        let reached = |r: &Reducer| r.live_tris <= max_triangles && r.live_verts <= max_vertices;
        if reached(&reducer) {
            return Err(ReduceRefusal::NothingToReduce.into());
        }
        if !reducer.run(reached) {
            return Err(ReduceRefusal::Unreachable {
                triangles: reducer.live_tris,
                vertices: reducer.live_verts,
            }
            .into());
        }
        self.rebuilt(&reducer)
    }

    fn rebuilt(&self, reducer: &Reducer) -> Result<Self, NativeError> {
        let mut renumber = BTreeMap::new();
        let live: Vec<[usize; 3]> = reducer
            .tris
            .iter()
            .zip(&reducer.tri_alive)
            .filter(|(_, alive)| **alive)
            .map(|(t, _)| *t)
            .collect();
        for v in live.iter().flatten() {
            renumber.entry(*v).or_insert(0);
        }
        for (new, slot) in renumber.values_mut().enumerate() {
            *slot = new;
        }
        let points: Vec<DVec3> = renumber.keys().map(|v| reducer.pos[*v]).collect();
        let loops = live
            .iter()
            .map(|t| t.iter().map(|v| renumber[v]).collect())
            .collect();
        let mut mesh = Self::from_loops(vec![[0.0; 3]; points.len()], loops);
        mesh.points = match self.points {
            Points::Float3(_) => {
                let narrowed: Vec<[f32; 3]> =
                    points.iter().map(|p| p.as_vec3().to_array()).collect();
                if narrowed.iter().flatten().any(|v| !v.is_finite()) {
                    return Err(NativeError::NonFinite);
                }
                Points::Float3(narrowed)
            }
            Points::Double3(_) => Points::Double3(points.iter().map(|p| p.to_array()).collect()),
        };
        mesh.validate()?;
        Ok(mesh)
    }
}

impl NativeScene {
    /// Reduces the shape's mesh to `target` as one edit; every instance shares it. Authored scenes
    /// only, and a shape with UVs refuses because UVs do not follow topology edits yet.
    pub fn reduce_mesh(&mut self, id: NativeId, target: ReduceTarget) -> Result<(), NativeError> {
        self.structural_edit()?;
        let index = self
            .shapes
            .iter()
            .position(|shape| shape.id == id)
            .ok_or(NativeError::MissingShape(id))?;
        self.check_effects(id)?;
        if self.uv_set(id).is_some() {
            return Err(NativeError::UvsBlockTopologyEdit);
        }
        let mesh = self.shapes[index].mesh.reduced(target)?;
        self.shapes[index].mesh = mesh;
        Ok(())
    }
}
