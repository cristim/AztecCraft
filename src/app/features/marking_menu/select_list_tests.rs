//! The Select overflow list: rows, availability and dispatch through the Select menu's callers.

use super::layout::mesh_menu;
use super::pass_tests::*;
use super::*;
use crate::app::menus::SELECT_RULE_COMPONENT_REASON;
use crate::editor_operations::{Operation, SelectRule, SelectionMode};
use azteccraft::marking_menu::{Resolved, Target};
use azteccraft::scene::Primitive;
use eframe::egui::{Event, accesskit};

const ROWS: [&str; 8] = [
    "Select All",
    "Select Hierarchy",
    "Invert Selection",
    "Deselect All",
    "Select Similar",
    "Make Live",
    "Metadata",
    "Assign Material",
];

#[test]
fn the_list_has_the_same_rows_in_every_mode_with_mode_dependent_availability() {
    for mode in [
        SelectionMode::Object,
        SelectionMode::Hierarchy,
        SelectionMode::Vertex,
        SelectionMode::Edge,
        SelectionMode::Face,
    ] {
        let menu = mesh_menu(mode, None);
        let labels: Vec<_> = menu.list.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ROWS, "{mode:?}");
        let rules_enabled = !mode.is_component();
        for row in 0..3 {
            assert_eq!(
                menu.is_enabled(Target::List(row)),
                rules_enabled,
                "{mode:?} row {row}"
            );
            if !rules_enabled {
                assert_eq!(
                    menu.list[row].disabled_reason(),
                    Some(SELECT_RULE_COMPONENT_REASON)
                );
            }
        }
        assert!(
            menu.is_enabled(Target::List(3)),
            "Deselect All is always enabled"
        );
        for (row, label) in ROWS.iter().enumerate().skip(4) {
            assert!(
                matches!(menu.resolve(Target::List(row)), Resolved::Disabled(_)),
                "{label} is visibly unsupported"
            );
        }
    }
}

/// Two cubes, the first selected, a sticky menu open, and the AccessKit default action sent to
/// list row `row` (a digit key reaches only the first nine entries).
fn choose_row(editor: &mut Editor, ctx: &egui::Context, row: usize, time: f64) {
    let request = accesskit::ActionRequest {
        action: accesskit::Action::Click,
        target_tree: accesskit::TreeId::ROOT,
        target_node: super::paint::entry_id(Target::List(row)).accesskit_id(),
        data: None,
    };
    at(
        editor,
        ctx,
        time,
        vec![Event::AccessKitActionRequest(request)],
    );
}

fn two_cubes() -> (Editor, egui::Context, Pos2) {
    let (mut editor, ctx, view) = setup();
    editor.create(Primitive::Cube);
    editor.select(None);
    let first = editor.history.scene().nodes()[0].id();
    editor.select(Some(first));
    (editor, ctx, nowhere(&view))
}

fn members(editor: &Editor) -> Vec<String> {
    editor
        .selection
        .members()
        .iter()
        .map(|t| format!("{:?}", t.node))
        .collect()
}

#[test]
fn each_enabled_rule_row_selects_what_the_select_menu_row_selects() {
    for (row, rule) in [
        (0, SelectRule::All),
        (1, SelectRule::Hierarchy),
        (2, SelectRule::Inverse),
    ] {
        let (mut menu_editor, ctx, origin) = two_cubes();
        open_sticky(&mut menu_editor, &ctx, origin);
        choose_row(&mut menu_editor, &ctx, row, 2.0);
        let (mut select_editor, _, _) = two_cubes();
        select_editor.native_select_by_rule(rule);
        assert_eq!(members(&menu_editor), members(&select_editor), "{rule:?}");
        assert!(
            !menu_editor.marking_menu.holds_overlay(),
            "the menu closes after a choice"
        );
    }
    // All really changes something here, so the comparison is not vacuous.
    let (mut editor, ctx, origin) = two_cubes();
    assert_eq!(editor.selection.members().len(), 1);
    open_sticky(&mut editor, &ctx, origin);
    choose_row(&mut editor, &ctx, 0, 2.0);
    assert_eq!(editor.selection.members().len(), 2);
}

#[test]
fn deselect_all_clears_the_selection_and_returns_to_object_mode_from_a_component_mode() {
    let (mut editor, ctx, origin) = two_cubes();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    open_sticky(&mut editor, &ctx, origin);
    choose_row(&mut editor, &ctx, 3, 2.0);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(editor.selection.members().is_empty());
}

#[test]
fn rule_rows_are_inert_in_a_component_mode_and_say_why() {
    for row in 0..3 {
        let (mut editor, ctx, origin) = two_cubes();
        editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
        let before = members(&editor);
        open_sticky(&mut editor, &ctx, origin);
        choose_row(&mut editor, &ctx, row, 2.0);
        assert!(editor.marking_menu.is_active(), "stays open");
        assert_eq!(members(&editor), before);
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert!(
            editor.status.ends_with(SELECT_RULE_COMPONENT_REASON),
            "{}",
            editor.status
        );
    }
}

#[test]
fn unsupported_rows_do_nothing_and_name_themselves() {
    for (row, label) in ROWS.iter().enumerate().skip(4) {
        let (mut editor, ctx, origin) = two_cubes();
        let before = members(&editor);
        open_sticky(&mut editor, &ctx, origin);
        choose_row(&mut editor, &ctx, row, 2.0);
        assert!(editor.marking_menu.is_active());
        assert_eq!(members(&editor), before);
        assert_eq!(
            editor.status,
            format!("{label}: {label} is not implemented yet")
        );
    }
}
