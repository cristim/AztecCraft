//! The mesh menu layout and its hit test: pure functions of the selection mode.

use crate::app::SelectionModeIntent;
use crate::app::menus::SELECT_RULE_COMPONENT_REASON;
use crate::editor_operations::{SelectRule, SelectionMode};
use azteccraft::marking_menu::{DEAD_ZONE, Entry, Interaction, MarkingMenu, Target, Zone, zone_at};
use eframe::egui::Rect;

/// Points from the press origin to the rim of the dial: a sticky press farther out is "elsewhere".
pub(in crate::app) const DIAL_RADIUS: f32 = 110.0;
/// What choosing an entry does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum MeshAction {
    Mode(SelectionMode),
    OpenExtrudeOptions,
    SelectByRule(SelectRule),
    DeselectAll,
}

impl MeshAction {
    /// The MCP tool that performs the same command, so the menu needs no tool of its own.
    #[cfg(all(test, feature = "mcp"))]
    pub(in crate::app) fn mcp_tool(self) -> &'static str {
        match self {
            Self::Mode(_) => "set_selection_mode",
            Self::OpenExtrudeOptions => "extrude_face",
            Self::SelectByRule(_) => "select_by_rule",
            Self::DeselectAll => "deselect_all",
        }
    }

    pub(in crate::app) fn intent(self) -> Option<SelectionModeIntent> {
        match self {
            Self::Mode(SelectionMode::Object) => Some(SelectionModeIntent::Object),
            Self::Mode(SelectionMode::Hierarchy) => Some(SelectionModeIntent::Hierarchy),
            Self::Mode(SelectionMode::Vertex) => Some(SelectionModeIntent::Vertex),
            Self::Mode(SelectionMode::Edge) => Some(SelectionModeIntent::Edge),
            Self::Mode(SelectionMode::Face) => Some(SelectionModeIntent::Face),
            Self::OpenExtrudeOptions | Self::SelectByRule(_) | Self::DeselectAll => None,
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
    menu.list = select_list(mode);
    menu
}

/// The overflow list: the Select menu's rows that work (the same native callers as the Select
/// menu), then rows that are visibly unsupported. In the component modes the rule rows are
/// disabled with the Select menu's reason; Deselect All stays enabled, since it returns to
/// Object mode. Original order, PROVISIONAL.
fn select_list(mode: SelectionMode) -> Vec<Entry<MeshAction>> {
    let rule = |label: &str, rule: SelectRule| {
        if mode.is_component() {
            Entry::disabled(label, SELECT_RULE_COMPONENT_REASON)
        } else {
            Entry::new(label, MeshAction::SelectByRule(rule))
        }
    };
    let unsupported =
        |label: &str| Entry::unsupported(label, format!("{label} is not implemented yet"));
    vec![
        rule("Select All", SelectRule::All),
        rule("Select Hierarchy", SelectRule::Hierarchy),
        rule("Invert Selection", SelectRule::Inverse),
        Entry::new("Deselect All", MeshAction::DeselectAll),
        unsupported("Select Similar"),
        unsupported("Make Live"),
        unsupported("Metadata"),
        unsupported("Assign Material"),
    ]
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

/// Points from the dial centre to the farthest label edge: the clamp keeps this much inside the
/// viewport (PROVISIONAL, set by the label layout in `paint.rs`).
pub(in crate::app) const DIAL_EXTENT: f32 = 120.0;

/// Where the dial is drawn for a press at `origin`: the origin pulled inside `viewport` so no
/// label is cut off. A viewport smaller than the dial gets it centred; an unusable rect leaves
/// the origin alone.
pub(in crate::app) fn dial_centre(origin: [f32; 2], viewport: Rect) -> [f32; 2] {
    if !viewport.is_finite() {
        return origin;
    }
    let inner = viewport.shrink(DIAL_EXTENT);
    if inner.width() < 0.0 || inner.height() < 0.0 {
        return [viewport.center().x, viewport.center().y];
    }
    [
        origin[0].clamp(inner.min.x, inner.max.x),
        origin[1].clamp(inner.min.y, inner.max.y),
    ]
}

/// The point entries are hit from, and the dial centre. A held menu is drawn and read from the
/// press origin (a flick goes where the hand went, so the dial is not moved under it); only a
/// sticky menu is pulled inside the viewport, since it is clicked where it is drawn.
pub(in crate::app) fn hit_origin(interaction: &Interaction, viewport: Rect) -> Option<[f32; 2]> {
    let origin = interaction.origin()?;
    Some(if interaction.is_sticky() {
        dial_centre(origin, viewport)
    } else {
        origin
    })
}
