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
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TabInput {
    kind: TabKind,
    /// Decimal id: the selected node for `transform`, a direct shape for `shape` and `material`.
    id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AttributeEditorTabArgs {
    scene_token: String,
    tab: TabInput,
}

fn parse_select_attribute_editor_tab(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: AttributeEditorTabArgs = decode(arguments)?;
    let tab = match args.tab.kind {
        TabKind::Transform => AttributeEditorTab::Transform(node_id(&args.tab.id)?),
        TabKind::Shape => AttributeEditorTab::Shape(native_id(&args.tab.id)?),
        TabKind::Material => AttributeEditorTab::Material(native_id(&args.tab.id)?),
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SelectAttributeEditorTab { tab },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_menu_set",
        description: "Switch the menu set the native menu bar shows, like the Status Line selector or F2 and F3. Only modeling and rigging exist; Animation, FX and Rendering are not implemented. Session-only and idempotent: it does not edit the scene, mark it dirty or change the scene token, New and Open keep it, and it cannot be undone. Refused while a file dialog is open. `inspect_scene` reports it as `menu_set`",
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
        description: "Select the Attribute Editor tab for the selected node: transform, a direct shape, or the material of a direct shape; only the first 16 direct shapes have tabs. Session-only and idempotent; does not edit the scene. Refused unless attribute_editor is the active sidebar panel, and refused when the tab does not resolve for the current selection or names a shape beyond the 16th",
        schema: schema::<AttributeEditorTabArgs>,
        parse: parse_select_attribute_editor_tab,
        idempotent: true,
        read_only: false,
    },
];
