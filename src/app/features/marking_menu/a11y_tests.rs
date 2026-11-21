use super::pass_tests::*;
use super::*;
use crate::editor_operations::{Operation, SelectionMode};
use azteccraft::marking_menu::{Target, Zone};
use eframe::egui::{Event, accesskit};

fn nodes(output: &egui::FullOutput) -> Vec<(String, accesskit::Role, bool, Option<String>)> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == accesskit::Role::MenuItem)
        .map(|(_, node)| {
            (
                node.label().unwrap_or_default().to_string(),
                node.role(),
                node.is_disabled(),
                node.description().map(str::to_string),
            )
        })
        .collect()
}

#[test]
fn the_drawn_dial_exposes_named_stateful_entries_with_reasons() {
    let (mut editor, ctx, view) = setup();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Vertex));
    at(&mut editor, &ctx, 0.5, vec![]);
    at(&mut editor, &ctx, 1.0, rmb(nowhere(&view), true));
    at(&mut editor, &ctx, 1.05, rmb(nowhere(&view), false));
    let output = at(&mut editor, &ctx, 1.1, vec![]);
    let items = nodes(&output);
    let find = |label: &str| items.iter().find(|item| item.0.starts_with(label)).unwrap();
    assert!(!find("North-west: Vertex, checked").2);
    assert!(find("North: Edge").3.is_none());
    let uv = find("West: UV");
    assert!(uv.2, "disabled");
    assert_eq!(
        uv.3.as_deref(),
        Some("Unavailable: UV selection mode is not implemented yet")
    );
    let delete = find("South: Delete");
    assert!(
        delete.2
            && delete
                .3
                .as_deref()
                .unwrap()
                .starts_with("Unavailable: Component delete")
    );
    assert_eq!(
        items.len(),
        16,
        "eight zones and eight list rows: {items:?}"
    );
}

#[test]
fn keyboard_focus_reaches_a_disabled_entry_and_reports_it_as_focused() {
    let (mut editor, ctx, view) = setup();
    at(
        &mut editor,
        &ctx,
        1.0,
        vec![Event::PointerMoved(nowhere(&view))],
    );
    at(&mut editor, &ctx, 1.1, key(Key::F10, Modifiers::SHIFT));
    at(&mut editor, &ctx, 1.2, key(Key::Num3, Modifiers::NONE));
    let output = at(&mut editor, &ctx, 1.3, vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let focused = update
        .nodes
        .iter()
        .find(|(id, _)| *id == update.focus)
        .map(|(_, node)| node.label().unwrap_or_default().to_string())
        .unwrap_or_default();
    assert!(focused.starts_with("East: Multi"), "{focused}");
}

fn action(editor: &mut Editor, ctx: &egui::Context, time: f64, act: accesskit::Action, zone: Zone) {
    let request = accesskit::ActionRequest {
        action: act,
        target_tree: accesskit::TreeId::ROOT,
        target_node: paint::entry_id(Target::Zone(zone)).accesskit_id(),
        data: None,
    };
    at(
        editor,
        ctx,
        time,
        vec![Event::AccessKitActionRequest(request)],
    );
}

#[test]
fn the_default_action_chooses_an_enabled_entry_like_enter() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    at(&mut editor, &ctx, 1.1, vec![]);
    action(&mut editor, &ctx, 1.2, accesskit::Action::Click, Zone::NW);
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn the_default_action_on_a_disabled_entry_keeps_focus_and_reports_the_reason() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    at(&mut editor, &ctx, 1.1, vec![]);
    action(&mut editor, &ctx, 1.2, accesskit::Action::Click, Zone::W);
    assert!(editor.marking_menu.is_active(), "stays open");
    assert_eq!(editor.marking_menu.focused(), Some(Target::Zone(Zone::W)));
    assert!(editor.status.starts_with("UV: "), "{}", editor.status);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
}

#[test]
fn a_screen_reader_focus_request_moves_the_menu_focus_too() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    at(&mut editor, &ctx, 1.1, vec![]);
    action(&mut editor, &ctx, 1.2, accesskit::Action::Focus, Zone::SE);
    assert_eq!(editor.marking_menu.focused(), Some(Target::Zone(Zone::SE)));
}

#[test]
fn the_dial_is_a_menu_node_that_parents_its_entries() {
    let (mut editor, ctx, view) = setup();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Vertex));
    open_sticky(&mut editor, &ctx, nowhere(&view));
    let output = at(&mut editor, &ctx, 1.1, vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let (_, menu) = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == accesskit::Role::Menu)
        .expect("a menu node");
    assert_eq!(menu.label(), Some("Marking menu"));
    let by_id: std::collections::HashMap<_, _> = update.nodes.iter().cloned().collect();
    let mut stack: Vec<_> = menu.children().to_vec();
    let mut items = 0;
    while let Some(id) = stack.pop() {
        let node = &by_id[&id];
        items += usize::from(node.role() == accesskit::Role::MenuItem);
        stack.extend(node.children().iter().copied());
    }
    assert_eq!(
        items, 16,
        "eight zones and eight rows are descendants of the menu node"
    );
}

#[test]
fn a_hover_reason_is_transient_and_never_overwrites_an_error() {
    let (mut editor, ctx, view) = setup();
    let origin = nowhere(&view);
    editor.status = "Ready".into();
    open_sticky(&mut editor, &ctx, origin);
    let uv = origin + offset(Zone::W, 60.0);
    at(&mut editor, &ctx, 1.1, vec![Event::PointerMoved(uv)]);
    assert!(editor.status.starts_with("UV: "), "{}", editor.status);
    at(&mut editor, &ctx, 1.2, vec![Event::PointerMoved(origin)]);
    assert_eq!(
        editor.status, "Ready",
        "leaving the entry restores the status"
    );
    at(&mut editor, &ctx, 1.3, vec![Event::PointerMoved(uv)]);
    at(&mut editor, &ctx, 1.4, key(Key::Escape, Modifiers::NONE));
    at(&mut editor, &ctx, 1.5, vec![]);
    assert_eq!(
        editor.status, "Ready",
        "closing the menu restores the status"
    );
    // An error is never overwritten by a hover reason.
    open_sticky(&mut editor, &ctx, origin);
    editor.status = "Export failed".into();
    editor.error = true;
    at(&mut editor, &ctx, 2.1, vec![Event::PointerMoved(uv)]);
    assert_eq!(editor.status, "Export failed");
    assert!(editor.error);
}
