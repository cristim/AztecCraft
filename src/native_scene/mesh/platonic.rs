use super::{NativeMesh, axis::orient};
use crate::primitive_options::{Axis, PlatonicSolidKind};

/// Golden ratio; the icosahedron and dodecahedron coordinates are built from it.
fn phi() -> f64 {
    (1.0 + 5.0_f64.sqrt()) / 2.0
}

/// Vertices on the unit sphere and one outward direction per face. A face is the set of vertices
/// farthest along its direction, so no face table is typed in by hand.
struct Solid {
    vertices: Vec<[f64; 3]>,
    face_normals: Vec<[f64; 3]>,
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let len = dot(v, v).sqrt();
    v.map(|c| c / len)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn signs() -> impl Iterator<Item = [f64; 3]> {
    (0..8).map(|i| [0, 1, 2].map(|b| if i >> b & 1 == 0 { 1.0 } else { -1.0 }))
}

/// `(0, ±a, ±b)` and its two cyclic shifts: the 12 icosahedron vertices for `(phi, 1)`, the
/// orientation whose vertices are the face directions of `dodecahedron_vertices`.
fn cyclic_pairs(a: f64, b: f64) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    for (sa, sb) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let (x, y) = (sa * a, sb * b);
        out.push([0.0, x, y]);
        out.push([x, y, 0.0]);
        out.push([y, 0.0, x]);
    }
    out
}

fn icosahedron_vertices() -> Vec<[f64; 3]> {
    cyclic_pairs(phi(), 1.0).into_iter().map(unit).collect()
}

fn dodecahedron_vertices() -> Vec<[f64; 3]> {
    signs()
        .chain(cyclic_pairs(1.0 / phi(), phi()))
        .map(unit)
        .collect()
}

fn solid(kind: PlatonicSolidKind) -> Solid {
    let axes = || {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
            .into_iter()
            .flat_map(|a| [a, a.map(|c| -c)])
    };
    match kind {
        PlatonicSolidKind::Tetrahedron => {
            let vertices: Vec<[f64; 3]> = [
                [1.0, 1.0, 1.0],
                [1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [-1.0, -1.0, 1.0],
            ]
            .into_iter()
            .map(unit)
            .collect();
            let face_normals = vertices.iter().map(|v| v.map(|c| -c)).collect();
            Solid {
                vertices,
                face_normals,
            }
        }
        PlatonicSolidKind::Cube => Solid {
            vertices: signs().map(unit).collect(),
            face_normals: axes().collect(),
        },
        PlatonicSolidKind::Octahedron => Solid {
            vertices: axes().collect(),
            face_normals: signs().map(unit).collect(),
        },
        PlatonicSolidKind::Icosahedron => Solid {
            vertices: icosahedron_vertices(),
            face_normals: dodecahedron_vertices(),
        },
        PlatonicSolidKind::Dodecahedron => Solid {
            vertices: dodecahedron_vertices(),
            face_normals: icosahedron_vertices(),
        },
    }
}

/// Loops counter-clockwise seen from outside, each starting at its lowest vertex index.
fn face_loops(solid: &Solid) -> Vec<Vec<usize>> {
    solid
        .face_normals
        .iter()
        .map(|&n| {
            let reach = solid
                .vertices
                .iter()
                .map(|&v| dot(v, n))
                .fold(f64::MIN, f64::max);
            let mut ring: Vec<usize> = (0..solid.vertices.len())
                .filter(|&i| dot(solid.vertices[i], n) > reach - 1e-9)
                .collect();
            let centre = ring.iter().fold([0.0; 3], |s, &i| {
                std::array::from_fn(|c| s[c] + solid.vertices[i][c])
            });
            let first = solid.vertices[ring[0]];
            let u = unit(std::array::from_fn(|c| {
                first[c] - centre[c] / ring.len() as f64
            }));
            let w = cross(n, u);
            let angle = |i: usize| {
                let v = solid.vertices[i];
                dot(v, w).atan2(dot(v, u))
            };
            ring.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
            let lowest = ring
                .iter()
                .enumerate()
                .min_by_key(|(_, i)| **i)
                .map_or(0, |(k, _)| k);
            ring.rotate_left(lowest);
            ring
        })
        .collect()
}

impl NativeMesh {
    /// Platonic solid of circumradius `radius` around `axis`, built as native polygons (triangles,
    /// quads or pentagons) with outward winding and shared hard edges (Euler characteristic 2).
    /// Vertex order is the order of `solid()`'s coordinate list.
    pub(super) fn platonic_solid(kind: PlatonicSolidKind, radius: f64, axis: Axis) -> Self {
        let shape = solid(kind);
        let points = shape
            .vertices
            .iter()
            .map(|v| orient(axis, v.map(|c| (c * radius) as f32)))
            .collect();
        Self::from_loops(points, face_loops(&shape))
    }
}
