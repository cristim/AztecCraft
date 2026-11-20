//! The shared overlay gate (`Editor::overlay_open`) and the remote guard read the marking menu.
//! The popup rows of the same gates are pinned where they were first written (`app/tests.rs`,
//! `manipulation_tool/tests.rs`); these rows hold the menu in a phase directly, since the
//! native wiring that opens it is a later slice.

use super::features::move_tool::tests::{View, button, frame, key, oblique, raw, viewport};
use super::*;
use azteccraft::marking_menu::{Button, FrameInput};
use eframe::egui::{Key, Pos2, vec2};

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

/// A sticky menu opened from the keyboard: the per-frame pass leaves it alone (a Held menu with
/// no Secondary button down is closed by the first frame), and it is `is_active` like a Held one.
pub(in crate::app) fn hold_menu(editor: &mut Editor) {
    editor.marking_menu.key_open([100.0, 100.0], 0.0);
    assert!(editor.marking_menu.is_active());
}

fn editor_with_cube() -> (Editor, egui::Context, View) {
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: oblique(),
        ..Editor::default()
    };
    editor.create(Primitive::Cube);
    let context = egui::Context::default();
    let output = frame(&mut editor, &context, raw(SIZE, Pos2::ZERO, 1.0, vec![]));
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&output),
        scale: 1.0,
    };
    (editor, context, view)
}

fn undo_request(editor: &Editor) -> RemoteRequest {
    RemoteRequest {
        expected_scene_token: Some(editor.scene_token),
        operation: Operation::Undo,
    }
}

#[test]
fn the_gate_follows_every_menu_phase_and_the_popup() {
    let context = egui::Context::default();
    let mut editor = Editor::default();
    assert!(!editor.overlay_open(&context), "closed menu, no popup");
    editor
        .marking_menu
        .press(Button::Secondary, [100.0, 100.0], None, 0.0, &|_| true);
    assert!(editor.marking_menu.is_active());
    assert!(editor.overlay_open(&context), "held");
    editor
        .marking_menu
        .release(Button::Secondary, [100.0, 100.0], None, 0.1, &|_| true);
    assert!(editor.overlay_open(&context), "sticky open");
    editor
        .marking_menu
        .press(Button::Secondary, [0.0, 0.0], None, 0.2, &|_| true);
    assert!(
        editor.marking_menu.holds_overlay() && !editor.marking_menu.is_active(),
        "swallow keeps the gate shut"
    );
    assert!(editor.overlay_open(&context), "swallow");
    editor
        .marking_menu
        .release(Button::Secondary, [0.0, 0.0], None, 0.3, &|_| true);
    assert!(!editor.overlay_open(&context), "closed again");
    egui::Popup::open_id(&context, egui::Id::unique("gate_probe"));
    assert!(editor.overlay_open(&context), "popup still counts");
}

#[test]
fn a_held_menu_blocks_the_shortcut_pass_and_the_frame_applies_nothing() {
    // `shortcuts()` itself, so a revert of its own gate is caught even though the frame also
    // gates the tool intent.
    let intent = |editor: &mut Editor| {
        let context = egui::Context::default();
        let input = raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::W)]);
        let mut found = None;
        let mut output = context.run_ui(input, |ui| found = editor.shortcuts(ui.ctx()));
        output.textures_delta.clear();
        found
    };
    let mut control = Editor::default();
    assert_eq!(intent(&mut control), Some(ActiveTool::Move), "control");
    let mut editor = Editor::default();
    hold_menu(&mut editor);
    assert_eq!(intent(&mut editor), None, "W swallowed under the menu");

    let (mut framed, context, _) = editor_with_cube();
    let before = framed.active_tool;
    hold_menu(&mut framed);
    frame(
        &mut framed,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::W)]),
    );
    assert_eq!(framed.active_tool, before, "and the frame applies no tool");
}

#[test]
fn a_held_menu_blocks_a_viewport_click_pick() {
    let (mut editor, context, view) = editor_with_cube();
    let empty = view.rect.left_top() + vec2(5.0, 5.0);
    let click = |editor: &mut Editor| {
        frame(
            editor,
            &context,
            raw(SIZE, Pos2::ZERO, 1.0, button(empty, true)),
        );
        frame(
            editor,
            &context,
            raw(SIZE, Pos2::ZERO, 1.0, button(empty, false)),
        );
    };
    assert!(editor.selected().is_some());
    hold_menu(&mut editor);
    click(&mut editor);
    assert!(
        editor.selected().is_some(),
        "click under the menu picks nothing"
    );
    assert!(
        !editor.marking_menu.holds_overlay(),
        "the press dismissed it"
    );
    click(&mut editor);
    assert!(
        editor.selected().is_none(),
        "control: the same click on empty space deselects"
    );
}

#[test]
fn a_held_or_open_menu_refuses_remote_mutation_and_is_reported() {
    let (mut editor, context, _) = editor_with_cube();
    let refused = |editor: &mut Editor| {
        matches!(
            editor.remote_operation(undo_request(editor), &context),
            Err(OperationError::BusyEditing)
        )
    };
    assert!(!editor.inspect(false).unwrap().native_input_pending);
    assert!(!refused(&mut editor), "closed menu admits the request");
    hold_menu(&mut editor);
    assert!(refused(&mut editor), "held");
    assert!(
        editor.inspect(false).unwrap().native_input_pending,
        "reported"
    );
    editor
        .marking_menu
        .release(Button::Secondary, [100.0, 100.0], None, 0.1, &|_| true);
    assert!(refused(&mut editor), "sticky open");
    editor.marking_menu.tick(
        100.0,
        FrameInput {
            human_input: false,
            primary_down: false,
            secondary_down: false,
        },
    );
    assert!(!editor.marking_menu.is_active(), "idle dismissal");
    assert!(!refused(&mut editor), "dismissed menu admits the request");
    assert!(!editor.inspect(false).unwrap().native_input_pending);
}

#[test]
fn a_swallowing_menu_does_not_refuse_remote_mutation() {
    let (mut editor, context, _) = editor_with_cube();
    hold_menu(&mut editor);
    editor
        .marking_menu
        .release(Button::Secondary, [100.0, 100.0], None, 0.1, &|_| true);
    editor
        .marking_menu
        .press(Button::Secondary, [0.0, 0.0], None, 0.2, &|_| true);
    assert!(editor.marking_menu.holds_overlay());
    assert!(!matches!(
        editor.remote_operation(undo_request(&editor), &context),
        Err(OperationError::BusyEditing)
    ));
}
