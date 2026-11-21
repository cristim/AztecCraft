//! End to end: the Editor runs a playblast with the REAL `WgpuFrameRenderer` on a headless
//! adapter. Exact equality is only asserted between two renders on the same adapter; analytic
//! pixel checks use a +-2 tolerance because the CI job (Lavapipe) and local runs (Metal) differ.

use super::fake::Seen;
use super::tests::{decode, editor, listing, request, run_to_end, ticks};
use super::*;
use crate::editor_operations::Operation;
use crate::viewport_renderer::ViewportRenderer;
use crate::viewport_renderer::tests::{Offscreen, creation_context};
use azteccraft::native_scene::ALL_CHANNELS;
use azteccraft::view_scene::ViewScene;
use std::path::Path;

const BACKGROUND: [u8; 4] = [31, 31, 31, 255];

/// A red cube keyed X 0 at frame 1 to 1.5 at frame 3 (it stays in view), parked at frame 1, with
/// the window's real renderer instead of the fake.
fn real_editor(creation: &eframe::CreationContext<'_>) -> Editor {
    let mut editor = editor(&Seen::default());
    let id = editor.selected().unwrap();
    let shape = editor.history.scene().shapes().first().unwrap().id();
    let paint = Operation::AssignSessionMaterial {
        shape,
        material: azteccraft::native_scene::MaterialChoice::New {
            color: [1.0, 0.0, 0.0],
        },
    };
    editor.apply_operation(paint).unwrap();
    let key = |editor: &mut Editor, time: f64, x: Option<f64>| {
        editor
            .apply_operation(Operation::SetCurrentTime { time })
            .unwrap();
        if let Some(x) = x {
            let edit = Operation::EditNode {
                id,
                name: None,
                translation: [Some(x), None, None],
                rotation_degrees: [None; 3],
                scale: [None; 3],
            };
            editor.apply_operation(edit).unwrap();
        }
        let set = Operation::SetKeyframe {
            nodes: None,
            time: Some(time),
            channels: ALL_CHANNELS,
        };
        editor.apply_operation(set).unwrap();
    };
    key(&mut editor, 1.0, None);
    key(&mut editor, 3.0, Some(1.5));
    editor
        .apply_operation(Operation::SetCurrentTime { time: 1.0 })
        .unwrap();
    editor.playblast.factory = None;
    editor.renderer = Some(ViewportRenderer::initialize(creation).unwrap());
    editor
}

fn playblast(dir: &Path, name: &str, size: (u32, u32), ornaments: bool) -> Operation {
    let mut request = request(dir);
    request.name = name.into();
    (request.width, request.height) = (Some(size.0), Some(size.1));
    request.show_ornaments = ornaments;
    Operation::Playblast(request)
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    pixels[((y * width + x) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 2)
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_editor_sequence_matches_the_viewport_render() {
    let creation = creation_context(eframe::egui::Context::default());
    let mut editor = real_editor(&creation);
    let before = (
        editor.history.scene().clone(),
        editor.history.undo_len(),
        editor.history.redo_len(),
        editor.history.revision(),
        editor.timeline.current,
    );
    let dir = tempfile::tempdir().unwrap();
    for (name, size) in [("small", (64, 48)), ("odd", (100, 60))] {
        editor
            .apply_operation(playblast(dir.path(), name, size, false))
            .unwrap();
        assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
        let frames: Vec<_> = (1..=3)
            .map(|n| decode(&dir.path().join(format!("{name}.{n:04}.png"))))
            .collect();
        for (info, pixels) in &frames {
            assert_eq!((info.width, info.height), size);
            assert_eq!(pixels.len(), (size.0 * size.1 * 4) as usize);
            assert_eq!(at(pixels, size.0, 0, 0), BACKGROUND, "corner is background");
        }
        let centre = |n: usize| at(&frames[n].1, size.0, size.0 / 2, size.1 / 2);
        let (red, green, blue) = (centre(0)[0], centre(0)[1], centre(0)[2]);
        assert!(
            red > green + 20 && red > blue + 20,
            "a red cube: {:?}",
            centre(0)
        );
        assert_ne!(frames[0].1, frames[1].1, "animated frames differ");
        assert_ne!(frames[1].1, frames[2].1, "animated frames differ");
    }
    let names = listing(dir.path());
    assert_eq!(names.len(), 6, "{names:?}");
    assert!(
        names
            .iter()
            .all(|n| n.ends_with(".png") && !n.contains("pb_tmp"))
    );
    assert!(names.contains(&"odd.0003.png".to_string()));
    let after = (
        editor.history.scene().clone(),
        editor.history.undo_len(),
        editor.history.redo_len(),
        editor.history.revision(),
        editor.timeline.current,
    );
    assert_eq!(
        after, before,
        "the authoritative scene and history are untouched"
    );

    // Frame 1 equals the test viewport render on the same adapter (grid on, 128 square).
    editor
        .apply_operation(playblast(dir.path(), "ref", (128, 128), true))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
    let (_, first) = decode(&dir.path().join("ref.0001.png"));
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let expected = Offscreen::from_context(&creation).image(&view, &editor.camera, None);
    assert_eq!(first, expected);
    assert!(close(at(&first, 128, 64, 64), at(&expected, 128, 64, 64)));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_cancel_mid_job_leaves_no_staged_files_and_allows_a_new_job() {
    let creation = creation_context(eframe::egui::Context::default());
    let mut editor = real_editor(&creation);
    let dir = tempfile::tempdir().unwrap();
    editor
        .apply_operation(playblast(dir.path(), "cut", (64, 48), false))
        .unwrap();
    ticks(&mut editor, 1);
    editor.apply_operation(Operation::CancelPlayblast).unwrap();
    assert!(editor.playblast.run.is_none());
    assert!(matches!(
        editor.playblast.last,
        Some(JobProgress::Cancelled { .. })
    ));
    let left = listing(dir.path());
    assert!(left.len() < 3, "{left:?}");
    for name in &left {
        assert!(name.starts_with("cut.") && name.ends_with(".png"), "{name}");
        let (info, _) = decode(&dir.path().join(name));
        assert_eq!((info.width, info.height), (64, 48));
    }
    editor
        .apply_operation(playblast(dir.path(), "next", (64, 48), false))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
    assert_eq!(
        listing(dir.path())
            .iter()
            .filter(|n| n.starts_with("next."))
            .count(),
        3
    );
}
