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
//!
//! A region reduce (a face set) runs the same engine on the whole mesh with every vertex that
//! touches a face outside the region pinned like a border: the region boundary is a border. Every
//! face is still triangulated for the engine (so a face outside the region that no fan can split,
//! the triangle cap and the manifold and loose-geometry checks apply to the whole mesh), but faces
//! outside the region are copied out as they were (polygons, not triangles); the percentage or count
//! target counts the region's own vertices and triangles (PROVISIONAL: the Maya page has no face
//! region flag, only a selection).

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
    /// A face region was given but holds no face.
    EmptyRegion,
    /// A region face index is not a face of the mesh.
    FaceOutOfRange { face: usize },
    /// A region face index appears twice.
    DuplicateFace { face: usize },
    /// The collapses ran out before the target; `triangles` and `vertices` are the best reachable (the
    /// region's own when a region was given).
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
            Self::EmptyRegion => f.write_str("the face region holds no face"),
            Self::FaceOutOfRange { face } => {
                write!(f, "region face {face} is not a face of the mesh")
            }
            Self::DuplicateFace { face } => write!(f, "region face {face} is listed twice"),
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
    /// triangles with hard edges, and the vertex order of the survivors is kept. With a `region`
    /// of face indices only those faces collapse; the rest of the mesh is pinned (see the module doc).
    pub(in crate::native_scene) fn reduced(
        &self,
        target: ReduceTarget,
        region: Option<&[usize]>,
    ) -> Result<Self, NativeError> {
        if !self.unsupported_channels.is_empty()
            || !self.point_offsets.is_empty()
            || self.faces.iter().any(|f| !f.holes.is_empty())
            || self.edges.iter().any(|e| e.smooth != Some(false))
        {
            return Err(NativeError::UnsupportedEdit);
        }
        let mut in_region = vec![region.is_none(); self.faces.len()];
        if let Some(faces) = region {
            if faces.is_empty() {
                return Err(ReduceRefusal::EmptyRegion.into());
            }
            for &face in faces {
                match in_region.get_mut(face) {
                    None => return Err(ReduceRefusal::FaceOutOfRange { face }.into()),
                    Some(seen) if *seen => return Err(ReduceRefusal::DuplicateFace { face }.into()),
                    Some(seen) => *seen = true,
                }
            }
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
        let mut face_of = Vec::new();
        let mut pinned = vec![false; positions.len()];
        let mut region_vertices = BTreeSet::new();
        for (index, face) in self.faces.iter().enumerate() {
            let corners = self.loop_vertices(&face.outer)?;
            let split = split_faces(&corners, &positions).ok_or(ReduceRefusal::NotTriangulable)?;
            face_of.extend(split.iter().map(|_| index));
            tris.extend(split);
            if in_region[index] {
                region_vertices.extend(corners);
            } else {
                for v in corners {
                    pinned[v] = true;
                }
            }
        }
        let mut reducer = Reducer::new(tris, positions, &pinned)?;
        let start_verts = reducer.live_verts;
        let outside_tris = face_of.iter().filter(|f| !in_region[**f]).count();
        let region_start_verts = region_vertices.len();
        let region_tris_now = |r: &Reducer| r.live_tris - outside_tris;
        let region_verts_now = |r: &Reducer| region_start_verts - (start_verts - r.live_verts);
        let (max_triangles, max_vertices) = match target {
            ReduceTarget::Percent(p) if p > 0.0 && p < 100.0 => (
                usize::MAX,
                region_start_verts - (region_start_verts as f64 * p / 100.0).floor() as usize,
            ),
            ReduceTarget::Triangles(n) if n >= 1 => (n, usize::MAX),
            ReduceTarget::Vertices(n) if n >= 3 => (usize::MAX, n),
            _ => return Err(ReduceRefusal::InvalidTarget.into()),
        };
        let reached = |r: &Reducer| {
            region_tris_now(r) <= max_triangles && region_verts_now(r) <= max_vertices
        };
        if reached(&reducer) {
            return Err(ReduceRefusal::NothingToReduce.into());
        }
        if !reducer.run(reached) {
            return Err(ReduceRefusal::Unreachable {
                triangles: region_tris_now(&reducer),
                vertices: region_verts_now(&reducer),
            }
            .into());
        }
        self.rebuilt(&reducer, &face_of, &in_region)
    }

    /// The reduced mesh: faces keep their order, a region face becomes its surviving triangles and
    /// a face outside the region is copied with its corners renumbered past the removed vertices.
    fn rebuilt(
        &self,
        reducer: &Reducer,
        face_of: &[usize],
        in_region: &[bool],
    ) -> Result<Self, NativeError> {
        let mut renumber = BTreeMap::new();
        let mut triangles_of: Vec<Vec<[usize; 3]>> = vec![Vec::new(); self.faces.len()];
        for (t, tri) in reducer.tris.iter().enumerate() {
            if reducer.tri_alive[t] {
                renumber.extend(tri.iter().map(|v| (*v, 0)));
                if in_region[face_of[t]] {
                    triangles_of[face_of[t]].push(*tri);
                }
            }
        }
        for (new, slot) in renumber.values_mut().enumerate() {
            *slot = new;
        }
        let points: Vec<DVec3> = renumber.keys().map(|v| reducer.pos[*v]).collect();
        let mut loops: Vec<Vec<usize>> = Vec::new();
        for (index, face) in self.faces.iter().enumerate() {
            if in_region[index] {
                loops.extend(
                    triangles_of[index]
                        .iter()
                        .map(|t| t.iter().map(|v| renumber[v]).collect::<Vec<_>>()),
                );
            } else {
                let corners = self.loop_vertices(&face.outer)?;
                loops.push(corners.iter().map(|v| renumber[v]).collect());
            }
        }
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
    /// Reduces the shape's mesh (only the `faces` region when given) to `target` as one edit; every
    /// instance shares it. Authored scenes
    /// only, and a shape with UVs refuses because UVs do not follow topology edits yet.
    pub fn reduce_mesh(
        &mut self,
        id: NativeId,
        target: ReduceTarget,
        faces: Option<&[usize]>,
    ) -> Result<(), NativeError> {
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
        let mesh = self.shapes[index].mesh.reduced(target, faces)?;
        self.shapes[index].mesh = mesh;
        Ok(())
    }
}
