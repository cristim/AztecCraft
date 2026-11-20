//! The remote lifecycle of a component Move at the Editor: it lands on the same bytes as a native
//! drag, needs the inspected generation, and cannot cross provenance with a native drag.
use super::tests::{begin_op, cube_with, update};
use super::*;
use crate::editor_operations::{
    MoveDistance, Operation, OperationError, OperationReply, SelectionMode,
};
use azteccraft::history::{ManipulationFinish, ManipulationId, ManipulationProvenance};

fn remote(editor: &mut Editor, operation: Operation) -> Result<OperationReply, OperationError> {
    editor.apply_operation_guarded(ManipulationProvenance::Remote, operation, &|| true)
}

fn remote_begin(editor: &mut Editor) -> ManipulationId {
    let generation = editor.selection_generation();
    let op = begin_op(editor, Some(generation));
    match remote(editor, op).unwrap() {
        OperationReply::ManipulationStarted { manipulation_id } => {
            ManipulationId::from_decimal(&manipulation_id).unwrap()
        }
        other => panic!("{other:?}"),
    }
}

fn finish(editor: &mut Editor, id: ManipulationId, remote_caller: bool) {
    let op = Operation::FinishMove {
        id,
        finish: ManipulationFinish::Commit,
    };
    if remote_caller {
        remote(editor, op).unwrap();
    } else {
        editor.apply_operation(op).unwrap();
    }
}

#[test]
fn a_remote_drag_in_declared_units_lands_on_the_same_bytes_as_a_native_one() {
    let mut native = cube_with(&[2, 5]);
    let id = super::tests::begin(&mut native);
    update(&mut native, id, 3.25);
    finish(&mut native, id, false);

    let mut editor = cube_with(&[2, 5]);
    let before = editor.history.scene().clone();
    let id = remote_begin(&mut editor);
    remote(
        &mut editor,
        Operation::UpdateMove {
            id,
            distance: MoveDistance::Declared(3.25),
        },
    )
    .unwrap();
    finish(&mut editor, id, true);
    assert_eq!(
        editor.history.scene().shapes()[0].mesh(),
        native.history.scene().shapes()[0].mesh()
    );
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_remote_begin_without_or_with_a_stale_generation_is_refused() {
    let mut editor = cube_with(&[1]);
    let before = editor.history.scene().clone();
    let op = begin_op(&editor, None);
    assert!(matches!(
        remote(&mut editor, op),
        Err(OperationError::InvalidInput(_))
    ));
    let inspected = editor.selection_generation();
    // The same indices after a mode round trip are a different selection.
    for mode in [SelectionMode::Edge, SelectionMode::Vertex] {
        editor
            .apply_operation(Operation::SetSelectionMode(mode))
            .unwrap();
    }
    let op = begin_op(&editor, Some(inspected));
    assert!(matches!(
        remote(&mut editor, op),
        Err(OperationError::StaleSelection { .. })
    ));
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_drag_cannot_be_updated_or_finished_by_the_other_provenance() {
    let mut editor = cube_with(&[0]);
    let id = super::tests::begin(&mut editor);
    update(&mut editor, id, 1.0);
    let preview = editor.history.scene().clone();
    let op = Operation::UpdateMove {
        id,
        distance: MoveDistance::Centimeters(2.0),
    };
    assert!(remote(&mut editor, op).is_err());
    let op = Operation::FinishMove {
        id,
        finish: ManipulationFinish::Commit,
    };
    assert!(remote(&mut editor, op).is_err());
    assert_eq!(editor.history.scene(), &preview);
    finish(&mut editor, id, false);

    let mut editor = cube_with(&[0]);
    let id = remote_begin(&mut editor);
    let preview = editor.history.scene().clone();
    assert!(
        editor
            .apply_operation(Operation::UpdateMove {
                id,
                distance: MoveDistance::Centimeters(2.0),
            })
            .is_err()
    );
    assert_eq!(editor.history.scene(), &preview);
}

#[test]
fn a_remote_begin_with_two_objects_selected_is_refused() {
    let mut editor = cube_with(&[0]);
    editor.create(azteccraft::scene::Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths);
    let generation = editor.selection_generation();
    let op = begin_op(&editor, Some(generation));
    assert!(matches!(
        remote(&mut editor, op),
        Err(OperationError::MultipleSelection)
    ));
}
