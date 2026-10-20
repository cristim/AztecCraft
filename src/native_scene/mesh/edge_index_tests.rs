use super::*;
use crate::scene::Primitive;
use glam::Vec3;
use std::time::Instant;

/// Rebuilds a mesh with the reference linear lookup from the face loops of `built`.
fn rebuild_linear(built: &NativeMesh) -> NativeMesh {
    let mut mesh = NativeMesh {
        point_offsets: Vec::new(),
        points: built.points.clone(),
        edges: Vec::new(),
        faces: Vec::new(),
        unsupported_channels: Vec::new(),
    };
    for face in &built.faces {
        let corners: Vec<usize> = face
            .outer
            .iter()
            .map(|e| built.endpoints(*e).unwrap()[0])
            .collect();
        let outer = (0..corners.len())
            .map(|i| mesh.edge_linear([corners[i], corners[(i + 1) % corners.len()]]))
            .collect();
        mesh.faces.push(NativeFace {
            outer,
            holes: Vec::new(),
        });
    }
    mesh
}

fn assert_bit_identical(built: &NativeMesh) {
    let oracle = rebuild_linear(built);
    assert_eq!(built.edges, oracle.edges);
    assert_eq!(built.faces, oracle.faces);
}

#[test]
fn indexed_edges_match_the_linear_oracle_for_all_primitives() {
    for primitive in [
        Primitive::Cube,
        Primitive::Plane,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ] {
        assert_bit_identical(&NativeMesh::primitive(primitive).unwrap());
    }
}

#[test]
fn indexed_edges_match_the_oracle_for_repeated_and_non_manifold_legacy_edges() {
    let vertices: Vec<Vec3> = (0..8).map(|i| Vec3::new(i as f32, 0.0, 0.0)).collect();
    let faces = vec![
        [0, 1, 2, 3],
        [0, 1, 4, 5], // same-direction repeat of 0->1
        [1, 0, 6, 7], // reversed 0-1
        [1, 0, 3, 2], // third face on 0-1 (non-manifold)
        [0, 1, 2, 3], // whole face duplicated
        [3, 2, 1, 0], // whole face reversed
    ];
    let legacy = Mesh::from_view(vertices, faces).unwrap();
    let built = NativeMesh::from_legacy(&legacy);
    assert_bit_identical(&built);
    assert!(
        built
            .faces
            .iter()
            .any(|f| f.outer.iter().any(|e| e.reversed))
    );
}

fn grid(n: usize) -> NativeMesh {
    let points = (0..=n)
        .flat_map(|y| (0..=n).map(move |x| [x as f32, 0.0, y as f32]))
        .collect();
    let at = |x: usize, y: usize| y * (n + 1) + x;
    let loops = (0..n)
        .flat_map(|y| (0..n).map(move |x| (x, y)))
        .map(|(x, y)| vec![at(x, y), at(x, y + 1), at(x + 1, y + 1), at(x + 1, y)])
        .collect();
    NativeMesh::from_loops(points, loops)
}

#[test]
fn largest_grid_builds_quickly() {
    let start = Instant::now();
    let mesh = grid(128);
    let elapsed = start.elapsed();
    assert_eq!(mesh.faces.len(), 128 * 128);
    assert_eq!(mesh.edges.len(), 2 * 128 * 129);
    assert!(elapsed.as_secs_f64() < 5.0, "{elapsed:?}");
}
