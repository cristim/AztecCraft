use super::*;
use crate::native_scene::Channel;
use crate::scene::Primitive;

pub(super) fn bare(names: &[&str]) -> NativeScene {
    let mut scene = NativeScene::default();
    for name in names {
        scene.create(Primitive::Cube, name).unwrap();
    }
    scene.shapes.clear();
    scene.dag.retain(|edge| edge.parent.is_none());
    scene
}

pub(super) fn meshed(primitives: &[Primitive]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (index, primitive) in primitives.iter().enumerate() {
        scene.create(*primitive, &format!("mesh{index}")).unwrap();
    }
    scene
}

const PRIMITIVES: [Primitive; 5] = [
    Primitive::Cube,
    Primitive::Plane,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Cone,
];

/// Encode, decode with the repo's decoder and compare node by node.
fn assert_round_trip(scene: &NativeScene) {
    let bytes = encode_authored(scene).unwrap();
    let reopened = crate::native_scene::binary::decode(std::sync::Arc::from(bytes)).unwrap();
    assert_eq!(reopened.units(), scene.units());
    assert_eq!(reopened.nodes().len(), scene.nodes().len());
    assert_eq!(reopened.shapes().len(), scene.shapes().len());
    for (before, after) in scene.nodes().iter().zip(reopened.nodes()) {
        assert_eq!(before.name(), after.name());
        let (a, b) = (
            scene.internal_transform(before.id()).unwrap(),
            reopened.internal_transform(after.id()).unwrap(),
        );
        for (x, y) in [
            (a.translation_cm, b.translation_cm),
            (a.rotation_radians, b.rotation_radians),
            (a.scale, b.scale),
        ] {
            assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
        }
        let shape = authored::shape_of(scene, before).unwrap();
        let again = authored::shape_of(&reopened, after).unwrap();
        assert_eq!(shape.name(), again.name());
        assert_eq!(shape.mesh().points(), again.mesh().points());
        let ends = |s: &crate::native_scene::NativeMeshShape| {
            s.mesh()
                .edges()
                .iter()
                .map(|e| e.endpoints())
                .collect::<Vec<_>>()
        };
        assert_eq!(ends(shape), ends(again));
        let loops = |s: &crate::native_scene::NativeMeshShape| {
            s.mesh()
                .faces()
                .iter()
                .map(|f| f.outer().to_vec())
                .collect::<Vec<_>>()
        };
        assert_eq!(loops(shape), loops(again));
    }
}

#[test]
fn every_primitive_encodes_decodes_and_matches() {
    for primitive in PRIMITIVES {
        assert_round_trip(&meshed(&[primitive]));
    }
    assert_round_trip(&meshed(&PRIMITIVES));
}

#[test]
fn every_primitive_still_matches_after_a_translate_rotate_and_scale() {
    for primitive in PRIMITIVES {
        let mut scene = meshed(&[primitive]);
        let id = scene.nodes()[0].id();
        for (channel, value) in [
            (Channel::TranslateX, 3.5),
            (Channel::TranslateZ, -0.125),
            (Channel::RotateY, 30.0),
            (Channel::ScaleY, 2.0),
        ] {
            scene.set_channel(id, channel, value).unwrap();
        }
        assert_round_trip(&scene);
    }
}

#[test]
fn encoding_is_deterministic_for_a_scene_with_stored_uuids() {
    let scene = meshed(&PRIMITIVES);
    assert_eq!(encode_authored(&scene), encode_authored(&scene));
}

#[test]
fn a_scene_of_bare_transforms_and_an_empty_scene_encode() {
    assert_round_trip_bare(&bare(&["a", "b"]));
    assert_round_trip_bare(&NativeScene::default());
}

fn assert_round_trip_bare(scene: &NativeScene) {
    let bytes = encode_authored(scene).unwrap();
    let reopened = crate::native_scene::binary::decode(std::sync::Arc::from(bytes)).unwrap();
    assert_eq!(reopened.nodes().len(), scene.nodes().len());
    assert!(reopened.shapes().is_empty());
}

#[test]
fn eligibility_refusals_come_first() {
    let mut cube = NativeScene::default();
    cube.create(Primitive::Cube, "persp").unwrap();
    for scene in [cube, bare(&["persp"])] {
        assert_eq!(
            encode_authored(&scene),
            Err(AuthoredBinaryRefusal::Ineligible(
                AuthoredIneligible::ReservedName
            ))
        );
    }
}

pub(super) fn bare_numbered(count: usize) -> NativeScene {
    let names: Vec<String> = (0..count).map(|i| format!("node{i}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut scene = bare(&names);
    for (i, node) in scene.nodes.iter_mut().enumerate() {
        let f = i as f64;
        node.channels[0].value = f + 0.5;
        node.channels[1].value = -f;
        node.channels[4].value = 45.0 + f;
        node.channels[6].value = 2.0;
    }
    scene
}

#[test]
fn a_bare_transform_file_reopens_in_the_decoder_with_the_same_nodes() {
    let scene = bare_numbered(3);
    let bytes = scene_file(&scene).unwrap();
    let reopened = crate::native_scene::binary::decode(std::sync::Arc::from(bytes)).unwrap();
    assert_eq!(reopened.nodes().len(), 3);
    assert!(reopened.shapes().is_empty());
    assert_eq!(reopened.units(), scene.units());
    for (before, after) in scene.nodes().iter().zip(reopened.nodes()) {
        assert_eq!(before.name(), after.name());
        let (a, b) = (
            scene.internal_transform(before.id()).unwrap(),
            reopened.internal_transform(after.id()).unwrap(),
        );
        for (x, y) in [
            (a.translation_cm, b.translation_cm),
            (a.rotation_radians, b.rotation_radians),
            (a.scale, b.scale),
        ] {
            assert_eq!(x.map(f64::to_bits), y.map(f64::to_bits));
        }
    }
}

#[test]
fn a_node_without_a_stored_uuid_is_refused() {
    let mut scene = bare_numbered(2);
    scene.nodes[1].transform_uuid = None;
    assert_eq!(scene_file(&scene), Err(AuthoredBinaryRefusal::MissingUuid));
}

#[test]
fn the_self_check_runs_on_the_built_bytes() {
    // The decoder refuses a node name containing `|`, which the builders accept, so only
    // the self-check stands between this scene and returned bytes.
    let mut scene = bare_numbered(2);
    scene.nodes[1].name = "a|b".to_owned();
    assert_eq!(
        scene_file(&scene),
        Err(AuthoredBinaryRefusal::SelfCheckMismatch)
    );
}

#[test]
fn a_meshed_scene_file_reopens_with_every_node_and_shape_in_order() {
    let primitives = [
        Primitive::Cube,
        Primitive::Plane,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ];
    let scene = meshed(&primitives);
    let bytes = scene_file(&scene).unwrap();
    let reopened = crate::native_scene::binary::decode(std::sync::Arc::from(bytes)).unwrap();
    assert_eq!(reopened.nodes().len(), 5);
    assert_eq!(reopened.shapes().len(), 5);
    for (before, after) in scene.shapes().iter().zip(reopened.shapes()) {
        assert_eq!(before.name(), after.name());
        assert_eq!(before.mesh().points(), after.mesh().points());
    }
}

#[test]
fn a_shape_without_a_stored_uuid_is_refused() {
    let mut scene = meshed(&[Primitive::Cube]);
    scene.shapes[0].uuid = None;
    assert_eq!(scene_file(&scene), Err(AuthoredBinaryRefusal::MissingUuid));
}

fn with_material(assign: bool) -> NativeScene {
    use crate::native_scene::MaterialChoice;
    let mut scene = meshed(&[Primitive::Cube]);
    let color = [0.25, 0.5, 0.75];
    if assign {
        let shape = scene.shapes()[0].id();
        scene
            .assign_session_material(shape, MaterialChoice::New { color })
            .unwrap();
    } else {
        scene.create_session_material(color).unwrap();
    }
    scene
}

#[test]
fn a_shape_assigned_a_non_initial_shading_group_is_refused() {
    assert_eq!(
        encode_authored(&with_material(true)),
        Err(AuthoredBinaryRefusal::MaterialAssigned)
    );
}

#[test]
fn a_created_but_unassigned_material_does_not_refuse() {
    encode_authored(&with_material(false)).unwrap();
}

#[test]
fn reassigning_back_to_the_initial_group_does_not_refuse() {
    use crate::native_scene::MaterialChoice;
    let mut scene = with_material(true);
    let shape = scene.shapes()[0].id();
    let initial = scene.session_material_id(&scene.shapes()[0]).unwrap();
    assert_eq!(scene.session_shading_groups().len(), 2);
    let default = crate::native_scene::SessionMaterialId::from_index(0);
    assert_ne!(initial, default);
    scene
        .assign_session_material(shape, MaterialChoice::Existing(default))
        .unwrap();
    encode_authored(&scene).unwrap();
}

#[test]
fn a_scene_of_root_joints_is_refused_not_written_as_bare_transforms() {
    let request = crate::native_scene::JointRequest {
        parent: None,
        position: [1.0, 2.0, 3.0],
        relative: false,
        radius: 1.0,
        name: None,
    };
    let mut roots = NativeScene::default();
    roots.create_joint(&request).unwrap();
    assert_eq!(
        encode_authored(&roots),
        Err(AuthoredBinaryRefusal::Ineligible(
            AuthoredIneligible::Joints
        ))
    );
    let mut mixed = meshed(&[Primitive::Cube]);
    mixed.create_joint(&request).unwrap();
    assert_eq!(
        encode_authored(&mixed),
        Err(AuthoredBinaryRefusal::Ineligible(
            AuthoredIneligible::Joints
        ))
    );
}
