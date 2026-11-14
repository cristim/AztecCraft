use super::*;
use crate::app::tests::remote;
use azteccraft::scene::Primitive;

#[cfg(unix)]
fn enabled(dir: &std::path::Path) -> Editor {
    let mut editor = Editor::default();
    editor.enable_recovery(dir.join("recovery")).unwrap();
    editor.create(Primitive::Cube);
    editor
}

#[cfg(unix)]
fn files(dir: &std::path::Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir.join("recovery")) else {
        return Vec::new();
    };
    let mut names: Vec<_> = read
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[cfg(unix)]
#[test]
fn autosave_writes_a_recovery_file_without_touching_scene_state() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    let (token, dirty, status) = (editor.scene_token, editor.is_dirty(), editor.status.clone());
    let undo = editor.history.can_undo();
    let reply = remote(&mut editor, Operation::AutosaveNow).unwrap();
    let OperationReply::Autosaved {
        sequence, bytes, ..
    } = reply
    else {
        panic!("expected Autosaved, got {reply:?}");
    };
    assert_eq!((sequence, bytes > 0), (1, true));
    assert_eq!(
        files(tmp.path()),
        [
            "untitled.autosave.p".to_string()
                + &std::process::id().to_string()
                + ".000001.recovery"
        ]
    );
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.can_undo(), undo);
    assert_eq!(
        editor.status, status,
        "autosave does not report on the status line"
    );
    assert!(editor.files.scene_stem().is_none());
    let second = remote(&mut editor, Operation::AutosaveNow).unwrap();
    assert!(matches!(
        second,
        OperationReply::Autosaved { sequence: 2, .. }
    ));
}

#[test]
fn disabled_by_default_writes_nothing_and_status_says_so() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert!(matches!(
        remote(&mut editor, Operation::AutosaveNow),
        Err(OperationError::Recovery(RecoveryFailure::Disabled))
    ));
    let status = remote(&mut editor, Operation::RecoveryStatus).unwrap();
    assert!(matches!(
        status,
        OperationReply::RecoveryStatus {
            enabled: false,
            max_backups: None,
            files: 0
        }
    ));
}

#[cfg(unix)]
#[test]
fn status_counts_files_and_reads_without_a_token() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    remote(&mut editor, Operation::AutosaveNow).unwrap();
    let reply = editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: None,
                operation: Operation::RecoveryStatus,
            },
            &egui::Context::default(),
        )
        .unwrap();
    assert!(matches!(
        reply,
        OperationReply::RecoveryStatus {
            enabled: true,
            files: 1,
            max_backups: Some(_)
        }
    ));
    let shown = serde_json::to_string(&reply).unwrap();
    assert!(
        !shown.contains("autosave") && !shown.contains(tmp.path().to_str().unwrap()),
        "{shown}"
    );
}

#[cfg(unix)]
#[test]
fn remote_guards_refuse_stale_token_draft_modal_closing_and_manipulation() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    let stale = editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(Uuid::new_v4()),
            operation: Operation::AutosaveNow,
        },
        &egui::Context::default(),
    );
    assert!(matches!(stale, Err(OperationError::StaleScene)));
    editor.name = "pending draft".into();
    assert!(editor.has_draft());
    assert!(matches!(
        remote(&mut editor, Operation::AutosaveNow),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.name, "pending draft", "the draft is not committed");
    assert!(files(tmp.path()).is_empty());

    let mut modal = enabled(tmp.path());
    modal.new_document().unwrap();
    assert!(modal.files.is_modal());
    assert!(matches!(
        remote(&mut modal, Operation::AutosaveNow),
        Err(OperationError::BusyEditing)
    ));
    assert!(matches!(
        modal.apply_operation(Operation::AutosaveNow),
        Err(OperationError::BusyEditing)
    ));

    let mut closing = enabled(tmp.path());
    closing.close_document().unwrap();
    assert!(matches!(
        remote(&mut closing, Operation::AutosaveNow),
        Err(OperationError::BusyEditing)
    ));
    assert!(files(tmp.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn a_native_draft_is_not_committed_by_autosave() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    editor.name = "pending draft".into();
    let before = editor.history.can_undo();
    editor.apply_operation(Operation::AutosaveNow).unwrap();
    assert_eq!(editor.name, "pending draft");
    assert_eq!(editor.history.can_undo(), before);
}

#[cfg(unix)]
#[test]
fn an_active_manipulation_refuses_autosave() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    let target = editor.selected_move_target().unwrap();
    remote(
        &mut editor,
        Operation::BeginMove {
            target,
            axis: azteccraft::native_scene::WorldAxis::X,
            expected_selection_generation: None,
        },
    )
    .unwrap();
    assert!(editor.history.active_manipulation().is_some());
    {
        assert!(matches!(
            remote(&mut editor, Operation::AutosaveNow),
            Err(OperationError::BusyManipulation)
        ));
        assert!(files(tmp.path()).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn a_saved_scene_is_never_overwritten_and_names_the_recovery_after_it() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    let scene = tmp.path().join("work.mb");
    remote(
        &mut editor,
        Operation::SaveSceneAs {
            path: scene.clone(),
        },
    )
    .unwrap();
    let saved = std::fs::read(&scene).unwrap();
    editor.apply_operation(Operation::AutosaveNow).unwrap();
    assert_eq!(std::fs::read(&scene).unwrap(), saved);
    assert!(files(tmp.path())[0].starts_with("work.autosave.p"));
    assert!(!editor.is_dirty());
}

#[cfg(unix)]
#[test]
fn an_unencodable_scene_writes_nothing_and_reports_a_typed_error() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    remote(
        &mut editor,
        Operation::CreateSessionMaterial {
            color: [1.0, 0.0, 0.0],
        },
    )
    .unwrap();
    let shape = editor
        .history
        .scene()
        .shapes()
        .first()
        .map(|s| s.id())
        .unwrap();
    editor
        .history
        .execute(azteccraft::history::Command::AssignSessionMaterial {
            shape,
            material: azteccraft::native_scene::MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        })
        .unwrap();
    assert!(matches!(
        remote(&mut editor, Operation::AutosaveNow),
        Err(OperationError::Recovery(RecoveryFailure::Encode(_)))
    ));
    assert!(files(tmp.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn status_reports_an_unlistable_folder_instead_of_zero_files() {
    let tmp = tempfile::tempdir().unwrap();
    let mut editor = enabled(tmp.path());
    std::fs::write(tmp.path().join("recovery"), b"not a directory").unwrap();
    assert!(matches!(
        remote(&mut editor, Operation::RecoveryStatus),
        Err(OperationError::Recovery(RecoveryFailure::Store(_)))
    ));
}
