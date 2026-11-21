use crate::{
    editor_operations::{AttributeEditorTab, MenuSet, Operation, RemoteRequest, SidebarPanel},
    mcp::{
        args::{decode, native_id, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PanelInput {
    ChannelBox,
    AttributeEditor,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SidebarPanelArgs {
    scene_token: String,
    panel: PanelInput,
    visible: bool,
}

fn parse_set_sidebar_panel_visibility(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SidebarPanelArgs = decode(arguments)?;
    let panel = match args.panel {
        PanelInput::ChannelBox => SidebarPanel::ChannelBox,
        PanelInput::AttributeEditor => SidebarPanel::AttributeEditor,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetSidebarPanelVisible {
            panel,
            visible: args.visible,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum MenuSetInput {
    Modeling,
    Rigging,
    Rendering,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MenuSetArgs {
    scene_token: String,
    set: MenuSetInput,
}

fn parse_set_menu_set(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: MenuSetArgs = decode(arguments)?;
    let set = match args.set {
        MenuSetInput::Modeling => MenuSet::Modeling,
        MenuSetInput::Rigging => MenuSet::Rigging,
        MenuSetInput::Rendering => MenuSet::Rendering,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetMenuSet { set },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum TabKind {
    Transform,
    Shape,
    Material,
    /// The live camera's image plane (#459): takes no `id`; refused when there is no plane.
    ImagePlane,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TabInput {
    kind: TabKind,
    /// Decimal id: the selected node for `transform`, a direct shape for `shape` and `material`.
    /// Required for those three, and not allowed for `image_plane`.
    id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AttributeEditorTabArgs {
    scene_token: String,
    tab: TabInput,
}

fn parse_select_attribute_editor_tab(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: AttributeEditorTabArgs = decode(arguments)?;
    let id = |id: &Option<String>| {
        id.clone()
            .ok_or_else(|| ErrorData::invalid_params("tab.id is required for this tab kind", None))
    };
    let tab = match args.tab.kind {
        TabKind::Transform => AttributeEditorTab::Transform(node_id(&id(&args.tab.id)?)?),
        TabKind::Shape => AttributeEditorTab::Shape(native_id(&id(&args.tab.id)?)?),
        TabKind::Material => AttributeEditorTab::Material(native_id(&id(&args.tab.id)?)?),
        TabKind::ImagePlane => {
            if args.tab.id.is_some() {
                return Err(ErrorData::invalid_params(
                    "tab.id is not allowed for the image_plane tab",
                    None,
                ));
            }
            AttributeEditorTab::ImagePlane
        }
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectAttributeEditorTab { tab },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SnapModeArgs {
    scene_token: String,
    /// Snap to grids on or off.
    grid: bool,
}

fn parse_set_snap_mode(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SnapModeArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetSnapToGrid { enabled: args.grid },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_snap_mode",
        description: "Set Snap to grids explicitly (Status Line button, `snapMode -grid`); off by default. While on, a Move begun with begin_manipulation lands the lead object's pivot on the nearest line of the drawn grid along its axis (the drawn grid has one line per centimetre, so 1 cm in every scene linear unit for now; following the scene unit waits until the viewport draws its grid in scene units, #454), absolute rather than relative; the rest of the selection moves by the same distance. The Move axis must lie along a world X, Y or Z axis (World orientation, or Object or Parent aligned to the world): otherwise begin_manipulation returns invalid_input and nothing starts. A pivot within a few ulps of a line stays put (PROVISIONAL, #5: the tolerance and snapping to subdivision lines have no Maya source). Only grid snapping exists: curve, point, surface and relative step snapping are not implemented. Session-only and idempotent; does not edit the scene. Refused with busy_manipulation during any manipulation and busy_editing during a modal or close; inspect_scene snap_to_grid reports the state and snap_spacing the spacing in centimetres and in the scene linear unit",
        schema: schema::<SnapModeArgs>,
        parse: parse_set_snap_mode,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_menu_set",
        description: "Switch the menu set the native menu bar shows, like the Status Line selector or F2, F3 and F6. Only modeling, rigging and rendering exist; Animation and FX are not implemented. Session-only and idempotent: it does not edit the scene, mark it dirty or change the scene token, New and Open keep it, and it cannot be undone. Refused while a file dialog is open. `inspect_scene` reports it as `menu_set`",
        schema: schema::<MenuSetArgs>,
        parse: parse_set_menu_set,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_sidebar_panel_visibility",
        description: "Set one sidebar panel's visibility explicitly. Session-only and idempotent; does not edit the scene. The dock shows one panel: showing attribute_editor hides channel_box and the reverse",
        schema: schema::<SidebarPanelArgs>,
        parse: parse_set_sidebar_panel_visibility,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "select_attribute_editor_tab",
        description: "Select the Attribute Editor tab for the selected node: transform, a direct shape, or the material of a direct shape; only the first 16 direct shapes have tabs. The image_plane tab (kind image_plane, no id) belongs to the camera's image plane and needs no selection, only a plane (set_image_plane); it is refused without one. Session-only and idempotent; does not edit the scene. Refused unless attribute_editor is the active sidebar panel, and refused when the tab does not resolve for the current selection or names a shape beyond the 16th",
        schema: schema::<AttributeEditorTabArgs>,
        parse: parse_select_attribute_editor_tab,
        idempotent: true,
        read_only: false,
    },
];
