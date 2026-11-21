use super::fake::{self, Seen};
use super::*;
use crate::editor_operations::Operation;
use azteccraft::scene::Primitive;
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
