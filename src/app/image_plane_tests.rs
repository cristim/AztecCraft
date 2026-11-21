//! Issue #459 P4: `SetImagePlane` and `RemoveImagePlane` through the real Editor dispatch. The
//! guard rows (held drag, draft, modal, close, stale token) are in `guard_matrix_tests.rs`.
use super::*;
use azteccraft::image_plane::model::ImagePlaneEdit;
use std::path::{Path, PathBuf};

fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
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

fn set(name: &Path) -> Operation {
    Operation::SetImagePlane(ImagePlaneEdit {
        image_name: Some(name.to_path_buf()),
        ..ImagePlaneEdit::default()
    })
}

fn depth(depth: f32) -> Operation {
    Operation::SetImagePlane(ImagePlaneEdit {
        depth: Some(depth),
        ..ImagePlaneEdit::default()
    })
}

fn remote_plate(
    editor: &mut Editor,
    operation: Operation,
) -> Result<OperationReply, OperationError> {
    let token = editor.scene_token;
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation,
        },
        &egui::Context::default(),
    )
}

fn changed(reply: Result<OperationReply, OperationError>) -> bool {
    match reply.unwrap() {
        OperationReply::Changed => true,
        OperationReply::Unchanged => false,
        other => panic!("unexpected reply {other:?}"),
    }
}

#[test]
fn set_creates_the_plane_from_a_good_first_frame_and_leaves_scene_and_history_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "plate.0001.png", 4, 2);
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let (dirty, scene, undoable) = (
        editor.is_dirty(),
        editor.history.scene().clone(),
        editor.history.can_undo(),
    );
    assert!(changed(remote_plate(&mut editor, set(&path))));
    let state = editor.image_plane.as_ref().unwrap();
    assert_eq!(state.image_size, (4, 2));
    assert_eq!(state.plane.depth, 100.0);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undoable);
    assert!(!changed(remote_plate(&mut editor, set(&path))));
}

#[test]
fn a_bad_file_changes_nothing_and_the_error_carries_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("SECRETSHOW");
    std::fs::create_dir(&secret).unwrap();
    let junk = secret.join("junk.png");
    std::fs::write(&junk, b"not an image").unwrap();
    let mut editor = Editor::default();
    for bad in [junk, secret.join("missing.png"), secret.join("a.txt")] {
        let error = remote_plate(&mut editor, set(&bad)).unwrap_err();
        assert!(matches!(error, OperationError::ImagePlane(_)), "{error}");
        assert!(!format!("{error} {error:?}").contains("SECRETSHOW"));
        assert!(editor.image_plane.is_none());
    }
    let good = write_png(dir.path(), "plate.png", 2, 2);
    remote_plate(&mut editor, set(&good)).unwrap();
    let before = editor.image_plane.clone();
    remote_plate(&mut editor, set(&secret.join("missing.png"))).unwrap_err();
    assert_eq!(editor.image_plane, before);
}

#[test]
fn an_empty_edit_is_refused_with_or_without_a_plane() {
    let mut editor = Editor::default();
    let error = remote_plate(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit::default()),
    )
    .unwrap_err();
    assert!(matches!(error, OperationError::ImagePlane(_)), "{error}");
    assert!(error.to_string().contains("at least one"), "{error}");
    let dir = tempfile::tempdir().unwrap();
    remote_plate(&mut editor, set(&write_png(dir.path(), "p.png", 2, 2))).unwrap();
    let before = editor.image_plane.clone();
    remote_plate(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit::default()),
    )
    .unwrap_err();
    assert_eq!(editor.image_plane, before);
}

#[test]
fn a_depth_only_edit_changes_the_plane_and_a_repeat_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&write_png(dir.path(), "p.png", 2, 2))).unwrap();
    assert!(changed(remote_plate(&mut editor, depth(250.0))));
    assert_eq!(editor.image_plane.as_ref().unwrap().plane.depth, 250.0);
    assert!(!changed(remote_plate(&mut editor, depth(250.0))));
}

#[test]
fn a_creation_without_a_name_and_an_invalid_value_leave_no_plane() {
    let mut editor = Editor::default();
    remote_plate(&mut editor, depth(250.0)).unwrap_err();
    assert!(editor.image_plane.is_none());
    let dir = tempfile::tempdir().unwrap();
    let edit = Operation::SetImagePlane(ImagePlaneEdit {
        image_name: Some(write_png(dir.path(), "p.png", 2, 2)),
        alpha_gain: Some(f32::NAN),
        ..ImagePlaneEdit::default()
    });
    let error = remote_plate(&mut editor, edit).unwrap_err();
    assert!(error.to_string().contains("alpha_gain"), "{error}");
    assert!(editor.image_plane.is_none());
}

#[test]
fn a_smaller_image_with_a_stored_coverage_surfaces_the_reset_text() {
    use azteccraft::image_plane::model::Coverage;
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&write_png(dir.path(), "big.png", 8, 8))).unwrap();
    remote_plate(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit {
            coverage: Some(Some(Coverage {
                origin_x: 4,
                origin_y: 4,
                width: 4,
                height: 4,
            })),
            ..ImagePlaneEdit::default()
        }),
    )
    .unwrap();
    let before = editor.image_plane.clone();
    let error =
        remote_plate(&mut editor, set(&write_png(dir.path(), "small.png", 4, 4))).unwrap_err();
    assert!(
        error.to_string().contains("reset it in the same edit"),
        "{error}"
    );
    assert_eq!(editor.image_plane, before);
}

#[test]
fn remove_reports_changed_only_when_a_plane_exists_then_a_new_one_can_be_created() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 2, 2);
    let mut editor = Editor::default();
    assert!(!changed(remote_plate(
        &mut editor,
        Operation::RemoveImagePlane
    )));
    remote_plate(&mut editor, set(&path)).unwrap();
    assert!(changed(remote_plate(
        &mut editor,
        Operation::RemoveImagePlane
    )));
    assert!(editor.image_plane.is_none());
    assert!(changed(remote_plate(&mut editor, set(&path))));
}

#[test]
fn new_scene_clears_the_plane() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&write_png(dir.path(), "p.png", 2, 2))).unwrap();
    remote_plate(&mut editor, Operation::NewScene).unwrap();
    assert!(editor.image_plane.is_none());
}

#[cfg(unix)]
#[test]
fn open_scene_clears_the_plane() {
    let dir = tempfile::tempdir().unwrap();
    let scene = dir.path().join("scene.ma");
    let mut authored = Editor::default();
    authored.create(Primitive::Cube);
    authored
        .apply_operation(Operation::SaveSceneAs {
            path: scene.clone(),
        })
        .unwrap();
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&write_png(dir.path(), "p.png", 2, 2))).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path: scene })
        .unwrap();
    assert!(editor.image_plane.is_none());
}

#[test]
fn a_stale_token_refuses_before_touching_the_plane() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    let stale = RemoteRequest {
        expected_scene_token: Some(Uuid::nil()),
        operation: set(&write_png(dir.path(), "p.png", 2, 2)),
    };
    editor
        .remote_operation(stale, &egui::Context::default())
        .unwrap_err();
    assert!(editor.image_plane.is_none());
}

#[test]
fn the_same_name_again_is_unchanged_and_does_not_reload_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 4, 2);
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&path)).unwrap();
    // The file is replaced by a larger image, then deleted: neither may matter to the same name.
    write_png(dir.path(), "p.png", 8, 8);
    assert!(!changed(remote_plate(&mut editor, set(&path))));
    assert_eq!(editor.image_plane.as_ref().unwrap().image_size, (4, 2));
    std::fs::remove_file(&path).unwrap();
    let edit = Operation::SetImagePlane(ImagePlaneEdit {
        image_name: Some(path),
        depth: Some(7.0),
        ..ImagePlaneEdit::default()
    });
    assert!(changed(remote_plate(&mut editor, edit)));
    assert_eq!(editor.image_plane.as_ref().unwrap().plane.depth, 7.0);
}

#[test]
fn a_failed_open_keeps_the_plane() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    remote_plate(&mut editor, set(&write_png(dir.path(), "p.png", 2, 2))).unwrap();
    let before = editor.image_plane.clone();
    editor
        .apply_operation(Operation::OpenScene {
            path: dir.path().join("missing.ma"),
        })
        .unwrap_err();
    assert!(before.is_some());
    assert_eq!(editor.image_plane, before);
}
