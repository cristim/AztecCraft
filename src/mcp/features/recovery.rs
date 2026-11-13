use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, schema},
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
struct NoArgs {}

fn parse_autosave_now(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::AutosaveNow)
}

fn parse_get_recovery_status(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let NoArgs {} = decode(arguments)?;
    Ok(RemoteRequest {
        expected_scene_token: None,
        operation: Operation::RecoveryStatus,
    })
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "autosave_now",
        description: "Write one crash-recovery file of the committed scene, when the host started the editor with --recovery-dir (otherwise recovery_disabled). The file lives in the recovery folder, never at the scene's own path: it is a separate, inert, private file that never replaces the scene file, is not an undo step, does not dirty the scene, does not change the scene token and does not commit a human draft (a draft, modal, close or an active manipulation is refused as busy). It encodes the scene as Save would, so a scene Save cannot write (for example with session materials or UVs) returns recovery_unsupported and writes nothing. Retention keeps a PROVISIONAL 10 files per scene name for this session; older ones are removed. Replies with the new sequence number, byte size and prune counts, never a path or name. Opt-in and off by default; there is no automatic timer yet. Maya's autoSave interval, destination and limitBackups options are not modelled (PROVISIONAL)",
        schema: schema::<MutationArgs>,
        parse: parse_autosave_now,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "get_recovery_status",
        description: "Report whether recovery files are enabled for this session (the host passes --recovery-dir; off by default), the retention limit and how many recovery files the folder holds. It needs no scene token and passes no busy guard, like inspect_scene. It never reports the folder path or any file or scene name",
        schema: schema::<NoArgs>,
        parse: parse_get_recovery_status,
        idempotent: false,
        read_only: true,
    },
];
