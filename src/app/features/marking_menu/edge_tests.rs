//! Guard edges of the menu pass: placement, unsupported variants, blocked opening, drafts and
//! text focus.

use super::layout::{DIAL_EXTENT, dial_centre};
use super::pass_tests::*;
use super::*;
use crate::editor_operations::{Operation, SelectionMode};
use azteccraft::history::ManipulationProvenance;
use azteccraft::marking_menu::Zone;
use azteccraft::native_scene::{TransformTarget, WorldAxis};
use eframe::egui::{Event, Key, Modifiers};

#[test]
fn the_dial_is_pulled_inside_the_viewport_and_unusable_rects_are_safe() {
    let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 600.0));
    assert_eq!(
        dial_centre([400.0, 300.0], rect),
        [400.0, 300.0],
        "inside: unchanged"
    );
    assert_eq!(
        dial_centre([11.0, 21.0], rect),
        [10.0 + DIAL_EXTENT, 20.0 + DIAL_EXTENT],
        "top-left corner"
    );
    assert_eq!(
        dial_centre([5000.0, 5000.0], rect),
        [810.0 - DIAL_EXTENT, 620.0 - DIAL_EXTENT],
        "far outside"
    );
    let tiny = Rect::from_min_size(Pos2::ZERO, egui::vec2(100.0, 100.0));
    assert_eq!(
        dial_centre([3.0, 4.0], tiny),
        [50.0, 50.0],
        "smaller than the dial: centred"
    );
    assert_eq!(
        dial_centre([3.0, 4.0], Rect::NOTHING),
        [3.0, 4.0],
        "no viewport: unchanged"
    );
}

#[test]
fn a_dial_opened_in_the_corner_is_drawn_and_clicked_where_it_was_pulled_to() {
    let (mut editor, ctx, view) = setup();
    let corner = view.rect.left_top() + egui::vec2(5.0, 5.0);
    open_sticky(&mut editor, &ctx, corner);
    let centre = dial_centre([corner.x, corner.y], view.rect);
    let centre = Pos2::new(centre[0], centre[1]);
    assert!(centre.x > corner.x + 100.0, "the dial moved inward");
    // The wedge is where it is drawn: Vertex (NW) is chosen by clicking there.
    let vertex = centre + offset(Zone::NW, 60.0);
    at(&mut editor, &ctx, 2.0, lmb(vertex, true));
    at(&mut editor, &ctx, 2.1, lmb(vertex, false));
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
}

#[test]
fn a_held_gesture_in_the_corner_still_reads_from_the_press_origin() {
    let (mut editor, ctx, view) = setup();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let corner = view.rect.left_top() + egui::vec2(5.0, 5.0);
    at(&mut editor, &ctx, 1.0, rmb(corner, true));
    let right = corner + offset(Zone::E, 40.0);
    at(&mut editor, &ctx, 1.05, vec![Event::PointerMoved(right)]);
    at(&mut editor, &ctx, 1.1, rmb(right, false));
    assert_eq!(
        editor.selection_mode,
        SelectionMode::Object,
        "E of the press is Object Mode"
    );
}

#[test]
fn shift_and_ctrl_rmb_say_they_are_unsupported_and_alt_and_outside_say_nothing() {
    let message = "Shift+RMB and Ctrl+RMB menus: not implemented yet";
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL] {
        let (mut editor, ctx, view) = setup();
        let press = button(nowhere(&view), PointerButton::Secondary, true, modifiers);
        at(&mut editor, &ctx, 1.0, press);
        assert_eq!(editor.status, message, "{modifiers:?}");
        assert!(!editor.error && !editor.marking_menu.holds_overlay());
    }
    let (mut editor, ctx, view) = setup();
    editor.status = "Ready".into();
    let press = button(
        nowhere(&view),
        PointerButton::Secondary,
        true,
        Modifiers::ALT,
    );
    at(&mut editor, &ctx, 1.0, press);
    assert_eq!(editor.status, "Ready", "Alt+RMB is the dolly");
    let alt_shift = Modifiers::SHIFT | Modifiers::ALT;
    let both = button(nowhere(&view), PointerButton::Secondary, true, alt_shift);
    at(&mut editor, &ctx, 1.5, both);
    assert_eq!(editor.status, "Ready", "Alt wins over Shift");
    let outside = button(
        Pos2::new(40.0, 4.0),
        PointerButton::Secondary,
        true,
        Modifiers::SHIFT,
    );
    at(&mut editor, &ctx, 2.0, outside);
    assert_eq!(
        editor.status, "Ready",
        "a press outside the viewport says nothing"
    );
}

#[test]
fn a_manipulation_in_flight_blocks_opening() {
    let (mut editor, ctx, view) = setup();
    editor.active_tool = crate::editor_operations::ActiveTool::Move;
    let target = TransformTarget {
        node: editor.selected().unwrap(),
        path: editor.selected_path().to_vec(),
    };
    editor
        .apply_operation_guarded(
            ManipulationProvenance::Native,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
                expected_selection_generation: None,
            },
            &|| true,
        )
        .unwrap();
    assert!(editor.history.active_manipulation().is_some());
    at(&mut editor, &ctx, 1.0, rmb(nowhere(&view), true));
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn choosing_a_mode_commits_a_pending_draft_like_the_status_line() {
    let (mut editor, ctx, view) = setup();
    editor.name = "renamed".into();
    assert!(editor.has_draft());
    open_sticky(&mut editor, &ctx, nowhere(&view));
    let vertex = nowhere(&view) + offset(Zone::NW, 60.0);
    at(&mut editor, &ctx, 2.0, lmb(vertex, true));
    at(&mut editor, &ctx, 2.1, lmb(vertex, false));
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    assert!(!editor.has_draft(), "the draft was committed first");
    let id = editor.selected().unwrap();
    let name = editor.history.scene().node(id).unwrap().name().to_string();
    assert_eq!(name, "renamed");
}

#[test]
fn a_disabled_entry_does_nothing_and_keeps_the_menu_open() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    let uv = nowhere(&view) + offset(Zone::W, 60.0);
    at(&mut editor, &ctx, 2.0, lmb(uv, true));
    at(&mut editor, &ctx, 2.1, lmb(uv, false));
    assert!(editor.marking_menu.is_active());
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(editor.status.starts_with("UV: "), "{}", editor.status);
}

#[test]
fn a_focused_text_field_keeps_its_keys() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    ctx.memory_mut(|memory| memory.request_focus(crate::app::name_edit_id()));
    at(&mut editor, &ctx, 1.5, vec![]);
    assert!(crate::app::text_edit_focused(&ctx));
    assert!(editor.marking_menu.is_active());
    let tab = at(&mut editor, &ctx, 2.0, key(Key::Tab, Modifiers::NONE));
    drop(tab);
    assert_eq!(editor.marking_menu.focused(), None, "the menu ignored Tab");
    assert!(
        ctx.input(|input| input
            .events
            .iter()
            .any(|e| matches!(e, Event::Key { key: Key::Tab, .. }))),
        "and the hook left it for the field"
    );
}
