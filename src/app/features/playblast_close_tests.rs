//! A close that is requested and then declined must not cost the playblast job (#460).
use super::*;
use crate::app::features::playblast::fake::{self, Seen};
use crate::editor_operations::{Operation, PlayblastFormat, PlayblastRequest};
use eframe::egui;

fn request(directory: &std::path::Path) -> PlayblastRequest {
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

/// A clean scene, so a close request is approved without a save prompt.
fn editor(seen: &Seen) -> Editor {
    let mut editor = Editor::default();
    editor.playblast.factory = Some(fake::factory(Seen::clone(seen)));
    editor
}

fn started(dir: &std::path::Path, seen: &Seen) -> Editor {
    let mut editor = editor(seen);
    editor
        .apply_operation(Operation::Playblast(request(dir)))
        .unwrap();
    editor
}

fn ticks(editor: &mut Editor, count: usize) {
    let ctx = egui::Context::default();
    for _ in 0..count {
        editor.tick_playblast(&ctx);
    }
}

#[test]
fn a_pending_close_pauses_the_tick_and_the_job_resumes_when_it_is_declined() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let mut editor = started(dir.path(), &seen);
    editor.request_close();
    assert!(editor.files.is_closing() && !editor.files.is_modal() && !editor.has_draft());
    ticks(&mut editor, 10);
    assert!(seen.lock().unwrap().is_empty());
    editor.decline_terminate();
    ticks(&mut editor, 1);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn a_declined_close_keeps_the_job_and_it_finishes_with_every_frame_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let mut editor = started(dir.path(), &Seen::default());
    editor.request_close();
    editor.decline_terminate();
    assert!(editor.playblast.run.is_some());
    ticks(&mut editor, 200);
    assert!(editor.playblast.run.is_none());
    assert_eq!(
        editor.playblast.last,
        Some(azteccraft::playblast::JobProgress::Finished { total: 3 })
    );
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["shot.0001.png", "shot.0002.png", "shot.0003.png"]);
}

/// The approval an MCP close holds while its reply is flushed is dropped when the delivery fails;
/// the job must survive that, which a cancel at approval time would not allow.
#[cfg(feature = "mcp")]
#[test]
fn a_close_whose_delivery_fails_keeps_the_job() {
    use crate::mcp::CloseDelivery;
    let dir = tempfile::tempdir().unwrap();
    let mut editor = started(dir.path(), &Seen::default());
    let ctx = egui::Context::default();
    let (bridge, ingress) = crate::mcp::test_bridge(ctx.clone());
    editor.mcp = Some(bridge);
    editor.request_close();
    ingress.force_close_delivery(CloseDelivery::Waiting);
    editor.handle_close(&ctx);
    assert!(editor.files.is_closing() && editor.playblast.run.is_some());
    ingress.force_close_delivery(CloseDelivery::Failed);
    editor.handle_close(&ctx);
    assert!(!editor.files.is_closing() && editor.playblast.run.is_some());
    ticks(&mut editor, 200);
    assert_eq!(
        editor.playblast.last,
        Some(azteccraft::playblast::JobProgress::Finished { total: 3 })
    );
}
