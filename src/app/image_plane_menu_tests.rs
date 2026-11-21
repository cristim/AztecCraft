//! #459 P9a: View > Image Plane > Import Image and Remove Image Plane through the real Editor.
//! The file dialog is the only untested step (`import_image_dialog`); everything after it is
//! `run_import_image`, which dispatches the same typed operation as the MCP `set_image_plane`.
use super::features::image_plane::{NO_ATTRIBUTES_TAB_REASON, NO_PLANE_REASON};
use super::*;
use azteccraft::image_plane::model::ImagePlaneEdit;
use azteccraft::scene::Primitive;
use std::path::{Path, PathBuf};

fn png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![0u8; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    path
}

fn set(path: &Path) -> Operation {
    Operation::SetImagePlane(ImagePlaneEdit {
        image_name: Some(path.to_path_buf()),
        ..ImagePlaneEdit::default()
    })
}

fn remote(editor: &mut Editor, operation: Operation) -> Result<OperationReply, OperationError> {
    let token = editor.scene_token;
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation,
        },
        &egui::Context::default(),
    )
}

#[test]
fn the_menu_import_and_the_mcp_call_give_the_same_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "plate.png", 6, 4);
    let mut menu = Editor::default();
    menu.run_import_image(path.clone());
    let mut mcp = Editor::default();
    remote(&mut mcp, set(&path)).unwrap();
    assert!(menu.image_plane.is_some());
    assert_eq!(menu.image_plane, mcp.image_plane);
    assert_eq!(menu.image_plane.as_ref().unwrap().image_size, (6, 4));
    assert!(!menu.error);
}

#[test]
fn the_same_bad_input_is_refused_the_same_way_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let junk = dir.path().join("junk.png");
    std::fs::write(&junk, b"not an image").unwrap();
    for bad in [
        junk,
        dir.path().join("missing.png"),
        dir.path().join("a.txt"),
    ] {
        let mut menu = Editor::default();
        menu.run_import_image(bad.clone());
        let mut mcp = Editor::default();
        let refusal = remote(&mut mcp, set(&bad)).unwrap_err();
        assert!(menu.image_plane.is_none() && mcp.image_plane.is_none());
        assert!(menu.error, "{bad:?}");
        assert!(
            menu.status.contains(&refusal.to_string()),
            "status {:?} vs {refusal}",
            menu.status
        );
        assert!(!menu.status.contains(dir.path().to_str().unwrap()));
    }
}

#[test]
fn a_valid_import_commits_a_pending_draft_first_and_a_refused_name_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "plate.png", 2, 2);
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.name = "renamed".into();
    editor.name_intent = true;
    // A name the operation refuses leaves the draft untouched.
    editor.run_import_image(dir.path().join("a.txt"));
    assert!(editor.image_plane.is_none());
    assert_eq!(editor.name, "renamed");
    assert!(
        editor
            .history
            .scene()
            .nodes()
            .iter()
            .all(|n| n.name() != "renamed")
    );
    // A good one commits it first.
    editor.run_import_image(path);
    assert!(editor.image_plane.is_some());
    assert!(
        editor
            .history
            .scene()
            .nodes()
            .iter()
            .any(|n| n.name() == "renamed")
    );
}

#[test]
fn importing_again_replaces_the_image_and_keeps_the_other_settings() {
    let dir = tempfile::tempdir().unwrap();
    let first = png(dir.path(), "a.png", 4, 4);
    let second = png(dir.path(), "b.png", 8, 2);
    let mut editor = Editor::default();
    editor.run_import_image(first);
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            depth: Some(250.0),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    editor.run_import_image(second.clone());
    let state = editor.image_plane.as_ref().unwrap();
    assert_eq!(state.plane.image_name, second);
    assert_eq!(state.plane.depth, 250.0);
    assert_eq!(state.image_size, (8, 2));
}

#[test]
fn remove_works_through_the_menu_operation_and_reports_unchanged_without_a_plane() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    assert!(matches!(
        editor.native_operation_result(Operation::RemoveImagePlane),
        Ok(OperationReply::Unchanged)
    ));
    editor.run_import_image(png(dir.path(), "a.png", 2, 2));
    assert!(matches!(
        editor.native_operation_result(Operation::RemoveImagePlane),
        Ok(OperationReply::Changed)
    ));
    assert!(editor.image_plane.is_none());
}

#[test]
fn the_disabled_rows_carry_path_free_reasons() {
    for reason in [NO_PLANE_REASON, NO_ATTRIBUTES_TAB_REASON] {
        assert!(!reason.is_empty() && !reason.contains('/'), "{reason}");
    }
    assert!(NO_ATTRIBUTES_TAB_REASON.contains("Attribute Editor"));
}
