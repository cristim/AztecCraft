use super::grid_tests::{assert_closed_manifold, assert_outward, bits, euler, extent};
use super::*;
use crate::primitive_options::Axis;
use std::f64::consts::PI;

fn cylinder(size: [f64; 2], n: [u32; 2], axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Cylinder {
        radius: size[0],
        height: size[1],
        sub_axis: n[0],
        sub_height: n[1],
        axis,
    })
    .unwrap()
}

/// The 20x1 radius-1 height-2 cylinder exactly as it was built before options existed.
fn frozen_cylinder() -> NativeMesh {
    const RADIUS: f64 = 1.0;
    const HALF_HEIGHT: f64 = 1.0;
    const AXIS_DIVISIONS: usize = 20;
    let mut points = Vec::with_capacity(2 * AXIS_DIVISIONS);
    for y in [HALF_HEIGHT, -HALF_HEIGHT] {
        for column in 0..AXIS_DIVISIONS {
            let azimuth = 2.0 * PI * column as f64 / AXIS_DIVISIONS as f64;
            points.push([
                (RADIUS * azimuth.cos()) as f32,
                y as f32,
                (RADIUS * azimuth.sin()) as f32,
            ]);
        }
    }
    let vertex = |ring: usize, column: usize| ring * AXIS_DIVISIONS + column % AXIS_DIVISIONS;
    let mut loops: Vec<Vec<usize>> = Vec::new();
    for column in 0..AXIS_DIVISIONS {
        loops.push(vec![
            vertex(0, column),
            vertex(0, column + 1),
            vertex(1, column + 1),
            vertex(1, column),
        ]);
    }
    loops.push(
        (0..AXIS_DIVISIONS)
            .map(|column| vertex(0, AXIS_DIVISIONS - column))
            .collect(),
    );
    loops.push(
        (0..AXIS_DIVISIONS)
            .map(|column| vertex(1, column))
            .collect(),
    );
    NativeMesh::from_loops(points, loops)
}

#[test]
fn default_cylinder_equals_the_frozen_builder_bit_for_bit() {
    let built = NativeMesh::primitive(Primitive::Cylinder).unwrap();
    let frozen = frozen_cylinder();
    assert_eq!(bits(&built), bits(&frozen));
    assert_eq!(built.edges, frozen.edges);
    assert_eq!(built.faces, frozen.faces);
}

#[test]
fn cylinder_counts_follow_the_closed_forms_and_euler_is_two() {
    for (sa, sh) in [(3, 1), (5, 2), (8, 3), (20, 1), (7, 4)] {
        let mesh = cylinder([2.0, 3.0], [sa, sh], Axis::Y);
        let (a, h) = (sa as usize, sh as usize);
        assert_eq!(mesh.point_count(), a * (h + 1));
        assert_eq!(mesh.edges.len(), a * (2 * h + 1));
        assert_eq!(mesh.faces.len(), a * h + 2);
        assert_eq!(euler(&mesh), 2);
        assert_closed_manifold(&mesh);
    }
}

#[test]
fn cylinder_winds_outward_and_runs_along_the_axis() {
    for (axis, along) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = cylinder([1.5, 3.0], [8, 3], axis);
        assert_outward(&mesh);
        assert_eq!(extent(&mesh, along), (-1.5, 1.5), "{axis:?} height");
        for other in (0..3).filter(|c| *c != along) {
            assert_eq!(extent(&mesh, other), (-1.5, 1.5), "{axis:?} radius");
        }
    }
}

#[test]
fn cylinder_height_rings_are_evenly_spaced_from_the_top_down() {
    let mesh = cylinder([1.0, 4.0], [4, 4], Axis::Y);
    let super::Points::Float3(p) = &mesh.points else {
        panic!("f32 points")
    };
    let ys: Vec<f32> = (0..5).map(|r| p[r * 4][1]).collect();
    assert_eq!(ys, [2.0, 1.0, 0.0, -1.0, -2.0]);
}
