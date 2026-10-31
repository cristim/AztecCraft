use super::*;
use crate::native_scene::authored::{AuthoredIneligible, check_content_kind};
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
fn each_axis_reads_the_two_other_object_axes() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    for (axis, plane) in [
        (UvProjectAxis::X, [2, 1]),
        (UvProjectAxis::Y, [0, 2]),
        (UvProjectAxis::Z, [0, 1]),
    ] {
        scene.project_uv_planar(shape, axis).unwrap();
        let mesh = scene.shape(shape).unwrap().mesh();
        let set = scene.uv_set(shape).unwrap();
        for (face, corners) in mesh.faces().iter().zip(set.faces()) {
            for (directed, uv) in face.outer().iter().zip(corners) {
                let point = mesh
                    .effective_point_cm(mesh.directed_start(*directed).unwrap())
                    .unwrap();
                assert_eq!(*uv, [point[plane[0]] + 0.5, point[plane[1]] + 0.5]);
            }
        }
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
