use super::layout::{DIAL_RADIUS, MeshAction, hit, mesh_menu};
use crate::editor_operations::SelectionMode;
use azteccraft::marking_menu::{Target, Zone};

#[test]
fn the_layout_keeps_the_component_zones_and_marks_the_current_mode() {
    for mode in [
        SelectionMode::Object,
        SelectionMode::Hierarchy,
        SelectionMode::Vertex,
        SelectionMode::Edge,
        SelectionMode::Face,
    ] {
        let menu = mesh_menu(mode, None);
        for (zone, label, target) in [
            (Zone::NW, "Vertex", SelectionMode::Vertex),
            (Zone::N, "Edge", SelectionMode::Edge),
            (Zone::NE, "Face", SelectionMode::Face),
        ] {
            let entry = menu.zones[zone.index()].as_ref().unwrap();
            assert_eq!(entry.label, label, "{mode:?}");
            assert_eq!(entry.checked, mode == target, "{mode:?} {label}");
            assert!(menu.is_enabled(Target::Zone(zone)));
        }
        let uv = menu.zones[Zone::W.index()].as_ref().unwrap();
        assert!(uv.disabled_reason().is_some(), "UV is visibly unsupported");
    }
}

#[test]
fn object_modes_and_component_modes_differ_in_the_rest_of_the_dial() {
    let object = mesh_menu(SelectionMode::Object, None);
    assert!(object.zones[Zone::SW.index()].is_none(), "no inert zone");
    for zone in [Zone::E, Zone::SE, Zone::S] {
        assert!(
            !object.is_enabled(Target::Zone(zone)),
            "{zone:?} is unsupported in Object mode"
        );
    }
    let face = mesh_menu(SelectionMode::Face, None);
    assert_eq!(
        face.resolve(Target::Zone(Zone::E)),
        azteccraft::marking_menu::Resolved::Dispatch(&MeshAction::Mode(SelectionMode::Object))
    );
    assert_eq!(
        face.resolve(Target::Zone(Zone::SE)),
        azteccraft::marking_menu::Resolved::Dispatch(&MeshAction::OpenExtrudeOptions)
    );
    let refused = mesh_menu(SelectionMode::Face, Some("Select a face first".into()));
    assert_eq!(
        refused
            .entry(Target::Zone(Zone::SE))
            .unwrap()
            .disabled_reason(),
        Some("Select a face first")
    );
    for zone in [Zone::S, Zone::SW] {
        assert!(
            face.entry(Target::Zone(zone))
                .unwrap()
                .disabled_reason()
                .is_some()
        );
    }
}

#[test]
fn hit_resolves_zones_inside_the_rim_only() {
    let menu = mesh_menu(SelectionMode::Face, None);
    let o = [100.0, 100.0];
    assert_eq!(hit(&menu, o, [100.0, 40.0]), Some(Target::Zone(Zone::N)));
    assert_eq!(hit(&menu, o, [100.0, 105.0]), None, "dead zone");
    assert_eq!(
        hit(&menu, o, [100.0, 100.0 - DIAL_RADIUS - 1.0]),
        None,
        "past the rim"
    );
    assert_eq!(
        hit(&menu, o, [100.0, 100.0 - DIAL_RADIUS]),
        Some(Target::Zone(Zone::N))
    );
    assert_eq!(hit(&menu, o, [f32::NAN, 40.0]), None);
    assert_eq!(hit(&menu, o, [f32::INFINITY, 40.0]), None);
    let object = mesh_menu(SelectionMode::Object, None);
    assert_eq!(hit(&object, o, [60.0, 140.0]), None, "empty SW zone");
}

#[cfg(feature = "mcp")]
#[test]
fn every_menu_action_names_a_registered_mcp_tool() {
    let tools: Vec<&str> = crate::mcp::tool_names().collect();
    for action in [
        MeshAction::Mode(SelectionMode::Vertex),
        MeshAction::Mode(SelectionMode::Object),
        MeshAction::OpenExtrudeOptions,
        MeshAction::SelectByRule(crate::editor_operations::SelectRule::All),
        MeshAction::DeselectAll,
    ] {
        assert!(
            tools.contains(&action.mcp_tool()),
            "{action:?} maps to {}",
            action.mcp_tool()
        );
    }
}

/// Development.md rows 65 to 67 name these values; changing one means updating the row.
#[test]
fn the_menu_policy_constants_are_pinned() {
    use super::layout::{DIAL_EXTENT, LIST_ROW, LIST_WIDTH};
    assert_eq!(azteccraft::marking_menu::DRAW_DELAY, 0.25);
    assert_eq!(azteccraft::marking_menu::FLICK_DISTANCE, 24.0);
    assert_eq!(azteccraft::marking_menu::IDLE_DISMISS, 30.0);
    assert_eq!(azteccraft::marking_menu::DEAD_ZONE, 12.0);
    assert_eq!(DIAL_RADIUS, 110.0);
    assert_eq!(DIAL_EXTENT, 120.0);
    assert_eq!((LIST_WIDTH, LIST_ROW), (150.0, 22.0));
    assert_eq!(super::paint::LABEL_RADIUS, 74.0);
}
