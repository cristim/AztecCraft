use super::*;
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

#[test]
fn nothing_is_returned_while_emission_is_incomplete() {
    let mut cube = NativeScene::default();
    cube.create(Primitive::Cube, "cube").unwrap();
    for scene in [cube, bare(&["a", "b"]), NativeScene::default()] {
        assert_eq!(
            encode_authored(&scene),
            Err(AuthoredBinaryRefusal::NotYetImplemented)
        );
    }
}

#[test]
fn eligibility_refusals_come_before_the_incomplete_refusal() {
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
    let bytes = transforms_file(&scene).unwrap();
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
    assert_eq!(
        transforms_file(&scene),
        Err(AuthoredBinaryRefusal::MissingUuid)
    );
}
