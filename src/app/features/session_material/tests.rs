use super::*;
use crate::app::tests::remote;
use azteccraft::native_scene::{GroupOptions, GroupPivot, GroupUnder, NativeScene};

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
    assert_eq!(editor.history.scene(), &before);
}

fn two_cube_editor() -> Editor {
    n_cube_editor(2)
}

fn shape_materials(editor: &Editor) -> Vec<String> {
    let scene = editor.history.scene();
    scene
        .shapes()
        .iter()
        .map(|shape| scene.session_material(shape).unwrap().name().to_owned())
        .collect()
}

#[test]
fn assign_applies_to_every_selected_object_as_one_undo_step() {
    let mut editor = two_cube_editor();
    assert_eq!(editor.session_material_targets().unwrap().len(), 2);
    assert_eq!(editor.session_material_disabled_reason(), None);
    let before = editor.history.scene().clone();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    assert_eq!(shape_materials(&editor), ["lambert2", "lambert2"]);
    assert_eq!(editor.history.scene().session_materials().len(), 2);
    assert_eq!(
        editor
            .session_material_choices()
            .iter()
            .map(|row| row.2)
            .collect::<Vec<_>>(),
        [false, true]
    );
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.history.scene(), &before);
    editor.native_operation(Operation::Redo);
    assert_eq!(shape_materials(&editor), ["lambert2", "lambert2"]);
}

#[test]
fn mixed_materials_mark_no_row() {
    let mut editor = two_cube_editor();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    let first = editor.history.scene().shapes()[0].id();
    editor.native_operation(Operation::AssignSessionMaterial {
        shapes: vec![first],
        material: MaterialChoice::Existing(SessionMaterialId::from_index(0)),
    });
    assert!(editor.session_material_choices().iter().all(|row| !row.2));
}

#[test]
fn a_selected_group_without_a_direct_shape_refuses_the_whole_selection() {
    let mut editor = two_cube_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all);
    editor
        .apply_operation(Operation::Group {
            options: GroupOptions {
                under: GroupUnder::Parent,
                pivot: GroupPivot::Center,
                preserve_position: true,
            },
            expected_selection_generation: None,
        })
        .unwrap();
    editor.create(Primitive::Cube);
    editor.native_operation(Operation::SelectByRule(SelectRule::All));
    assert!(
        editor.selection.len() > 1,
        "the group, its children and the new cube"
    );
    let before = editor.history.scene().clone();
    let reason = editor.session_material_disabled_reason().unwrap();
    assert_eq!(reason, MULTI_SHAPE_REASON);
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::InvalidInput(ref r)) if r == MULTI_SHAPE_REASON
    ));
    assert_eq!(editor.status, MULTI_SHAPE_REASON);
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_transform_with_two_direct_shapes_refuses_the_selection() {
    let (mut editor, _directory) = open_with_attribute_editor("two.mb", &many_shapes(2));
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn two_selected_instances_of_one_transform_give_one_shape() {
    let mut editor = two_cube_editor();
    let first = editor.history.scene().transform_paths().unwrap()[0].clone();
    let bare = TransformTarget {
        node: first.node,
        path: Vec::new(),
    };
    editor.set_selection_members_for_test(vec![first, bare]);
    assert_eq!(editor.selection.len(), 2);
    assert_eq!(editor.session_material_targets().unwrap().len(), 1);
}

#[test]
fn multi_selection_assignment_keeps_the_draft_and_token_guards() {
    let mut editor = two_cube_editor();
    let shapes = editor.session_material_targets().unwrap();
    let operation = || Operation::AssignSessionMaterial {
        shapes: shapes.clone(),
        material: new_lambert(),
    };
    let before = editor.history.scene().clone();
    editor.name = "draft name".into();
    assert!(matches!(
        remote(&mut editor, operation()),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &before);
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
    let shape = other.session_material_targets().unwrap()[0];
    remote(
        &mut other,
        Operation::AssignSessionMaterial {
            shapes: vec![shape],
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
        editor.session_material_targets(),
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
    let shape = editor.session_material_targets().unwrap()[0];
    let operation = || Operation::AssignSessionMaterial {
        shapes: vec![shape],
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

fn n_cube_editor(count: usize) -> Editor {
    let mut editor = Editor::default();
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    editor.native_operation(Operation::SelectByRule(SelectRule::All));
    editor
}

#[test]
fn exactly_the_cap_of_selected_shapes_assigns_as_one_undo_step() {
    let mut editor = n_cube_editor(MAX_ASSIGN_SHAPES);
    assert_eq!(editor.selection.len(), MAX_ASSIGN_SHAPES);
    assert_eq!(
        editor.session_material_targets().unwrap().len(),
        MAX_ASSIGN_SHAPES
    );
    let before = editor.history.scene().clone();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    let assigned = shape_materials(&editor);
    assert_eq!(assigned.len(), MAX_ASSIGN_SHAPES);
    assert!(assigned.iter().all(|name| name == "lambert2"));
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.history.scene(), &before);
    assert!(
        shape_materials(&editor)
            .iter()
            .all(|name| name == "standardSurface1")
    );
}

#[test]
fn one_over_the_cap_refuses_natively_and_leaves_the_scene_unchanged() {
    let mut editor = n_cube_editor(MAX_ASSIGN_SHAPES + 1);
    assert_eq!(editor.selection.len(), MAX_ASSIGN_SHAPES + 1);
    let before = editor.history.scene().clone();
    let could_undo = editor.history.can_undo();
    assert!(matches!(
        editor.session_material_targets(),
        Err(OperationError::InvalidInput(ref r)) if r == TOO_MANY_REASON
    ));
    assert!(matches!(
        editor.assign_material_to_selection(new_lambert()),
        Err(OperationError::InvalidInput(ref r)) if r == TOO_MANY_REASON
    ));
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.can_undo(), could_undo);
}
