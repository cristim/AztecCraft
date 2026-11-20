use super::pass_tests::*;
use super::*;
use crate::editor_operations::{Operation, SelectionMode};
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
    assert_eq!(items.len(), 8, "{items:?}");
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
