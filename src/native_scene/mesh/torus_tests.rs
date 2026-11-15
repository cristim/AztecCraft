use super::grid_tests::{
    assert_closed_manifold, bits, euler, extent, face_area_vector, loops, points,
};
use super::*;
use crate::primitive_options::{Axis, PrimitiveOptionsError};

fn torus(
    radius: f64,
    section_radius: f64,
    sub_axis: u32,
    sub_height: u32,
    axis: Axis,
) -> NativeMesh {
    twisted(radius, section_radius, 0.0, sub_axis, sub_height, axis)
}

fn twisted(
    radius: f64,
    section_radius: f64,
    twist: f64,
    sub_axis: u32,
    sub_height: u32,
    axis: Axis,
) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Torus {
        radius,
        section_radius,
        twist,
        sub_axis,
        sub_height,
        axis,
    })
    .unwrap()
}

#[test]
fn torus_counts_follow_the_closed_forms_and_euler_is_zero() {
    for (sa, sh) in [(3, 3), (5, 4), (13, 9), (20, 20), (3, 128)] {
        let mesh = torus(2.0, 0.5, sa, sh, Axis::Y);
        let (sa, sh) = (sa as usize, sh as usize);
        assert_eq!(mesh.point_count(), sa * sh);
        assert_eq!(mesh.edges.len(), 2 * sa * sh);
        assert_eq!(mesh.faces.len(), sa * sh);
        assert_eq!(euler(&mesh), 0);
        assert!(loops(&mesh).iter().all(|face| face.len() == 4));
        assert_closed_manifold(&mesh);
        mesh.validate().unwrap();
    }
}

#[test]
fn default_torus_is_a_radius_one_tube_of_half_a_centimetre_with_400_quads() {
    let mesh = NativeMesh::primitive(Primitive::Torus).unwrap();
    assert_eq!(
        (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
        (400, 800, 400)
    );
    assert_eq!(extent(&mesh, 0), (-1.5, 1.5));
    assert_eq!(extent(&mesh, 1), (-0.5, 0.5));
    assert_eq!(extent(&mesh, 2), (-1.5, 1.5));
}

#[test]
fn torus_hole_axis_and_outward_winding_follow_the_axis() {
    for (axis, up) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = torus(2.0, 0.5, 9, 8, axis);
        let p = points(&mesh);
        assert_eq!(extent(&mesh, up), (-0.5, 0.5), "{axis:?} thickness");
        for face in loops(&mesh) {
            let n = face_area_vector(&mesh, &face);
            let centre: [f64; 3] = std::array::from_fn(|c| {
                face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
            });
            // Away from the tube's core circle: the ring-plane component of the centre
            // is scaled to the ring radius.
            let planar: Vec<usize> = (0..3).filter(|c| *c != up).collect();
            let len = (centre[planar[0]].powi(2) + centre[planar[1]].powi(2)).sqrt();
            let mut core = centre;
            for c in &planar {
                core[*c] -= centre[*c] / len * 2.0;
            }
            assert!(
                n[0] * core[0] + n[1] * core[1] + n[2] * core[2] > 0.0,
                "{axis:?} face {face:?} points into the tube"
            );
        }
    }
}

#[test]
fn a_section_radius_larger_than_the_ring_radius_is_accepted() {
    // PROVISIONAL: the Maya pages state no relation between the two radii.
    torus(0.5, 2.0, 4, 4, Axis::Y).validate().unwrap();
}

/// The torus exactly as it was built before `twist` existed.
fn frozen_torus(radius: f64, section_radius: f64, rings: usize, steps: usize) -> Vec<[u32; 3]> {
    let mut out = Vec::new();
    for ring in 0..rings {
        let azimuth = 2.0 * std::f64::consts::PI * ring as f64 / rings as f64;
        for step in 0..steps {
            let section = 2.0 * std::f64::consts::PI * step as f64 / steps as f64;
            let radial = radius + section_radius * section.cos();
            out.push(
                [
                    (radial * azimuth.cos()) as f32,
                    (section_radius * section.sin()) as f32,
                    (radial * azimuth.sin()) as f32,
                ]
                .map(f32::to_bits),
            );
        }
    }
    out
}

#[test]
fn twist_zero_and_full_turns_are_bit_identical_to_the_untwisted_torus() {
    let frozen = frozen_torus(1.0, 0.5, 20, 20);
    for twist in [0.0, -0.0, 360.0, -720.0, 3600.0] {
        let mesh = twisted(1.0, 0.5, twist, 20, 20, Axis::Y);
        assert_eq!(bits(&mesh), frozen, "twist {twist}");
    }
}

#[test]
fn a_quarter_turn_twist_moves_every_section_point_to_the_next_step() {
    let (rings, steps) = (4_usize, 4_usize);
    let mesh = twisted(2.0, 0.5, 90.0, rings as u32, steps as u32, Axis::Y);
    let p = points(&mesh);
    for ring in 0..rings {
        let azimuth = 2.0 * std::f64::consts::PI * ring as f64 / rings as f64;
        for step in 0..steps {
            let section = 2.0 * std::f64::consts::PI * (step + 1) as f64 / steps as f64;
            let radial = 2.0 + 0.5 * section.cos();
            let expected = [
                radial * azimuth.cos(),
                0.5 * section.sin(),
                radial * azimuth.sin(),
            ];
            let got = p[ring * steps + step];
            for c in 0..3 {
                assert!(
                    (f64::from(got[c]) - expected[c]).abs() < 1e-6,
                    "ring {ring} step {step} component {c}: {got:?} vs {expected:?}"
                );
            }
        }
    }
}

#[test]
fn twisted_faces_stay_non_degenerate_and_identical_across_the_seam() {
    for twist in [0.0, 37.0, 360.0, -720.0, 1e6, -1e6, 90.0] {
        let (rings, steps) = (7_usize, 5_usize);
        let mesh = twisted(2.0, 0.5, twist, rings as u32, steps as u32, Axis::Y);
        assert_eq!(
            (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
            (35, 70, 35)
        );
        assert_eq!(euler(&mesh), 0);
        assert_closed_manifold(&mesh);
        mesh.validate().unwrap();
        let areas: Vec<f64> = loops(&mesh)
            .iter()
            .map(|face| {
                let n = face_area_vector(&mesh, face);
                (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
            })
            .collect();
        assert!(areas.iter().all(|a| *a > 1e-3), "twist {twist}: {areas:?}");
        // Faces are ring-major: face (ring, step) at ring * steps + step. Every ring pair,
        // including the seam (ring 6 to ring 0), is the same shape.
        for ring in 1..rings {
            for step in 0..steps {
                let (a, b) = (areas[step], areas[ring * steps + step]);
                assert!(
                    (a - b).abs() < 1e-5 * a.max(1.0),
                    "twist {twist} ring {ring} step {step}: {a} vs {b}"
                );
            }
        }
        assert_outward_from_core(&mesh, twist);
    }
}

fn assert_outward_from_core(mesh: &NativeMesh, twist: f64) {
    let p = points(mesh);
    for face in loops(mesh) {
        let n = face_area_vector(mesh, &face);
        let centre: [f64; 3] = std::array::from_fn(|c| {
            face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
        });
        let len = (centre[0].powi(2) + centre[2].powi(2)).sqrt();
        let core = [
            centre[0] - centre[0] / len * 2.0,
            centre[1],
            centre[2] - centre[2] / len * 2.0,
        ];
        assert!(
            n[0] * core[0] + n[1] * core[1] + n[2] * core[2] > 0.0,
            "twist {twist}: face {face:?} points into the tube"
        );
    }
}

#[test]
fn a_non_finite_twist_is_rejected_before_any_mesh_is_built() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let options = PrimitiveOptions::Torus {
            radius: 1.0,
            section_radius: 0.5,
            twist: bad,
            sub_axis: 4,
            sub_height: 4,
            axis: Axis::Y,
        };
        assert!(matches!(
            NativeMesh::with_options(&options),
            Err(NativeError::Legacy(SceneError::InvalidPrimitiveOptions(
                PrimitiveOptionsError::TwistNotFinite
            )))
        ));
    }
}
