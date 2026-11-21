//! #459 P9b-2: the read-only rows, the Fit to choice, the unsupported rows and the notes of the
//! Image Plane tab, read from the AccessKit tree of real frames. Expected strings are written out.
use super::image_plane_tab::png;
use super::rig::Rig;
use crate::app::features::image_plane::PlateRuntime;
use crate::app::features::image_plane_attributes::resolved_frame_text;
use crate::editor_operations::{AttributeEditorTab, Operation, SidebarPanel};
use azteccraft::image_plane::{
    decode::{PlateError, PlateFrame},
    model::ImagePlaneEdit,
    worker::{DecodeFn, PlateSource},
};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

fn sequence_png(dir: &Path) -> std::path::PathBuf {
    for n in 1..=3 {
        png(dir, &format!("plate.{n:04}.png"), 8, 4);
    }
    dir.join("plate.0001.png")
}

/// A rig on the Image Plane tab of a sequence plate. `decode` replaces the file decoder.
fn rig_with(dir: &Path, offset: i32, decode: Option<DecodeFn>) -> Rig {
    let mut rig = Rig::new(true);
    if let Some(decode) = decode {
        rig.editor.plate = PlateRuntime::with_decode(decode);
    }
    rig.editor.run_import_image(sequence_png(dir));
    rig.editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            use_frame_extension: Some(true),
            frame_offset: Some(offset),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    for operation in [
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::ImagePlane,
        },
    ] {
        rig.editor.native_operation(operation);
    }
    rig
}

/// The value the node labelled `name` carries (a read-only row), if there is one.
fn value_of(rig: &Rig, name: &str) -> Option<String> {
    rig.update()
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some(name))
        .find_map(|(_, node)| node.value().map(str::to_owned))
}

/// (disabled, description) of the node labelled `name` that is a button.
fn button(rig: &Rig, name: &str) -> (bool, String) {
    let update = rig.update();
    let (_, node) = update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some(name) && n.role() == egui::accesskit::Role::Button)
        .unwrap_or_else(|| panic!("no button {name:?}"));
    (
        node.is_disabled(),
        node.description().unwrap_or("").to_owned(),
    )
}

fn has_text(rig: &Rig, text: &str) -> bool {
    rig.update()
        .nodes
        .iter()
        .any(|(_, n)| n.label() == Some(text) || n.value() == Some(text))
}

fn inspect(rig: &Rig) -> serde_json::Value {
    serde_json::to_value(rig.editor.inspect(false).unwrap()).unwrap()["camera"]["image_plane"]
        .clone()
}

use eframe::egui;

#[test]
fn the_read_only_rows_carry_their_name_and_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_with(dir.path(), 0, None);
    for _ in 0..40 {
        rig.settle();
        if value_of(&rig, "Resolved Frame").as_deref() == Some("frame 1, plate.0001.png, ready") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        value_of(&rig, "Image Number").as_deref(),
        Some("follows the time slider")
    );
    assert_eq!(
        value_of(&rig, "Image Size").as_deref(),
        Some("8 x 4 pixels")
    );
    assert_eq!(
        value_of(&rig, "Resolved Frame").as_deref(),
        Some("frame 1, plate.0001.png, ready")
    );
}

#[test]
fn the_resolved_frame_text_matches_inspect_scene_in_every_state() {
    let dir = tempfile::tempdir().unwrap();
    // not_loaded: nothing has ticked yet.
    let rig = rig_with(dir.path(), 0, None);
    let plane = rig.editor.image_plane.as_ref().unwrap().plane.clone();
    let report = rig.editor.plate_frame_report(&plane);
    assert_eq!(
        resolved_frame_text(&report),
        "frame 1, plate.0001.png, not_loaded"
    );
    assert_eq!(inspect(&rig)["frame_state"], "not_loaded");

    // loading: frame 1 blocks in the decoder.
    let gate: Arc<(Mutex<bool>, Condvar)> = Arc::default();
    let waiting = gate.clone();
    let decode: DecodeFn = Arc::new(move |_: &PlateSource, frame: i64| {
        let mut open = waiting.0.lock().unwrap();
        while !*open {
            open = waiting.1.wait(open).unwrap();
        }
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: vec![frame as u8; 16],
        })
    });
    let mut rig = rig_with(dir.path(), 0, Some(decode));
    rig.settle();
    assert_eq!(
        value_of(&rig, "Resolved Frame").as_deref(),
        Some("frame 1, plate.0001.png, loading")
    );
    assert_eq!(inspect(&rig)["frame_state"], "loading");
    // ready: release the decoder.
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    for _ in 0..40 {
        rig.settle();
        if inspect(&rig)["frame_state"] == "ready" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(inspect(&rig)["frame_state"], "ready");
    assert_eq!(
        value_of(&rig, "Resolved Frame").as_deref(),
        Some("frame 1, plate.0001.png, ready")
    );

    // missing: offset -2 at frame 1 has no file.
    let mut rig = rig_with(dir.path(), -2, None);
    for _ in 0..20 {
        rig.settle();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        value_of(&rig, "Resolved Frame").as_deref(),
        Some("frame -1, no file, missing")
    );
    assert_eq!(inspect(&rig)["frame_state"], "missing");

    // failed: the decoder reports a corrupt frame.
    let corrupt: DecodeFn = Arc::new(|_: &PlateSource, _: i64| Err(PlateError::Corrupt));
    let mut rig = rig_with(dir.path(), 0, Some(corrupt));
    for _ in 0..20 {
        rig.settle();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        value_of(&rig, "Resolved Frame").as_deref(),
        Some("frame 1, plate.0001.png, failed: corrupt")
    );
    let inspected = inspect(&rig);
    assert_eq!(
        (
            inspected["frame_state"].clone(),
            inspected["frame_error"].clone()
        ),
        ("failed".into(), "corrupt".into())
    );
}

#[test]
fn fit_to_and_every_unsupported_row_say_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_with(dir.path(), 0, None);
    rig.settle();
    assert!(has_text(&rig, "Film Gate"));
    assert_eq!(
        button(&rig, "Resolution Gate"),
        (true, "Unavailable: needs the resolution gate, #55".into())
    );
    let placement = "Unavailable: Placement inside the gate is not implemented: the plate always fits the film gate";
    for (name, reason) in [
        ("Size", placement),
        ("Offset", placement),
        ("Rotate", placement),
        (
            "Squeeze Correction",
            "Unavailable: Squeeze correction is not implemented",
        ),
        ("Color Gain", "Unavailable: Colour gain is not implemented"),
        (
            "Color Offset",
            "Unavailable: Colour offset is not implemented",
        ),
        (
            "Color Space",
            "Unavailable: document colour management is #49",
        ),
        (
            "Depth Map",
            "Unavailable: Depth maps need EXR images, which are not supported",
        ),
    ] {
        assert_eq!(button(&rig, name), (true, reason.into()), "{name}");
    }
}

#[test]
fn the_notes_say_session_only_reload_and_perspective() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_with(dir.path(), 0, None);
    rig.settle();
    assert!(has_text(
        &rig,
        "Session only, not saved with the scene. The same image name does not reload a changed file: Remove Image Plane, then Import Image."
    ));
    assert!(has_text(&rig, "No effect until the view is perspective"));
}

/// The status line text and the Resolved Frame row, each derived from what `inspect_scene` says.
#[test]
fn the_status_text_and_the_resolved_frame_row_share_one_report_in_every_state() {
    fn expected_status(rig: &Rig) -> Option<String> {
        let i = inspect(rig);
        let label = format!("Plate frame {}", i["resolved_frame"]);
        match i["frame_state"].as_str().unwrap() {
            "not_loaded" | "ready" => None,
            "failed" => Some(format!(
                "{label}: failed: {}",
                i["frame_error"].as_str().unwrap()
            )),
            state => Some(format!("{label}: {state}")),
        }
    }
    fn check(rig: &Rig, state: &str, row: &str, status: Option<&str>) {
        assert_eq!(inspect(rig)["frame_state"], state);
        assert_eq!(value_of(rig, "Resolved Frame").as_deref(), Some(row));
        assert_eq!(rig.editor.plate_status_text().as_deref(), status);
        assert_eq!(rig.editor.plate_status_text(), expected_status(rig));
    }
    fn wait(rig: &mut Rig, state: &str) {
        for _ in 0..40 {
            rig.settle();
            if inspect(rig)["frame_state"] == state {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let rig = rig_with(dir.path(), 1, None);
    assert_eq!(rig.editor.plate_status_text(), None);
    assert_eq!(inspect(&rig)["frame_state"], "not_loaded");

    let gate: Arc<(Mutex<bool>, Condvar)> = Arc::default();
    let waiting = gate.clone();
    let decode: DecodeFn = Arc::new(move |_: &PlateSource, _: i64| {
        let mut open = waiting.0.lock().unwrap();
        while !*open {
            open = waiting.1.wait(open).unwrap();
        }
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: vec![0; 16],
        })
    });
    let mut rig = rig_with(dir.path(), 1, Some(decode));
    rig.settle();
    check(
        &rig,
        "loading",
        "frame 2, plate.0002.png, loading",
        Some("Plate frame 2: loading"),
    );
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
    wait(&mut rig, "ready");
    check(&rig, "ready", "frame 2, plate.0002.png, ready", None);

    // The resolved frame (-1) differs from the time slider (1): the label must follow the report.
    let mut rig = rig_with(dir.path(), -2, None);
    wait(&mut rig, "missing");
    check(
        &rig,
        "missing",
        "frame -1, no file, missing",
        Some("Plate frame -1: missing"),
    );

    let corrupt: DecodeFn = Arc::new(|_: &PlateSource, _: i64| Err(PlateError::Corrupt));
    let mut rig = rig_with(dir.path(), 1, Some(corrupt));
    wait(&mut rig, "failed");
    check(
        &rig,
        "failed",
        "frame 2, plate.0002.png, failed: corrupt",
        Some("Plate frame 2: failed: corrupt"),
    );
}

#[test]
fn image_number_says_whether_the_plate_is_a_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_with(dir.path(), 0, None);
    rig.settle();
    assert_eq!(
        value_of(&rig, "Image Number").as_deref(),
        Some("follows the time slider")
    );
    rig.editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            use_frame_extension: Some(false),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    rig.settle();
    assert_eq!(
        value_of(&rig, "Image Number").as_deref(),
        Some("not used (single image)")
    );
}

#[test]
fn fit_to_is_a_read_only_film_gate_value_that_a_click_cannot_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = rig_with(dir.path(), 0, None);
    rig.settle();
    assert_eq!(value_of(&rig, "Fit to").as_deref(), Some("Film Gate"));
    let before = (
        inspect(&rig),
        rig.editor.image_plane.as_ref().unwrap().plane.clone(),
    );
    rig.click("Fit to");
    rig.settle();
    let after = (
        inspect(&rig),
        rig.editor.image_plane.as_ref().unwrap().plane.clone(),
    );
    assert_eq!(before, after);
    assert_eq!(value_of(&rig, "Fit to").as_deref(), Some("Film Gate"));
}
