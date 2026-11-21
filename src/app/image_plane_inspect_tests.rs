//! Issue #459 P5: `camera.image_plane` in the real Editor inspection.
use super::*;
use azteccraft::image_plane::model::{Coverage, ImagePlaneEdit, PlateDisplayMode, PlateFit};
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

fn run(editor: &mut Editor, operation: Operation) {
    let token = editor.scene_token;
    editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: Some(token),
                operation,
            },
            &egui::Context::default(),
        )
        .unwrap();
}

fn plane(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.inspect(false).unwrap()).unwrap()["camera"]["image_plane"].clone()
}

fn with_plane(dir: &Path, sequence: bool) -> Editor {
    let mut editor = Editor::default();
    run(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit {
            image_name: Some(write_png(dir, "plate.0001.png", 4, 2)),
            use_frame_extension: Some(sequence),
            ..ImagePlaneEdit::default()
        }),
    );
    editor
}

#[test]
fn no_plane_is_null() {
    assert!(plane(&Editor::default()).is_null());
}

#[test]
fn a_new_plane_reports_defaults_size_and_resolved_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let editor = with_plane(dir.path(), false);
    let p = plane(&editor);
    assert_eq!(
        p["settings"],
        serde_json::json!({
            "use_frame_extension": false, "frame_offset": 0, "frame_cache": 24,
            "fit": "best", "depth_cm": 100.0, "alpha_gain": 1.0,
            "display_mode": "rgba", "coverage": null
        })
    );
    assert_eq!(
        p["image_size"],
        serde_json::json!({"width": 4, "height": 2})
    );
    assert_eq!(p["resolved_file_name"], "plate.0001.png");
    assert!(p["resolved_frame"].is_null());
    assert_eq!(p["frame_state"], "not_loaded");
    assert_eq!(p["cache"]["frames"], 0);
    assert_eq!(p["cache"]["bytes"], 0);
    assert_eq!(p["cache"]["budget_bytes"], 512 * 1024 * 1024);
}

#[test]
fn edited_settings_are_reported_in_their_units() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = with_plane(dir.path(), false);
    run(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit {
            frame_offset: Some(-2),
            frame_cache: Some(10),
            fit: Some(PlateFit::ToSize),
            depth: Some(250.5),
            alpha_gain: Some(0.5),
            display_mode: Some(PlateDisplayMode::Luminance),
            coverage: Some(Some(Coverage {
                origin_x: 1,
                origin_y: 0,
                width: 3,
                height: 2,
            })),
            ..ImagePlaneEdit::default()
        }),
    );
    assert_eq!(
        plane(&editor)["settings"],
        serde_json::json!({
            "use_frame_extension": false, "frame_offset": -2, "frame_cache": 10,
            "fit": "to_size", "depth_cm": 250.5, "alpha_gain": 0.5,
            "display_mode": "luminance",
            "coverage": {"origin_x": 1, "origin_y": 0, "width": 3, "height": 2}
        })
    );
}

#[test]
fn a_sequence_resolves_current_time_plus_offset_into_a_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = with_plane(dir.path(), true);
    run(&mut editor, Operation::SetCurrentTime { time: 5.0 });
    let p = plane(&editor);
    assert_eq!(p["resolved_frame"], 5);
    assert_eq!(p["resolved_file_name"], "plate.0005.png");
    assert_eq!(p["frame_state"], "not_loaded");
    run(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit {
            frame_offset: Some(7),
            ..ImagePlaneEdit::default()
        }),
    );
    let p = plane(&editor);
    assert_eq!(p["resolved_frame"], 12);
    assert_eq!(p["resolved_file_name"], "plate.0012.png");
}

#[test]
fn a_frame_before_the_sequence_is_missing_with_no_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = with_plane(dir.path(), true);
    run(&mut editor, Operation::SetCurrentTime { time: 1.0 });
    run(
        &mut editor,
        Operation::SetImagePlane(ImagePlaneEdit {
            frame_offset: Some(-2),
            ..ImagePlaneEdit::default()
        }),
    );
    let p = plane(&editor);
    assert_eq!(p["resolved_frame"], -1);
    assert!(p["resolved_file_name"].is_null());
    assert_eq!(p["frame_state"], "missing");
}

#[test]
fn the_reply_never_contains_the_directory_and_inspecting_changes_nothing() {
    for sequence in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("SECRETSHOW");
        std::fs::create_dir(&secret).unwrap();
        let mut editor = Editor::default();
        run(
            &mut editor,
            Operation::SetImagePlane(ImagePlaneEdit {
                image_name: Some(write_png(&secret, "plate.0001.png", 4, 2)),
                use_frame_extension: Some(sequence),
                ..ImagePlaneEdit::default()
            }),
        );
        let before = editor.image_plane.clone();
        let first = serde_json::to_string(&editor.inspect(false).unwrap()).unwrap();
        let second = serde_json::to_string(&editor.inspect(false).unwrap()).unwrap();
        assert_eq!(first, second);
        assert_eq!(editor.image_plane, before);
        assert!(first.contains("plate.0001.png"));
        assert!(!first.contains("SECRETSHOW"), "{first}");
        assert!(!first.contains(dir.path().to_str().unwrap()), "{first}");
    }
}

#[test]
fn every_fit_and_display_mode_has_its_own_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = with_plane(dir.path(), false);
    for (fit, name) in [
        (PlateFit::Fill, "fill"),
        (PlateFit::Best, "best"),
        (PlateFit::Horizontal, "horizontal"),
        (PlateFit::Vertical, "vertical"),
        (PlateFit::ToSize, "to_size"),
    ] {
        run(
            &mut editor,
            Operation::SetImagePlane(ImagePlaneEdit {
                fit: Some(fit),
                ..ImagePlaneEdit::default()
            }),
        );
        assert_eq!(plane(&editor)["settings"]["fit"], name);
    }
    for (mode, name) in [
        (PlateDisplayMode::None, "none"),
        (PlateDisplayMode::Outline, "outline"),
        (PlateDisplayMode::Rgb, "rgb"),
        (PlateDisplayMode::Rgba, "rgba"),
        (PlateDisplayMode::Luminance, "luminance"),
        (PlateDisplayMode::Alpha, "alpha"),
    ] {
        run(
            &mut editor,
            Operation::SetImagePlane(ImagePlaneEdit {
                display_mode: Some(mode),
                ..ImagePlaneEdit::default()
            }),
        );
        assert_eq!(plane(&editor)["settings"]["display_mode"], name);
    }
}

#[test]
fn remove_and_new_scene_return_the_key_to_null() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = with_plane(dir.path(), false);
    run(&mut editor, Operation::RemoveImagePlane);
    assert!(plane(&editor).is_null());
    let mut editor = with_plane(dir.path(), false);
    run(&mut editor, Operation::NewScene);
    assert!(plane(&editor).is_null());
}
