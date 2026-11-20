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
