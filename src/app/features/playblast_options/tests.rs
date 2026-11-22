use super::*;
use crate::app::features::playblast::fake::{self, Asked, Seen};
use eframe::egui::accesskit;

const SIZE: egui::Vec2 = egui::vec2(1200.0, 800.0);

fn editor() -> Editor {
    let mut editor = Editor::default();
    editor.playblast.factory = Some(fake::factory(Seen::default()));
    editor
}

/// One frame as `logic` then `ui` run it: `frame_with` runs only the UI pass.
fn run(editor: &mut Editor, ctx: &egui::Context) -> egui::FullOutput {
    editor.tick_playblast(ctx);
    crate::app::tests::frame_with(editor, ctx, SIZE, 1.0, vec![])
}

/// The centre, enabled state and description of the control with this role and name.
fn control(
    output: &egui::FullOutput,
    role: accesskit::Role,
    label: &str,
) -> Option<(egui::Pos2, bool, Option<String>)> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role && node.label() == Some(label))
        .find_map(|(_, node)| {
            let b = node.bounds()?;
            let at = egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
            Some((
                at,
                !node.is_disabled(),
                node.description().map(str::to_owned),
            ))
        })
}

fn press(editor: &mut Editor, ctx: &egui::Context, label: &str) {
    for _ in 0..2 {
        run(editor, ctx);
    }
    let output = run(editor, ctx);
    let (at, enabled, _) = control(&output, accesskit::Role::Button, label).expect(label);
    assert!(enabled, "{label} is disabled");
    crate::app::tests::click_at(editor, ctx, SIZE, at);
}

fn fill(editor: &mut Editor, directory: &std::path::Path, end: i32) {
    let dialog = editor.playblast_ui.dialog.as_mut().unwrap();
    dialog.directory = directory.to_string_lossy().into_owned();
    dialog.name = "shot".into();
    dialog.start = "1".into();
    dialog.end = end.to_string();
    dialog.width = "8".into();
    dialog.height = "8".into();
    dialog.percent = "100".into();
}

fn progress_label(output: &egui::FullOutput) -> String {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::ProgressIndicator)
        .and_then(|(_, node)| node.label().map(str::to_owned))
        .unwrap()
}

#[test]
fn the_windows_menu_item_opens_the_option_box_with_the_timeline_range_and_defaults() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    run(&mut editor, &ctx);
    press(&mut editor, &ctx, "Windows");
    press(&mut editor, &ctx, "Playblast");
    let dialog = editor.playblast_ui.dialog.as_ref().expect("window open");
    assert_eq!(dialog.start, editor.timeline.start.to_string());
    assert_eq!(dialog.end, editor.timeline.end.to_string());
    assert_eq!(
        (dialog.width.as_str(), dialog.height.as_str()),
        ("1280", "720")
    );
    assert_eq!(dialog.percent, "100");
    assert!(dialog.ornaments);
    for _ in 0..2 {
        run(&mut editor, &ctx);
    }
    let output = run(&mut editor, &ctx);
    for label in [
        "Start frame",
        "Output directory",
        "Width",
        "Percent",
        "Name",
    ] {
        assert!(
            control(&output, accesskit::Role::TextInput, label).is_some(),
            "{label}"
        );
    }
}

#[test]
fn start_dispatches_the_typed_playblast_operation_and_cancel_stops_it() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    editor.open_playblast_options();
    fill(&mut editor, dir.path(), 5000);
    assert_eq!(
        request(editor.playblast_ui.dialog.as_ref().unwrap()),
        Ok(PlayblastRequest {
            directory: dir.path().to_path_buf(),
            name: "shot".into(),
            start: Some(1.0),
            end: Some(5000.0),
            width: Some(8),
            height: Some(8),
            percent: 100,
            format: PlayblastFormat::Png,
            offscreen: true,
            show_ornaments: true,
        })
    );
    let scene = editor.history.scene().clone();
    press(&mut editor, &ctx, "Start");
    assert!(editor.playblast.run.is_some(), "{}", editor.status);
    assert_eq!(editor.inspect_playblast().frames_total, 5000);

    let mut output = run(&mut editor, &ctx);
    for _ in 0..2000 {
        if editor.inspect_playblast().frames_done > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
        output = run(&mut editor, &ctx);
    }
    let inspected = editor.inspect_playblast();
    assert_eq!(inspected.state, "running");
    assert!(inspected.frames_done > 0);

    assert_eq!(
        progress_label(&output),
        format!("Rendering {} of 5000 frames", inspected.frames_done)
    );
    let (_, enabled, reason) = control(&output, accesskit::Role::Button, "Start").unwrap();
    assert!(!enabled);
    assert!(reason.unwrap().contains("already running"));
    assert!(
        control(&output, accesskit::Role::Button, "Cancel playblast")
            .unwrap()
            .1
    );

    press(&mut editor, &ctx, "Cancel playblast");
    assert!(editor.playblast.run.is_none());
    let output = run(&mut editor, &ctx);
    let inspected = editor.inspect_playblast();
    assert_eq!(inspected.state, "cancelled");
    assert!(progress_label(&output).starts_with("Cancelled after"));
    assert_eq!(progress_label(&output), progress_text(&inspected));
    assert!(
        control(&output, accesskit::Role::Button, "Start")
            .unwrap()
            .1
    );
    assert!(
        !control(&output, accesskit::Role::Button, "Cancel playblast")
            .unwrap()
            .1
    );
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn a_finished_job_reports_through_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    editor.open_playblast_options();
    fill(&mut editor, dir.path(), 2);
    press(&mut editor, &ctx, "Start");
    for _ in 0..2000 {
        run(&mut editor, &ctx);
        if editor.inspect_playblast().state == "finished" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let output = run(&mut editor, &ctx);
    assert_eq!(progress_label(&output), "Finished: 2 frames");
}

#[test]
fn bad_fields_and_refusals_are_reported_without_starting_a_job() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    editor.open_playblast_options();
    for (relative, text) in [
        ("directory", "relative/dir"),
        ("percent", "0"),
        ("width", "abc"),
    ] {
        fill(&mut editor, std::path::Path::new("/"), 3);
        let dialog = editor.playblast_ui.dialog.as_mut().unwrap();
        match relative {
            "directory" => dialog.directory = text.into(),
            "percent" => dialog.percent = text.into(),
            _ => dialog.width = text.into(),
        }
        assert!(request(dialog).is_err(), "{relative}");
    }
    press(&mut editor, &ctx, "Start");
    assert!(editor.playblast.run.is_none());
    assert!(
        editor
            .playblast_ui
            .dialog
            .as_ref()
            .unwrap()
            .message
            .is_some()
    );
    // A missing directory is refused by the operation, with its typed message.
    fill(&mut editor, std::path::Path::new("/no/such/dir"), 3);
    press(&mut editor, &ctx, "Start");
    assert!(editor.playblast.run.is_none());
    let message = editor.playblast_ui.dialog.as_ref().unwrap().message.clone();
    assert!(message.unwrap().contains("directory"));
}

#[test]
fn start_is_unavailable_while_a_file_prompt_is_open() {
    let mut editor = editor();
    editor.create(azteccraft::scene::Primitive::Cube);
    assert_eq!(editor.playblast_refusal(), None);
    editor.native_operation(Operation::NewScene);
    assert!(editor.files.is_modal());
    assert!(editor.playblast_refusal().unwrap().contains("file prompt"));
}

#[test]
fn percent_and_ornaments_reach_the_operation_and_the_renderer() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Asked::default()));
    editor.playblast.factory = Some(fake::recording_factory(
        Seen::default(),
        std::sync::Arc::clone(&asked),
    ));
    editor.open_playblast_options();
    fill(&mut editor, dir.path(), 2);
    let dialog = editor.playblast_ui.dialog.as_mut().unwrap();
    dialog.percent = "50".into();
    dialog.ornaments = false;
    let sent = request(dialog).unwrap();
    assert_eq!((sent.percent, sent.show_ornaments), (50, false));
    press(&mut editor, &ctx, "Start");
    for _ in 0..2000 {
        run(&mut editor, &ctx);
        if editor.inspect_playblast().state == "finished" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let inspected = editor.inspect_playblast();
    assert_eq!(
        (inspected.state, inspected.width, inspected.height),
        ("finished", 4, 4)
    );
    let asked = asked.lock().unwrap();
    assert_eq!(asked.grid, [false]);
    assert_eq!(asked.sizes, [(4, 4); 2]);
}

#[test]
fn the_default_option_box_asks_for_the_grid_at_full_size() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Asked::default()));
    editor.playblast.factory = Some(fake::recording_factory(
        Seen::default(),
        std::sync::Arc::clone(&asked),
    ));
    editor.open_playblast_options();
    fill(&mut editor, dir.path(), 1);
    press(&mut editor, &ctx, "Start");
    for _ in 0..2000 {
        run(&mut editor, &ctx);
        if editor.inspect_playblast().state == "finished" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let asked = asked.lock().unwrap();
    assert_eq!(asked.grid, [true]);
    assert_eq!(asked.sizes, [(8, 8)]);
}

#[test]
fn the_validation_error_is_exposed_as_an_alert() {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut editor = editor();
    editor.open_playblast_options();
    fill(&mut editor, std::path::Path::new("/"), 3);
    editor.playblast_ui.dialog.as_mut().unwrap().percent = "0".into();
    press(&mut editor, &ctx, "Start");
    let output = run(&mut editor, &ctx);
    assert!(
        control(&output, accesskit::Role::Alert, "Percent must be 1 to 100").is_some(),
        "the error has the alert role and its text as the name"
    );
}

#[test]
fn the_progress_text_names_frames_rendered_without_the_image_plane_image() {
    let inspection = |state, frames_without_plate| PlayblastInspection {
        state,
        frames_done: 2,
        frames_total: 3,
        frames_without_plate,
        width: 8,
        height: 8,
        directory_name: None,
        last_error: None,
    };
    assert_eq!(
        progress_text(&inspection("finished", 0)),
        "Finished: 3 frames"
    );
    assert_eq!(
        progress_text(&inspection("finished", 2)),
        "Finished: 3 frames (2 without the image plane's image)"
    );
}
