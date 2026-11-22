//! The component-mode menus (Vertex, Edge, Face): the full entry table per mode, Extrude's
//! availability, Delete and Merge staying inert, and the empty Object-mode zone.

use super::pass_tests::*;
use super::*;
use crate::app::features::move_tool::tests::View;
use crate::editor_operations::{Operation, SelectionMode};
use azteccraft::marking_menu::{EntryState, Zone};
use azteccraft::native_scene::FaceTarget;
use eframe::egui::Event;

/// One line per entry: place, label, check mark and state with its reason.
fn table(menu: &MarkingMenu<MeshAction>) -> String {
    let mut lines = Vec::new();
    let describe = |entry: &azteccraft::marking_menu::Entry<MeshAction>| {
        let state = match &entry.state {
            EntryState::Enabled(_) => "enabled".to_string(),
            EntryState::Disabled(reason) => format!("disabled: {reason}"),
            EntryState::Unsupported(reason) => format!("unsupported: {reason}"),
        };
        format!(
            "{}{} | {state}",
            entry.label,
            if entry.checked { " [current]" } else { "" }
        )
    };
    for zone in Zone::ALL {
        match &menu.zones[zone.index()] {
            Some(entry) => lines.push(format!("{}: {}", zone.label(), describe(entry))),
            None => lines.push(format!("{}: -", zone.label())),
        }
    }
    for (row, entry) in menu.list.iter().enumerate() {
        lines.push(format!("row {row}: {}", describe(entry)));
    }
    lines.join("\n")
}

fn pick_face_zero(editor: &mut Editor) {
    let id = editor.single_target().unwrap().0;
    let shape = editor.history.scene().shapes()[0].id();
    let target = FaceTarget {
        transform: id,
        shape,
        path: vec![
            editor.history.scene().node(id).unwrap().transform_id(),
            shape,
        ],
        face_index: 0,
    };
    editor.native_operation(Operation::SelectFace {
        target: Some(target),
    });
    assert!(editor.remembered_face().is_some());
}

const SELECT_REASON: &str = "Selection changes need Object or Hierarchy selection mode";

/// The list and the zones that do not depend on the component mode.
fn fixed_part() -> String {
    format!(
        "South: Delete | disabled: Component delete is not implemented yet (#46)
South-west: Merge | unsupported: Merge is not implemented yet
West: UV | unsupported: UV selection mode is not implemented yet
row 0: Select All | disabled: {SELECT_REASON}
row 1: Select Hierarchy | disabled: {SELECT_REASON}
row 2: Invert Selection | disabled: {SELECT_REASON}
row 3: Deselect All | enabled
row 4: Select Similar | unsupported: Select Similar is not implemented yet
row 5: Make Live | unsupported: Make Live is not implemented yet
row 6: Metadata | unsupported: Metadata is not implemented yet
row 7: Assign Material | unsupported: Assign Material is not implemented yet
row 8: Bevel | unsupported: Bevel is not implemented yet (#46)
row 9: Multi-Cut | unsupported: Multi-Cut is not implemented yet (#46)
row 10: Insert Edge Loop | unsupported: Insert Edge Loop is not implemented yet (#46)
row 11: Connect | unsupported: Connect is not implemented yet (#46)
row 12: Detach | unsupported: Detach is not implemented yet (#46)"
    )
}

fn menu_in(mode: SelectionMode, with_face: bool) -> String {
    let (mut editor, _, _) = setup();
    editor.native_operation(Operation::SetSelectionMode(mode));
    if with_face {
        pick_face_zero(&mut editor);
    }
    table(&editor.marking_menu_for_mode())
}

#[test]
fn the_component_menus_have_exactly_these_entries_in_every_mode() {
    let expected = [
        (
            SelectionMode::Vertex,
            false,
            "North: Edge | enabled
North-east: Face | enabled
East: Object Mode | enabled
South-east: Extrude | disabled: Extrude needs Face selection mode (F11)
North-west: Vertex [current] | enabled",
        ),
        (
            SelectionMode::Edge,
            false,
            "North: Edge [current] | enabled
North-east: Face | enabled
East: Object Mode | enabled
South-east: Extrude | disabled: Extrude needs Face selection mode (F11)
North-west: Vertex | enabled",
        ),
        (
            SelectionMode::Face,
            false,
            "North: Edge | enabled
North-east: Face [current] | enabled
East: Object Mode | enabled
South-east: Extrude | disabled: Select a face first
North-west: Vertex | enabled",
        ),
        (
            SelectionMode::Face,
            true,
            "North: Edge | enabled
North-east: Face [current] | enabled
East: Object Mode | enabled
South-east: Extrude | enabled
North-west: Vertex | enabled",
        ),
    ];
    for (mode, with_face, modes) in expected {
        let actual = menu_in(mode, with_face);
        let (changing, rest): (Vec<_>, Vec<_>) = actual.lines().partition(|line| {
            [
                "North:",
                "North-east:",
                "North-west:",
                "East:",
                "South-east:",
            ]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        });
        assert_eq!(changing.join("\n"), modes, "{mode:?} face {with_face}");
        assert_eq!(rest.join("\n"), fixed_part(), "{mode:?} face {with_face}");
    }
}

/// Clicks the zone `zone` of a sticky menu opened at the usual point.
fn choose_zone(editor: &mut Editor, ctx: &egui::Context, view: &View, zone: Zone) {
    let origin = nowhere(view);
    open_sticky(editor, ctx, origin);
    let at_zone = origin + offset(zone, 60.0);
    at(editor, ctx, 2.0, lmb(at_zone, true));
    at(editor, ctx, 2.1, lmb(at_zone, false));
}

#[test]
fn extrude_opens_its_window_only_in_face_mode_with_a_face_and_otherwise_says_why() {
    for (mode, with_face, opens) in [
        (SelectionMode::Face, true, true),
        (SelectionMode::Face, false, false),
        (SelectionMode::Vertex, false, false),
        (SelectionMode::Edge, false, false),
    ] {
        let (mut editor, ctx, view) = setup();
        editor.native_operation(Operation::SetSelectionMode(mode));
        if with_face {
            pick_face_zero(&mut editor);
        }
        let scene = editor.history.scene().clone();
        choose_zone(&mut editor, &ctx, &view, Zone::SE);
        assert_eq!(
            editor.extrude_window_open(),
            opens,
            "{mode:?} face {with_face}"
        );
        assert_eq!(editor.history.scene(), &scene, "the menu never extrudes");
        if !opens {
            assert!(
                editor.marking_menu.is_active(),
                "a rejected choice stays open"
            );
            assert!(editor.status.starts_with("Extrude: "), "{}", editor.status);
        }
    }
}

#[test]
fn delete_and_merge_are_inert_in_a_component_mode() {
    for zone in [Zone::S, Zone::SW] {
        let (mut editor, ctx, view) = setup();
        editor.native_operation(Operation::SetSelectionMode(SelectionMode::Vertex));
        let scene = editor.history.scene().clone();
        let selection = editor.selection.members().len();
        choose_zone(&mut editor, &ctx, &view, zone);
        assert_eq!(editor.history.scene(), &scene, "{zone:?}");
        assert_eq!(editor.selection.members().len(), selection);
        assert_eq!(editor.selection_mode, SelectionMode::Vertex);
        assert!(editor.marking_menu.is_active());
        assert!(
            editor.status.contains("not implemented yet"),
            "{}",
            editor.status
        );
    }
}

#[test]
fn a_flick_into_the_empty_object_mode_zone_dismisses_and_dispatches_nothing() {
    let (mut editor, ctx, view) = setup();
    let scene = editor.history.scene().clone();
    let selected = editor.selected();
    editor.status = "Ready".into();
    let origin = nowhere(&view);
    at(&mut editor, &ctx, 1.0, rmb(origin, true));
    let out = origin + offset(Zone::SW, 40.0);
    at(&mut editor, &ctx, 1.05, vec![Event::PointerMoved(out)]);
    at(&mut editor, &ctx, 1.1, rmb(out, false));
    assert!(
        !editor.marking_menu.holds_overlay(),
        "a held menu closes on a rejected flick"
    );
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.selected(), selected);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.status, "Ready");
}

#[test]
fn switching_mode_through_the_menu_keeps_the_object_selection() {
    for zone in [Zone::NW, Zone::N, Zone::NE] {
        let (mut editor, ctx, view) = setup();
        let selected = editor.selected();
        assert!(selected.is_some());
        choose_zone(&mut editor, &ctx, &view, zone);
        assert!(editor.selection_mode.is_component(), "{zone:?}");
        assert_eq!(editor.selected(), selected, "{zone:?}");
    }
}
