use super::tests::{connections, ids, link, shape, transform, tree, two_objects};
use super::*;
use crate::history::{Command, History};
use crate::native_scene::{
    Channel, DeleteUnsupported, NativeError, NativeScene,
    binary::{
        decode,
        tests::{
            appearance_fixture,
            site_fixture::*,
            wire_fixture::{chunk, creation, group, mesh_payload, scene},
        },
    },
};
use crate::scene::NodeId;

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
fn an_unrecognised_record_in_a_connection_group_refuses_the_delete_and_changes_nothing() {
    let prnt = chunk(b"PRNT", b"\0as\0b\0", 8);
    let extra = group(b"CONS", &group(b"CONN", &prnt, 4), 4);
    let mut scene = decode(two_objects(&extra)).unwrap();
    let before = scene.clone();
    let a = scene.nodes[0].id;
    assert_eq!(
        scene.remove(a),
        refused(RemovalRefusal::UnrecognisedRecord(*b"PRNT"))
    );
    assert_eq!(scene, before);
}

#[test]
fn removing_a_shape_drops_it_from_the_diffuse_users() {
    let mut scene = decode(appearance_fixture::appearance_specimen_with_users(2).into()).unwrap();
    let (kept, gone) = (scene.shapes[0].id, scene.shapes[1].id);
    scene.apply_removal(&[gone]);
    let appearance = scene.source.as_ref().unwrap().appearance.as_ref().unwrap();
    assert_eq!(appearance.diffuse[0].shapes, [kept]);
}

#[test]
fn removing_nothing_from_an_opened_scene_changes_nothing() {
    let mut scene = opened();
    let before = scene.clone();
    assert_eq!(scene.remove_many(&[]), Ok(()));
    assert_eq!(scene, before);
}

#[test]
fn a_refused_removal_leaves_the_scene_untouched() {
    let linked = link(1, "as.iog.og[0]", ":initialShadingGroup.dsm[0]");
    let mut scene = decode(two_objects(&connections(&[linked]))).unwrap();
    let before = scene.clone();
    let a = scene.nodes[0].id;
    assert_eq!(scene.remove(a), refused(RemovalRefusal::NextAvailableIndex));
    assert_eq!(scene, before);
    // A sourced `remove_many` analyses the union once and is all or nothing; it names the member
    // whose own removal is refused.
    let b = scene.nodes[1].id;
    let name = scene.nodes[0].name.clone();
    assert_eq!(
        scene.remove_many(&[b, a]),
        Err(NativeError::DeleteMemberRefused {
            node: name,
            reason: Box::new(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
                RemovalRefusal::NextAvailableIndex
            ))),
        })
    );
    assert_eq!(scene, before);
}

/// A shape of `a` carries text that names `b`: once `a` is gone for good, it no longer stops `b`.
#[test]
fn an_earlier_removal_is_part_of_the_union_for_the_next_one() {
    let text = chunk(b"STR ", b"dcc\0\x20see b.t\0", 8);
    let mesh = chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8);
    let a_shape = group(
        b"DMSH",
        &[creation(b"as", Some(b"a")), text, mesh].concat(),
        4,
    );
    let bytes = scene(
        &[
            transform(b"a"),
            a_shape,
            transform(b"b"),
            shape(b"bs", b"b"),
        ]
        .concat(),
    );
    let opened = decode(bytes).unwrap();
    let (a, b) = (opened.nodes[0].id, opened.nodes[1].id);
    let mut by_b = opened.clone();
    assert_eq!(by_b.remove(b), refused(RemovalRefusal::TextReference));
    let mut both = opened.clone();
    assert_eq!(both.remove_many(&[a, b]), Ok(()));
    let mut one_by_one = opened;
    assert_eq!(one_by_one.remove(a), Ok(()));
    assert_eq!(one_by_one.remove(b), Ok(()));
    assert_eq!(one_by_one, both);
    both.validate().unwrap();
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
fn an_edit_needing_an_insertion_is_refused_after_a_delete_and_changes_nothing() {
    let children = [
        sited_transform(b"a", &[triple_aux(b"t", SITE_AUX, [7.0; 3])]),
        sited_transform(b"c", &[triple_aux(b"r", SITE_AUX, [1.0; 3])]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut scene = decode(scene(&children)).unwrap();
    let (a, c) = (scene.nodes[0].id, scene.nodes[1].id);
    assert_eq!(
        scene.clone().set_channel(c, Channel::TranslateX, 5.0),
        Ok(())
    );
    scene.remove(a).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.set_channel(c, Channel::TranslateX, 5.0),
        Err(NativeError::InsertionAfterDelete)
    );
    assert_eq!(scene, before);
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

#[test]
fn a_group_a_child_and_ancestor_with_descendant_delete_from_an_opened_scene() {
    let opened = decode(tree(&[])).unwrap();
    let id = |name: &str| opened.nodes.iter().find(|n| n.name == name).unwrap().id;
    let (g, c) = (id("g"), id("c"));
    for (targets, gone) in [
        (vec![g], ids(&[1, 2, 3])),
        (vec![c], ids(&[2, 3])),
        (vec![g, c], ids(&[1, 2, 3])),
        (vec![c, g], ids(&[1, 2, 3])),
    ] {
        let mut scene = opened.clone();
        scene.remove_many(&targets).unwrap();
        scene.validate().unwrap();
        assert_eq!(scene.removed, gone, "{targets:?}");
        let mut history = History::new(opened.clone());
        history.execute(Command::RemoveMany(targets)).unwrap();
        assert_eq!(*history.scene(), scene);
        assert!(history.undo());
        assert!(history.scene().removed.is_empty());
        assert_eq!(*history.scene(), opened);
    }
}

/// `a` (indexed set link) and `b` (unindexed set link) share a set, so each is refused alone but
/// the pair is fine; `r` is
/// restricted, so the whole set is refused with `UnsupportedEdit`. Only `r` reproduces that error.
fn linked_pair_and_restricted_node() -> (NativeScene, [NodeId; 3]) {
    let bytes = scene(
        &[
            transform(b"a"),
            shape(b"as", b"a"),
            transform(b"b"),
            shape(b"bs", b"b"),
            transform(b"r"),
            shape(b"rs", b"r"),
            connections(&[link(0, "as.iog", ":S.dsm[0]"), link(1, "bs.iog", ":S.dsm")]),
        ]
        .concat(),
    );
    let mut scene = decode(bytes).unwrap();
    scene.nodes[2]
        .unsupported_transform
        .push("unproved effect".into());
    let ids = [scene.nodes[0].id, scene.nodes[1].id, scene.nodes[2].id];
    (scene, ids)
}

#[test]
fn a_member_refused_alone_for_another_reason_is_not_named() {
    let (mut scene, [a, b, r]) = linked_pair_and_restricted_node();
    assert_eq!(scene.remove(a), refused(RemovalRefusal::NextAvailableIndex));
    assert_eq!(scene.remove_many(&[a, b]), Ok(()));
    let (mut scene, _) = linked_pair_and_restricted_node();
    let before = scene.clone();
    assert_eq!(
        scene.remove_many(&[a, b, r]),
        Err(NativeError::DeleteMemberRefused {
            node: "r".into(),
            reason: Box::new(NativeError::UnsupportedEdit),
        })
    );
    assert_eq!(scene, before);
}

#[test]
fn a_combined_refusal_no_member_reproduces_stays_unnamed() {
    let (scene, ids) = linked_pair_and_restricted_node();
    assert_eq!(
        scene.name_refusing_member(&ids, NativeError::NameCollision),
        NativeError::NameCollision
    );
}

/// `members` plain objects followed by the restricted `r`, which alone explains the refusal.
fn many_objects_and_a_restricted_one(members: usize) -> (NativeScene, Vec<NodeId>) {
    let names: Vec<String> = (0..members).map(|at| format!("n{at}")).collect();
    let children: Vec<u8> = names
        .iter()
        .flat_map(|name| {
            let shape_name = format!("{name}s");
            [
                transform(name.as_bytes()),
                shape(shape_name.as_bytes(), name.as_bytes()),
            ]
            .concat()
        })
        .collect();
    let mut scene = decode(scene(&children)).unwrap();
    scene.nodes[members - 1]
        .unsupported_transform
        .push("unproved effect".into());
    let ids = scene.nodes.iter().map(|node| node.id).collect();
    (scene, ids)
}

/// Attribution decodes the source once and never holds every leave-one-out set at once (the old
/// shape built N+N sets of up to N ids, about 128 MiB of ids at 4096 members).
#[test]
fn naming_a_refusing_member_decodes_once_and_holds_one_candidate_set() {
    let (scene, ids) = many_objects_and_a_restricted_one(48);
    crate::native_scene::attribution_probe::reset();
    assert_eq!(
        scene.name_refusing_member(&ids, NativeError::UnsupportedEdit),
        NativeError::DeleteMemberRefused {
            node: "n47".into(),
            reason: Box::new(NativeError::UnsupportedEdit),
        }
    );
    let (decodes, checks, held) = crate::native_scene::attribution_probe::counts();
    assert_eq!((decodes, held), (1, 1));
    assert!(checks <= ids.len() + 1, "{checks} checks");
}

/// `a` and `b` share set `S` (fine as a pair); `c` shares set `T` with the survivor `d`. A, b and c
/// each fail alone with `NextAvailableIndex`, but only the set without `c` is accepted.
#[test]
fn only_the_member_whose_absence_clears_the_refusal_is_named() {
    let bytes = scene(
        &[
            transform(b"a"),
            shape(b"as", b"a"),
            transform(b"b"),
            shape(b"bs", b"b"),
            transform(b"c"),
            shape(b"cs", b"c"),
            transform(b"d"),
            shape(b"ds", b"d"),
            connections(&[
                link(0, "as.iog", ":S.dsm[0]"),
                link(1, "bs.iog", ":S.dsm"),
                link(0, "cs.iog", ":T.dsm[0]"),
                link(1, "ds.iog", ":T.dsm"),
            ]),
        ]
        .concat(),
    );
    let mut scene = decode(bytes).unwrap();
    let [a, b, c] = [0, 1, 2].map(|at| scene.nodes[at].id);
    let before = scene.clone();
    assert_eq!(scene.remove(a), refused(RemovalRefusal::NextAvailableIndex));
    assert_eq!(scene.remove_many(&[a, b]), Ok(()));
    let mut scene = before.clone();
    assert_eq!(
        scene.remove_many(&[a, b, c]),
        Err(NativeError::DeleteMemberRefused {
            node: "c".into(),
            reason: Box::new(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
                RemovalRefusal::NextAvailableIndex
            ))),
        })
    );
    assert_eq!(scene, before);
}

/// The pending insertion sits on `x`, outside the removed set, so it blocks every subset and no
/// member explains the refusal.
#[test]
fn a_pending_insertion_on_a_third_node_leaves_a_multi_delete_refusal_unnamed() {
    let children = [
        sited_transform(b"a", &[]),
        sited_transform(b"b", &[]),
        sited_transform(b"x", &[]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut scene = decode(scene(&children)).unwrap();
    let [a, b, x] = [0, 1, 2].map(|at| scene.nodes[at].id);
    scene.set_channel(x, Channel::TranslateX, 5.0).unwrap();
    let before = scene.clone();
    assert_eq!(scene.remove(a), refused(RemovalRefusal::PendingInsertion));
    assert_eq!(scene.remove(b), refused(RemovalRefusal::PendingInsertion));
    assert_eq!(
        scene.remove_many(&[a, b]),
        refused(RemovalRefusal::PendingInsertion)
    );
    assert_eq!(scene, before);
}

/// The scene-wide `PendingInsertion` refusal outlives every subset, so no member can explain it:
/// the refused Delete is answered after its one source decode, with no attribution check.
#[test]
fn a_scene_wide_pending_insertion_refusal_runs_no_attribution_checks() {
    let children = [
        sited_transform(b"a", &[]),
        sited_transform(b"b", &[]),
        sited_transform(b"x", &[]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut scene = decode(scene(&children)).unwrap();
    let [a, b, x] = [0, 1, 2].map(|at| scene.nodes[at].id);
    scene.set_channel(x, Channel::TranslateX, 5.0).unwrap();
    crate::native_scene::attribution_probe::reset();
    assert_eq!(
        scene.remove_many(&[a, b]),
        refused(RemovalRefusal::PendingInsertion)
    );
    let (decodes, checks, held) = crate::native_scene::attribution_probe::counts();
    assert_eq!((decodes, checks, held), (1, 0, 0));
}
