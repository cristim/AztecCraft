//! The Select overflow list: rows, availability and dispatch through the Select menu's callers.

use super::layout::{DIAL_RADIUS, LIST_ROW, LIST_WIDTH, dial_centre, list_rect, mesh_menu, pick};
use super::pass_tests::*;
use super::*;
use crate::app::features::move_tool::tests::View;
use crate::app::menus::SELECT_RULE_COMPONENT_REASON;
use crate::editor_operations::{Operation, SelectRule, SelectionMode};
use azteccraft::marking_menu::{Resolved, Target, Zone};
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

/// The centre of list row `row` for the menu the editor would open in its current mode.
fn list_center(editor: &Editor, origin: Pos2, view: &View, row: usize) -> Pos2 {
    let menu = editor.marking_menu_for_mode();
    let centre = dial_centre([origin.x, origin.y], view.rect);
    let rect = list_rect(centre, menu.list.len(), view.rect);
    rect.min + egui::vec2(rect.width() / 2.0, LIST_ROW * (row as f32 + 0.5))
}

#[test]
fn the_list_sits_right_of_the_dial_flips_left_near_the_edge_and_stays_in_the_viewport() {
    let viewport = Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(1000.0, 600.0));
    let right = list_rect([400.0, 300.0], 8, viewport);
    assert_eq!(right.min.x, 400.0 + DIAL_RADIUS + 12.0);
    assert_eq!(right.size(), egui::vec2(LIST_WIDTH, 8.0 * LIST_ROW));
    assert!(viewport.contains_rect(right));
    let flipped = list_rect([900.0, 300.0], 8, viewport);
    assert_eq!(
        flipped.max.x,
        900.0 - DIAL_RADIUS - 12.0,
        "flips to the left"
    );
    let high = list_rect([400.0, 21.0], 8, viewport);
    assert_eq!(high.min.y, 20.0, "shifted down into the viewport");
    let low = list_rect([400.0, 619.0], 8, viewport);
    assert_eq!(low.max.y, 620.0, "shifted up into the viewport");
    let tall = list_rect([400.0, 300.0], 100, viewport);
    assert_eq!(
        tall.min.y, 20.0,
        "a list taller than the viewport starts at its top"
    );
    // Narrow, tiny and empty viewports: the list never leaves the viewport horizontally, and a
    // viewport narrower than the list pins it to its left edge.
    for width in [0.0, 100.0, 300.0, 400.0] {
        let narrow = Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(width, 600.0));
        for x in [0.0, width / 2.0, width] {
            let rect = list_rect([x, 300.0], 8, narrow);
            assert!(rect.min.x >= narrow.min.x, "width {width} x {x}: {rect:?}");
            assert!(
                rect.max.x <= narrow.max.x || rect.min.x == narrow.min.x,
                "width {width} x {x}: {rect:?}"
            );
        }
    }
    // Neither side fits: the roomier one is used, then clamped inside.
    let narrow = Rect::from_min_size(Pos2::ZERO, egui::vec2(300.0, 600.0));
    assert_eq!(
        list_rect([50.0, 300.0], 8, narrow).min.x,
        150.0,
        "left edge is tighter: right side"
    );
    assert_eq!(
        list_rect([250.0, 300.0], 8, narrow).min.x,
        0.0,
        "right edge is tighter: left side"
    );
    let tiny = Rect::from_min_size(Pos2::new(7.0, 9.0), egui::vec2(100.0, 100.0));
    let pinned = list_rect([50.0, 50.0], 8, tiny);
    assert_eq!(
        (pinned.min.x, pinned.min.y),
        (7.0, 9.0),
        "smaller than the list: top left"
    );
    let zero = Rect::from_min_size(Pos2::new(7.0, 9.0), egui::Vec2::ZERO);
    assert_eq!(list_rect([50.0, 50.0], 8, zero).min, Pos2::new(7.0, 9.0));
    let unbounded = list_rect([5.0, 5.0], 8, Rect::NOTHING);
    assert_eq!(
        unbounded.min.x,
        5.0 + DIAL_RADIUS + 12.0,
        "no viewport: no clamp"
    );
}

#[test]
fn pick_finds_list_rows_only_for_a_sticky_menu_and_dial_zones_elsewhere() {
    for mode in [SelectionMode::Object, SelectionMode::Face] {
        let (mut editor, ctx, view) = setup();
        editor.native_operation(Operation::SetSelectionMode(mode));
        let origin = nowhere(&view);
        let menu = editor.marking_menu_for_mode();
        assert_eq!(menu.list.len(), 8, "{mode:?}");
        at(&mut editor, &ctx, 1.0, rmb(origin, true));
        let row0 = list_center(&editor, origin, &view, 0);
        let held = pick(&menu, &editor.marking_menu, view.rect, [row0.x, row0.y]);
        assert_eq!(held, None, "a held menu has no list");
        at(&mut editor, &ctx, 1.05, rmb(origin, false));
        assert!(editor.marking_menu.is_sticky());
        for row in 0..menu.list.len() {
            let p = list_center(&editor, origin, &view, row);
            assert_eq!(
                pick(&menu, &editor.marking_menu, view.rect, [p.x, p.y]),
                Some(Target::List(row)),
                "{mode:?} row {row}"
            );
        }
        let below = list_center(&editor, origin, &view, menu.list.len()) + egui::vec2(0.0, 1.0);
        assert_eq!(
            pick(&menu, &editor.marking_menu, view.rect, [below.x, below.y]),
            None
        );
        let n = origin + offset(Zone::N, 60.0);
        assert_eq!(
            pick(&menu, &editor.marking_menu, view.rect, [n.x, n.y]),
            Some(Target::Zone(Zone::N))
        );
        assert_eq!(
            pick(&menu, &editor.marking_menu, view.rect, [f32::NAN, 0.0]),
            None
        );
    }
}

fn click(editor: &mut Editor, ctx: &egui::Context, at_pos: Pos2, time: f64) {
    at(editor, ctx, time, lmb(at_pos, true));
    at(editor, ctx, time + 0.1, lmb(at_pos, false));
}

#[test]
fn clicking_a_list_row_runs_it_and_a_disabled_row_keeps_the_menu_open() {
    let (mut editor, ctx, view) = setup();
    editor.create(Primitive::Cube);
    editor.select(None);
    let first = editor.history.scene().nodes()[0].id();
    editor.select(Some(first));
    let origin = nowhere(&view);
    open_sticky(&mut editor, &ctx, origin);
    let (row_zero, row_four) = (
        list_center(&editor, origin, &view, 0),
        list_center(&editor, origin, &view, 4),
    );
    click(&mut editor, &ctx, row_four, 2.0);
    assert!(
        editor.marking_menu.is_active(),
        "Select Similar is unsupported"
    );
    assert_eq!(editor.selection.members().len(), 1);
    assert_eq!(
        editor.status,
        "Select Similar: Select Similar is not implemented yet"
    );
    click(&mut editor, &ctx, row_zero, 3.0);
    assert_eq!(editor.selection.members().len(), 2, "Select All");
    assert!(!editor.marking_menu.holds_overlay());
    // The release that chose the row picked nothing under the list.
    assert_eq!(editor.selection.members().len(), 2);
}

#[test]
fn the_drawn_list_exposes_its_rows_under_the_menu_node() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    let output = at(&mut editor, &ctx, 1.1, vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let by_id: std::collections::HashMap<_, _> = update.nodes.iter().cloned().collect();
    let (_, menu) = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Marking menu"))
        .unwrap();
    let mut stack = menu.children().to_vec();
    let mut rows = Vec::new();
    while let Some(id) = stack.pop() {
        let node = &by_id[&id];
        if node.role() == accesskit::Role::MenuItem {
            rows.push((
                node.label().unwrap_or_default().to_string(),
                node.is_disabled(),
                node.description().map(str::to_string),
            ));
        }
        stack.extend(node.children().iter().copied());
    }
    let row = |label: &str| {
        rows.iter()
            .find(|r| r.0 == label)
            .unwrap_or_else(|| panic!("{label}"))
    };
    assert_eq!(rows.len(), 7 + 8, "seven dial entries and eight rows");
    assert!(!row("Select All").1 && row("Select All").2.is_none());
    assert!(row("Select Similar").1);
    assert_eq!(
        row("Select Similar").2.as_deref(),
        Some("Unavailable: Select Similar is not implemented yet")
    );
}

#[test]
fn hovering_a_disabled_row_shows_its_reason_in_the_help_line() {
    let (mut editor, ctx, view) = setup();
    let origin = nowhere(&view);
    editor.status = "Ready".into();
    open_sticky(&mut editor, &ctx, origin);
    let row_five = list_center(&editor, origin, &view, 5);
    at(&mut editor, &ctx, 1.2, vec![Event::PointerMoved(row_five)]);
    assert_eq!(editor.status, "Make Live: Make Live is not implemented yet");
    at(&mut editor, &ctx, 1.3, vec![Event::PointerMoved(origin)]);
    assert_eq!(editor.status, "Ready");
}
