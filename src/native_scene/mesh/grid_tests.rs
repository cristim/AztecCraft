use super::axis::orient;
use super::*;
use crate::primitive_options::Axis;

pub(super) fn points(mesh: &NativeMesh) -> Vec<[f32; 3]> {
    match &mesh.points {
        Points::Float3(p) => p.clone(),
        Points::Double3(_) => panic!("primitives are f32"),
    }
}

pub(super) fn bits(mesh: &NativeMesh) -> Vec<[u32; 3]> {
    points(mesh).iter().map(|p| p.map(f32::to_bits)).collect()
}

pub(super) fn loops(mesh: &NativeMesh) -> Vec<Vec<usize>> {
    mesh.faces
        .iter()
        .map(|f| {
            f.outer
                .iter()
                .map(|e| mesh.endpoints(*e).unwrap()[0])
                .collect()
        })
        .collect()
}

fn cube(size: [f64; 3], n: [u32; 3], axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Cube {
        width: size[0],
        height: size[1],
        depth: size[2],
        sub_width: n[0],
        sub_height: n[1],
        sub_depth: n[2],
        axis,
    })
    .unwrap()
}

fn plane(size: [f64; 2], n: [u32; 2], axis: Axis) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Plane {
        width: size[0],
        height: size[1],
        sub_width: n[0],
        sub_height: n[1],
        axis,
    })
    .unwrap()
}

pub(super) fn euler(mesh: &NativeMesh) -> i64 {
    mesh.point_count() as i64 - mesh.edges.len() as i64 + mesh.faces.len() as i64
}

#[test]
fn default_cube_and_plane_equal_the_legacy_meshes_bit_for_bit() {
    for primitive in [Primitive::Cube, Primitive::Plane] {
        let built = NativeMesh::primitive(primitive).unwrap();
        let legacy = NativeMesh::from_legacy(&Mesh::primitive(primitive).unwrap());
        assert_eq!(bits(&built), bits(&legacy), "{primitive:?} points");
        assert_eq!(built.edges, legacy.edges, "{primitive:?} edge order");
        assert_eq!(
            built.faces, legacy.faces,
            "{primitive:?} loops and reversed flags"
        );
        assert!(built == legacy);
    }
}

#[test]
fn cube_counts_follow_the_closed_forms_and_euler_is_two() {
    for [x, y, z] in [[1, 1, 1], [2, 1, 1], [1, 3, 2], [4, 3, 5], [2, 2, 2]] {
        let mesh = cube([2.0, 3.0, 4.0], [x as u32, y as u32, z as u32], Axis::Y);
        let pairs = x * y + y * z + x * z;
        let interior = (x - 1) * (y - 1) * (z - 1);
        assert_eq!(mesh.point_count(), (x + 1) * (y + 1) * (z + 1) - interior);
        assert_eq!(mesh.edges.len(), 4 * pairs);
        assert_eq!(mesh.faces.len(), 2 * pairs);
        assert_eq!(euler(&mesh), 2);
        let mut uses = vec![0; mesh.edges.len()];
        for face in &mesh.faces {
            for e in &face.outer {
                uses[e.edge] += 1;
            }
        }
        assert!(uses.iter().all(|n| *n == 2), "closed manifold {x}x{y}x{z}");
        mesh.validate().unwrap();
    }
}

#[test]
fn plane_counts_follow_the_closed_forms_and_euler_is_one() {
    for [x, z] in [[1, 1], [3, 1], [2, 5], [4, 4]] {
        let mesh = plane([2.0, 3.0], [x as u32, z as u32], Axis::Y);
        assert_eq!(mesh.point_count(), (x + 1) * (z + 1));
        assert_eq!(mesh.edges.len(), x * (z + 1) + z * (x + 1));
        assert_eq!(mesh.faces.len(), x * z);
        assert_eq!(euler(&mesh), 1);
        mesh.validate().unwrap();
    }
}

pub(super) fn face_area_vector(mesh: &NativeMesh, face: &[usize]) -> [f64; 3] {
    let p = points(mesh);
    let at = |i: usize| p[face[i]].map(f64::from);
    let mut sum = [0.0; 3];
    for i in 0..face.len() {
        let (a, b) = (at(i), at((i + 1) % face.len()));
        sum[0] += a[1] * b[2] - a[2] * b[1];
        sum[1] += a[2] * b[0] - a[0] * b[2];
        sum[2] += a[0] * b[1] - a[1] * b[0];
    }
    sum
}

#[test]
fn cube_faces_wind_outward_for_every_axis() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let mesh = cube([2.0, 3.0, 4.0], [2, 3, 2], axis);
        let p = points(&mesh);
        let mut volume = 0.0;
        for face in loops(&mesh) {
            let n = face_area_vector(&mesh, &face);
            let centre: [f64; 3] = std::array::from_fn(|c| {
                face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
            });
            let outward = n[0] * centre[0] + n[1] * centre[1] + n[2] * centre[2];
            assert!(outward > 0.0, "{axis:?} face {face:?} points inward");
            volume += outward / 6.0;
        }
        assert!((volume - 24.0).abs() < 1e-4, "{axis:?} volume {volume}");
    }
}

#[test]
fn plane_faces_point_along_the_axis() {
    for (axis, normal) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = plane([2.0, 3.0], [3, 2], axis);
        for face in loops(&mesh) {
            let n = face_area_vector(&mesh, &face);
            assert!(n[normal] > 0.0, "{axis:?} {n:?}");
            assert!(n.iter().enumerate().all(|(c, v)| c == normal || *v == 0.0));
        }
    }
}

pub(super) fn extent(mesh: &NativeMesh, component: usize) -> (f32, f32) {
    let values = points(mesh).into_iter().map(|p| p[component]);
    (
        values.clone().fold(f32::MAX, f32::min),
        values.fold(f32::MIN, f32::max),
    )
}

#[test]
fn cube_extents_follow_the_axis() {
    // Local size is (2, 3, 5); axis X maps local y to world x and axis Z maps it to world z.
    for (axis, half) in [
        (Axis::Y, [1.0, 1.5, 2.5]),
        (Axis::X, [1.5, 1.0, 2.5]),
        (Axis::Z, [1.0, 2.5, 1.5]),
    ] {
        let mesh = cube([2.0, 3.0, 5.0], [2, 2, 2], axis);
        for (component, half) in half.into_iter().enumerate() {
            assert_eq!(
                extent(&mesh, component),
                (-half, half),
                "{axis:?} {component}"
            );
        }
    }
}

#[test]
fn plane_extents_follow_the_axis_and_stay_flat() {
    for (axis, half) in [
        (Axis::Y, [1.0, 0.0, 1.5]),
        (Axis::X, [0.0, 1.0, 1.5]),
        (Axis::Z, [1.0, 1.5, 0.0]),
    ] {
        let mesh = plane([2.0, 3.0], [2, 3], axis);
        for (component, half) in half.into_iter().enumerate() {
            assert_eq!(
                extent(&mesh, component),
                (-half + 0.0, half),
                "{axis:?} {component}"
            );
        }
    }
}

#[test]
fn orient_is_an_exact_rotation_and_never_yields_negative_zero() {
    assert_eq!(orient(Axis::Y, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    assert_eq!(orient(Axis::X, [1.0, 2.0, 3.0]), [2.0, -1.0, 3.0]);
    assert_eq!(orient(Axis::Z, [1.0, 2.0, 3.0]), [1.0, -3.0, 2.0]);
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        let out = orient(axis, [-0.0, 0.0, -0.0]);
        assert!(out.iter().all(|v| v.to_bits() == 0), "{axis:?} {out:?}");
    }
}

#[test]
fn no_primitive_grid_contains_negative_zero() {
    for axis in [Axis::X, Axis::Y, Axis::Z] {
        for mesh in [
            cube([1.0; 3], [3, 2, 4], axis),
            plane([1.0; 2], [3, 2], axis),
        ] {
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
fn invalid_grid_options_fail_before_building() {
    let mut options = PrimitiveOptions::default_for(Primitive::Cube);
    if let PrimitiveOptions::Cube { width, .. } = &mut options {
        *width = 0.0;
    }
    assert!(matches!(
        NativeMesh::with_options(&options),
        Err(NativeError::Legacy(SceneError::InvalidPrimitiveOptions(_)))
    ));
}

pub(super) fn assert_closed_manifold(mesh: &NativeMesh) {
    let mut uses = vec![0; mesh.edges.len()];
    for face in &mesh.faces {
        for e in &face.outer {
            uses[e.edge] += 1;
        }
    }
    assert!(uses.iter().all(|n| *n == 2), "every edge borders two faces");
    mesh.validate().unwrap();
}

/// Valid for convex solids that contain the origin.
pub(super) fn assert_outward(mesh: &NativeMesh) {
    let p = points(mesh);
    for face in loops(mesh) {
        let n = face_area_vector(mesh, &face);
        let c: [f64; 3] = std::array::from_fn(|k| {
            face.iter().map(|i| f64::from(p[*i][k])).sum::<f64>() / face.len() as f64
        });
        assert!(
            n[0] * c[0] + n[1] * c[1] + n[2] * c[2] > 0.0,
            "face {face:?} points inward"
        );
    }
    assert!(
        p.iter()
            .flatten()
            .all(|v| v.to_bits() != (-0.0f32).to_bits())
    );
}
