use super::*;
use crate::editor_operations::{Operation, OperationReply};
use azteccraft::scene::Primitive;

const TWO_OBJECTS: &str =
    "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvn 0 0 1\no first\nf 1 2 3\no second\nf 2 4 3\n";

fn write(directory: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn an_import_adds_the_objects_selects_them_with_the_last_as_lead_and_is_one_undo_step() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let directory = tempfile::tempdir().unwrap();
    let path = write(&directory, "two.obj", TWO_OBJECTS);

    let reply = editor
        .apply_operation(Operation::ImportObj { path: path.clone() })
        .unwrap();

    let OperationReply::Imported {
        objects,
        vertices,
        faces,
        ignored_records,
        node_ids,
    } = reply
    else {
        panic!("{reply:?}");
    };
    assert_eq!((objects, vertices, faces, ignored_records), (2, 6, 2, 1));
    assert_eq!(node_ids.len(), 2);
    let members: Vec<String> = editor
        .selection
        .members()
        .iter()
        .map(|target| target.node.to_decimal())
        .collect();
    assert_eq!(members, node_ids);
    assert_eq!(
        editor.selection.lead().map(|t| t.node.to_decimal()),
        node_ids.last().cloned()
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Import"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);

    let mut editor = Editor::default();
    editor.native_operation(Operation::ImportObj { path });
    assert!(!editor.error);
    assert!(
        editor
            .status
            .starts_with("Imported 2 object(s), 6 vertices and 2 faces")
    );
}

#[test]
fn a_file_that_fails_to_parse_changes_nothing_and_reports_the_line() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = (
        editor.history.scene().clone(),
        editor.selection.clone(),
        editor.history.undo_label(),
    );
    let directory = tempfile::tempdir().unwrap();
    let path = write(
        &directory,
        "bad.obj",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nsurf 1\n",
    );

    editor.native_operation(Operation::ImportObj { path: path.clone() });

    assert!(editor.error);
    assert!(editor.status.contains("line 5"), "{}", editor.status);
    assert_eq!(
        before,
        (
            editor.history.scene().clone(),
            editor.selection.clone(),
            editor.history.undo_label()
        )
    );
    assert!(matches!(
        editor.apply_operation(Operation::ImportObj { path }),
        Err(OperationError::ObjImport(_))
    ));
}

#[test]
fn a_bad_path_is_refused_before_a_pending_name_draft_is_committed() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.name = "drafted".into();
    editor.name_intent = true;
    let scene = editor.history.scene().clone();
    for path in ["model.txt", "no_extension", ""] {
        let result = editor.apply_operation(Operation::ImportObj { path: path.into() });
        assert!(
            matches!(result, Err(OperationError::InvalidInput(_))),
            "{path}: {result:?}"
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.name, "drafted");
    }
}

#[test]
fn a_native_import_commits_a_valid_pending_draft_first() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.name = "renamed".into();
    editor.name_intent = true;
    let directory = tempfile::tempdir().unwrap();
    let path = write(&directory, "two.obj", TWO_OBJECTS);
    editor
        .apply_operation(Operation::ImportObj { path })
        .unwrap();
    assert!(
        editor
            .history
            .scene()
            .nodes()
            .iter()
            .any(|n| n.name() == "renamed")
    );
    assert_eq!(editor.history.scene().nodes().len(), 3);
}

#[test]
fn an_import_while_a_dialog_is_open_is_refused_and_changes_nothing() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert!(matches!(
        editor.apply_operation(Operation::NewScene),
        Ok(OperationReply::PendingConfirmation { .. })
    ));
    assert!(editor.files.is_modal());
    let before = editor.history.scene().clone();
    let directory = tempfile::tempdir().unwrap();
    let path = write(&directory, "two.obj", TWO_OBJECTS);
    assert!(matches!(
        editor.apply_operation(Operation::ImportObj { path }),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &before);
}
