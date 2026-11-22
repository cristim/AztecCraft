use super::fake::{self, Seen};
use super::*;
use crate::editor_operations::FileDecision;
use crate::editor_operations::Operation;
use azteccraft::native_scene::ALL_CHANNELS;
use azteccraft::scene::Primitive;
use eframe::egui;
use std::path::Path;

pub(in crate::app) fn editor(seen: &Seen) -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.playblast.factory = Some(fake::factory(Seen::clone(seen)));
    editor
}

pub(in crate::app) fn request(directory: &Path) -> PlayblastRequest {
    PlayblastRequest {
        directory: directory.to_path_buf(),
        name: "shot".into(),
        start: Some(1.0),
        end: Some(3.0),
        width: Some(8),
        height: Some(6),
        percent: 100,
        format: PlayblastFormat::Png,
        offscreen: true,
        show_ornaments: true,
    }
}

fn refusal(editor: &mut Editor, request: PlayblastRequest) -> PlayblastError {
    match editor.apply_operation(Operation::Playblast(request)) {
        Err(OperationError::Playblast(error)) => error,
        other => panic!("expected a playblast refusal, got {other:?}"),
    }
}

#[test]
fn start_reports_the_frame_count_and_size_and_a_second_start_is_refused_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut half = request(dir.path());
    half.percent = 50;
    let reply = editor.apply_operation(Operation::Playblast(half)).unwrap();
    assert!(matches!(
        reply,
        OperationReply::PlayblastStarted {
            frames: 3,
            width: 4,
            height: 3
        }
    ));
    let mut other = request(dir.path());
    other.name = "other".into();
    assert_eq!(refusal(&mut editor, other), PlayblastError::Busy);
    assert!(editor.playblast.run.is_some());
}

#[test]
fn invalid_requests_are_typed_and_start_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut relative = request(dir.path());
    relative.directory = "relative".into();
    assert_eq!(
        refusal(&mut editor, relative),
        PlayblastError::InvalidDirectory
    );
    let mut missing = request(dir.path());
    missing.directory = dir.path().join("absent");
    assert_eq!(
        refusal(&mut editor, missing),
        PlayblastError::InvalidDirectory
    );
    let mut name = request(dir.path());
    name.name = "../x".into();
    assert_eq!(refusal(&mut editor, name), PlayblastError::InvalidName);
    let mut range = request(dir.path());
    (range.start, range.end) = (Some(4.0), Some(2.0));
    assert_eq!(refusal(&mut editor, range), PlayblastError::InvalidRange);
    let mut size = request(dir.path());
    size.width = None;
    assert_eq!(refusal(&mut editor, size), PlayblastError::InvalidSize);
    let mut onscreen = request(dir.path());
    onscreen.offscreen = false;
    assert!(matches!(
        refusal(&mut editor, onscreen),
        PlayblastError::Unsupported(_)
    ));
    assert!(editor.playblast.run.is_none());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn an_existing_destination_is_refused_before_any_frame() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("shot.0002.png"), b"mine").unwrap();
    let seen = Seen::default();
    let mut editor = editor(&seen);
    assert_eq!(
        refusal(&mut editor, request(dir.path())),
        PlayblastError::DestinationExists { frame: 2 }
    );
    assert!(editor.playblast.run.is_none());
    assert_eq!(
        std::fs::read(dir.path().join("shot.0002.png")).unwrap(),
        b"mine"
    );
}

#[test]
fn a_session_without_a_renderer_reports_no_adapter() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert_eq!(
        refusal(&mut editor, request(dir.path())),
        PlayblastError::NoAdapter
    );
}

#[test]
fn cancel_reports_whether_a_job_was_running() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    assert!(matches!(
        editor.apply_operation(Operation::CancelPlayblast).unwrap(),
        OperationReply::Unchanged
    ));
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::CancelPlayblast).unwrap(),
        OperationReply::Changed
    ));
    assert!(editor.playblast.run.is_none());
}

pub(in crate::app) fn ticks(editor: &mut Editor, count: usize) {
    let ctx = egui::Context::default();
    for _ in 0..count {
        editor.tick_playblast(&ctx);
    }
}

pub(in crate::app) fn run_to_end(editor: &mut Editor) -> JobProgress {
    let ctx = egui::Context::default();
    for _ in 0..200 {
        editor.tick_playblast(&ctx);
        if editor.playblast.run.is_none() {
            break;
        }
    }
    editor.playblast.last.clone().expect("a job ran")
}

pub(in crate::app) fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub(in crate::app) fn decode(path: &Path) -> (png::OutputInfo, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
        .read_info()
        .unwrap();
    let mut buffer = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buffer).unwrap();
    (info, buffer)
}

/// A cube keyed to translate X from 0 at frame 1 to 20 at frame 3, parked at frame 1.
fn keyed(seen: &Seen) -> Editor {
    let mut editor = editor(seen);
    let id = editor.selected().unwrap();
    let key = |editor: &mut Editor| {
        editor
            .apply_operation(Operation::SetKeyframe {
                nodes: None,
                time: Some(f64::from(editor.timeline.current)),
                channels: ALL_CHANNELS,
            })
            .unwrap();
    };
    let go = |editor: &mut Editor, time| {
        editor
            .apply_operation(Operation::SetCurrentTime { time })
            .unwrap();
    };
    go(&mut editor, 1.0);
    key(&mut editor);
    go(&mut editor, 3.0);
    editor
        .apply_operation(Operation::EditNode {
            id,
            name: None,
            translation: [Some(20.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    key(&mut editor);
    go(&mut editor, 1.0);
    editor
}

#[test]
fn frames_land_on_disk_as_numbered_pngs_that_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
    assert_eq!(
        listing(dir.path()),
        ["shot.0001.png", "shot.0002.png", "shot.0003.png"]
    );
    for frame in 1..=3u8 {
        let (info, pixels) = decode(&dir.path().join(format!("shot.{frame:04}.png")));
        assert_eq!((info.width, info.height), (8, 6));
        let at = |x: usize, y: usize| &pixels[(y * 8 + x) * 4..][..4];
        assert_eq!(at(2, 1), [frame, 2, 1, 255]);
        assert_eq!(at(7, 5), [frame, 7, 5, 255]);
    }
}

#[test]
fn a_full_run_leaves_scene_selection_and_history_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let mut editor = keyed(&seen);
    let baseline = |editor: &Editor| {
        (
            editor.history.scene().clone(),
            editor.selection.members().to_vec(),
            editor.history.undo_len(),
            editor.history.redo_len(),
            editor.history.revision(),
            editor.history.pose_frame(),
            editor.scene_token,
            editor.timeline.current,
        )
    };
    let before = baseline(&editor);
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
    assert_eq!(baseline(&editor), before);
}

#[test]
fn each_frame_is_evaluated_from_the_keys_like_a_fresh_scene() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let mut editor = keyed(&seen);
    let authoritative = editor.history.scene().clone();
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    run_to_end(&mut editor);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.iter().map(|(f, _)| *f).collect::<Vec<_>>(), [1, 2, 3]);
    for (frame, vertices) in seen.iter() {
        let mut fresh = authoritative.clone();
        let writes = fresh.evaluated_writes(*frame as i32).unwrap();
        fresh.apply_evaluated(writes);
        let view = ViewScene::from_native(&fresh, VIEW_BUDGET).unwrap();
        assert_eq!(vertices.as_slice(), view.instances()[0].world_vertices());
    }
    assert_ne!(seen[0].1, seen[1].1);
    assert_ne!(seen[1].1, seen[2].1);
}

#[test]
fn an_unevaluable_frame_fails_the_job_with_a_typed_error_and_no_staged_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut over = request(dir.path());
    let max = f64::from(azteccraft::native_scene::MAX_FRAME);
    (over.start, over.end) = (Some(max - 1.0), Some(max + 1.0));
    editor.apply_operation(Operation::Playblast(over)).unwrap();
    let progress = run_to_end(&mut editor);
    assert_eq!(
        progress,
        JobProgress::Failed(PlayblastError::Evaluate {
            frame: max as i64 + 1
        })
    );
    assert!(
        listing(dir.path())
            .iter()
            .all(|name| name.ends_with(".png"))
    );
}

#[test]
fn cancel_mid_job_keeps_only_complete_numbered_pngs() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut long = request(dir.path());
    long.end = Some(40.0);
    editor.apply_operation(Operation::Playblast(long)).unwrap();
    ticks(&mut editor, 3);
    editor.apply_operation(Operation::CancelPlayblast).unwrap();
    assert!(matches!(
        editor.playblast.last,
        Some(JobProgress::Cancelled { .. })
    ));
    let names = listing(dir.path());
    assert!(names.len() < 40);
    assert!(
        names
            .iter()
            .all(|name| name.starts_with("shot.") && name.ends_with(".png"))
    );
    for name in names {
        decode(&dir.path().join(name));
    }
}

#[test]
fn a_modal_pauses_the_tick_and_the_job_resumes_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let mut editor = editor(&seen);
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    editor.request_new();
    assert!(editor.files.is_modal());
    ticks(&mut editor, 10);
    assert!(seen.lock().unwrap().is_empty());
    let pending = editor.files.pending_inspection().unwrap();
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: pending.id.parse().unwrap(),
            decision: FileDecision::Cancel,
        })
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
}

#[test]
fn a_pending_draft_pauses_the_tick_and_the_job_resumes_after_it_is_resolved() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let mut editor = editor(&seen);
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    editor.name = "renamed in the field".into();
    assert!(editor.has_draft() && !editor.files.is_modal() && !editor.files.is_closing());
    ticks(&mut editor, 10);
    assert!(seen.lock().unwrap().is_empty());
    assert!(editor.playblast.run.is_some());
    editor.name = editor
        .history
        .scene()
        .node(editor.selected().unwrap())
        .unwrap()
        .name()
        .to_owned();
    assert!(!editor.has_draft());
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
}

#[test]
fn new_scene_cancels_the_job_and_forgets_its_report() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut long = request(dir.path());
    long.end = Some(40.0);
    editor.apply_operation(Operation::Playblast(long)).unwrap();
    ticks(&mut editor, 2);
    editor.request_new();
    let pending = editor.files.pending_inspection().unwrap();
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: pending.id.parse().unwrap(),
            decision: FileDecision::Discard,
        })
        .unwrap();
    assert!(editor.playblast.run.is_none());
    assert!(editor.playblast.last.is_none());
    assert!(
        listing(dir.path())
            .iter()
            .all(|name| name.ends_with(".png"))
    );
}

#[test]
fn an_approved_close_cancels_the_job_and_joins_its_writer() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = Editor::default();
    editor.playblast.factory = Some(fake::factory(Seen::default()));
    let mut long = request(dir.path());
    long.end = Some(40.0);
    editor.apply_operation(Operation::Playblast(long)).unwrap();
    ticks(&mut editor, 2);
    editor.request_close();
    assert!(editor.files.is_closing());
    editor.handle_close(&egui::Context::default());
    assert!(editor.playblast.run.is_none());
    assert!(matches!(
        editor.playblast.last,
        Some(JobProgress::Cancelled { .. })
    ));
    assert!(
        listing(dir.path())
            .iter()
            .all(|name| name.ends_with(".png"))
    );
}

#[test]
fn the_inspection_follows_the_job_and_names_only_the_directory() {
    let parent = tempfile::tempdir().unwrap();
    let dir = parent.path().join("out_dir");
    std::fs::create_dir(&dir).unwrap();
    let mut editor = editor(&Seen::default());
    let idle = editor.inspect_playblast();
    assert_eq!(
        (idle.state, idle.frames_total, idle.directory_name),
        ("idle", 0, None)
    );
    editor
        .apply_operation(Operation::Playblast(request(&dir)))
        .unwrap();
    let running = editor.inspect_playblast();
    assert_eq!(
        (running.state, running.frames_done, running.frames_total),
        ("running", 0, 3)
    );
    assert_eq!((running.width, running.height), (8, 6));
    assert_eq!(running.directory_name.as_deref(), Some("out_dir"));
    run_to_end(&mut editor);
    let finished = editor.inspect_playblast();
    assert_eq!((finished.state, finished.frames_done), ("finished", 3));
    let json = serde_json::to_string(&finished).unwrap();
    assert!(!json.contains(parent.path().to_str().unwrap()));
}

#[test]
fn the_inspection_reports_cancel_progress_and_a_typed_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = editor(&Seen::default());
    let mut long = request(dir.path());
    long.end = Some(40.0);
    editor.apply_operation(Operation::Playblast(long)).unwrap();
    ticks(&mut editor, 2);
    editor.apply_operation(Operation::CancelPlayblast).unwrap();
    let cancelled = editor.inspect_playblast();
    assert_eq!((cancelled.state, cancelled.frames_total), ("cancelled", 40));
    assert!(cancelled.last_error.is_none());
    let other = tempfile::tempdir().unwrap();
    let mut over = request(other.path());
    let max = f64::from(azteccraft::native_scene::MAX_FRAME);
    (over.start, over.end) = (Some(max), Some(max + 1.0));
    editor.apply_operation(Operation::Playblast(over)).unwrap();
    run_to_end(&mut editor);
    let failed = editor.inspect_playblast();
    assert_eq!(failed.state, "failed");
    assert!(failed.last_error.unwrap().contains("cannot be evaluated"));
}
