//! Region reduce (issue #462 slice 2): only a face set is collapsed, its boundary is pinned.

use super::grid_tests::{bits, loops, points};
use super::*;
use crate::primitive_options::Axis;
use std::collections::HashSet;

fn plane(n: usize) -> NativeMesh {
    NativeMesh::plane_grid([8.0, 8.0], [n, n], Axis::Y)
}

fn sphere() -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Sphere {
        radius: 1.0,
        sub_axis: 16,
        sub_height: 12,
        axis: Axis::Y,
    })
    .unwrap()
}

fn refusal(error: NativeError) -> ReduceRefusal {
    match error {
        NativeError::Reduce(reason) => reason,
        other => panic!("expected a reduce refusal, got {other:?}"),
    }
}

fn centroid(mesh: &NativeMesh, face: &[usize]) -> [f32; 3] {
    let p = points(mesh);
    let mut sum = [0.0; 3];
    for &v in face {
        for k in 0..3 {
            sum[k] += p[v][k] / face.len() as f32;
        }
    }
    sum
}

/// Indices of the faces whose centroid satisfies `inside`.
fn pick(mesh: &NativeMesh, inside: impl Fn([f32; 3]) -> bool) -> Vec<usize> {
    loops(mesh)
        .iter()
        .enumerate()
        .filter(|(_, face)| inside(centroid(mesh, face)))
        .map(|(index, _)| index)
        .collect()
}

/// A face as its corner positions, so it compares across vertex renumbering.
fn corner_bits(mesh: &NativeMesh, face: &[usize]) -> Vec<[u32; 3]> {
    let p = bits(mesh);
    face.iter().map(|&v| p[v]).collect()
}

fn faces_at(mesh: &NativeMesh, indices: impl Iterator<Item = usize>) -> Vec<Vec<[u32; 3]>> {
    let all = loops(mesh);
    indices.map(|i| corner_bits(mesh, &all[i])).collect()
}

/// V - E + F over the given faces only.
fn region_euler(mesh: &NativeMesh, faces: &[usize]) -> i64 {
    let all = loops(mesh);
    let mut vertices = HashSet::new();
    let mut edges = HashSet::new();
    for &f in faces {
        let face = &all[f];
        for k in 0..face.len() {
            let (a, b) = (face[k], face[(k + 1) % face.len()]);
            vertices.insert(a);
            edges.insert((a.min(b), a.max(b)));
        }
    }
    vertices.len() as i64 - edges.len() as i64 + faces.len() as i64
}

fn middle(c: [f32; 3]) -> bool {
    c[0].abs() <= 2.0 && c[2].abs() <= 2.0
}

#[test]
fn a_region_reduces_its_own_vertices_and_leaves_every_other_face_alone() {
    let before = plane(8);
    let region = pick(&before, middle);
    assert_eq!(region.len(), 16);
    let after = before
        .reduced(ReduceTarget::Percent(30.0), Some(&region))
        .unwrap();
    // 25 region vertices, 30 percent removes 7 (rounded down); the other 40 are untouched.
    assert_eq!(after.point_count(), before.point_count() - 7);
    let outside = |m: &NativeMesh| {
        let faces = pick(m, |c| !middle(c));
        faces_at(m, faces.into_iter())
    };
    assert_eq!(outside(&after), outside(&before), "an outside face changed");
    assert!(
        faces_at(&after, pick(&after, |c| !middle(c)).into_iter())
            .iter()
            .all(|f| f.len() == 4),
        "an outside quad was triangulated"
    );
    let inside = pick(&after, middle);
    assert!(inside.iter().all(|&f| loops(&after)[f].len() == 3));
    assert!(inside.len() < region.len() * 2);
}

#[test]
fn the_region_boundary_is_pinned_and_the_region_keeps_its_topology() {
    let before = plane(8);
    let region = pick(&before, middle);
    let boundary: HashSet<[u32; 3]> = bits(&before)
        .into_iter()
        .zip(points(&before))
        .filter(|(_, p)| (p[0].abs() == 2.0 || p[2].abs() == 2.0) && middle([p[0], 0.0, p[2]]))
        .map(|(b, _)| b)
        .collect();
    assert_eq!(boundary.len(), 16);
    let after = before
        .reduced(ReduceTarget::Percent(30.0), Some(&region))
        .unwrap();
    let now: HashSet<[u32; 3]> = bits(&after).into_iter().collect();
    assert!(
        boundary.is_subset(&now),
        "a boundary vertex moved or vanished"
    );
    let inside = pick(&after, middle);
    assert_eq!(region_euler(&before, &region), 1);
    assert_eq!(region_euler(&after, &inside), 1);
    let p = points(&after);
    for &f in &inside {
        let tri = &loops(&after)[f];
        let at = |i: usize| glam::DVec3::from_array(p[tri[i]].map(f64::from));
        let n = (at(1) - at(0)).cross(at(2) - at(0));
        assert!(
            n.y.abs() > 1e-9 && n.y.signum() == before_sign(&before),
            "a triangle flipped"
        );
    }
}

fn before_sign(mesh: &NativeMesh) -> f64 {
    let p = points(mesh);
    let tri = &loops(mesh)[0];
    let at = |i: usize| glam::DVec3::from_array(p[tri[i]].map(f64::from));
    (at(1) - at(0)).cross(at(2) - at(0)).y.signum()
}

#[test]
fn a_hemisphere_region_keeps_its_disk_topology_and_the_rest_of_the_sphere() {
    let before = sphere();
    let region = pick(&before, |c| c[1] > 0.1);
    let after = before
        .reduced(ReduceTarget::Vertices(30), Some(&region))
        .unwrap();
    let inside = |c: [f32; 3]| c[1] > 0.1;
    assert_eq!(region_euler(&before, &region), 1);
    assert_eq!(region_euler(&after, &pick(&after, inside)), 1);
    let rest = |m: &NativeMesh| faces_at(m, pick(m, |c| !inside(c)).into_iter());
    assert_eq!(rest(&after), rest(&before));
    // The whole mesh stays a closed sphere: V - E + F = 2.
    assert_eq!(super::grid_tests::euler(&after), 2);
}

#[test]
fn counts_target_the_region_and_the_same_input_gives_the_same_bits() {
    let before = plane(8);
    let region = pick(&before, middle);
    let run = |target| before.reduced(target, Some(&region)).unwrap();
    // The region holds 25 vertices (9 of them inside), so 20 is 5 collapses.
    assert_eq!(
        run(ReduceTarget::Vertices(20)).point_count(),
        before.point_count() - 5
    );
    let by_triangles = run(ReduceTarget::Triangles(20));
    assert!(pick(&by_triangles, middle).len() <= 20);
    let [a, b] = [(); 2].map(|_| run(ReduceTarget::Percent(30.0)));
    assert_eq!(bits(&a), bits(&b));
    assert_eq!(loops(&a), loops(&b));
}

#[test]
fn the_whole_mesh_as_a_region_is_the_plain_reduce() {
    let before = sphere();
    let all: Vec<usize> = (0..before.faces.len()).collect();
    let plain = before.reduced(ReduceTarget::Percent(50.0), None).unwrap();
    let region = before
        .reduced(ReduceTarget::Percent(50.0), Some(&all))
        .unwrap();
    assert_eq!(bits(&plain), bits(&region));
    assert_eq!(loops(&plain), loops(&region));
}

#[test]
fn bad_regions_and_unreachable_targets_are_refused_with_the_mesh_untouched() {
    let before = plane(8);
    let copy = before.clone();
    let target = ReduceTarget::Percent(30.0);
    let check = |region: &[usize], expected: ReduceRefusal| {
        assert_eq!(
            refusal(before.reduced(target, Some(region)).unwrap_err()),
            expected
        );
        assert!(before == copy);
    };
    check(&[], ReduceRefusal::EmptyRegion);
    check(&[0, 64], ReduceRefusal::FaceOutOfRange { face: 64 });
    check(
        &[0, usize::MAX],
        ReduceRefusal::FaceOutOfRange { face: usize::MAX },
    );
    check(&[3, 5, 3], ReduceRefusal::DuplicateFace { face: 3 });
    // One quad: all four vertices touch other faces, so nothing may collapse.
    check(
        &[27],
        ReduceRefusal::Unreachable {
            triangles: 2,
            vertices: 4,
        },
    );
    // Four middle faces have a single interior vertex; 9 vertices cannot drop below 8.
    let four = pick(&before, |c| c[0].abs() <= 0.5 && c[2].abs() <= 0.5);
    assert_eq!(four.len(), 4);
    assert_eq!(
        refusal(
            before
                .reduced(ReduceTarget::Vertices(5), Some(&four))
                .unwrap_err()
        ),
        ReduceRefusal::Unreachable {
            triangles: 6,
            vertices: 8
        }
    );
    assert_eq!(
        refusal(
            before
                .reduced(ReduceTarget::Percent(10.0), Some(&four))
                .unwrap_err()
        ),
        ReduceRefusal::NothingToReduce
    );
}
