use super::grid_tests::{
    assert_closed_manifold, assert_outward, euler, extent, face_area_vector, loops, points,
};
use super::*;
use crate::primitive_options::Axis;

const R: f64 = 1.5;
const H: f64 = 3.0;
const COLUMNS: usize = 8;
const BANDS: usize = 2;

fn options(primitive: Primitive, sub_caps: u32, round_cap: bool, axis: Axis) -> PrimitiveOptions {
    let (sub_axis, sub_height) = (COLUMNS as u32, BANDS as u32);
    match primitive {
        Primitive::Cylinder => PrimitiveOptions::Cylinder {
            radius: R,
            height: H,
            sub_axis,
            sub_height,
            sub_caps,
            round_cap,
            axis,
        },
        _ => PrimitiveOptions::Cone {
            radius: R,
            height: H,
            sub_axis,
            sub_height,
            sub_caps,
            round_cap,
            axis,
        },
    }
}

pub(super) fn build(
    primitive: Primitive,
    sub_caps: u32,
    round_cap: bool,
    axis: Axis,
) -> NativeMesh {
    NativeMesh::with_options(&options(primitive, sub_caps, round_cap, axis)).unwrap()
}

/// Vertices, faces: cylinder `(bands+1)*columns` ring points, cone the apex and `bands*columns`.
fn expected_counts(primitive: Primitive, n: usize) -> (usize, usize) {
    let cap_points = if n == 0 { 0 } else { (n - 1) * COLUMNS + 1 };
    let cap_faces = if n == 0 { 1 } else { COLUMNS * n };
    if primitive == Primitive::Cylinder {
        (
            (BANDS + 1) * COLUMNS + 2 * cap_points,
            COLUMNS * BANDS + 2 * cap_faces,
        )
    } else {
        (
            1 + BANDS * COLUMNS + cap_points,
            COLUMNS * BANDS + cap_faces,
        )
    }
}

fn volume(mesh: &NativeMesh) -> f64 {
    let p = points(mesh);
    loops(mesh)
        .iter()
        .map(|face| {
            let n = face_area_vector(mesh, face);
            let c = p[face[0]].map(f64::from);
            (n[0] * c[0] + n[1] * c[1] + n[2] * c[2]) / 6.0
        })
        .sum()
}

#[test]
fn counts_euler_and_closure_hold_for_every_cap_subdivision() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        for n in 0..=4u32 {
            for round in [false, true] {
                let mesh = build(primitive, n, round, Axis::Y);
                let (v, f) = expected_counts(primitive, n as usize);
                assert_eq!(mesh.point_count(), v, "{primitive:?} {n} points");
                assert_eq!(mesh.faces.len(), f, "{primitive:?} {n} faces");
                assert_eq!(euler(&mesh), 2, "{primitive:?} {n} {round}");
                assert_closed_manifold(&mesh);
                let counted = options(primitive, n, round, Axis::Y).face_count();
                assert_eq!(counted, f as u64, "face_count matches the mesh");
            }
        }
    }
}

#[test]
fn caps_wind_outward_with_positive_volume_on_every_axis() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for (n, round) in [(1, false), (3, false), (1, true), (3, true)] {
                let mesh = build(primitive, n, round, axis);
                assert_outward(&mesh);
                assert!(volume(&mesh) > 0.0, "{primitive:?} {axis:?} {n} {round}");
            }
        }
    }
}

#[test]
fn a_centre_vertex_exists_only_from_one_cap_subdivision() {
    let on_axis = |m: &NativeMesh| {
        points(m)
            .iter()
            .filter(|p| p[0] == 0.0 && p[2] == 0.0)
            .count()
    };
    for n in 0..=3 {
        assert_eq!(
            on_axis(&build(Primitive::Cylinder, n, false, Axis::Y)),
            if n == 0 { 0 } else { 2 }
        );
        // The cone's apex is always on the axis.
        assert_eq!(
            on_axis(&build(Primitive::Cone, n, false, Axis::Y)),
            if n == 0 { 1 } else { 2 }
        );
    }
}

#[test]
fn round_cap_rings_follow_the_quarter_circle_and_the_bbox_grows_by_r() {
    let n = 4usize;
    let mesh = build(Primitive::Cylinder, n as u32, true, Axis::Y);
    let p = points(&mesh);
    let top = (BANDS + 1) * COLUMNS;
    let bottom = top + (n - 1) * COLUMNS + 1;
    for k in 1..n {
        let t = std::f64::consts::FRAC_PI_2 * k as f64 / n as f64;
        for (base, sign) in [(top, 1.0), (bottom, -1.0)] {
            let v = p[base + (k - 1) * COLUMNS];
            assert!((f64::from(v[0]) - R * t.cos()).abs() < 1e-6, "r_{k}");
            assert!(
                (f64::from(v[1]) - sign * (H / 2.0 + R * t.sin())).abs() < 1e-6,
                "h_{k}"
            );
        }
    }
    assert_eq!(p[top + (n - 1) * COLUMNS], [0.0, (H / 2.0 + R) as f32, 0.0]);
    assert_eq!(
        p[bottom + (n - 1) * COLUMNS],
        [0.0, -(H / 2.0 + R) as f32, 0.0]
    );
    assert_eq!(
        extent(&mesh, 1),
        (-(H / 2.0 + R) as f32, (H / 2.0 + R) as f32)
    );
    assert_eq!(extent(&mesh, 0), (-R as f32, R as f32));
    let flat = build(Primitive::Cylinder, n as u32, false, Axis::Y);
    assert_eq!(extent(&flat, 1), (-(H / 2.0) as f32, (H / 2.0) as f32));
    assert!(volume(&mesh) > volume(&flat));
    let cone = build(Primitive::Cone, n as u32, true, Axis::Y);
    assert_eq!(extent(&cone, 1), (-(H / 2.0 + R) as f32, (H / 2.0) as f32));
}

#[test]
fn flat_cap_rings_shrink_linearly_at_the_cap_plane() {
    let mesh = build(Primitive::Cone, 3, false, Axis::Y);
    let p = points(&mesh);
    let base = 1 + BANDS * COLUMNS;
    for k in 1..3 {
        let v = p[base + (k - 1) * COLUMNS];
        assert!((f64::from(v[0]) - R * (1.0 - k as f64 / 3.0)).abs() < 1e-6);
        assert_eq!(v[1], -(H / 2.0) as f32);
    }
}
