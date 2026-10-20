use super::grid_tests::{bits, euler, extent, face_area_vector, loops, points};
use super::*;
use crate::primitive_options::Axis;
use std::f64::consts::PI;

fn sphere(radius: f64, sub_axis: u32, sub_height: u32, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Sphere {
        radius,
        sub_axis,
        sub_height,
        axis,
    })
    .unwrap()
}

/// The 20x20 radius-1 sphere exactly as it was built before options existed.
fn frozen_default_sphere() -> NativeMesh {
    const N: usize = 20;
    let mut points = vec![[0.0, 1.0, 0.0]];
    for ring in 1..N {
        let polar = PI * ring as f64 / N as f64;
        let y = if 2 * ring == N { 0.0 } else { polar.cos() };
        for column in 0..N {
            let azimuth = 2.0 * PI * column as f64 / N as f64;
            let radial = polar.sin();
            points.push([
                (radial * azimuth.cos()) as f32,
                y as f32,
                (radial * azimuth.sin()) as f32,
            ]);
        }
    }
    points.push([0.0, -1.0, 0.0]);
    let bottom = points.len() - 1;
    let vertex = |ring: usize, column: usize| 1 + (ring - 1) * N + column % N;
    let mut loops: Vec<Vec<usize>> = Vec::new();
    for column in 0..N {
        loops.push(vec![0, vertex(1, column + 1), vertex(1, column)]);
    }
    for ring in 1..N - 1 {
        for column in 0..N {
            loops.push(vec![
                vertex(ring, column),
                vertex(ring, column + 1),
                vertex(ring + 1, column + 1),
                vertex(ring + 1, column),
            ]);
        }
    }
    for column in 0..N {
        loops.push(vec![
            bottom,
            vertex(N - 1, column),
            vertex(N - 1, column + 1),
        ]);
    }
    NativeMesh::from_loops(points, loops)
}

#[test]
fn default_sphere_equals_the_frozen_builder_bit_for_bit() {
    let built = NativeMesh::primitive(Primitive::Sphere).unwrap();
    let frozen = frozen_default_sphere();
    assert_eq!(bits(&built), bits(&frozen));
    assert_eq!(built.edges, frozen.edges);
    assert_eq!(built.faces, frozen.faces);
}

#[test]
fn sphere_counts_follow_the_closed_forms_and_euler_is_two() {
    for (sa, sh) in [(3, 2), (5, 3), (13, 9), (8, 4), (20, 20)] {
        let mesh = sphere(2.5, sa, sh, Axis::Y);
        let (sa, sh) = (sa as usize, sh as usize);
        assert_eq!(mesh.point_count(), 2 + sa * (sh - 1));
        assert_eq!(mesh.edges.len(), sa * (2 * sh - 1));
        assert_eq!(mesh.faces.len(), sa * sh);
        assert_eq!(euler(&mesh), 2);
        mesh.validate().unwrap();
    }
}

#[test]
fn sphere_poles_and_winding_follow_the_axis() {
    for (axis, pole) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = sphere(2.5, 7, 5, axis);
        let p = points(&mesh);
        assert_eq!(p[0][pole], 2.5, "{axis:?} first pole");
        assert_eq!(p[p.len() - 1][pole], -2.5, "{axis:?} last pole");
        assert_eq!(extent(&mesh, pole), (-2.5, 2.5));
        assert!(
            p.iter()
                .flatten()
                .all(|v| v.to_bits() != (-0.0f32).to_bits())
        );
        for face in loops(&mesh) {
            let n = face_area_vector(&mesh, &face);
            let centre: [f64; 3] = std::array::from_fn(|c| {
                face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
            });
            assert!(
                n[0] * centre[0] + n[1] * centre[1] + n[2] * centre[2] > 0.0,
                "{axis:?} face {face:?} points inward"
            );
        }
    }
}

#[test]
fn even_height_divisions_keep_an_exact_equator() {
    let mesh = sphere(3.0, 6, 4, Axis::Y);
    let p = points(&mesh);
    assert!(p[1 + 6..1 + 12].iter().all(|v| v[1].to_bits() == 0));
}
