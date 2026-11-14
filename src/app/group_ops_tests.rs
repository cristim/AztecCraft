//! Issue #42: `Operation::Group` and `Operation::Ungroup` through the shared dispatch path.
//! Native and remote callers reach the same `Command`; the guards, the `commit_pending` policy
//! and the selection rules are the other selection operations'.
use super::tests::remote;
use super::*;
use azteccraft::native_scene::{
    GroupOptions, GroupPivot, GroupUnder, NativeError, ReparentKeep, ReparentRefusal,
    TransformTarget, UngroupOptions,
};

const OPTIONS: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Center,
    preserve_position: true,
};
const UNGROUP: UngroupOptions = UngroupOptions {
    under: GroupUnder::Parent,
    keep: ReparentKeep::World,
};

fn group(generation: Option<u64>) -> Operation {
    Operation::Group {
        options: OPTIONS,
        expected_selection_generation: generation,
    }
}

fn ungroup(generation: Option<u64>) -> Operation {
    Operation::Ungroup {
        options: UNGROUP,
        expected_selection_generation: generation,
    }
}

fn cubes(count: usize) -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    (editor, all)
}

fn baseline(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.selection.clone(),
        editor.selection_mode,
        editor.scene_token,
        editor.history.undo_label(),
        editor.channels.clone(),
        editor.name.clone(),
    )
}

fn shape(editor: &Editor) -> serde_json::Value {
    let mut value = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    value["scene_token"] = serde_json::Value::Null;
    value
}

fn name_of(editor: &Editor, id: NodeId) -> String {
    editor.history.scene().node(id).unwrap().name().to_string()
}

#[test]
fn native_and_remote_group_and_ungroup_reach_the_same_scene() {
    let (mut native, _) = cubes(2);
    let (mut other, _) = cubes(2);
    let generation = Some(other.selection.generation());
    assert!(matches!(
        native.apply_operation(group(None)),
        Ok(OperationReply::Created { .. })
    ));
    assert!(matches!(
        remote(&mut other, group(generation)),
        Ok(OperationReply::Created { .. })
    ));
    assert_eq!(shape(&native), shape(&other));
    assert_eq!(native.history.undo_label().as_deref(), Some("Group"));
    let generation = Some(other.selection.generation());
    let Ok(OperationReply::Nodes { node_ids }) = native.apply_operation(ungroup(None)) else {
        panic!("ungroup");
    };
    assert_eq!(node_ids.len(), 2);
    assert!(matches!(
        remote(&mut other, ungroup(generation)),
        Ok(OperationReply::Nodes { .. })
    ));
    assert_eq!(shape(&native), shape(&other));
    assert_eq!(native.history.undo_label().as_deref(), Some("Ungroup"));
}

#[test]
fn group_selects_the_new_group_and_ungroup_selects_the_released_children() {
    let (mut editor, cubes) = cubes(2);
    let Ok(OperationReply::Created { node_id }) = editor.apply_operation(group(None)) else {
        panic!("group");
    };
    let group_id = NodeId::from_decimal(&node_id).unwrap();
    assert_eq!(name_of(&editor, group_id), "group1");
    assert_eq!(editor.selection.len(), 1);
    assert_eq!(editor.selection.members()[0].node, group_id);
    editor.apply_operation(ungroup(None)).unwrap();
    let selected: Vec<NodeId> = editor.selection.members().iter().map(|t| t.node).collect();
    assert_eq!(selected, vec![cubes[0].node, cubes[1].node]);
    assert!(editor.history.scene().node(group_id).is_err());
}

#[test]
fn a_group_under_a_non_root_parent_is_selected_at_its_full_path() {
    let (mut editor, cubes) = cubes(2);
    let (parent, child) = (cubes[0].node, cubes[1].node);
    editor
        .apply_operation(Operation::Reparent {
            id: child,
            parent: Some(parent),
            keep: ReparentKeep::Local,
        })
        .unwrap();
    let nested = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|t| t.node == child)
        .unwrap();
    editor.set_selection_members_for_test(vec![nested]);
    editor.sync_fields();
    let Ok(OperationReply::Created { node_id }) = editor.apply_operation(group(None)) else {
        panic!("group");
    };
    let group_id = NodeId::from_decimal(&node_id).unwrap();
    let selected = editor.selection.members()[0].clone();
    assert_eq!(selected.node, group_id);
    assert_eq!(selected.path.len(), 2, "{selected:?}");
    let real = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|t| t.node == group_id)
        .unwrap();
    assert_eq!(selected, real);
}

#[test]
fn undo_redo_restore_the_scene_and_leave_no_stale_selection() {
    let (mut editor, _) = cubes(2);
    let before = editor.history.scene().clone();
    editor.apply_operation(group(None)).unwrap();
    let grouped = editor.history.scene().clone();
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene(), &before);
    // PROVISIONAL: Maya restores the previous selection; the members are not reselected.
    assert!(editor.selection.is_empty());
    editor.apply_operation(Operation::Redo).unwrap();
    assert_eq!(editor.history.scene(), &grouped);
    assert!(editor.selection.is_empty());
}

#[test]
fn refusals_keep_selection_scene_and_undo_stack() {
    let (mut editor, ids) = cubes(1);
    let before = baseline(&editor);
    // A mesh transform is not a group.
    let refused = editor.apply_operation(ungroup(None));
    assert!(
        matches!(
            refused,
            Err(OperationError::Reparent(NativeError::InvalidReparent(
                ReparentRefusal::NotAGroup
            )))
        ),
        "{refused:?}"
    );
    assert_eq!(baseline(&editor), before);
    editor.deselect_all();
    let before = baseline(&editor);
    for native in [true, false] {
        for operation in [group(None), ungroup(None)] {
            let result = if native {
                editor.apply_operation(operation)
            } else {
                remote(&mut editor, operation)
            };
            assert!(matches!(result, Err(OperationError::NoSelection)));
            assert_eq!(baseline(&editor), before);
        }
    }
    let _ = ids;
}
