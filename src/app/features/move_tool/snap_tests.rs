//! Snap to grids for Move: absolute pivot landing, Axis Orientation, guards and exact undo.

use super::*;
use crate::app::tests::remote;
use azteccraft::scene::Primitive;

fn one_cube() -> (Editor, TransformTarget) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = editor.selected_move_target().unwrap();
    (editor, target)
}
fn snap(editor: &mut Editor, enabled: bool) -> OperationReply {
    remote(editor, Operation::SetSnapToGrid { enabled }).unwrap()
}

#[test]
fn snap_is_off_by_default_and_the_setter_is_idempotent() {
    let (mut editor, _) = one_cube();
    assert!(!editor.snap_to_grid);
    let undo_depth = editor.history.undo_len();
    assert!(matches!(
        snap(&mut editor, false),
        OperationReply::Unchanged
    ));
    assert!(matches!(snap(&mut editor, true), OperationReply::Changed));
    assert!(matches!(snap(&mut editor, true), OperationReply::Unchanged));
    let inspected = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert_eq!(inspected["snap_to_grid"], true);
    assert_eq!(editor.history.undo_len(), undo_depth);
}


#[test]
fn setting_snap_is_refused_while_a_file_prompt_is_open() {
    let (mut editor, _) = one_cube();
    editor.apply_operation(Operation::NewScene).unwrap();
    // NewScene on a dirty scene opens the confirmation modal.
    assert!(editor.files.is_modal());
    assert!(matches!(
        remote(&mut editor, Operation::SetSnapToGrid { enabled: true }),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.snap_to_grid);
    // The native path has no earlier remote modal check, so only the dispatch guard stops it.
    let scene = editor.history.scene().clone();
    let undo_depth = editor.history.undo_len();
    assert!(matches!(
        editor.apply_operation(Operation::SetSnapToGrid { enabled: true }),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.snap_to_grid);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_len(), undo_depth);
}
