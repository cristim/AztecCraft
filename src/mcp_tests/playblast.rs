use super::*;
use crate::app::features::playblast::fake::{self, Seen};
use crate::editor_operations::{PlayblastFormat, PlayblastRequest};
use azteccraft::playblast::{JobProgress, PlayblastError};

fn request(directory: &std::path::Path) -> PlayblastRequest {
    PlayblastRequest {
        directory: directory.to_path_buf(),
        name: "shot".into(),
        start: Some(1.0),
        end: Some(30.0),
        width: Some(4),
        height: Some(4),
        percent: 100,
        format: PlayblastFormat::Png,
        offscreen: true,
        show_ornaments: true,
    }
}

fn remote(editor: &Editor, operation: Operation) -> RemoteRequest {
    RemoteRequest {
        expected_scene_token: Some(editor.scene_token),
        operation,
    }
}

fn editor(seen: &Seen) -> Editor {
    let mut editor = Editor::default();
    editor.create(azteccraft::scene::Primitive::Cube);
    editor.playblast.factory = Some(fake::factory(Seen::clone(seen)));
    editor
}

fn pngs_only(dir: &std::path::Path) -> bool {
    std::fs::read_dir(dir).unwrap().all(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".png")
    })
}

#[test]
fn a_hidden_session_ticks_the_job_after_process_mcp_in_the_same_pass() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = editor(&seen);
    editor.mcp = Some(bridge);
    let mut reply = ingress
        .queue(remote(&editor, Operation::Playblast(request(dir.path()))))
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        reply.try_recv().unwrap(),
        Ok(OperationReply::PlayblastStarted { frames: 30, .. })
    ));
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn a_hidden_close_cancels_the_job_before_the_tick_and_leaves_only_complete_pngs() {
    let dir = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    // A clean scene, so the close is approved without a save prompt.
    let mut editor = Editor::default();
    editor.playblast.factory = Some(fake::factory(Seen::default()));
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    editor.request_close();
    assert!(editor.files.is_closing());
    hidden_frame(&mut editor, &context, vec![]);
    assert!(editor.playblast.run.is_none());
    assert!(matches!(
        editor.playblast.last,
        Some(JobProgress::Cancelled { .. })
    ));
    assert!(pngs_only(dir.path()));
}

#[test]
fn remote_guards_refuse_a_start_and_leave_no_job() {
    let dir = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let start = |editor: &mut Editor, token: Option<Uuid>| {
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: token,
                operation: Operation::Playblast(request(dir.path())),
            },
            &context,
        )
    };
    let mut editor = editor(&Seen::default());
    assert!(matches!(
        start(&mut editor, Some(Uuid::nil())),
        Err(OperationError::StaleScene)
    ));
    editor.name = "differing_human_draft".into();
    let token = Some(editor.scene_token);
    assert!(matches!(
        start(&mut editor, token),
        Err(OperationError::BusyEditing)
    ));
    editor.name.clear();
    editor.request_close();
    assert!(matches!(
        start(&mut editor, token),
        Err(OperationError::BusyEditing)
    ));
    assert!(editor.playblast.run.is_none());
    let mut clean = self::editor(&Seen::default());
    let token = Some(clean.scene_token);
    start(&mut clean, token).unwrap();
    assert!(matches!(
        start(&mut clean, token),
        Err(OperationError::Playblast(PlayblastError::Busy))
    ));
}

#[test]
fn a_visible_window_ticks_the_job_in_logic() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Seen::default();
    let context = egui::Context::default();
    let mut editor = editor(&seen);
    editor
        .apply_operation(Operation::Playblast(request(dir.path())))
        .unwrap();
    frame(&mut editor, &context, vec![]);
    // A running job requests a repaint, so egui may run more than one logic pass per frame.
    assert!(!seen.lock().unwrap().is_empty());
}
