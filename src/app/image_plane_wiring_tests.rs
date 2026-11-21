//! #459 P8: the plate in the running editor, through the real tick, the real MCP inspection and
//! the real status text. Frames come from synthetic PNGs written by the test.
use super::features::image_plane::FrameStatus;
use super::*;
use azteccraft::image_plane::{
    decode::{PlateError, PlateFrame},
    model::ImagePlaneEdit,
    worker::{DecodeFn, PlateSource},
};
use std::sync::{
    Arc, Condvar, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

/// Writes plate.0001.png .. plate.0003.png (4 x 2), frame N filled with grey 40 * N.
fn sequence(dir: &Path) -> std::path::PathBuf {
    for n in 1..=3u8 {
        let path = dir.join(format!("plate.{n:04}.png"));
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 4, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[40 * n; 32]).unwrap();
        writer.finish().unwrap();
    }
    dir.join("plate.0001.png")
}

fn editor_with_plane(first: &Path, offset: i32) -> Editor {
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            image_name: Some(first.to_path_buf()),
            use_frame_extension: Some(true),
            frame_offset: Some(offset),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    editor
}

fn set_time(editor: &mut Editor, time: f64) {
    editor
        .apply_operation(Operation::SetCurrentTime { time })
        .unwrap();
}

/// Ticks until the resolved frame is no longer loading (or three seconds pass).
fn settle(editor: &mut Editor, ctx: &egui::Context) {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        editor.update_plate(ctx);
        let plane = &editor.image_plane.as_ref().unwrap().plane;
        let frame = editor.plate_frame(plane);
        if editor.plate.status_of(frame) != Some(&FrameStatus::Loading) {
            return;
        }
        assert!(Instant::now() < until, "the frame never settled");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn plane_json(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.inspect(false).unwrap()).unwrap()["camera"]["image_plane"].clone()
}

#[test]
fn stepping_the_time_follows_current_not_start_and_inspect_reports_each_frame() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor_with_plane(&sequence(dir.path()), 0);
    let ctx = egui::Context::default();
    // Before any tick nothing has been requested.
    assert_eq!(plane_json(&editor)["frame_state"], "not_loaded");
    settle(&mut editor, &ctx);
    let p = plane_json(&editor);
    assert_eq!(
        (p["resolved_frame"].clone(), p["frame_state"].clone()),
        (1.into(), "ready".into())
    );
    // The worker may already have prefetched a neighbour: only the shown frame is certain.
    assert!(p["cache"]["frames"].as_u64().unwrap() >= 1);
    assert!(editor.plate_draw().is_some());
    // The playback range does not matter: the plate follows `timeline.current`.
    editor.timeline.start = 3;
    for (time, name) in [
        (2.0, "plate.0002.png"),
        (3.0, "plate.0003.png"),
        (1.0, "plate.0001.png"),
    ] {
        set_time(&mut editor, time);
        settle(&mut editor, &ctx);
        let p = plane_json(&editor);
        assert_eq!(p["resolved_frame"], time as i64, "{time}");
        assert_eq!(p["resolved_file_name"], name);
        assert_eq!(p["frame_state"], "ready", "{time}");
        // The grey the viewport would draw is that frame's: 40 * N.
        let shown = editor.plate.status_of(time as i64);
        assert_eq!(shown, Some(&FrameStatus::Ready));
    }
}

#[test]
fn an_offset_before_the_sequence_is_missing_and_nothing_is_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor_with_plane(&sequence(dir.path()), -2);
    let ctx = egui::Context::default();
    settle(&mut editor, &ctx);
    let p = plane_json(&editor);
    assert_eq!(p["resolved_frame"], -1);
    assert_eq!(p["frame_state"], "missing");
    assert_eq!(editor.plate.status_of(-1), Some(&FrameStatus::Missing));
    assert!(editor.plate_draw().is_none());
    assert_eq!(
        editor.plate_status_text().unwrap(),
        "Plate frame -1: missing"
    );
    // A frame that was showing is dropped when the time moves onto a missing one.
    let mut editor = editor_with_plane(&sequence(dir.path()), 0);
    settle(&mut editor, &ctx);
    assert!(editor.plate_draw().is_some());
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            frame_offset: Some(-2),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    settle(&mut editor, &ctx);
    assert!(editor.plate_draw().is_none());
}

/// A decoder that returns a grey frame at once, except for frame 2, which waits for `release`.
fn gated(release: Arc<(Mutex<bool>, Condvar)>, calls: Arc<AtomicUsize>) -> DecodeFn {
    Arc::new(move |_: &PlateSource, frame: i64| {
        calls.fetch_add(1, Ordering::SeqCst);
        if frame == 2 {
            let (lock, wake) = &*release;
            let mut open = lock.lock().unwrap();
            while !*open {
                open = wake.wait(open).unwrap();
            }
        }
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: vec![(frame as u8) * 40; 16],
        })
    })
}

#[test]
fn the_held_frame_stays_on_screen_while_the_next_loads_and_the_ui_is_kept_awake() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut editor = editor_with_plane(&first, 0);
    editor.plate = super::features::image_plane::PlateRuntime::with_decode(gated(
        release.clone(),
        calls.clone(),
    ));
    let ctx = egui::Context::default();
    settle(&mut editor, &ctx);
    let held = editor.plate_draw().unwrap();
    assert_eq!(held.key.frame, 1);
    // Frame 2 is requested and blocks: the status says loading, the frame on screen is still 1,
    // inspect and the status text agree, and a repaint was requested.
    set_time(&mut editor, 2.0);
    assert!(
        editor.update_plate(&ctx),
        "a repaint must be asked for while loading"
    );
    assert_eq!(
        editor.plate_status_text().unwrap(),
        "Plate frame 2: loading"
    );
    assert_eq!(plane_json(&editor)["frame_state"], "loading");
    assert_eq!(editor.plate_draw().unwrap().key.frame, 1);
    // Releasing the decode shows frame 2 and ends the status text.
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    settle(&mut editor, &ctx);
    assert_eq!(editor.plate_draw().unwrap().key.frame, 2);
    assert_eq!(plane_json(&editor)["frame_state"], "ready");
    assert!(editor.plate_status_text().is_none());
    // Once the prefetched neighbours have decoded, everything wanted is cached: further ticks
    // neither decode nor ask for a repaint.
    let until = Instant::now() + Duration::from_secs(5);
    while editor.update_plate(&ctx) {
        assert!(Instant::now() < until, "the window never finished loading");
        std::thread::sleep(Duration::from_millis(2));
    }
    let decoded = calls.load(Ordering::SeqCst);
    for _ in 0..20 {
        assert!(!editor.update_plate(&ctx), "nothing is outstanding");
    }
    assert_eq!(calls.load(Ordering::SeqCst), decoded);
}

#[test]
fn a_failed_frame_reports_its_code_everywhere_and_draws_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path());
    let mut editor = editor_with_plane(&first, 0);
    editor.plate = super::features::image_plane::PlateRuntime::with_decode(Arc::new(
        |_: &PlateSource, _: i64| Err(PlateError::Corrupt),
    ));
    let ctx = egui::Context::default();
    settle(&mut editor, &ctx);
    let p = plane_json(&editor);
    assert_eq!(
        (p["frame_state"].clone(), p["frame_error"].clone()),
        ("failed".into(), "corrupt".into())
    );
    assert_eq!(
        editor.plate_status_text().unwrap(),
        "Plate frame 1: failed: corrupt"
    );
    assert!(editor.plate_draw().is_none());
}

#[test]
fn new_scene_and_remove_clear_the_plate_and_release_its_frames() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path());
    let ctx = egui::Context::default();
    for clear in [Operation::NewScene, Operation::RemoveImagePlane] {
        let mut editor = editor_with_plane(&first, 0);
        settle(&mut editor, &ctx);
        assert!(editor.plate_draw().is_some());
        editor.apply_operation(clear).unwrap();
        editor.update_plate(&ctx);
        assert!(plane_json(&editor).is_null());
        assert!(editor.plate_draw().is_none());
        assert!(editor.plate_status_text().is_none());
        assert_eq!(editor.plate.cache_usage().0, 0);
    }
}

#[test]
fn a_plate_with_an_orthographic_camera_or_display_none_is_not_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor_with_plane(&sequence(dir.path()), 0);
    let ctx = egui::Context::default();
    settle(&mut editor, &ctx);
    assert!(editor.plate_draw().is_some());
    editor
        .apply_operation(Operation::SetView(ViewPreset::Top))
        .unwrap();
    assert!(editor.plate_draw().is_none());
}

/// A decoder that records every frame it is asked for and returns a small grey frame.
fn recording(log: Arc<Mutex<Vec<i64>>>) -> DecodeFn {
    Arc::new(move |_: &PlateSource, frame: i64| {
        log.lock().unwrap().push(frame);
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: vec![(frame as u8).wrapping_mul(40); 16],
        })
    })
}

fn three_frame_editor(first: &Path, decode: DecodeFn) -> Editor {
    let mut editor = editor_with_plane(first, 0);
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            frame_cache: Some(3),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    editor.plate = super::features::image_plane::PlateRuntime::with_decode(decode);
    editor
}

#[test]
fn the_playing_window_is_the_current_frame_and_the_next_capacity_minus_one() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path());
    let ctx = egui::Context::default();
    // Capacity 3 (the cache holds 3 frames, small ones): at frame 1 the window is 1, 2, 3 and at
    // frame 5 it is 5, 6, 7. The current frame is in it, nothing beyond the capacity is asked
    // for, and nothing before the current frame is.
    for (start, expected) in [(1.0, vec![1, 2, 3]), (5.0, vec![5, 6, 7])] {
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut editor = three_frame_editor(&first, recording(log.clone()));
        set_time(&mut editor, start);
        editor.timeline.playing = true;
        let until = Instant::now() + Duration::from_secs(3);
        while log.lock().unwrap().len() < expected.len() {
            editor.update_plate(&ctx);
            assert!(
                Instant::now() < until,
                "window not requested: {:?}",
                log.lock().unwrap()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        std::thread::sleep(Duration::from_millis(50));
        editor.update_plate(&ctx);
        let mut asked = log.lock().unwrap().clone();
        asked.sort_unstable();
        assert_eq!(asked, expected, "window at {start}");
        // The current frame itself is ready on the very first window.
        assert_eq!(
            editor.plate.status_of(start as i64),
            Some(&FrameStatus::Ready),
            "current frame at {start}"
        );
    }
}

#[test]
fn playback_holds_the_previous_frame_while_the_next_loads() {
    let dir = tempfile::tempdir().unwrap();
    let first = sequence(dir.path());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut editor = three_frame_editor(&first, gated(release.clone(), calls));
    let ctx = egui::Context::default();
    editor.timeline.playing = true;
    settle(&mut editor, &ctx);
    assert_eq!(editor.plate_draw().unwrap().key.frame, 1);
    // Playback advances to frame 2, whose decode blocks.
    editor.timeline.current = 2;
    assert!(
        editor.update_plate(&ctx),
        "a repaint must be asked for while loading"
    );
    assert_eq!(
        editor.plate_status_text().unwrap(),
        "Plate frame 2: loading"
    );
    assert_eq!(editor.plate_draw().unwrap().key.frame, 1, "frame 1 is held");
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    settle(&mut editor, &ctx);
    assert_eq!(editor.plate_draw().unwrap().key.frame, 2);
    assert!(editor.plate_status_text().is_none());
}

#[test]
fn renaming_the_plate_drops_the_old_frame_until_the_new_source_decodes() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let first_a = sequence(dir_a.path());
    let first_b = sequence(dir_b.path());
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let gate = release.clone();
    let blocked = first_b.clone();
    // Frames of the first sequence decode at once; the second sequence waits for the release.
    let decode: DecodeFn = Arc::new(move |source: &PlateSource, frame: i64| {
        if source.template == blocked {
            let mut open = gate.0.lock().unwrap();
            while !*open {
                open = gate.1.wait(open).unwrap();
            }
        }
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: vec![(frame as u8) * 40; 16],
        })
    });
    let mut editor = editor_with_plane(&first_a, 0);
    editor.plate = super::features::image_plane::PlateRuntime::with_decode(decode);
    let ctx = egui::Context::default();
    settle(&mut editor, &ctx);
    let old = editor.plate_draw().unwrap();
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            image_name: Some(first_b),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    // The tick that applies the rename starts a new source generation: the old file's frame must
    // not stay on screen while the new one loads.
    assert!(editor.update_plate(&ctx));
    assert!(
        editor.plate_draw().is_none(),
        "the old source's frame is gone"
    );
    assert_eq!(
        editor.plate_status_text().unwrap(),
        "Plate frame 1: loading"
    );
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    settle(&mut editor, &ctx);
    let new = editor.plate_draw().unwrap();
    assert_ne!(new.key.generation, old.key.generation);
}
