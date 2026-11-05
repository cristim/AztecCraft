use super::*;
use crate::app::tests::remote;
use crate::editor_operations::OperationError;
use azteccraft::native_scene::FaceTarget;

/// A face-mode editor with face 0 of a new `primitive` picked.
fn face_editor(primitive: Primitive) -> (Editor, FaceTarget) {
    let mut editor = Editor::default();
    editor.create(primitive);
    pick_face_zero(editor)
}

/// Face mode with face 0 of the only object picked.
fn pick_face_zero(mut editor: Editor) -> (Editor, FaceTarget) {
    let id = editor.single_target().unwrap().0;
    let shape = editor.history.scene().shapes()[0].id();
    let target = FaceTarget {
        transform: id,
        shape,
        path: vec![
            editor.history.scene().node(id).unwrap().transform_id(),
            shape,
        ],
        face_index: 0,
    };
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    editor.native_operation(Operation::SelectFace {
        target: Some(target.clone()),
    });
    assert_eq!(editor.remembered_face, Some(target.clone()));
    (editor, target)
}

fn frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) {
    crate::app::tests::frame_with(editor, ctx, egui::vec2(1200.0, 800.0), 1.0, events);
}

fn points(editor: &Editor) -> usize {
    editor.history.scene().shapes()[0].mesh().point_count()
}

fn lowest_z(editor: &Editor) -> f64 {
    let mesh = editor.history.scene().shapes()[0].mesh();
    (0..mesh.point_count())
        .map(|index| mesh.effective_point_cm(index).unwrap()[2])
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn extruding_the_picked_face_moves_the_cap_by_the_thickness_in_one_undo_step() {
    let (mut editor, target) = face_editor(Primitive::Cube);
    let baseline = editor.history.scene().clone();
    let before = editor.history.undo_label();
    assert_eq!(lowest_z(&editor), -0.5);
    assert_eq!(editor.extrude_selected_face(0.5), Ok(()));
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(points(&editor), 12);
    assert_eq!(
        lowest_z(&editor),
        -1.0,
        "face 0 faces -Z; the cap moves 0.5 out"
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Extrude"));
    assert_eq!(editor.remembered_face, Some(target));
    let extruded = editor.history.scene().clone();
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(editor.history.undo_label(), before);
    editor.history_step(true);
    assert_eq!(editor.history.scene(), &extruded);
}

#[test]
fn the_two_blockers_return_their_variants_and_change_nothing() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.history_step(false);
    editor.channels[0] = "2".into();
    let baseline = editor.history.scene().clone();
    assert_eq!(editor.extrude_blocker(), Some(ExtrudeBlocker::NotFaceMode));
    assert_eq!(
        editor.extrude_selected_face(1.0),
        Err(ExtrudeBlocker::NotFaceMode)
    );
    editor.selection_mode = SelectionMode::Face;
    assert_eq!(
        editor.extrude_blocker(),
        Some(ExtrudeBlocker::NoFaceSelected)
    );
    assert_eq!(
        editor.extrude_selected_face(1.0),
        Err(ExtrudeBlocker::NoFaceSelected)
    );
    assert_eq!(editor.history.scene(), &baseline);
    assert!(editor.history.can_redo(), "the redo stack is kept");
    assert_eq!(editor.channels[0], "2", "the draft is kept");
    assert_eq!(
        (
            ExtrudeBlocker::NotFaceMode.to_string(),
            ExtrudeBlocker::NoFaceSelected.to_string()
        ),
        (
            "Extrude needs Face selection mode (F11)".into(),
            "Select a face first".into()
        )
    );
}

#[test]
fn a_stale_face_and_an_all_face_extrusion_are_refused_with_the_baseline_kept() {
    let (mut editor, target) = face_editor(Primitive::Cube);
    let baseline = editor.history.scene().clone();
    editor.remembered_face = Some(FaceTarget {
        face_index: 99,
        ..target
    });
    assert_eq!(editor.extrude_selected_face(1.0), Ok(()));
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &baseline);
    assert!(!editor.history.can_redo());

    let mut single = Editor::default();
    single.native_operation(Operation::Create {
        primitive: Primitive::Plane,
        options: Some(azteccraft::primitive_options::SINGLE_QUAD_PLANE),
        name: None,
    });
    let (mut plane, _) = pick_face_zero(single);
    let baseline = plane.history.scene().clone();
    assert_eq!(plane.extrude_selected_face(1.0), Ok(()));
    assert!(plane.error, "{}", plane.status);
    assert_eq!(
        plane.status,
        OperationError::Scene(azteccraft::native_scene::NativeError::Legacy(
            azteccraft::scene::SceneError::UnsupportedAllFacesExtrusion
        ))
        .to_string()
    );
    assert_eq!(plane.history.scene(), &baseline);
}

#[test]
fn a_valid_channel_draft_is_committed_first_and_an_invalid_one_refuses() {
    let (mut editor, _) = face_editor(Primitive::Cube);
    editor.channels[0] = "2".into();
    assert_eq!(editor.extrude_selected_face(0.5), Ok(()));
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(points(&editor), 12);
    let id = editor.selected().unwrap();
    let node = editor.history.scene().node(id).unwrap();
    assert_eq!(node.transform().translation[0], 2.0);

    let (mut editor, _) = face_editor(Primitive::Cube);
    let baseline = editor.history.scene().clone();
    editor.channels[0] = "nan".into();
    assert_eq!(editor.extrude_selected_face(0.5), Ok(()));
    assert!(editor.error);
    assert_eq!(editor.channels[0], "nan");
    assert_eq!(editor.history.scene(), &baseline);
}

#[test]
fn the_window_reports_a_bad_thickness_dispatches_nothing_then_extrudes_a_good_one() {
    let (mut editor, _) = face_editor(Primitive::Cube);
    let baseline = editor.history.scene().clone();
    editor.open_extrude_options();
    for text in ["", "abc", "NaN", "inf", "1e400", "1,5"] {
        editor.extrude_options.dialog.as_mut().unwrap().thickness = text.into();
        editor.extrude_from_dialog();
        let message = editor
            .extrude_options
            .dialog
            .as_ref()
            .unwrap()
            .message
            .clone();
        assert!(
            message.is_some_and(|message| message.starts_with("Thickness must be")),
            "{text:?}"
        );
        assert!(editor.error, "{text:?}");
        assert_eq!(editor.history.scene(), &baseline, "{text:?}");
    }
    assert!(
        !editor.history.can_redo() && editor.history.undo_label().as_deref() != Some("Extrude")
    );
}

#[test]
fn a_focused_thickness_field_makes_a_remote_edit_busy() {
    let (mut editor, _) = face_editor(Primitive::Cube);
    let ctx = egui::Context::default();
    editor.open_extrude_options();
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let reply = remote(&mut editor, Operation::SetGridVisible { visible: false });
    assert!(reply.is_ok());
    ctx.memory_mut(|memory| memory.request_focus(thickness_field_id()));
    frame(&mut editor, &ctx, vec![]);
    let busy = editor
        .remote_operation(
            crate::editor_operations::RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation: Operation::SetGridVisible { visible: true },
            },
            &ctx,
        )
        .unwrap_err();
    assert!(matches!(busy, OperationError::BusyEditing));
}

#[test]
fn escape_closes_the_window_when_nothing_else_wants_it() {
    let (mut editor, _) = face_editor(Primitive::Cube);
    let ctx = egui::Context::default();
    editor.open_extrude_options();
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    assert!(editor.extrude_options.dialog.is_some());
    let escape = egui::Event::Key {
        key: Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut editor, &ctx, vec![escape]);
    assert!(editor.extrude_options.dialog.is_none());
}
