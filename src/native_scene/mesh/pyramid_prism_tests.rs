use super::grid_tests::{
    assert_closed_manifold, assert_outward, euler, face_area_vector, loops, points,
};
use super::*;
use crate::primitive_options::{Axis, PrimitiveOptionsError};
use std::collections::HashSet;

fn pyramid(side_length: f64, sides: u32, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Pyramid {
        side_length,
        sides,
        axis,
    })
    .unwrap()
}

fn prism(side_length: f64, length: f64, sides: u32, axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Prism {
        side_length,
        length,
        sides,
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

/// Every face is planar: all its points sit at one offset along the face normal.
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
fn pyramid_has_n_plus_one_faces_and_euler_characteristic_two() {
    for n in [3_u32, 4, 5, 8, 128] {
        let mesh = pyramid(1.0, n, Axis::Y);
        let n = n as usize;
        assert_eq!(
            (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
            (n + 1, 2 * n, n + 1)
        );
        assert_eq!(euler(&mesh), 2);
        let mut sizes: Vec<usize> = loops(&mesh).iter().map(Vec::len).collect();
        sizes.sort_unstable();
        let mut expected = [vec![n], vec![3; n]].concat();
        expected.sort_unstable();
        assert_eq!(sizes, expected);
        assert_closed_manifold(&mesh);
        assert_planar(&mesh);
        assert_directed_edges_pair(&mesh);
    }
}

#[test]
fn prism_has_n_plus_two_faces_and_euler_characteristic_two() {
    for n in [3_u32, 4, 6, 128] {
        let mesh = prism(1.0, 2.0, n, Axis::Y);
        let n = n as usize;
        assert_eq!(
            (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
            (2 * n, 3 * n, n + 2)
        );
        assert_eq!(euler(&mesh), 2);
        let mut sizes: Vec<usize> = loops(&mesh).iter().map(Vec::len).collect();
        sizes.sort_unstable();
        let mut expected = [vec![n; 2], vec![4; n]].concat();
        expected.sort_unstable();
        assert_eq!(sizes, expected);
        assert_closed_manifold(&mesh);
        assert_planar(&mesh);
        assert_directed_edges_pair(&mesh);
    }
}

#[test]
fn faces_wind_outward_on_every_axis_and_side_count() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        for n in [3, 4, 7, 128] {
            assert_outward(&pyramid(2.0, n, axis));
            assert_outward(&prism(2.0, 3.0, n, axis));
        }
    }
}

#[test]
fn base_edges_equal_the_side_length_and_the_extent_follows_the_axis() {
    for n in [3_u32, 4, 5, 12] {
        let mesh = pyramid(2.5, n, Axis::Y);
        let p = points(&mesh);
        for i in 0..n as usize {
            let d = dist(p[i], p[(i + 1) % n as usize]);
            assert!((d - 2.5).abs() < 1e-5, "{n} sides: {d}");
        }
        let mesh = prism(1.5, 4.0, n, Axis::Y);
        let p = points(&mesh);
        for i in 0..n as usize {
            assert!((dist(p[i], p[(i + 1) % n as usize]) - 1.5).abs() < 1e-5);
            assert!((dist(p[i], p[n as usize + i]) - 4.0).abs() < 1e-5);
        }
    }
    // Pyramid: apex above the base along the chosen axis, centred (PROVISIONAL).
    for (axis, k) in [(Axis::X, 0), (Axis::Y, 1), (Axis::Z, 2)] {
        let p = points(&pyramid(2.0, 4, axis));
        let apex = p[4][k].abs();
        assert!((f64::from(apex) - 1.0).abs() < 1e-6, "{axis:?}");
        assert!(
            p[..4]
                .iter()
                .all(|v| (f64::from(v[k].abs()) - 1.0).abs() < 1e-6)
        );
        assert_ne!(p[4][k].signum(), p[0][k].signum(), "{axis:?}");
    }
}

#[test]
fn no_point_is_negative_zero() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        for mesh in [pyramid(1.0, 4, axis), prism(2.0, 2.0, 3, axis)] {
            assert!(
                points(&mesh)
                    .iter()
                    .flatten()
                    .all(|v| v.to_bits() != (-0.0f32).to_bits())
            );
        }
    }
}

#[test]
fn defaults_follow_the_command_pages() {
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Pyramid),
        PrimitiveOptions::Pyramid {
            side_length: 1.0,
            sides: 4,
            axis: Axis::Y
        }
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Prism),
        PrimitiveOptions::Prism {
            side_length: 2.0,
            length: 2.0,
            sides: 3,
            axis: Axis::Y
        }
    );
    let mesh = NativeMesh::primitive(Primitive::Pyramid).unwrap();
    assert_eq!((mesh.point_count(), mesh.faces.len()), (5, 5));
    let mesh = NativeMesh::primitive(Primitive::Prism).unwrap();
    assert_eq!((mesh.point_count(), mesh.faces.len()), (6, 5));
}

#[test]
fn side_count_range_edges_and_bad_sizes_are_rejected() {
    for sides in [0, 1, 2, 129, u32::MAX] {
        for options in [
            PrimitiveOptions::Pyramid {
                side_length: 1.0,
                sides,
                axis: Axis::Y,
            },
            PrimitiveOptions::Prism {
                side_length: 1.0,
                length: 1.0,
                sides,
                axis: Axis::Y,
            },
        ] {
            assert_eq!(
                options.validate(options.primitive()),
                Err(PrimitiveOptionsError::DivisionsOutOfRange {
                    field: "number_of_sides",
                    min: 3,
                    max: 128
                }),
                "{sides}"
            );
        }
    }
    for sides in [3, 128] {
        let options = PrimitiveOptions::Pyramid {
            side_length: 1.0,
            sides,
            axis: Axis::Y,
        };
        assert_eq!(options.validate(Primitive::Pyramid), Ok(()));
    }
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e7] {
        let options = PrimitiveOptions::Prism {
            side_length: bad,
            length: 1.0,
            sides: 3,
            axis: Axis::Y,
        };
        assert_eq!(
            options.validate(Primitive::Prism),
            Err(PrimitiveOptionsError::SizeOutOfRange {
                field: "side_length"
            })
        );
        let options = PrimitiveOptions::Prism {
            side_length: 1.0,
            length: bad,
            sides: 3,
            axis: Axis::Y,
        };
        assert_eq!(
            options.validate(Primitive::Prism),
            Err(PrimitiveOptionsError::SizeOutOfRange { field: "length" })
        );
    }
}
