use azteccraft::{
    native_scene::{Channel, NativeError, NativeScene, NativeTransform, Points},
    primitive_options::SINGLE_QUAD_PLANE,
    scene::{Primitive, Scene, SceneError, Transform},
};
use glam::Vec3;

fn shape(
    scene: &NativeScene,
    transform: azteccraft::scene::NodeId,
) -> &azteccraft::native_scene::NativeMeshShape {
    let native_id = scene.node(transform).unwrap().transform_id();
    let shape_id = scene
        .dag_edges()
        .iter()
        .find(|edge| edge.parent() == Some(native_id))
        .unwrap()
        .child();
    scene.shape(shape_id).unwrap()
}

#[test]
fn legacy_promotion_preserves_authoring_and_generates_distinct_migration_identities() {
    let mut legacy = Scene::default();
    let id = legacy.create(Primitive::Cube, "legacy name").unwrap();
    legacy
        .set_transform(
            id,
            Transform {
                translation: Vec3::new(2.0, -3.0, 4.0),
                rotation_degrees: Vec3::new(90.0, 90.0, 0.0),
                scale: Vec3::new(-2.0, 3.0, 0.5),
            },
        )
        .unwrap();
    let native = NativeScene::from_legacy(legacy.clone()).unwrap();
    assert_eq!(native.supported_view().unwrap(), legacy);
    let node = native.node(id).unwrap();
    let mesh_shape = shape(&native, id);
    assert_ne!(node.transform_id(), mesh_shape.id());
    assert_ne!(node.transform_uuid(), mesh_shape.uuid());
    assert!(!node.transform_uuid().unwrap().is_nil());
    assert_eq!(mesh_shape.name(), "legacy nameShape");
    assert!(matches!(mesh_shape.mesh().points(), Points::Float3(_)));
    assert_eq!(
        native.clone().node(id).unwrap().transform_uuid(),
        node.transform_uuid()
    );
}

#[test]
fn channel_precision_signed_zero_and_view_failure_do_not_corrupt_authoring() {
    let mut native = NativeScene::default();
    let id = native.create(Primitive::Plane, "pPlane1").unwrap();
    let a = 1.0 + 1e-10;
    let b = 1.0 + 2e-10;
    assert_eq!(a as f32, b as f32);
    native
        .set_transform(
            id,
            NativeTransform {
                translation: [a, b, -0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    native.set_channel(id, Channel::TranslateX, 3.0).unwrap();
    assert_eq!(
        native.node(id).unwrap().transform().translation[1].to_bits(),
        b.to_bits()
    );
    assert_eq!(
        native.node(id).unwrap().transform().translation[2].to_bits(),
        (-0.0f64).to_bits()
    );
    native
        .set_channel(id, Channel::TranslateX, f64::MAX)
        .unwrap();
    native.validate().unwrap();
    assert!(matches!(
        native.supported_view(),
        Err(NativeError::UnsupportedView(_))
    ));
    assert_eq!(
        native.node(id).unwrap().transform().translation[0],
        f64::MAX
    );
    let before = native.clone();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            native.set_channel(id, Channel::RotateY, value),
            Err(NativeError::NonFinite)
        );
        assert_eq!(native, before);
    }
}

#[test]
fn snapshot_restore_preserves_reserved_session_and_native_identities() {
    let mut native = NativeScene::default();
    let saved = native.clone();
    let discarded = native.create(Primitive::Cube, "pCube1").unwrap();
    let old_native_id = native.node(discarded).unwrap().transform_id();
    native.restore_authoring(saved.clone());
    assert_eq!(native, saved);
    let replacement = native.create(Primitive::Plane, "pPlane1").unwrap();
    assert_ne!(replacement, discarded);
    assert_ne!(
        native.node(replacement).unwrap().transform_id(),
        old_native_id
    );
}

#[test]
fn arbitrary_duplicate_legacy_names_survive_channel_edits_and_snapshot_restore() {
    let mut legacy = Scene::default();
    let id = legacy.create(Primitive::Cube, "name with spaces").unwrap();
    legacy.create(Primitive::Plane, "name with spaces").unwrap();
    let mut native = NativeScene::from_legacy(legacy).unwrap();
    let before = native.clone();
    native
        .set_channel(id, Channel::TranslateY, 2.000000000001)
        .unwrap();
    assert_eq!(native.node(id).unwrap().name(), "name with spaces");
    native.restore_authoring(before.clone());
    assert_eq!(native, before);
    assert_eq!(
        native.rename(id, "invalid name"),
        Err(NativeError::InvalidName)
    );
    assert_eq!(native, before);
    native.rename(id, "pCube1").unwrap();
    assert_eq!(shape(&native, id).name(), "name with spacesShape");
    let changed = native.clone();
    assert_eq!(
        native.create(Primitive::Plane, "pCube1"),
        Err(NativeError::NameCollision)
    );
    assert_eq!(native, changed);
}

#[test]
fn native_extrusion_matches_legacy_delta_without_rebuilding_existing_edges() {
    for distance in [0.0, 1.25, -0.75] {
        let mut legacy = Scene::default();
        let id = legacy.create(Primitive::Cube, "pCube1").unwrap();
        let mut native = NativeScene::from_legacy(legacy.clone()).unwrap();
        let shape_id = shape(&native, id).id();
        let edges = native.shape(shape_id).unwrap().mesh().edges().to_vec();
        let faces = native.shape(shape_id).unwrap().mesh().faces().to_vec();
        let points = native.shape(shape_id).unwrap().mesh().points().clone();
        native.extrude_face(shape_id, 1, distance).unwrap();
        legacy.extrude_face(id, 1, distance as f32).unwrap();
        assert_eq!(native.supported_view().unwrap(), legacy);
        let mesh = native.shape(shape_id).unwrap().mesh();
        assert_eq!(&mesh.edges()[..edges.len()], edges.as_slice());
        for (index, face) in faces.iter().enumerate() {
            if index != 1 {
                assert_eq!(&mesh.faces()[index], face);
            }
        }
        let (Points::Float3(old), Points::Float3(new)) = (&points, mesh.points()) else {
            panic!()
        };
        assert_eq!(&new[..old.len()], old.as_slice());
        assert_eq!(new.len(), old.len() + 4);
    }
}

#[test]
fn failures_preserve_contents_and_allocator_before_next_real_create() {
    let mut native = NativeScene::default();
    assert_eq!(
        native.create(Primitive::Cube, "bad name"),
        Err(NativeError::InvalidName)
    );
    let id = native
        .create_with_options(Primitive::Plane, &SINGLE_QUAD_PLANE, "pPlane1")
        .unwrap();
    let shape_id = shape(&native, id).id();
    assert_eq!(id.to_decimal(), "1");
    let before = native.clone();
    assert_eq!(
        native.extrude_face(shape_id, 0, 1.0),
        Err(NativeError::Legacy(
            SceneError::UnsupportedAllFacesExtrusion
        ))
    );
    assert_eq!(native, before);
    assert_eq!(
        native.extrude_face(shape_id, 0, f64::NAN),
        Err(NativeError::Legacy(SceneError::InvalidExtrusionDistance))
    );
    assert_eq!(native, before);
    native.remove(id).unwrap();
    assert_eq!(
        native
            .create(Primitive::Cube, "pCube1")
            .unwrap()
            .to_decimal(),
        "2"
    );
}
