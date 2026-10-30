use super::*;
use crate::app::tests::{deep_group_editor, remote};

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
        editor.status.clone(),
    )
}

#[test]
fn delete_selection_removes_every_selected_object_as_one_undo_step() {
    let (mut editor, all) = cubes(3);
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.apply_operation(Operation::DeleteSelection),
        Ok(OperationReply::Changed)
    ));
    assert!(editor.history.scene().nodes().is_empty());
    assert!(editor.selection.is_empty());
    assert_eq!(editor.history.undo_label().as_deref(), Some("Delete"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.redo());
    assert!(editor.history.scene().nodes().is_empty());
    assert_eq!(all.len(), 3);
}

#[test]
fn delete_selection_leaves_unselected_objects_alone() {
    let (mut editor, all) = cubes(3);
    editor.set_selection_members_for_test(vec![all[2].clone(), all[0].clone()]);
    editor.sync_fields();
    editor.apply_operation(Operation::DeleteSelection).unwrap();
    let left: Vec<_> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|n| n.id())
        .collect();
    assert_eq!(left, [all[1].node]);
    assert!(editor.selection.is_empty());
}

/// PROVISIONAL (P17): selection is not on the undo queue, so Undo restores the nodes only.
#[test]
fn undo_of_delete_selection_restores_nodes_but_not_the_selection() {
    let (mut editor, _) = cubes(2);
    editor.apply_operation(Operation::DeleteSelection).unwrap();
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene().nodes().len(), 2);
    assert!(editor.selection.is_empty());
}

#[test]
fn delete_selection_with_nothing_selected_reports_no_selection() {
    let (mut editor, _) = cubes(2);
    editor.deselect_all();
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(Operation::DeleteSelection),
        Err(OperationError::NoSelection)
    ));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn delete_selection_in_component_modes_is_refused_and_keeps_state() {
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        for native in [true, false] {
            let (mut editor, all) = cubes(2);
            editor.set_selection_members_for_test(vec![all[0].clone()]);
            editor.sync_fields();
            editor.selection_mode = mode;
            let before = baseline(&editor);
            let result = if native {
                editor.apply_operation(Operation::DeleteSelection)
            } else {
                remote(&mut editor, Operation::DeleteSelection)
            };
            assert!(
                matches!(result, Err(OperationError::InvalidInput(ref text))
                    if text.ends_with("deletion is unsupported; switch to Object or Hierarchy selection")),
                "{mode:?} native={native}: {result:?}"
            );
            assert_eq!(baseline(&editor), before);
        }
    }
}

/// An opened file refuses every delete; the multi-delete reports what the single delete does for
/// the same node, naming it, and leaves scene, selection and history untouched.
#[test]
fn delete_selection_refusals_match_single_delete_and_keep_state() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    let before = baseline(&editor);
    let single = editor
        .apply_operation(Operation::Delete(all[0].node))
        .unwrap_err();
    let many = editor
        .apply_operation(Operation::DeleteSelection)
        .unwrap_err();
    let name = editor.history.scene().node(all[0].node).unwrap().name();
    assert_eq!(many.to_string(), format!("{name}: {single}"));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn delete_selection_in_hierarchy_mode_deletes_the_selected_roots() {
    let (mut editor, all) = cubes(2);
    editor.selection_mode = SelectionMode::Hierarchy;
    editor.set_selection_members_for_test(all);
    editor.sync_fields();
    editor.apply_operation(Operation::DeleteSelection).unwrap();
    assert!(editor.history.scene().nodes().is_empty());
}
