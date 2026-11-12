use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::history::UndoQueue;
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

/// `queue` is "finite" (with `size`) or "infinite" (no `size`).
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UndoQueueArgs {
    scene_token: String,
    queue: QueueKind,
    /// Finite queue size, 1 to 1000; required for "finite", refused for "infinite".
    #[schemars(range(min = 1, max = 1000))]
    size: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum QueueKind {
    Finite,
    Infinite,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UndoEnabledArgs {
    scene_token: String,
    enabled: bool,
    /// Clear both queues (`undoInfo -state`) or keep them (`-stateWithoutFlush`). Default true when turning undo off and false when turning it on (PROVISIONAL: the docs state no flush rule).
    flush: Option<bool>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OpenChunkArgs {
    scene_token: String,
    /// Name of the single undo step, at most 64 characters; empty or omitted is "Chunk".
    #[schemars(length(max = 64))]
    name: Option<String>,
}

fn parse_set_undo_queue(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: UndoQueueArgs = decode(arguments)?;
    let queue = match (args.queue, args.size) {
        (QueueKind::Finite, Some(size)) => UndoQueue::finite(size)
            .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?,
        (QueueKind::Finite, None) => {
            return Err(ErrorData::invalid_params("a finite queue needs size", None));
        }
        (QueueKind::Infinite, None) => UndoQueue::Infinite,
        (QueueKind::Infinite, Some(_)) => {
            return Err(ErrorData::invalid_params(
                "an infinite queue takes no size",
                None,
            ));
        }
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetUndoQueue(queue),
    ))
}

fn parse_set_undo_enabled(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: UndoEnabledArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetUndoEnabled {
            enabled: args.enabled,
            flush: args.flush.unwrap_or(!args.enabled),
        },
    ))
}

fn parse_open_undo_chunk(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: OpenChunkArgs = decode(arguments)?;
    let name = args.name.unwrap_or_default();
    if name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ErrorData::invalid_params(
            "name must be at most 64 characters without control characters",
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::OpenUndoChunk { name },
    ))
}

fn parse_close_undo_chunk(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::CloseUndoChunk)
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "delete_node",
        description: "Delete an object and all its descendants through editor undo history; the form of Maya's delete command with no flags, applied to one node (PROVISIONAL: the documentation does not state the default of -hierarchy, so the subtree semantics are pinned by tests). A descendant transform that also hangs under a node outside the subtree is refused; a shape shared with such a node stays. Refused in Face, Vertex or Edge selection mode",
        schema: schema::<NodeArgs>,
        parse: parse_delete_node,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "delete_selected",
        description: "Delete every selected object (Edit > Delete) as one undo step; the form of Maya's delete command with no flags, each node removed with its whole subtree (PROVISIONAL, see delete_node; that the Edit > Delete menu item runs exactly this form, not a wrapper script, is also PROVISIONAL). All or nothing: if any selected object or its subtree cannot be deleted (a shared descendant, or an opened file whose records cannot be dropped) the whole delete is refused with that object's error and nothing changes. A selected descendant of another selected object is deleted once, with it. The selection is empty afterwards and Undo restores the objects but not the selection. Refused in Face, Vertex or Edge selection mode and with nothing selected",
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
    ToolSpec {
        name: "set_undo_queue",
        description: "Set the Undo queue (Preferences > Undo): finite with a size from 1 to 1000 (default finite 50) or infinite. A smaller size drops the oldest steps at once. Session-only: it survives New and Open but not a restart. Refused while an undo chunk or manipulation is open. Read it from inspect_scene undo_state",
        schema: schema::<UndoQueueArgs>,
        parse: parse_set_undo_queue,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_undo_enabled",
        description: "Turn Undo on or off (undoInfo -state, or -stateWithoutFlush with flush false). While off, edits apply and record nothing, Undo and Redo do nothing, and an edit flushes both queues so no stale step can erase it. Refused while an undo chunk or manipulation is open. Read it from inspect_scene undo_state",
        schema: schema::<UndoEnabledArgs>,
        parse: parse_set_undo_enabled,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "open_undo_chunk",
        description: "Start an undo chunk: every edit until close_undo_chunk joins one undo step named name (default Chunk). Chunks do not nest. While one is open the native Undo and Redo close it first, so an abandoned chunk cannot block them; an open chunk is reported by inspect_scene undo_state.open_chunk. Close every chunk you open: until then edits from any source, native ones included, join it",
        schema: schema::<OpenChunkArgs>,
        parse: parse_open_undo_chunk,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "close_undo_chunk",
        description: "Close the open undo chunk and record its edits as one undo step; a chunk with no net scene change records no step",
        schema: schema::<MutationArgs>,
        parse: parse_close_undo_chunk,
        idempotent: false,
        read_only: false,
    },
];
