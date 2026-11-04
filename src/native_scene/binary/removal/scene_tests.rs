use super::tests::{connections, ids, link, shape, transform, two_objects};
use super::*;
use crate::history::{Command, History};
use crate::native_scene::{
    Channel, DeleteUnsupported, NativeError, NativeScene,
    binary::{
        decode,
        tests::{appearance_fixture, site_fixture::*, wire_fixture::scene},
    },
};

fn refused(reason: RemovalRefusal) -> Result<(), NativeError> {
    Err(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
        reason,
    )))
}

fn opened() -> NativeScene {
    decode(two_objects(&[])).unwrap()
}

#[test]
fn removing_an_opened_node_drops_it_in_memory_and_keeps_the_source_bytes() {
    let mut scene = opened();
    let bytes = scene.source.as_ref().unwrap().bytes.clone();
    let (a, b) = (scene.nodes[0].id, scene.nodes[1].id);
    assert_eq!(scene.remove(a), Ok(()));
    scene.validate().unwrap();
    assert_eq!((scene.nodes.len(), scene.shapes.len()), (1, 1));
    assert_eq!(scene.removed, ids(&[1, 2]));
    assert!(Arc::ptr_eq(&scene.source.as_ref().unwrap().bytes, &bytes));
    assert_eq!(scene.nodes[0].id, b);
    assert_eq!(scene.nodes[0].transform_id, NativeId(3));
    assert_eq!(scene.shapes[0].id, NativeId(4));
    assert_eq!(scene.remove_many(&[b, b]), Ok(()));
    assert_eq!(scene.removed, ids(&[1, 2, 3, 4]));
}

#[test]
fn a_refused_removal_leaves_the_scene_untouched() {
    let linked = link(1, "as.iog.og[0]", ":initialShadingGroup.dsm[0]");
    let mut scene = decode(two_objects(&connections(&[linked]))).unwrap();
    let before = scene.clone();
    let a = scene.nodes[0].id;
    assert_eq!(scene.remove(a), refused(RemovalRefusal::NextAvailableIndex));
    assert_eq!(scene, before);
    // `remove_many` keeps the members removed before the first refusal, as documented.
    let b = scene.nodes[1].id;
    assert_eq!(
        scene.remove_many(&[b, a]),
        refused(RemovalRefusal::NextAvailableIndex)
    );
    assert_eq!(scene.removed, ids(&[3, 4]));
}

#[test]
fn undo_and_redo_restore_the_removal_exactly() {
    let scene = opened();
    let a = scene.nodes[0].id;
    let mut history = History::new(scene.clone());
    history.execute(Command::Remove(a)).unwrap();
    let removed = history.scene().clone();
    assert_eq!(removed.removed, ids(&[1, 2]));
    assert!(history.undo());
    assert_eq!(*history.scene(), scene);
    assert!(history.redo());
    assert_eq!(*history.scene(), removed);
}

#[test]
fn a_pending_insertion_on_another_node_blocks_a_removal() {
    let t = |v| triple_aux(b"t", SITE_AUX, v);
    let children = [
        sited_transform(b"a", &[]),
        sited_transform(b"b", &[t([7.0; 3]), triple_aux(b"r", SITE_AUX, [1.0; 3])]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut scene = decode(scene(&children)).unwrap();
    let (a, b) = (scene.nodes[0].id, scene.nodes[1].id);
    scene.set_channel(a, Channel::TranslateX, 5.0).unwrap();
    let before = scene.clone();
    assert_eq!(scene.remove(b), refused(RemovalRefusal::PendingInsertion));
    assert_eq!(scene, before);
    assert_eq!(scene.remove(a), Ok(()));
    scene.validate().unwrap();
}

#[test]
fn an_unshaded_object_leaves_a_sourced_scene_with_appearance_valid() {
    let extra = [transform(b"plain"), shape(b"plainShape", b"plain")].concat();
    let bytes = appearance_fixture::appearance_specimen_with_fields(appearance_fixture::Fields {
        groups: &extra,
        ..Default::default()
    });
    let mut scene = decode(bytes.into()).unwrap();
    let plain = scene.nodes.iter().find(|n| n.name == "plain").unwrap().id;
    let before = scene.appearance_readiness();
    assert_eq!(scene.remove(plain), Ok(()));
    scene.validate().unwrap();
    assert_eq!(scene.shapes.len(), 1);
    assert_eq!(scene.appearance_readiness().shapes, before.shapes - 1);
}

#[test]
fn a_diffuse_shader_left_without_a_user_is_refused() {
    let scene = decode(appearance_fixture::appearance_specimen_with_users(2).into()).unwrap();
    let ids: Vec<_> = scene.shapes.iter().map(|shape| shape.id).collect();
    let mut shared = (**scene.source.as_ref().unwrap().appearance.as_ref().unwrap()).clone();
    let material = shared.shapes[0].binding.as_ref().unwrap().material;
    shared.shapes[1].binding.as_mut().unwrap().material = material;
    assert_eq!(check_materials(&shared, &ids[1..]), Ok(()));
    shared.diffuse[0].shapes = vec![ids[1]];
    assert_eq!(
        check_materials(&shared, &ids[1..]),
        Err(RemovalRefusal::LastDiffuseUser)
    );
}
