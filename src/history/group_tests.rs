use super::*;
use crate::native_scene::{GroupPivot, GroupUnder};
use crate::scene::Primitive;

const GROUP: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Center,
    preserve_position: true,
};
const UNGROUP: UngroupOptions = UngroupOptions {
    under: GroupUnder::Parent,
    keep: ReparentKeep::World,
};

fn two_cubes() -> (History, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let a = scene.create(Primitive::Cube, "a").unwrap();
    let b = scene.create(Primitive::Cube, "b").unwrap();
    (History::new(scene), a, b)
}

fn group(history: &mut History, ids: &[NodeId]) -> NodeId {
    match history
        .execute(Command::Group {
            ids: ids.to_vec(),
            options: GROUP,
        })
        .unwrap()
    {
        EditOutcome::Created(group) => group,
        other => panic!("{other:?}"),
    }
}

#[test]
fn group_is_one_named_undo_step_that_restores_the_scene_exactly() {
    let (mut history, a, b) = two_cubes();
    let before = history.scene().clone();
    let group = group(&mut history, &[a, b]);
    assert_eq!(history.undo_label().as_deref(), Some("Group"));
    assert_eq!(history.scene().nodes().len(), 3);
    let grouped = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
    assert!(!history.can_undo());
    assert!(history.redo());
    assert_eq!(history.scene(), &grouped);
    assert_eq!(history.scene().node(group).unwrap().name(), "group1");
}

#[test]
fn ungroup_is_one_named_undo_step_and_reports_the_moved_children() {
    let (mut history, a, b) = two_cubes();
    let group = group(&mut history, &[a, b]);
    let grouped = history.scene().clone();
    let outcome = history
        .execute(Command::Ungroup {
            ids: vec![group],
            options: UNGROUP,
        })
        .unwrap();
    assert_eq!(outcome, EditOutcome::Nodes(vec![a, b]));
    assert_eq!(history.undo_label().as_deref(), Some("Ungroup"));
    assert_eq!(history.scene().nodes().len(), 2);
    let flat = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &grouped);
    assert!(history.redo());
    assert_eq!(history.scene(), &flat);
}

#[test]
fn a_refused_group_or_ungroup_changes_neither_scene_nor_stacks() {
    let (mut history, a, _) = two_cubes();
    history
        .execute(Command::Rename {
            id: a,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    let (scene, revision) = (history.scene().clone(), history.revision());
    let (undo, redo) = (history.undo.len(), history.redo.len());
    for command in [
        Command::Group {
            ids: Vec::new(),
            options: GROUP,
        },
        Command::Ungroup {
            ids: vec![a],
            options: UNGROUP,
        },
    ] {
        assert!(history.execute(command).is_err());
        assert_eq!(history.scene(), &scene);
        assert_eq!(history.revision(), revision);
        assert_eq!((history.undo.len(), history.redo.len()), (undo, redo));
    }
}

#[test]
fn an_open_chunk_merges_group_with_a_following_move_into_one_step() {
    let (mut history, a, b) = two_cubes();
    let before = history.scene().clone();
    history.open_chunk("Group and move").unwrap();
    let group = group(&mut history, &[a, b]);
    history
        .execute(Command::SetTransform {
            id: group,
            transform: NativeTransform {
                translation: [0.0, 5.0, 0.0],
                ..NativeTransform::default()
            },
        })
        .unwrap();
    history.close_chunk().unwrap();
    assert_eq!(history.undo.len(), 1);
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
}
