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

#![allow(dead_code, reason = "wired by NativeMesh::reduced in the next commit")]

mod engine;

/// How far to reduce: by a share of the triangles, or to a triangle or vertex count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReduceTarget {
    /// Remove this percentage (above 0, below 100) of the triangles, rounded to leave at least
    /// that many removed.
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
    /// The collapses ran out before the target; `triangles` and `vertices` are the best reachable.
    Unreachable { triangles: usize, vertices: usize },
}
