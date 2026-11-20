//! The mesh menu layout and its hit test: pure functions of the selection mode.

#![allow(dead_code)]

use crate::app::SelectionModeIntent;
use crate::editor_operations::SelectionMode;
use azteccraft::marking_menu::{DEAD_ZONE, Entry, MarkingMenu, Target, Zone, zone_at};

/// Points from the press origin to the rim of the dial: a sticky press farther out is "elsewhere".
pub(in crate::app) const DIAL_RADIUS: f32 = 110.0;
/// What choosing an entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum MeshAction {
    Mode(SelectionMode),
    OpenExtrudeOptions,
}

impl MeshAction {
    /// The MCP tool that performs the same command, so the menu needs no tool of its own.
    #[cfg(all(test, feature = "mcp"))]
    pub(in crate::app) fn mcp_tool(self) -> &'static str {
        match self {
            Self::Mode(_) => "set_selection_mode",
            Self::OpenExtrudeOptions => "extrude_face",
        }
    }

    pub(in crate::app) fn intent(self) -> Option<SelectionModeIntent> {
        match self {
            Self::Mode(SelectionMode::Object) => Some(SelectionModeIntent::Object),
            Self::Mode(SelectionMode::Hierarchy) => Some(SelectionModeIntent::Hierarchy),
            Self::Mode(SelectionMode::Vertex) => Some(SelectionModeIntent::Vertex),
            Self::Mode(SelectionMode::Edge) => Some(SelectionModeIntent::Edge),
            Self::Mode(SelectionMode::Face) => Some(SelectionModeIntent::Face),
            Self::OpenExtrudeOptions => None,
        }
    }
}

/// The mesh menu for `mode`. `extrude_refusal` is `Editor::extrude_refusal()`. Vertex, Edge and
/// Face keep their zones in every mode; the rest of the dial differs between Object and
/// component modes (original layout, PROVISIONAL).
pub(in crate::app) fn mesh_menu(
    mode: SelectionMode,
    extrude_refusal: Option<String>,
) -> MarkingMenu<MeshAction> {
    let mut menu = MarkingMenu::default();
    let mode_entry = |label: &str, target: SelectionMode| {
        Entry::new(label, MeshAction::Mode(target)).checked(mode == target)
    };
    menu.zones[Zone::NW.index()] = Some(mode_entry("Vertex", SelectionMode::Vertex));
    menu.zones[Zone::N.index()] = Some(mode_entry("Edge", SelectionMode::Edge));
    menu.zones[Zone::NE.index()] = Some(mode_entry("Face", SelectionMode::Face));
    menu.zones[Zone::W.index()] = Some(Entry::unsupported(
        "UV",
        "UV selection mode is not implemented yet",
    ));
    if mode.is_component() {
        menu.zones[Zone::E.index()] = Some(mode_entry("Object Mode", SelectionMode::Object));
        menu.zones[Zone::SE.index()] = Some(match extrude_refusal {
            None => Entry::new("Extrude", MeshAction::OpenExtrudeOptions),
            Some(reason) => Entry::disabled("Extrude", reason),
        });
        menu.zones[Zone::S.index()] = Some(Entry::disabled(
            "Delete",
            "Component delete is not implemented yet (#46)",
        ));
        menu.zones[Zone::SW.index()] =
            Some(Entry::unsupported("Merge", "Merge is not implemented yet"));
    } else {
        menu.zones[Zone::E.index()] = Some(Entry::unsupported(
            "Multi",
            "Multi selection is not implemented: component modes are exclusive",
        ));
        menu.zones[Zone::SE.index()] =
            Some(Entry::unsupported("Paint", "Paint is not implemented yet"));
        menu.zones[Zone::S.index()] = Some(Entry::unsupported(
            "Inputs",
            "Inputs is not implemented yet",
        ));
    }
    menu
}

/// The entry under `pos` for a sticky menu opened at `origin`: a zone with an entry, inside the
/// dial. `None` outside the rim, in the dead zone and over an empty zone.
pub(in crate::app) fn hit(
    menu: &MarkingMenu<MeshAction>,
    origin: [f32; 2],
    pos: [f32; 2],
) -> Option<Target> {
    let delta = [pos[0] - origin[0], pos[1] - origin[1]];
    if !delta[0].is_finite() || !delta[1].is_finite() {
        return None;
    }
    if f32::hypot(delta[0], delta[1]) > DIAL_RADIUS {
        return None;
    }
    let target = Target::Zone(zone_at(delta, DEAD_ZONE)?);
    menu.entry(target).map(|_| target)
}
