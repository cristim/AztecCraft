use super::grid_tests::{bits, euler, loops, points};
use super::reduce::triangulate;
use super::*;
use crate::primitive_options::Axis;
use std::collections::{HashMap, HashSet};

fn sphere() -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Sphere {
        radius: 1.0,
        sub_axis: 16,
        sub_height: 12,
        axis: Axis::Y,
    })
    .unwrap()
}

fn plane(n: usize) -> NativeMesh {
    NativeMesh::plane_grid([8.0, 8.0], [n, n], Axis::Y)
}

fn refusal(error: NativeError) -> ReduceRefusal {
    match error {
        NativeError::Reduce(reason) => reason,
        other => panic!("expected a reduce refusal, got {other:?}"),
    }
}

fn normal(p: &[[f32; 3]], tri: &[usize]) -> glam::DVec3 {
    let at = |i: usize| glam::DVec3::from_array(p[tri[i]].map(f64::from));
    (at(1) - at(0)).cross(at(2) - at(0))
}

fn assert_valid_triangles(mesh: &NativeMesh) {
    let p = points(mesh);
    let mut directed = HashSet::new();
    let mut undirected: HashMap<[usize; 2], usize> = HashMap::new();
    let mut used = HashSet::new();
    for tri in loops(mesh) {
        assert_eq!(tri.len(), 3);
        assert!(normal(&p, &tri).length() > 1e-9);
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            assert!(directed.insert([a, b]), "directed edge repeated");
            *undirected.entry([a.min(b), a.max(b)]).or_default() += 1;
            used.insert(a);
        }
    }
    assert!(undirected.values().all(|n| *n <= 2));
    assert_eq!(used.len(), mesh.point_count());
    assert_eq!(undirected.len(), mesh.edges.len());
}

#[test]
fn percent_reduces_a_sphere_and_keeps_its_topology() {
    let before = sphere();
    let after = before.reduced(ReduceTarget::Percent(50.0)).unwrap();
    let triangles: usize = before.faces.iter().map(|f| f.outer.len() - 2).sum();
    assert!(after.faces.len() <= triangles / 2);
    assert!(after.faces.len() > triangles / 4);
    assert_valid_triangles(&after);
    assert_eq!(euler(&after), 2);
}

#[test]
fn the_same_input_gives_the_same_bits() {
    let [a, b] = [(); 2].map(|_| sphere().reduced(ReduceTarget::Percent(60.0)).unwrap());
    assert_eq!(bits(&a), bits(&b));
    assert_eq!(loops(&a), loops(&b));
}

#[test]
fn counts_stop_at_the_target() {
    let by_vertices = sphere().reduced(ReduceTarget::Vertices(40)).unwrap();
    assert_eq!(by_vertices.point_count(), 40);
    let by_triangles = sphere().reduced(ReduceTarget::Triangles(80)).unwrap();
    assert!((79..=80).contains(&by_triangles.faces.len()));
    assert_valid_triangles(&by_vertices);
    assert_valid_triangles(&by_triangles);
}

#[test]
fn borders_keep_their_vertices_and_no_triangle_flips() {
    let before = plane(8);
    let q = points(&before);
    let kept: HashSet<[u32; 3]> = q
        .iter()
        .filter(|v| v[0].abs() == 4.0 || v[2].abs() == 4.0)
        .map(|v| v.map(f32::to_bits))
        .collect();
    let reference = normal(&q, &loops(&before)[0]);
    let after = before.reduced(ReduceTarget::Percent(70.0)).unwrap();
    let now: HashSet<[u32; 3]> = bits(&after).into_iter().collect();
    assert!(kept.is_subset(&now), "a border vertex moved or vanished");
    assert_valid_triangles(&after);
    let p = points(&after);
    for tri in loops(&after) {
        assert!(normal(&p, &tri).dot(reference) > 0.0, "a triangle flipped");
    }
}

#[test]
fn a_target_the_collapses_cannot_reach_is_refused() {
    let error = refusal(plane(1).reduced(ReduceTarget::Triangles(1)).unwrap_err());
    assert_eq!(
        error,
        ReduceRefusal::Unreachable {
            triangles: 2,
            vertices: 4
        }
    );
    let cube = NativeMesh::primitive(Primitive::Cube).unwrap();
    let ReduceRefusal::Unreachable { triangles, .. } =
        refusal(cube.reduced(ReduceTarget::Triangles(1)).unwrap_err())
    else {
        panic!("expected unreachable");
    };
    assert!(triangles >= 4);
}

#[test]
fn invalid_and_empty_targets_are_refused() {
    let mesh = sphere();
    for target in [
        ReduceTarget::Percent(0.0),
        ReduceTarget::Percent(100.0),
        ReduceTarget::Percent(f64::NAN),
        ReduceTarget::Triangles(0),
        ReduceTarget::Vertices(2),
    ] {
        assert_eq!(
            refusal(mesh.reduced(target).unwrap_err()),
            ReduceRefusal::InvalidTarget,
            "{target:?}"
        );
    }
    for target in [
        ReduceTarget::Triangles(100_000),
        ReduceTarget::Percent(0.0001),
    ] {
        assert_eq!(
            refusal(mesh.reduced(target).unwrap_err()),
            ReduceRefusal::NothingToReduce,
            "{target:?}"
        );
    }
}

#[test]
fn non_manifold_loose_and_holed_meshes_are_refused() {
    let fin = NativeMesh::polygon_fixture(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        &[vec![0, 1, 2], vec![0, 1, 3], vec![0, 1, 4]],
    );
    let target = ReduceTarget::Triangles(1);
    assert_eq!(
        refusal(fin.reduced(target).unwrap_err()),
        ReduceRefusal::NotManifold
    );

    let mut loose_edge = plane(2);
    loose_edge.add_loose_edge_fixture([0, 1]);
    assert_eq!(
        refusal(loose_edge.reduced(target).unwrap_err()),
        ReduceRefusal::LooseGeometry
    );

    let mut loose_vertex = plane(2);
    if let Points::Float3(p) = &mut loose_vertex.points {
        p.push([9.0, 9.0, 9.0]);
    }
    assert_eq!(
        refusal(loose_vertex.reduced(target).unwrap_err()),
        ReduceRefusal::LooseGeometry
    );

    let mut holed = NativeMesh::polygon_fixture(
        vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, 2.0, 0.0],
        ],
        &[vec![0, 1, 2, 3]],
    );
    holed.add_hole_fixture(&[4, 6, 5]);
    assert!(matches!(
        holed.reduced(target),
        Err(NativeError::UnsupportedEdit)
    ));
}

#[test]
fn a_pinched_vertex_is_not_manifold() {
    let bowtie = NativeMesh::polygon_fixture(
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [-1.0, -1.0, 0.0],
        ],
        &[vec![0, 2, 1], vec![0, 3, 4]],
    );
    assert_eq!(
        refusal(bowtie.reduced(ReduceTarget::Triangles(1)).unwrap_err()),
        ReduceRefusal::NotManifold
    );
}

#[test]
fn a_concave_quad_splits_along_the_diagonal_that_keeps_both_triangles_facing_up() {
    let p = [
        glam::DVec3::new(0.0, 3.0, 0.0),
        glam::DVec3::new(-0.5, 0.0, 0.0),
        glam::DVec3::new(0.0, 1.0, 0.0),
        glam::DVec3::new(0.5, 0.0, 0.0),
    ];
    let fan = triangulate(&[0, 1, 2, 3], &p).unwrap();
    assert_eq!(fan, vec![[0, 1, 2], [0, 2, 3]]);
}
