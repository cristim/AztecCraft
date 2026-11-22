use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{DisplayLayerEdit, MAX_DISPLAY_LAYER_MEMBERS};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    scene_token: String,
    edit: Edit,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "action", deny_unknown_fields)]
enum Edit {
    Create { members: Vec<String> },
    Assign { layer: u32, members: Vec<String> },
    Visibility { layer: u32, visible: bool },
    Delete { layer: u32 },
}

fn parse_edit(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: EditArgs = decode(arguments)?;
    let nodes = |members: Vec<String>| {
        if members.len() > MAX_DISPLAY_LAYER_MEMBERS {
            return Err(ErrorData::invalid_params(
                "Too many display layer members",
                None,
            ));
        }
        members.iter().map(|id| node_id(id)).collect()
    };
    let edit = match args.edit {
        Edit::Create { members } => DisplayLayerEdit::Create {
            members: nodes(members)?,
        },
        Edit::Assign { layer, members } => DisplayLayerEdit::Assign {
            layer,
            members: nodes(members)?,
        },
        Edit::Visibility { layer, visible } => DisplayLayerEdit::Visibility { layer, visible },
        Edit::Delete { layer } => DisplayLayerEdit::Delete { layer },
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::EditDisplayLayer(edit),
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectArgs {}

fn parse_inspection(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    decode::<InspectArgs>(arguments)?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::InspectDisplayLayers,
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "inspect_display_layers",
        description: "List named session display layers with their visibility, transform ID membership and scene token. Unlisted transforms belong to default layer 0",
        schema: schema::<InspectArgs>,
        parse: parse_inspection,
        idempotent: false,
        read_only: true,
    },
    ToolSpec {
        name: "edit_display_layer",
        description: "Edit authored-scene display layers as one undo step. Create accepts an empty list or explicit transform IDs. Assign transfers transforms from their current layer, with layer 0 the default. Visibility sets a named layer's visible flag. Delete moves its transforms to the default without deleting them. Save and recovery refuse while named layers exist",
        schema: schema::<EditArgs>,
        parse: parse_edit,
        idempotent: false,
        read_only: false,
    },
];
