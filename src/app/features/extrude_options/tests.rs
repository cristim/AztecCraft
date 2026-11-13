use super::*;
use crate::app::tests::remote;
use crate::editor_operations::OperationError;
use azteccraft::native_scene::FaceTarget;
use eframe::egui::accesskit;

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
    assert_eq!(editor.remembered_face(), Some(target.clone()));
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
    editor.extrude_selected_face(0.5).unwrap();
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(points(&editor), 12);
    assert_eq!(
        lowest_z(&editor),
        -1.0,
        "face 0 faces -Z; the cap moves 0.5 out"
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Extrude"));
    assert_eq!(editor.remembered_face(), Some(target));
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
    assert!(matches!(
        editor.extrude_selected_face(1.0),
        Err(ExtrudeFailure::Blocked(ExtrudeBlocker::NotFaceMode))
    ));
    editor.selection_mode = SelectionMode::Face;
    assert_eq!(
        editor.extrude_blocker(),
        Some(ExtrudeBlocker::NoFaceSelected)
    );
    assert!(matches!(
        editor.extrude_selected_face(1.0),
        Err(ExtrudeFailure::Blocked(ExtrudeBlocker::NoFaceSelected))
    ));
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
    editor.set_remembered_face(Some(FaceTarget {
        face_index: 99,
        ..target
    }));
    assert!(matches!(
        editor.extrude_selected_face(1.0),
        Err(ExtrudeFailure::Refused(OperationError::Scene(_)))
    ));
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
    assert!(matches!(
        plane.extrude_selected_face(1.0),
        Err(ExtrudeFailure::Refused(OperationError::Scene(_)))
    ));
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
    editor.extrude_selected_face(0.5).unwrap();
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(points(&editor), 12);
    let id = editor.selected().unwrap();
    let node = editor.history.scene().node(id).unwrap();
    assert_eq!(node.transform().translation[0], 2.0);

    let (mut editor, _) = face_editor(Primitive::Cube);
    let baseline = editor.history.scene().clone();
    editor.channels[0] = "nan".into();
    assert!(matches!(
        editor.extrude_selected_face(0.5),
        Err(ExtrudeFailure::Refused(_))
    ));
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
    // The window's success path: a good value extrudes, clears the message and stays open.
    let low = lowest_z(&editor);
    editor.extrude_options.dialog.as_mut().unwrap().thickness = "0.25".into();
    editor.extrude_from_dialog();
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(lowest_z(&editor), low - 0.25);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Extrude"));
    let dialog = editor
        .extrude_options
        .dialog
        .as_ref()
        .expect("the window stays open");
    assert!(dialog.message.is_none());
}

#[test]
fn a_core_refusal_from_the_window_shows_its_message_and_the_function_returns_it() {
    let mut single = Editor::default();
    single.native_operation(Operation::Create {
        primitive: Primitive::Plane,
        options: Some(azteccraft::primitive_options::SINGLE_QUAD_PLANE),
        name: None,
    });
    let (mut editor, _) = pick_face_zero(single);
    editor.open_extrude_options();
    editor.extrude_from_dialog();
    let message = editor
        .extrude_options
        .dialog
        .as_ref()
        .unwrap()
        .message
        .clone();
    assert_eq!(message.as_deref(), Some(editor.status.as_str()));
    assert!(editor.error);
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

/// The shelf button: the topmost enabled-state of the control labelled "Extrude".
fn shelf_button(output: &egui::FullOutput) -> (egui::Pos2, bool) {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == accesskit::Role::Button && node.label() == Some("Extrude")
        })
        .filter_map(|(_, node)| node.bounds().map(|b| (b, node.is_disabled())))
        .min_by(|a, b| a.0.y0.total_cmp(&b.0.y0))
        .map(|(b, disabled)| {
            (
                egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32),
                disabled,
            )
        })
        .unwrap()
}

fn window_open(output: &egui::FullOutput) -> bool {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some("Extrude Options"))
}

#[test]
fn the_shelf_button_is_disabled_without_a_face_and_opens_the_window_with_one() {
    let size = egui::vec2(1200.0, 800.0);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let (_, disabled) = shelf_button(&crate::app::tests::frame_with(
        &mut editor,
        &ctx,
        size,
        1.0,
        vec![],
    ));
    assert!(disabled, "Object mode");
    assert_eq!(editor.extrude_blocker(), Some(ExtrudeBlocker::NotFaceMode));
    editor.selection_mode = SelectionMode::Face;
    let output = crate::app::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    assert!(shelf_button(&output).1, "Face mode, no face");
    assert!(editor.extrude_options.dialog.is_none());

    let (mut editor, _) = face_editor(Primitive::Cube);
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let output = crate::app::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    let (at, disabled) = shelf_button(&output);
    assert!(!disabled && !window_open(&output));
    let baseline = editor.history.scene().clone();
    crate::app::tests::click_at(&mut editor, &ctx, size, at);
    assert!(editor.extrude_options.dialog.is_some(), "{}", editor.status);
    assert_eq!(editor.history.scene(), &baseline, "opening changes nothing");
}

#[test]
fn the_shelf_button_is_disabled_with_the_read_only_reason_on_an_ascii_scene() {
    mod ascii_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_ascii.rs"
        ));
    }
    let size = egui::vec2(1200.0, 800.0);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let (_, target) = face_editor(Primitive::Cube);
    let mut editor = Editor::default();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    assert!(
        editor
            .history
            .scene()
            .authoring_read_only_reason()
            .is_some()
    );
    editor.selection_mode = SelectionMode::Face;
    editor.set_remembered_face(Some(target));
    assert_eq!(
        editor.extrude_blocker(),
        None,
        "only the read-only state refuses"
    );
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let output = crate::app::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    assert!(shelf_button(&output).1, "disabled");
}

#[test]
fn the_shelf_button_is_disabled_with_the_uv_reason_while_the_shape_has_uvs() {
    let size = egui::vec2(1200.0, 800.0);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let (mut editor, _) = face_editor(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id();
    editor.native_operation(Operation::ProjectUvPlanar {
        shape,
        axis: azteccraft::native_scene::UvProjectAxis::Y,
    });
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(
        editor.extrude_refusal().as_deref(),
        Some(NativeError::UvsBlockTopologyEdit.to_string().as_str())
    );
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let output = crate::app::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    assert!(shelf_button(&output).1, "disabled");
    editor.native_operation(Operation::DeleteUvs { shape });
    let output = crate::app::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    assert!(!shelf_button(&output).1, "enabled again without UVs");
}
