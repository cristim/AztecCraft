use super::*;
use crate::app::tests::remote;
use azteccraft::camera_bookmarks::BookmarkError;
use azteccraft::scene::Primitive;

fn create(editor: &mut Editor, name: &str) -> Result<OperationReply, OperationError> {
    remote(
        editor,
        Operation::CreateCameraBookmark { name: name.into() },
    )
}

fn apply(editor: &mut Editor, name: &str) -> Result<OperationReply, OperationError> {
    remote(editor, Operation::ApplyCameraBookmark { name: name.into() })
}

fn names(editor: &Editor) -> Vec<String> {
    editor
        .bookmarks
        .list()
        .iter()
        .map(|b| b.name.clone())
        .collect()
}

fn move_camera(editor: &mut Editor) {
    editor
        .camera
        .set_pose([4.0, -2.0, 9.0].into(), 3.0, 1.0, -0.4)
        .unwrap();
}

#[test]
fn apply_restores_the_captured_pose_bit_exactly() {
    let mut editor = Editor::default();
    editor
        .camera
        .set_pose([0.1, -0.0, 3.3e5].into(), 7.123_456, 6.1, -1.2)
        .unwrap();
    let saved = editor.camera.clone();
    assert!(matches!(
        create(&mut editor, "front"),
        Ok(OperationReply::Changed)
    ));
    move_camera(&mut editor);
    assert!(!editor.camera.same_capture(&saved));
    assert!(matches!(
        apply(&mut editor, "front"),
        Ok(OperationReply::Changed)
    ));
    assert!(editor.camera.same_capture(&saved));
    assert!(matches!(
        apply(&mut editor, "front"),
        Ok(OperationReply::Unchanged)
    ));
}

#[test]
fn apply_after_framing_restores_the_pose_and_keeps_the_clip_planes() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let saved = BookmarkPose::of(&editor.camera);
    create(&mut editor, "home").unwrap();
    remote(&mut editor, Operation::FrameAll { fit_factor: None }).unwrap();
    let far = editor.camera.far();
    apply(&mut editor, "home").unwrap();
    assert_eq!(BookmarkPose::of(&editor.camera), saved);
    assert_eq!(editor.camera.far(), far);
}

#[test]
fn listing_reports_the_pose_in_inspection_units_without_a_token() {
    let mut editor = Editor::default();
    move_camera(&mut editor);
    create(&mut editor, "a").unwrap();
    let reply = editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: None,
                operation: Operation::ListCameraBookmarks,
            },
            &egui::Context::default(),
        )
        .unwrap();
    let OperationReply::CameraBookmarks { bookmarks } = reply else {
        panic!("expected a bookmark list");
    };
    let inspected = editor.inspect(false).unwrap().camera;
    assert_eq!(bookmarks.len(), 1);
    assert_eq!(bookmarks[0].name, "a");
    assert_eq!(bookmarks[0].target, inspected.target);
    assert_eq!(bookmarks[0].distance, inspected.distance);
    assert_eq!(bookmarks[0].yaw_degrees, inspected.yaw_degrees);
    assert_eq!(bookmarks[0].pitch_degrees, inspected.pitch_degrees);
}

#[test]
fn errors_are_typed_and_leave_the_store_unchanged() {
    let mut editor = Editor::default();
    create(&mut editor, "a").unwrap();
    let cases = [
        (create(&mut editor, "a"), BookmarkError::Duplicate),
        (apply(&mut editor, "zz"), BookmarkError::NotFound),
        (
            remote(
                &mut editor,
                Operation::DeleteCameraBookmark { name: "zz".into() },
            ),
            BookmarkError::NotFound,
        ),
        (
            remote(
                &mut editor,
                Operation::RenameCameraBookmark {
                    name: "a".into(),
                    new_name: "".into(),
                },
            ),
            BookmarkError::InvalidName(azteccraft::camera_bookmarks::NameProblem::Empty),
        ),
    ];
    for (result, expected) in cases {
        let Err(OperationError::CameraBookmark(actual)) = result else {
            panic!("expected a bookmark error, got {result:?}");
        };
        assert_eq!(actual, expected);
    }
    assert_eq!(names(&editor), ["a"]);
}

#[test]
fn rename_and_delete_work_through_operations() {
    let mut editor = Editor::default();
    create(&mut editor, "a").unwrap();
    let rename = |editor: &mut Editor, to: &str| {
        remote(
            editor,
            Operation::RenameCameraBookmark {
                name: "a".into(),
                new_name: to.into(),
            },
        )
    };
    assert!(matches!(
        rename(&mut editor, "a"),
        Ok(OperationReply::Unchanged)
    ));
    assert!(matches!(
        rename(&mut editor, "b"),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(names(&editor), ["b"]);
    assert!(matches!(
        remote(
            &mut editor,
            Operation::DeleteCameraBookmark { name: "b".into() }
        ),
        Ok(OperationReply::Changed)
    ));
    assert!(names(&editor).is_empty());
}

#[test]
fn bookmarks_never_dirty_the_scene_or_change_the_token_and_survive_undo() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let token = editor.scene_token;
    let scene = editor.history.scene().clone();
    create(&mut editor, "a").unwrap();
    apply(&mut editor, "a").unwrap();
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.history.scene(), &scene);
    editor.native_operation(Operation::Undo);
    editor.native_operation(Operation::Redo);
    assert_eq!(names(&editor), ["a"]);
}

#[test]
fn new_and_open_clear_but_a_cancelled_replacement_keeps_them() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.aztec");
    azteccraft::document::save_new(&path, &azteccraft::scene::Scene::default()).unwrap();
    for open in [false, true] {
        let operation = || {
            if open {
                Operation::OpenScene { path: path.clone() }
            } else {
                Operation::NewScene
            }
        };
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        create(&mut editor, "a").unwrap();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = remote(&mut editor, operation()).unwrap()
        else {
            panic!("expected unsaved confirmation");
        };
        // Pending, not yet applied: still there.
        assert_eq!(names(&editor), ["a"]);
        remote(
            &mut editor,
            Operation::ResolveFileAction {
                pending_id: pending_action_id.parse().unwrap(),
                decision: crate::editor_operations::FileDecision::Cancel,
            },
        )
        .unwrap();
        assert_eq!(names(&editor), ["a"]);
        let mut clean = Editor::default();
        create(&mut clean, "a").unwrap();
        assert!(matches!(
            remote(&mut clean, operation()).unwrap(),
            OperationReply::DocumentApplied { .. }
        ));
        assert!(names(&clean).is_empty());
    }
}

#[test]
fn remote_guards_refuse_stale_token_draft_modal_and_closing_but_not_the_list() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    create(&mut editor, "a").unwrap();
    let ops = || {
        [
            Operation::CreateCameraBookmark { name: "b".into() },
            Operation::RenameCameraBookmark {
                name: "a".into(),
                new_name: "c".into(),
            },
            Operation::DeleteCameraBookmark { name: "a".into() },
            Operation::ApplyCameraBookmark { name: "a".into() },
        ]
    };
    for operation in ops() {
        let stale = editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(Uuid::new_v4()),
                operation,
            },
            &egui::Context::default(),
        );
        assert!(matches!(stale, Err(OperationError::StaleScene)));
    }
    editor.name = "pending draft".into();
    assert!(editor.has_draft());
    for operation in ops() {
        assert!(matches!(
            remote(&mut editor, operation),
            Err(OperationError::BusyEditing)
        ));
    }
    assert_eq!(editor.name, "pending draft");
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    create(&mut editor, "a").unwrap();
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    for operation in ops() {
        assert!(matches!(
            remote(&mut editor, operation),
            Err(OperationError::BusyEditing)
        ));
    }
    assert_eq!(names(&editor), ["a"]);
    let mut closing = Editor::default();
    closing.close_document().unwrap();
    assert!(matches!(
        create(&mut closing, "a"),
        Err(OperationError::BusyEditing)
    ));
}

#[test]
fn native_edits_refuse_modal_but_apply_follows_camera_moves() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    create(&mut editor, "a").unwrap();
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    for operation in [
        Operation::CreateCameraBookmark { name: "b".into() },
        Operation::RenameCameraBookmark {
            name: "a".into(),
            new_name: "c".into(),
        },
        Operation::DeleteCameraBookmark { name: "a".into() },
    ] {
        assert!(matches!(
            editor.apply_operation(operation),
            Err(OperationError::BusyEditing)
        ));
    }
    move_camera(&mut editor);
    assert!(
        editor
            .apply_operation(Operation::ApplyCameraBookmark { name: "a".into() })
            .is_ok()
    );
}
