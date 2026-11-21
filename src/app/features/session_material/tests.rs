use super::*;
use crate::app::tests::remote;
use azteccraft::native_scene::NativeScene;

fn cube_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor
}

fn shape_material(editor: &Editor) -> String {
    let scene = editor.history.scene();
    let shape = scene.shapes().first().expect("one shape");
    scene.session_material(shape).unwrap().name().to_owned()
}

fn new_lambert() -> MaterialChoice {
    MaterialChoice::New {
        color: DEFAULT_LAMBERT_COLOR,
    }
}

#[test]
fn assign_new_creates_one_default_lambert_and_undo_restores() {
    let mut editor = cube_editor();
    let before = editor.history.scene().clone();
    let reply = editor.assign_material_to_selection(new_lambert()).unwrap();
    assert!(
        matches!(reply, OperationReply::SessionMaterial { ref material_id } if material_id == "1")
    );
    let scene = editor.history.scene();
    assert_eq!(scene.session_materials().len(), 2);
    assert_eq!(scene.session_materials()[1].color(), [0.5; 3]);
    assert_eq!(shape_material(&editor), "lambert2");
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.history.scene(), &before);
    editor.native_operation(Operation::Redo);
    assert_eq!(shape_material(&editor), "lambert2");
}

#[test]
fn assign_existing_marks_the_current_row_and_reassign_keeps_the_old_lambert() {
    let mut editor = cube_editor();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    let rows = editor.session_material_choices();
    let names: Vec<_> = rows.iter().map(|row| row.1.as_str()).collect();
    assert_eq!(names, ["standardSurface1", "lambert2"]);
    assert_eq!(
        rows.iter().map(|row| row.2).collect::<Vec<_>>(),
        [false, true]
    );
    editor
        .assign_material_to_selection(MaterialChoice::Existing(rows[0].0))
        .unwrap();
    assert_eq!(shape_material(&editor), "standardSurface1");
    // PROVISIONAL: the unassigned lambert stays in the scene.
    assert_eq!(editor.history.scene().session_materials().len(), 2);
    assert_eq!(
        editor
            .session_material_choices()
            .iter()
            .map(|row| row.2)
            .collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn refusals_name_the_reason_and_leave_the_scene_unchanged() {
    let mut editor = Editor::default();
    let before: NativeScene = editor.history.scene().clone();
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::NoSelection)
    ));
    assert!(editor.session_material_disabled_reason().is_some());
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.native_operation(Operation::SelectByRule(SelectRule::All));
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::MultipleSelection)
    ));
    assert!(editor.error);
    assert_eq!(editor.session_material_choices().len(), 1);
    assert_eq!(
        editor.history.scene().session_materials(),
        before.session_materials()
    );
}

#[test]
fn component_mode_is_refused_with_its_reason() {
    let mut editor = cube_editor();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let reason = editor.session_material_disabled_reason().unwrap();
    assert!(reason.contains("Object mode"), "{reason}");
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(editor.history.scene().session_materials().len(), 1);
}

#[test]
fn native_and_remote_assignment_give_equal_scenes() {
    let mut native = cube_editor();
    let mut other = Editor {
        history: History::new(native.history.scene().clone()),
        ..Editor::default()
    };
    other.native_operation(Operation::SelectByRule(SelectRule::All));
    assert_eq!(native.history.scene(), other.history.scene());
    native.assign_material_to_selection(new_lambert()).unwrap();
    let shape = other.session_material_target().unwrap();
    remote(
        &mut other,
        Operation::AssignSessionMaterial {
            shape,
            material: new_lambert(),
        },
    )
    .unwrap();
    assert_eq!(native.history.scene(), other.history.scene());
    assert_eq!(shape_material(&native), shape_material(&other));
}

#[test]
fn an_open_file_modal_blocks_the_assignment_and_keeps_the_scene() {
    let mut editor = cube_editor();
    editor.request_new();
    assert!(editor.files.is_modal());
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &before);
}

use crate::app::features::attribute_editor::ui_tests::{many_shapes, open_with_attribute_editor};

const UNSUPPORTED_EDIT: &str = "native ownership or edit effects are unsupported";

#[test]
fn disabled_reasons_are_pinned() {
    let mut editor = Editor::default();
    assert_eq!(
        editor.session_material_disabled_reason().as_deref(),
        Some("Select an object or face first")
    );
    editor.create(Primitive::Cube);
    assert_eq!(editor.session_material_disabled_reason(), None);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    assert_eq!(
        editor.session_material_disabled_reason().as_deref(),
        Some(COMPONENT_REASON)
    );
}

#[test]
fn an_opened_binary_scene_refuses_assignment_with_the_typed_reason() {
    let (mut editor, _directory) = open_with_attribute_editor("one.mb", &many_shapes(1));
    assert_eq!(
        editor.session_material_disabled_reason().as_deref(),
        Some(UNSUPPORTED_EDIT)
    );
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::SessionMaterial(_))
    ));
    assert!(editor.error);
    assert!(
        editor.status.contains(UNSUPPORTED_EDIT),
        "{}",
        editor.status
    );
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn several_direct_shapes_have_no_assignment_target() {
    let (mut editor, _directory) = open_with_attribute_editor("two.mb", &many_shapes(2));
    assert!(matches!(
        editor.session_material_target(),
        Err(OperationError::InvalidInput(ref reason)) if reason == NO_SHAPE_REASON
    ));
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::InvalidInput(ref reason)) if reason == NO_SHAPE_REASON
    ));
    assert_eq!(editor.status, NO_SHAPE_REASON);
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
}

#[cfg(unix)]
#[test]
fn a_reopened_editor_written_ma_scene_stays_editable() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("reopened.ma");
    cube_editor()
        .apply_operation(Operation::SaveSceneAs { path: path.clone() })
        .unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    editor.native_operation(Operation::SelectByRule(SelectRule::All));
    assert_eq!(editor.session_material_disabled_reason(), None);
    editor.assign_material_to_selection(new_lambert()).unwrap();
    assert_eq!(editor.history.scene().session_materials().len(), 2);
}

#[test]
fn a_pending_valid_draft_is_committed_by_the_native_assignment() {
    let mut editor = cube_editor();
    editor.name = "renamed".into();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    assert!(!editor.has_draft());
    assert_eq!(editor.history.scene().nodes()[0].name(), "renamed");
    assert_eq!(shape_material(&editor), "lambert2");
}

#[test]
fn remote_assignment_is_refused_for_a_draft_and_a_stale_token() {
    let mut editor = cube_editor();
    let shape = editor.session_material_target().unwrap();
    let operation = || Operation::AssignSessionMaterial {
        shape,
        material: new_lambert(),
    };
    let before = editor.history.scene().clone();
    editor.name = "draft name".into();
    assert!(matches!(
        remote(&mut editor, operation()),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(
        editor.name, "draft name",
        "the remote refusal keeps the draft"
    );
    editor.name = editor.history.scene().nodes()[0].name().to_owned();
    let result = editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(uuid::Uuid::nil()),
            operation: operation(),
        },
        &egui::Context::default(),
    );
    assert!(
        matches!(result, Err(OperationError::StaleScene)),
        "{result:?}"
    );
    assert_eq!(editor.history.scene(), &before);
}
