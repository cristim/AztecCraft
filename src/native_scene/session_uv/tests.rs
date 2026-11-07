use super::*;
use crate::native_scene::authored::{AuthoredIneligible, check_content_kind};
use crate::primitive_options::{Axis, PrimitiveOptions};
use crate::scene::Primitive;

fn scene_with(primitive: Primitive) -> (NativeScene, NativeId) {
    let mut scene = NativeScene::default();
    scene.create(primitive, "object").unwrap();
    let shape = scene.shapes()[0].id();
    (scene, shape)
}

#[test]
fn planar_fills_the_unit_square_with_one_uv_per_corner() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let set = scene.uv_set(shape).unwrap();
    assert_eq!(set.faces().len(), 6);
    assert_eq!(set.corner_count(), 24);
    let all: Vec<f64> = set.faces().iter().flatten().flatten().copied().collect();
    assert!(all.iter().all(|value| (0.0..=1.0).contains(value)));
    assert!(all.contains(&0.0) && all.contains(&1.0));
}

#[test]
fn unused_points_do_not_change_face_uv_bounds() {
    let (scene, shape) = scene_with(Primitive::Cube);
    let mesh = scene.shape(shape).unwrap().mesh();
    let mut points: Vec<[f32; 3]> = (0..mesh.point_count())
        .map(|index| mesh.effective_point_cm(index).unwrap().map(|v| v as f32))
        .collect();
    points.extend([[-100.0, -200.0, -300.0], [400.0, 500.0, 600.0]]);
    let with_unused = NativeMesh::from_binary_parts(
        points,
        mesh.edges()
            .iter()
            .cloned()
            .map(|mut edge| {
                edge.smooth = None;
                edge
            })
            .collect(),
        mesh.faces().to_vec(),
        Vec::new(),
    )
    .unwrap();
    for (axis, _, _) in AXES {
        let expected = planar(mesh, axis).unwrap();
        let actual = planar(&with_unused, axis).unwrap();
        assert_eq!(actual, expected, "{axis:?}");
        for component in 0..2 {
            let values: Vec<f64> = actual
                .faces()
                .iter()
                .flatten()
                .map(|uv| uv[component])
                .collect();
            assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
            assert!(
                values.contains(&0.0) && values.contains(&1.0),
                "{axis:?} component {component}"
            );
        }
    }
}

const AXES: [(UvProjectAxis, usize, Axis); 3] = [
    (UvProjectAxis::X, 0, Axis::X),
    (UvProjectAxis::Y, 1, Axis::Y),
    (UvProjectAxis::Z, 2, Axis::Z),
];

/// Each face's corner positions with its projected UVs, in face order.
fn face_corners(scene: &NativeScene, shape: NativeId) -> Vec<Vec<([f64; 3], [f64; 2])>> {
    let mesh = scene.shape(shape).unwrap().mesh();
    let set = scene.uv_set(shape).unwrap();
    mesh.faces()
        .iter()
        .zip(set.faces())
        .map(|(face, uvs)| {
            face.outer()
                .iter()
                .zip(uvs)
                .map(|(directed, uv)| {
                    let start = mesh.directed_start(*directed).unwrap();
                    (mesh.effective_point_cm(start).unwrap(), *uv)
                })
                .collect()
        })
        .collect()
}

/// Newell normal of the face's 3D corners, unnormalised.
fn face_normal(corners: &[([f64; 3], [f64; 2])]) -> [f64; 3] {
    let mut n = [0.0; 3];
    for (i, (a, _)) in corners.iter().enumerate() {
        let (b, _) = corners[(i + 1) % corners.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}

/// Shoelace area of the UV polygon: positive when the corners run counter-clockwise.
fn uv_signed_area(corners: &[([f64; 3], [f64; 2])]) -> f64 {
    let mut twice = 0.0;
    for (i, (_, a)) in corners.iter().enumerate() {
        let (_, b) = corners[(i + 1) % corners.len()];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    twice / 2.0
}

fn box_options(axis: Axis) -> PrimitiveOptions {
    PrimitiveOptions::Cube {
        width: 2.0,
        height: 3.0,
        depth: 5.0,
        sub_width: 1,
        sub_height: 1,
        sub_depth: 1,
        axis,
    }
}

#[test]
fn faces_looking_along_the_axis_get_positive_uv_area() {
    for (project, axis, _) in AXES {
        let (mut scene, shape) = scene_with(Primitive::Cube);
        scene.project_uv_planar(shape, project).unwrap();
        let (mut toward, mut away) = (0, 0);
        for face in face_corners(&scene, shape) {
            let n = face_normal(&face);
            let area = uv_signed_area(&face);
            if n[axis].abs() > 0.5 {
                // Precondition: the normal points outward, so a bad fixture cannot pass vacuously.
                let centre = face.iter().map(|(p, _)| p[axis]).sum::<f64>() / face.len() as f64;
                assert!(n[axis] * centre > 0.0, "{project:?} winding {n:?}");
                assert!(
                    area.abs() > 1e-9 && area.signum() == n[axis].signum(),
                    "{project:?}"
                );
                if n[axis] > 0.0 {
                    toward += 1
                } else {
                    away += 1
                }
            } else {
                assert!(area.abs() < 1e-12, "{project:?} side face {area}");
            }
        }
        assert_eq!((toward, away), (1, 1), "{project:?}");
    }
}

#[test]
fn a_plane_projected_along_its_normal_is_counter_clockwise() {
    for (project, axis, plane_axis) in AXES {
        let mut scene = NativeScene::default();
        let options = PrimitiveOptions::Plane {
            width: 2.0,
            height: 3.0,
            sub_width: 3,
            sub_height: 2,
            axis: plane_axis,
        };
        scene
            .create_with_options(Primitive::Plane, &options, "plane")
            .unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, project).unwrap();
        let faces = face_corners(&scene, shape);
        assert_eq!(faces.len(), 6);
        for face in faces {
            assert!(face_normal(&face)[axis] > 0.0, "{project:?} fixture");
            assert!(uv_signed_area(&face) > 0.0, "{project:?}");
        }
    }
}

/// PROVISIONAL: Maya's pages state no per-axis orientation. Each projection matches the Y-up
/// orthographic view looking at the shape from that axis: (screen right, screen up) in object
/// axes. The signed-area test cannot see a u/v swap or a double flip; this one can.
#[test]
fn uv_directions_follow_the_side_top_and_front_views() {
    let views: [(UvProjectAxis, usize, [f64; 3], [f64; 3]); 3] = [
        (UvProjectAxis::X, 0, [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        (UvProjectAxis::Y, 1, [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        (UvProjectAxis::Z, 2, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    for (project, axis, right, up) in views {
        let mut scene = NativeScene::default();
        scene
            .create_with_options(Primitive::Cube, &box_options(Axis::Y), "box")
            .unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, project).unwrap();
        let faces = face_corners(&scene, shape);
        let face = faces
            .iter()
            .find(|face| face_normal(face)[axis] > 0.5)
            .expect("a face looking along +axis");
        let mut compared = 0;
        for (pa, ua) in face {
            for (pb, ub) in face {
                let delta = [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]];
                for (component, direction) in [(0, right), (1, up)] {
                    let along = dot(delta, direction);
                    let diff = ua[component] - ub[component];
                    if along.abs() < 1e-9 {
                        assert!(diff.abs() < 1e-9, "{project:?} component {component}");
                    } else {
                        assert!(diff * along > 0.0, "{project:?} component {component}");
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 0);
    }
}

#[test]
fn a_projection_with_no_extent_is_refused_and_keeps_the_scene() {
    let (mut scene, shape) = scene_with(Primitive::Plane);
    let before = scene.clone();
    assert_eq!(
        scene.project_uv_planar(shape, UvProjectAxis::X),
        Err(NativeError::DegenerateUvProjection)
    );
    assert!(scene == before);
}

#[test]
fn an_unknown_shape_is_refused() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    let missing = NativeId(shape.0 + 100);
    assert_eq!(
        scene.project_uv_planar(missing, UvProjectAxis::Y),
        Err(NativeError::MissingShape(missing))
    );
    assert_eq!(
        scene.delete_uvs(missing),
        Err(NativeError::MissingShape(missing))
    );
}

#[test]
fn delete_clears_only_that_shape_and_tolerates_no_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.create(Primitive::Cube, "other").unwrap();
    let other = scene.shapes()[1].id();
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene.project_uv_planar(other, UvProjectAxis::Y).unwrap();
    scene.delete_uvs(shape).unwrap();
    assert!(scene.uv_set(shape).is_none());
    assert!(scene.uv_set(other).is_some());
    scene.delete_uvs(shape).unwrap();
}

#[test]
fn removing_a_node_drops_its_uvs_and_stays_valid() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let node = scene.nodes()[0].id();
    scene.remove(node).unwrap();
    assert!(scene.uv_set(shape).is_none());
    scene.validate().unwrap();
}

#[test]
fn extrude_is_refused_while_the_shape_has_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.extrude_face(shape, 0, 1.0),
        Err(NativeError::UvsBlockTopologyEdit)
    );
    assert!(scene == before);
    scene.delete_uvs(shape).unwrap();
    scene.extrude_face(shape, 0, 1.0).unwrap();
}

#[test]
fn validate_rejects_uvs_that_no_longer_match_the_faces() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene
        .session_uvs
        .sets
        .get_mut(&shape)
        .unwrap()
        .faces
        .pop()
        .unwrap();
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn validate_rejects_non_finite_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene.session_uvs.sets.get_mut(&shape).unwrap().faces[0][0][0] = f64::NAN;
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn authored_writers_refuse_a_scene_with_session_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    assert_eq!(check_content_kind(&scene), Ok(()));
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    assert_eq!(
        check_content_kind(&scene),
        Err(AuthoredIneligible::UnsupportedMeshChannels)
    );
}

#[test]
fn the_legacy_view_refuses_rather_than_dropping_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    assert!(scene.supported_view().is_ok());
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    assert_eq!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView("session UVs"))
    );
}
