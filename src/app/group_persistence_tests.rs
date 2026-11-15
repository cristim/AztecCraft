//! Issue #42: the authored Maya writers accept one transform per mesh and no hierarchy yet, so
//! Save, Save As and autosave of a grouped scene refuse with typed errors and write nothing.
//! The expected results differ per platform: Unix has the native writers, Windows refuses every
//! authored save. Ungrouping back to a flat scene saves again.
use super::tests::remote;
use super::*;
use azteccraft::document::NativeSaveError;
use azteccraft::native_scene::{GroupOptions, GroupPivot, GroupUnder};

const GROUP: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Center,
    preserve_position: true,
};

/// Two cubes grouped through the shared operation.
fn grouped() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all);
    editor.sync_fields();
    editor
        .apply_operation(Operation::Group {
            options: GROUP,
            expected_selection_generation: None,
        })
        .unwrap();
    editor
}

#[test]
fn save_of_an_untitled_grouped_scene_asks_for_a_path_on_unix_and_is_unsupported_on_windows() {
    let mut editor = grouped();
    let before = editor.history.scene().clone();
    let result = remote(&mut editor, Operation::SaveScene);
    let expected = if cfg!(unix) {
        NativeSaveError::NoDestination
    } else {
        NativeSaveError::Unsupported
    };
    assert!(
        matches!(&result, Err(OperationError::NativeSave(error)) if *error == expected),
        "{result:?}"
    );
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.is_dirty());
}

#[test]
fn save_as_of_a_grouped_scene_is_refused_and_writes_no_file() {
    let directory = tempfile::tempdir().unwrap();
    for name in ["grouped.mb", "grouped.ma"] {
        let mut editor = grouped();
        let before = editor.history.scene().clone();
        let destination = directory.path().join(name);
        let result = remote(
            &mut editor,
            Operation::SaveSceneAs {
                path: destination.clone(),
            },
        );
        #[cfg(unix)]
        {
            use azteccraft::native_scene::binary::AuthoredRefusalCode;
            if name == "grouped.mb" {
                assert!(
                    matches!(
                        result,
                        Err(OperationError::NativeSave(NativeSaveError::Unwritable(
                            AuthoredRefusalCode::NonAuthoredHierarchy
                        )))
                    ),
                    "{result:?}"
                );
            } else {
                assert!(
                    matches!(&result, Err(OperationError::NativeSave(NativeSaveError::AsciiWrite(refusal)))
                        if refusal.to_string() == "only flat transform-over-mesh ownership is supported"),
                    "{result:?}"
                );
            }
        }
        #[cfg(not(unix))]
        assert!(
            matches!(
                result,
                Err(OperationError::NativeSave(NativeSaveError::Unsupported))
            ),
            "{result:?}"
        );
        assert!(!destination.exists(), "{name}");
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.is_dirty(), "{name} stays unsaved");
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn ungrouping_back_to_a_flat_scene_saves_again() {
    use azteccraft::native_scene::{ReparentKeep, UngroupOptions};
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("flat.mb");
    let mut editor = grouped();
    editor
        .apply_operation(Operation::Ungroup {
            options: UngroupOptions {
                under: GroupUnder::Parent,
                keep: ReparentKeep::World,
            },
            expected_selection_generation: None,
        })
        .unwrap();
    assert!(
        remote(
            &mut editor,
            Operation::SaveSceneAs {
                path: destination.clone(),
            },
        )
        .is_ok()
    );
    assert!(destination.metadata().unwrap().len() > 0);
}

/// The recovery encoder is the same authored writer, so a grouped scene has no recovery bytes:
/// the typed refusal is the result on every platform (a different one per platform).
#[test]
fn the_recovery_encoder_refuses_a_grouped_scene_with_a_typed_error() {
    use azteccraft::document::{RecoveryEncodeError, encode_recovery};
    let editor = grouped();
    let result = encode_recovery(editor.history.scene());
    if cfg!(unix) {
        assert_eq!(
            result,
            Err(RecoveryEncodeError::Unwritable(
                azteccraft::native_scene::binary::AuthoredRefusalCode::NonAuthoredHierarchy
            ))
        );
    } else {
        assert!(
            matches!(result, Err(RecoveryEncodeError::Encode(_))),
            "{result:?}"
        );
    }
}

/// After Group the autosave fails typed, keeps the older recovery file (the flat scene from before
/// the Group, so restoring it silently drops the hierarchy: known-issues.md) and does not touch
/// the status line, which remote refusals never reach.
#[cfg(unix)]
#[test]
fn autosave_after_group_fails_typed_and_leaves_the_previous_recovery_file() {
    use crate::editor_operations::RecoveryFailure;
    use azteccraft::document::RecoveryEncodeError;
    let tmp = tempfile::tempdir().unwrap();
    let recovery = tmp.path().join("recovery");
    let list = || {
        let mut names: Vec<_> = std::fs::read_dir(&recovery)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let mut editor = Editor::default();
    editor.enable_recovery(recovery.clone()).unwrap();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    assert!(matches!(
        remote(&mut editor, Operation::AutosaveNow),
        Ok(OperationReply::Autosaved { .. })
    ));
    let files = list();
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all);
    editor.sync_fields();
    editor
        .apply_operation(Operation::Group {
            options: GROUP,
            expected_selection_generation: None,
        })
        .unwrap();
    let status = editor.status.clone();
    let result = remote(&mut editor, Operation::AutosaveNow);
    assert!(
        matches!(
            result,
            Err(OperationError::Recovery(RecoveryFailure::Encode(
                RecoveryEncodeError::Unwritable(_)
            )))
        ),
        "{result:?}"
    );
    assert_eq!(list(), files);
    assert_eq!(editor.status, status);
}
