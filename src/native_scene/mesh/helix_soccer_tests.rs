use super::grid_tests::{
    assert_closed_manifold, assert_outward, euler, face_area_vector, loops, points,
};
use super::*;
use crate::primitive_options::{Axis, PrimitiveOptionsError};
use std::collections::HashSet;

fn soccer(side_length: f64, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::SoccerBall { side_length, axis }).unwrap()
}

fn helix_options(coils: f64, sub_axis: u32, sub_coil: u32, axis: Axis) -> PrimitiveOptions {
    PrimitiveOptions::Helix {
        coils,
        height: 4.0,
        width: 2.0,
        radius: 0.25,
        sub_axis,
        sub_coil,
        axis,
    }
}

fn helix(coils: f64, sub_axis: u32, sub_coil: u32, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&helix_options(coils, sub_axis, sub_coil, axis)).unwrap()
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f64 {
    (0..3)
        .map(|c| (f64::from(a[c]) - f64::from(b[c])).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn assert_planar(mesh: &NativeMesh) {
    let p = points(mesh);
    for face in loops(mesh) {
        let area = face_area_vector(mesh, &face);
        let len = area.iter().map(|c| c * c).sum::<f64>().sqrt();
        assert!(len > 1e-9, "face {face:?} is degenerate");
        let offset = |i: usize| -> f64 { (0..3).map(|c| area[c] / len * f64::from(p[i][c])).sum() };
        for &i in &face {
            assert!((offset(i) - offset(face[0])).abs() < 1e-5, "{face:?}");
        }
    }
}

fn assert_directed_edges_pair(mesh: &NativeMesh) {
    let mut seen = HashSet::new();
    for face in loops(mesh) {
        for k in 0..face.len() {
            assert!(seen.insert((face[k], face[(k + 1) % face.len()])));
        }
    }
    for &(a, b) in &seen {
        assert!(seen.contains(&(b, a)), "edge {a}-{b} has no opposite");
    }
}

#[test]
fn soccer_ball_is_a_truncated_icosahedron() {
    let mesh = soccer(1.0, Axis::Y);
    assert_eq!(
        (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
        (60, 90, 32)
    );
    assert_eq!(euler(&mesh), 2);
    let mut sizes: Vec<usize> = loops(&mesh).iter().map(Vec::len).collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [vec![5; 12], vec![6; 20]].concat());
    assert_closed_manifold(&mesh);
    assert_planar(&mesh);
    assert_directed_edges_pair(&mesh);
    // Three faces meet at every vertex: two hexagons and one pentagon.
    let mut corners = vec![(0, 0); 60];
    for face in loops(&mesh) {
        for &i in &face {
            if face.len() == 5 {
                corners[i].0 += 1;
            } else {
                corners[i].1 += 1;
            }
        }
    }
    assert!(corners.iter().all(|c| *c == (1, 2)), "{corners:?}");
}

#[test]
fn every_soccer_ball_edge_equals_the_side_length() {
    for side in [0.5, 1.0, 2.5] {
        let mesh = soccer(side, Axis::Y);
        let p = points(&mesh);
        for face in loops(&mesh) {
            for k in 0..face.len() {
                let d = dist(p[face[k]], p[face[(k + 1) % face.len()]]);
                assert!((d - side).abs() < 1e-5 * side.max(1.0), "{side}: {d}");
            }
        }
    }
}

#[test]
fn soccer_ball_points_are_distinct_and_on_one_sphere() {
    let p = points(&soccer(1.0, Axis::Y));
    let radius = (0.25 * (58.0 + 18.0 * 5.0_f64.sqrt())).sqrt() / 2.0;
    for (i, a) in p.iter().enumerate() {
        let r = dist(*a, [0.0; 3]);
        assert!((r - radius).abs() < 1e-5, "{i}: {r} vs {radius}");
        for b in &p[i + 1..] {
            assert!(dist(*a, *b) > 0.5);
        }
    }
}

#[test]
fn soccer_ball_faces_wind_outward_on_every_axis() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let mesh = soccer(2.0, axis);
        assert_outward(&mesh);
        assert_eq!(euler(&mesh), 2);
    }
}

#[test]
fn soccer_ball_default_follows_the_command_pages() {
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::SoccerBall),
        PrimitiveOptions::SoccerBall {
            side_length: 1.0,
            axis: Axis::Y
        }
    );
    let mesh = NativeMesh::primitive(Primitive::SoccerBall).unwrap();
    assert_eq!((mesh.point_count(), mesh.faces.len()), (60, 32));
}

#[test]
fn helix_counts_follow_the_subdivisions_and_euler_is_two() {
    for (n, s) in [(3_u32, 1_u32), (8, 50), (5, 7), (128, 128)] {
        let mesh = helix(3.0, n, s, Axis::Y);
        let (n, s) = (n as usize, s as usize);
        assert_eq!(
            (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
            ((s + 1) * n, (2 * s + 1) * n, s * n + 2)
        );
        assert_eq!(euler(&mesh), 2);
        assert_closed_manifold(&mesh);
        assert_directed_edges_pair(&mesh);
        let mut sizes: Vec<usize> = loops(&mesh).iter().map(Vec::len).collect();
        sizes.sort_unstable();
        let mut expected = [vec![4; s * n], vec![n; 2]].concat();
        expected.sort_unstable();
        assert_eq!(sizes, expected);
    }
}

/// Centre of ring `k` of a helix built with `n` points per ring.
fn ring_centre(p: &[[f32; 3]], k: usize, n: usize) -> [f64; 3] {
    std::array::from_fn(|c| {
        p[k * n..(k + 1) * n]
            .iter()
            .map(|v| f64::from(v[c]))
            .sum::<f64>()
            / n as f64
    })
}

#[test]
fn helix_turns_the_requested_number_of_coils_along_its_height() {
    let (n, s) = (8_usize, 120_usize);
    for coils in [1.0, 2.0, 3.0, 5.0] {
        let mesh = helix(coils, n as u32, s as u32, Axis::Y);
        let p = points(&mesh);
        // Accumulate the signed angle of the ring centres about +Y (counter-clockwise from +Y
        // is positive; the mesh starts on +X and runs towards -Z).
        let mut total = 0.0;
        let mut last = ring_centre(&p, 0, n);
        for k in 1..=s {
            let now = ring_centre(&p, k, n);
            let step = (-now[2]).atan2(now[0]) - (-last[2]).atan2(last[0]);
            total += (step + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                - std::f64::consts::PI;
            last = now;
        }
        let turns = total / std::f64::consts::TAU;
        assert!((turns - coils).abs() < 0.05, "{coils}: {turns}");
        // The centreline spans `height` (4) along Y and is centred.
        assert!((ring_centre(&p, 0, n)[1] + 2.0).abs() < 1e-5);
        assert!((ring_centre(&p, s, n)[1] - 2.0).abs() < 1e-5);
        // The centreline circle has diameter `width` (2).
        let c = ring_centre(&p, 0, n);
        assert!((c[0].hypot(c[2]) - 1.0).abs() < 1e-5);
    }
}

#[test]
fn helix_tube_radius_and_extent_follow_the_options() {
    let mesh = helix(3.0, 8, 60, Axis::Y);
    let p = points(&mesh);
    for k in 0..=60 {
        let c = ring_centre(&p, k, 8);
        for v in &p[k * 8..(k + 1) * 8] {
            let d = dist(*v, [c[0] as f32, c[1] as f32, c[2] as f32]);
            assert!((d - 0.25).abs() < 1e-5, "ring {k}: {d}");
        }
    }
    let (lo, hi) = p.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| {
        (lo.min(v[1]), hi.max(v[1]))
    });
    assert!(lo < -2.0 && hi > 2.0 && lo > -2.3 && hi < 2.3, "{lo} {hi}");
}

#[test]
fn helix_faces_wind_outward_on_every_axis() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let mesh = helix(3.0, 8, 40, axis);
        outward_by_ring(&mesh, 8, 40);
        let q = points(&mesh);
        // Signed volume is positive for an outward-wound closed mesh.
        let mut volume = 0.0;
        for face in loops(&mesh) {
            let a = face_area_vector(&mesh, &face);
            let c = q[face[0]].map(f64::from);
            volume += (a[0] * c[0] + a[1] * c[1] + a[2] * c[2]) / 6.0;
        }
        assert!(volume > 0.0, "{axis:?}: {volume}");
    }
}

/// Every side quad and cap points away from its own ring centre or along the helix.
fn outward_by_ring(mesh: &NativeMesh, n: usize, s: usize) {
    let p = points(mesh);
    let faces = loops(mesh);
    // Axis-permuted meshes still group points ring by ring, so centres are ring means.
    let centre = |ring: usize| -> [f64; 3] { ring_centre(&p, ring, n) };
    for face in &faces[1..=s * n] {
        let ring = face[0] / n;
        let area = face_area_vector(mesh, face);
        let mid: [f64; 3] = std::array::from_fn(|c| {
            face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
        });
        let from = centre(ring);
        let dot: f64 = (0..3).map(|c| area[c] * (mid[c] - from[c])).sum();
        assert!(dot > 0.0, "quad {face:?} points into the tube");
    }
    let first = face_area_vector(mesh, &faces[0]);
    let last = face_area_vector(mesh, faces.last().unwrap());
    let along: [f64; 3] = std::array::from_fn(|c| centre(s)[c] - centre(0)[c]);
    let dot = |a: [f64; 3], b: [f64; 3]| -> f64 { (0..3).map(|c| a[c] * b[c]).sum() };
    assert!(dot(first, along) < 0.0 && dot(last, along) > 0.0);
}

#[test]
fn helix_defaults_follow_the_command_pages() {
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Helix),
        PrimitiveOptions::Helix {
            coils: 3.0,
            height: 2.0,
            width: 2.0,
            radius: 0.4,
            sub_axis: 8,
            sub_coil: 50,
            axis: Axis::Y
        }
    );
    let mesh = NativeMesh::primitive(Primitive::Helix).unwrap();
    assert_eq!((mesh.point_count(), mesh.faces.len()), (408, 402));
}

#[test]
fn helix_and_soccer_ball_ranges_and_budget_are_enforced() {
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e7] {
        for field in ["coils", "height", "width", "radius"] {
            let mut o = (3.0, 2.0, 2.0, 0.4);
            match field {
                "coils" => o.0 = bad,
                "height" => o.1 = bad,
                "width" => o.2 = bad,
                _ => o.3 = bad,
            }
            let options = PrimitiveOptions::Helix {
                coils: o.0,
                height: o.1,
                width: o.2,
                radius: o.3,
                sub_axis: 8,
                sub_coil: 50,
                axis: Axis::Y,
            };
            assert_eq!(
                options.validate(Primitive::Helix),
                Err(PrimitiveOptionsError::SizeOutOfRange { field }),
                "{field} {bad}"
            );
        }
        let options = PrimitiveOptions::SoccerBall {
            side_length: bad,
            axis: Axis::Y,
        };
        assert_eq!(
            options.validate(Primitive::SoccerBall),
            Err(PrimitiveOptionsError::SizeOutOfRange {
                field: "side_length"
            })
        );
    }
    for (sub_axis, sub_coil, field, min) in [
        (2, 50, "subdivisions_axis", 3),
        (129, 50, "subdivisions_axis", 3),
        (8, 0, "subdivisions_coil", 1),
        (8, 129, "subdivisions_coil", 1),
    ] {
        let options = helix_options(3.0, sub_axis, sub_coil, Axis::Y);
        assert_eq!(
            options.validate(Primitive::Helix),
            Err(PrimitiveOptionsError::DivisionsOutOfRange {
                field,
                min,
                max: 128
            })
        );
    }
    for (sub_axis, sub_coil) in [(3, 1), (128, 128)] {
        let options = helix_options(3.0, sub_axis, sub_coil, Axis::Y);
        assert_eq!(options.validate(Primitive::Helix), Ok(()));
        assert_eq!(options.face_count(), u64::from(sub_axis * sub_coil + 2));
    }
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::SoccerBall).face_count(),
        32
    );
}

#[test]
fn no_point_is_negative_zero() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        for mesh in [soccer(1.0, axis), helix(2.0, 4, 8, axis)] {
            assert!(
                points(&mesh)
                    .iter()
                    .flatten()
                    .all(|v| v.to_bits() != (-0.0f32).to_bits())
            );
        }
    }
}
