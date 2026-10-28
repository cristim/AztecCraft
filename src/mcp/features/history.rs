use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    scene_token: String,
    node_id: String,
}

fn parse_delete_node(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: NodeArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::Delete(node_id(&args.node_id)?),
    ))
}

fn parse_delete_selected(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::DeleteSelection)
}

fn parse_undo(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::Undo)
}

fn parse_redo(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::Redo)
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "delete_node",
        description: "Delete an object through editor undo history; refused in Face, Vertex or Edge selection mode",
        schema: schema::<NodeArgs>,
        parse: parse_delete_node,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "delete_selected",
        description: "Delete every selected object (Edit > Delete) as one undo step. All or nothing: if any selected object cannot be deleted (it has children or a parent, or the scene was opened from a file) the whole delete is refused with that object's error and nothing changes. The selection is empty afterwards and Undo restores the objects but not the selection. Refused in Face, Vertex or Edge selection mode and with nothing selected",
        schema: schema::<MutationArgs>,
        parse: parse_delete_selected,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "undo",
        description: "Undo the last editor authoring operation",
        schema: schema::<MutationArgs>,
        parse: parse_undo,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "redo",
        description: "Redo the last undone authoring operation",
        schema: schema::<MutationArgs>,
        parse: parse_redo,
        idempotent: false,
        read_only: false,
    },
];
