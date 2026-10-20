use super::grid_tests::{assert_closed_manifold, assert_outward, bits, euler, extent, points};
use super::*;
use crate::primitive_options::Axis;
use std::f64::consts::PI;

fn cone(size: [f64; 2], n: [u32; 2], axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Cone {
        radius: size[0],
        height: size[1],
        sub_axis: n[0],
        sub_height: n[1],
        axis,
    })
    .unwrap()
}

/// The 20-column radius-1 height-2 cone exactly as it was built before options existed.
fn frozen_cone() -> NativeMesh {
    const RADIUS: f64 = 1.0;
    const HALF_HEIGHT: f64 = 1.0;
    const AXIS_DIVISIONS: usize = 20;
    let mut points = vec![[0.0, HALF_HEIGHT as f32, 0.0]];
    for column in 0..AXIS_DIVISIONS {
        let azimuth = 2.0 * PI * column as f64 / AXIS_DIVISIONS as f64;
        points.push([
            (RADIUS * azimuth.cos()) as f32,
            -HALF_HEIGHT as f32,
            (RADIUS * azimuth.sin()) as f32,
        ]);
    }
    let base = |column: usize| 1 + column % AXIS_DIVISIONS;
    let mut loops: Vec<Vec<usize>> = Vec::new();
    for column in 0..AXIS_DIVISIONS {
        loops.push(vec![0, base(column + 1), base(column)]);
    }
    loops.push((0..AXIS_DIVISIONS).map(base).collect());
    NativeMesh::from_loops(points, loops)
}

#[test]
fn default_cone_equals_the_frozen_builder_bit_for_bit() {
    let built = NativeMesh::primitive(Primitive::Cone).unwrap();
    let frozen = frozen_cone();
    assert_eq!(bits(&built), bits(&frozen));
    assert_eq!(built.edges, frozen.edges);
    assert_eq!(built.faces, frozen.faces);
}

#[test]
fn cone_counts_follow_the_closed_forms_and_euler_is_two() {
    for (sa, sh) in [(3, 1), (5, 2), (8, 3), (20, 1), (7, 4)] {
        let mesh = cone([2.0, 3.0], [sa, sh], Axis::Y);
        let (a, h) = (sa as usize, sh as usize);
        assert_eq!(mesh.point_count(), 1 + a * h);
        assert_eq!(mesh.edges.len(), 2 * a * h);
        assert_eq!(mesh.faces.len(), a * h + 1);
        assert_eq!(euler(&mesh), 2);
        assert_closed_manifold(&mesh);
    }
}

#[test]
fn cone_winds_outward_with_its_apex_on_the_positive_axis() {
    for (axis, along) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = cone([1.5, 3.0], [8, 3], axis);
        assert_outward(&mesh);
        assert_eq!(extent(&mesh, along), (-1.5, 1.5), "{axis:?} height");
        let apex = points(&mesh)[0];
        assert_eq!(apex[along], 1.5, "{axis:?} apex");
        for other in (0..3).filter(|c| *c != along) {
            assert_eq!(apex[other], 0.0);
            assert_eq!(extent(&mesh, other), (-1.5, 1.5), "{axis:?} radius");
        }
    }
}

#[test]
fn cone_rings_are_evenly_spaced_and_shrink_linearly_to_the_apex() {
    let p = points(&cone([2.0, 4.0], [4, 4], Axis::Y));
    for ring in 1..=4 {
        let first = p[1 + (ring - 1) * 4];
        assert_eq!(first[0], 0.5 * ring as f32);
        assert_eq!(first[1], 2.0 - ring as f32);
    }
}
