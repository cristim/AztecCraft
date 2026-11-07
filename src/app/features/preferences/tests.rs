use super::*;
use crate::app::tests::remote;
use azteccraft::preferences::{Baseline, PreferencesError};
use std::path::PathBuf;

fn v(year: u64) -> MayaVersion {
    MayaVersion::try_from(year).unwrap()
}

fn set(version: u64) -> Operation {
    Operation::SetCompatibilityVersion {
        version: v(version),
    }
}

fn writable(dir: &tempfile::TempDir) -> (Editor, PathBuf) {
    let path = dir.path().join("preferences.json");
    let loaded = azteccraft::preferences::load(&path);
    let editor = Editor {
        preferences: loaded.preferences,
        preferences_store: loaded.store,
        ..Editor::default()
    };
    (editor, path)
}

fn blocked(dir: &tempfile::TempDir) -> (Editor, PathBuf) {
    let path = dir.path().join("preferences.json");
    std::fs::write(&path, b"not json").unwrap();
    let loaded = azteccraft::preferences::load(&path);
    assert!(matches!(loaded.store, Store::Blocked { .. }));
    let editor = Editor {
        preferences: loaded.preferences,
        preferences_store: loaded.store,
        ..Editor::default()
    };
    (editor, path)
}

#[test]
fn a_default_editor_never_touches_a_file() {
    let editor = Editor::default();
    assert_eq!(editor.preferences_store, Store::SessionOnly);
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
}

#[test]
fn set_changed_persists_and_applies() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    assert!(matches!(
        editor.apply_operation(set(2015)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2015));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("2015"), "{text}");
}

#[test]
fn set_unchanged_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    assert!(matches!(
        editor.apply_operation(set(2027)),
        Ok(OperationReply::Unchanged)
    ));
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn a_write_failure_keeps_the_value_and_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = writable(&dir);
    editor.apply_operation(set(2020)).unwrap();
    let before = std::fs::read(&path).unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = editor.apply_operation(set(2012));
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        result,
        Err(OperationError::Preferences(PreferencesError::Write(_)))
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2020));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(matches!(
        editor.preferences_store,
        Store::Writable {
            baseline: Baseline::Bytes(_),
            ..
        }
    ));
}

#[test]
fn a_blocked_store_applies_the_value_for_the_session_only() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = blocked(&dir);
    assert!(matches!(
        editor.apply_operation(set(2018)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2018));
    assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    assert!(matches!(editor.preferences_store, Store::Blocked { .. }));
}

#[test]
fn a_session_only_store_applies_the_value() {
    let mut editor = Editor::default();
    assert!(matches!(
        editor.apply_operation(set(2016)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2016));
}

#[test]
fn reset_works_only_when_blocked() {
    let dir = tempfile::tempdir().unwrap();
    let (mut editor, _) = writable(&dir);
    editor.apply_operation(set(2015)).unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::ResetPreferences),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2015));

    let dir = tempfile::tempdir().unwrap();
    let (mut editor, path) = blocked(&dir);
    editor.apply_operation(set(2015)).unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::ResetPreferences),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
    assert!(!path.exists());
    assert!(dir.path().join("preferences.json.bad-1").exists());
    assert!(matches!(editor.preferences_store, Store::Writable { .. }));
}

#[test]
fn preferences_operations_leave_the_scene_alone() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let (undo, redo, token) = (
        editor.history.can_undo(),
        editor.history.can_redo(),
        editor.scene_token,
    );
    let undo_label = editor.history.undo_label();
    let dirty = editor.is_dirty();
    editor.apply_operation(set(2015)).unwrap();
    editor.apply_operation(Operation::ResetPreferences).unwrap();
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), undo_label);
    assert_eq!(
        (editor.history.can_undo(), editor.history.can_redo()),
        (undo, redo)
    );
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
}

#[test]
fn a_pending_value_does_not_dirty_the_scene_or_count_as_a_draft() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    assert!(!editor.is_dirty());
    assert!(!editor.has_draft());
}

#[test]
fn remote_set_and_reset_are_busy_while_a_value_is_pending() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    for operation in [set(2020), Operation::ResetPreferences] {
        assert!(matches!(
            remote(&mut editor, operation),
            Err(OperationError::BusyEditing)
        ));
    }
    assert_eq!(editor.preferences.maya_compatibility_version, v(2027));
    editor.preferences_window.pending = None;
    assert!(matches!(
        remote(&mut editor, set(2020)),
        Ok(OperationReply::Changed)
    ));
}

#[test]
fn a_pending_value_does_not_block_other_remote_work() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    remote(
        &mut editor,
        Operation::Orbit {
            radians: [0.1, 0.0],
        },
    )
    .unwrap();
    assert!(matches!(
        remote(&mut editor, Operation::SetGridVisible { visible: false }),
        Ok(OperationReply::Changed)
    ));
}

#[test]
fn native_set_is_not_blocked_by_a_pending_value() {
    let mut editor = Editor::default();
    editor.preferences_window.pending = Some(v(2015));
    assert!(matches!(
        editor.apply_operation(set(2020)),
        Ok(OperationReply::Changed)
    ));
}
