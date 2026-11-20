use super::grid_tests::{
    assert_closed_manifold, assert_outward, euler, face_area_vector, loops, points,
};
use super::*;
use crate::primitive_options::{Axis, PlatonicSolidKind, PrimitiveOptionsError};
use std::collections::HashSet;

/// (kind, vertices, edges, faces, sides per face)
const SOLIDS: [(PlatonicSolidKind, usize, usize, usize, usize); 5] = [
    (PlatonicSolidKind::Dodecahedron, 20, 30, 12, 5),
    (PlatonicSolidKind::Icosahedron, 12, 30, 20, 3),
    (PlatonicSolidKind::Octahedron, 6, 12, 8, 3),
    (PlatonicSolidKind::Tetrahedron, 4, 6, 4, 3),
    (PlatonicSolidKind::Cube, 8, 12, 6, 4),
];

fn solid(solid: PlatonicSolidKind, radius: f64, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::PlatonicSolid {
        solid,
        radius,
        axis,
    })
    .unwrap()
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f64 {
    (0..3)
        .map(|c| (f64::from(a[c]) - f64::from(b[c])).powi(2))
        .sum::<f64>()
        .sqrt()
}

#[test]
fn every_solid_has_its_counts_and_euler_characteristic_two() {
    for (kind, v, e, f, sides) in SOLIDS {
        let mesh = solid(kind, 1.0, Axis::Y);
        assert_eq!(
            (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
            (v, e, f),
            "{kind:?}"
        );
        assert_eq!(euler(&mesh), 2, "{kind:?}");
        assert_eq!(kind.face_count(), f as u64, "{kind:?}");
        assert!(
            loops(&mesh).iter().all(|face| face.len() == sides),
            "{kind:?}"
        );
        assert_closed_manifold(&mesh);
    }
}

#[test]
fn every_vertex_lies_at_the_radius_on_every_axis() {
    for (kind, ..) in SOLIDS {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for radius in [0.25, 1.0, 7.5] {
                for p in points(&solid(kind, radius, axis)) {
                    let r = dist(p, [0.0; 3]);
                    assert!(
                        (r - radius).abs() < radius * 1e-6,
                        "{kind:?} {r} vs {radius}"
                    );
                }
            }
        }
    }
}

#[test]
fn faces_are_planar_regular_and_wind_outward() {
    for (kind, _, _, _, _) in SOLIDS {
        let mesh = solid(kind, 2.0, Axis::Y);
        assert_outward(&mesh);
        let p = points(&mesh);
        let mut edge_lengths = Vec::new();
        for face in loops(&mesh) {
            let area = face_area_vector(&mesh, &face);
            let len = area.iter().map(|c| c * c).sum::<f64>().sqrt();
            let n = area.map(|c| c / len);
            let offsets: Vec<f64> = face
                .iter()
                .map(|&i| (0..3).map(|c| n[c] * f64::from(p[i][c])).sum())
                .collect();
            for o in &offsets {
                assert!(
                    (o - offsets[0]).abs() < 1e-5,
                    "{kind:?} face {face:?} not planar"
                );
            }
            for k in 0..face.len() {
                edge_lengths.push(dist(p[face[k]], p[face[(k + 1) % face.len()]]));
            }
        }
        let first = edge_lengths[0];
        assert!(
            edge_lengths.iter().all(|l| (l - first).abs() < 1e-5),
            "{kind:?}"
        );
    }
}

#[test]
fn every_directed_edge_is_used_once_so_normals_are_consistent() {
    for (kind, ..) in SOLIDS {
        let mesh = solid(kind, 1.0, Axis::Y);
        let mut seen = HashSet::new();
        for face in loops(&mesh) {
            for k in 0..face.len() {
                assert!(
                    seen.insert((face[k], face[(k + 1) % face.len()])),
                    "{kind:?}"
                );
            }
        }
        for &(a, b) in &seen {
            assert!(
                seen.contains(&(b, a)),
                "{kind:?} edge {a}-{b} has no opposite"
            );
        }
    }
}

#[test]
fn every_face_lists_distinct_vertices_and_no_two_faces_match() {
    for (kind, ..) in SOLIDS {
        let mut sets = HashSet::new();
        for face in loops(&solid(kind, 1.0, Axis::Y)) {
            let mut sorted = face.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), face.len(), "{kind:?}");
            assert!(sets.insert(sorted), "{kind:?} repeats a face");
        }
    }
}

#[test]
fn the_default_is_a_radius_one_dodecahedron() {
    let mesh = NativeMesh::primitive(Primitive::PlatonicSolid).unwrap();
    assert_eq!((mesh.point_count(), mesh.faces.len()), (20, 12));
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::PlatonicSolid),
        PrimitiveOptions::PlatonicSolid {
            solid: PlatonicSolidKind::Dodecahedron,
            radius: 1.0,
            axis: Axis::Y,
        }
    );
}

#[test]
fn the_cube_has_the_expected_corners() {
    let p = points(&solid(PlatonicSolidKind::Cube, 3.0_f64.sqrt(), Axis::Y));
    for v in p {
        assert!(v.iter().all(|c| (c.abs() - 1.0).abs() < 1e-6), "{v:?}");
    }
}

#[test]
fn invalid_radius_is_rejected_before_building() {
    for radius in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e7] {
        let options = PrimitiveOptions::PlatonicSolid {
            solid: PlatonicSolidKind::Cube,
            radius,
            axis: Axis::Y,
        };
        assert_eq!(
            options.validate(Primitive::PlatonicSolid),
            Err(PrimitiveOptionsError::SizeOutOfRange { field: "radius" })
        );
    }
}
