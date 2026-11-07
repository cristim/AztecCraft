use crate::app::{Editor, History};
use crate::editor_operations::InspectPage;
use azteccraft::compatibility::MayaVersion;
use azteccraft::native_scene::NativeScene;
use azteccraft::preferences::{PreferencesError, Store};
use azteccraft::scene::{Primitive, Scene};
use serde_json::Value;

fn prefs(editor: &Editor) -> Value {
    let value = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    value["preferences"].clone()
}

fn store_in(dir: &tempfile::TempDir, bytes: Option<&[u8]>) -> Editor {
    let path = dir.path().join("preferences.json");
    if let Some(bytes) = bytes {
        std::fs::write(&path, bytes).unwrap();
    }
    let loaded = azteccraft::preferences::load(&path);
    Editor {
        preferences: loaded.preferences,
        preferences_store: loaded.store,
        ..Editor::default()
    }
}

#[test]
fn a_writable_store_reports_saved_with_no_error() {
    let dir = tempfile::tempdir().unwrap();
    let p = prefs(&store_in(&dir, None));
    assert_eq!(p["persistence"]["state"], "saved");
    assert_eq!(p["persistence"]["error"], Value::Null);
    assert_eq!(p["persistence"]["reset_available"], false);
    assert_eq!(p["maya_compatibility_version"], 2027);
    assert_eq!(p["oldest"], 2011);
    assert_eq!(p["latest"], 2027);
}

#[test]
fn a_session_only_store_reports_session_only_with_no_reset() {
    let p = prefs(&Editor::default());
    assert_eq!(p["persistence"]["state"], "session_only");
    assert_eq!(p["persistence"]["error"], Value::Null);
    assert_eq!(p["persistence"]["reset_available"], false);
}

#[test]
fn a_blocked_store_reports_not_saved_with_a_code_and_offers_reset() {
    for (bytes, code) in [
        (
            &br#"{"schema":9,"maya_compatibility_version":2027}"#[..],
            "unknown_schema",
        ),
        (&b"not json"[..], "malformed"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let p = prefs(&store_in(&dir, Some(bytes)));
        assert_eq!(p["persistence"]["state"], "not_saved");
        assert_eq!(p["persistence"]["error"], code);
        assert_eq!(p["persistence"]["reset_available"], true);
    }
}

#[test]
fn a_file_changed_on_disk_reports_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = store_in(&dir, None);
    std::fs::write(dir.path().join("preferences.json"), b"{\"schema\":2}").unwrap();
    let saved = editor.apply_operation(
        crate::editor_operations::Operation::SetCompatibilityVersion {
            version: MayaVersion::try_from(2020).unwrap(),
        },
    );
    assert!(saved.is_err());
    let p = prefs(&editor);
    assert_eq!(p["persistence"]["state"], "not_saved");
    assert_eq!(p["persistence"]["error"], "changed_on_disk");
    assert_eq!(p["persistence"]["reset_available"], true);
}

#[test]
fn the_error_is_a_path_free_machine_code_not_the_display_text() {
    let dir = tempfile::tempdir().unwrap();
    let editor = store_in(&dir, Some(b"not json"));
    let text = serde_json::to_string(&prefs(&editor)).unwrap();
    assert!(!text.contains(dir.path().to_str().unwrap()), "{text}");
    assert!(
        !text.contains(&PreferencesError::Malformed.to_string()),
        "{text}"
    );
    let error = prefs(&editor)["persistence"]["error"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        error.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
        "{error}"
    );
}

#[test]
fn every_error_code_is_distinct_and_path_free() {
    let kind = std::io::ErrorKind::NotFound;
    let all = [
        PreferencesError::NoConfigDir,
        PreferencesError::Symlink,
        PreferencesError::NotRegularFile,
        PreferencesError::TooLarge,
        PreferencesError::Unreadable(kind),
        PreferencesError::Malformed,
        PreferencesError::UnknownSchema,
        PreferencesError::VersionOutOfRange,
        PreferencesError::ChangedOnDisk,
        PreferencesError::Write(kind),
        PreferencesError::NotWritable,
        PreferencesError::ResetChanged,
        PreferencesError::ResetSymlink,
        PreferencesError::ResetBackup(kind),
        PreferencesError::ResetRemove(kind),
    ];
    let codes: std::collections::HashSet<_> = all.iter().map(|e| e.code()).collect();
    assert_eq!(codes.len(), all.len());
    for code in codes {
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{code}"
        );
    }
}

#[test]
fn pending_mirrors_the_unsaved_picker_value_not_the_scene_draft() {
    let mut editor = Editor::default();
    assert_eq!(prefs(&editor)["pending"], false);
    editor.preferences_window.pending = Some(MayaVersion::try_from(2015).unwrap());
    assert_eq!(prefs(&editor)["pending"], true);
    let whole = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert_eq!(whole["has_draft"], false);
}

#[test]
fn gated_features_name_each_gate_and_follow_the_version() {
    let mut editor = Editor::default();
    let p = prefs(&editor);
    assert_eq!(
        p["gated_features"][0]["id"],
        "frame_selection_children_split"
    );
    assert_eq!(p["gated_features"][0]["available"], true);
    editor.preferences.maya_compatibility_version = MayaVersion::try_from(2015).unwrap();
    assert_eq!(prefs(&editor)["gated_features"][0]["available"], false);
}

#[test]
fn units_are_read_only_with_a_reason() {
    let value = serde_json::to_value(Editor::default().inspect(false).unwrap()).unwrap();
    assert_eq!(value["units"]["editable"], false);
    assert!(
        !value["units"]["unsupported_reason"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(value["units"]["linear"], "cm");
    assert!(value.get("working_units").is_none());
}

#[test]
fn every_page_repeats_the_preferences_block() {
    let mut legacy = Scene::default();
    for index in 0..5 {
        legacy
            .create(Primitive::Cube, &format!("c{index}"))
            .unwrap();
    }
    let editor = Editor {
        history: History::new(NativeScene::from_legacy(legacy).unwrap()),
        preferences_store: Store::SessionOnly,
        ..Editor::default()
    };
    let first = editor
        .inspect_page(
            false,
            Some(&InspectPage {
                limit: 2,
                cursor: None,
            }),
        )
        .unwrap();
    let cursor = first
        .page
        .as_ref()
        .unwrap()
        .next_cursor
        .as_ref()
        .unwrap()
        .parse()
        .unwrap();
    let second = editor
        .inspect_page(
            false,
            Some(&InspectPage {
                limit: 2,
                cursor: Some(cursor),
            }),
        )
        .unwrap();
    assert!(second.page.as_ref().unwrap().node_offset > 0);
    for page in [first, second] {
        let value = serde_json::to_value(page).unwrap();
        assert_eq!(value["preferences"]["persistence"]["state"], "session_only");
    }
}
