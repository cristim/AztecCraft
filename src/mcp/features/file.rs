use crate::{
    editor_operations::{FileDecision, Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, schema, scoped, token},
        registry::ToolSpec,
    },
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OpenArgs {
    scene_token: String,
    #[schemars(length(min = 1, max = 4096))]
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FileDecisionInput {
    Cancel,
    Discard,
    Save,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResolveFileArgs {
    scene_token: String,
    pending_action_id: String,
    decision: FileDecisionInput,
}

fn parse_path(arguments: JsonObject) -> Result<(Uuid, String), ErrorData> {
    let args: OpenArgs = decode(arguments)?;
    if args.path.is_empty() || args.path.len() > 4096 || args.path.contains('\0') {
        return Err(ErrorData::invalid_params(
            "path must contain 1 to 4096 UTF-8 bytes and no NUL",
            None,
        ));
    }
    Ok((token(&args.scene_token)?, args.path))
}

fn parse_open_scene(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let (token, path) = parse_path(arguments)?;
    Ok(scoped(token, Operation::OpenScene { path: path.into() }))
}

fn parse_save_scene(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::SaveScene)
}

fn parse_save_scene_as(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let (token, path) = parse_path(arguments)?;
    Ok(scoped(token, Operation::SaveSceneAs { path: path.into() }))
}

fn parse_new_scene(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::NewScene)
}

fn parse_resolve_file_action(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ResolveFileArgs = decode(arguments)?;
    let pending_id = Uuid::parse_str(&args.pending_action_id)
        .map_err(|_| ErrorData::invalid_params("pending_action_id must be a UUID", None))?;
    let decision = match args.decision {
        FileDecisionInput::Cancel => FileDecision::Cancel,
        FileDecisionInput::Discard => FileDecision::Discard,
        FileDecisionInput::Save => FileDecision::Save,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ResolveFileAction {
            pending_id,
            decision,
        },
    ))
}

fn parse_request_close(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::RequestClose)
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "open_scene",
        description: "Open a Maya binary .mb, supported read-only static cubic Maya ASCII .ma, or legacy .aztec scene from a local UTF-8 path (1 to 4096 bytes, no NUL). ASCII scripts stay inert; unsupported commands/effects refuse the whole candidate. Dirty documents require confirmation. Imported ASCII scenes remain read-only; mesh files produced by new-scene ASCII Save As cannot currently be reopened by this editor",
        schema: schema::<OpenArgs>,
        parse: parse_open_scene,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "save_scene",
        description: "Save the active supported Maya binary document, or on Unix an authored Maya ASCII document previously saved by this editor. Imported ASCII scenes remain read-only and assigned session materials refuse before writing. An uncertain prior installation is reconciled without another write",
        schema: schema::<MutationArgs>,
        parse: parse_save_scene,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "save_scene_as",
        description: "Save a supported Maya binary scene to a fresh .mb path, or on Unix an authored source-free scene (Untitled or previously saved by this editor) to a fresh .ma path, without overwriting. Assigned session materials and unsupported ASCII content refuse before writing. An uncertain prior save is reconciled instead. PROVISIONAL: Maya acceptance is unverified; saved mesh ASCII files cannot currently be reopened by this editor",
        schema: schema::<OpenArgs>,
        parse: parse_save_scene_as,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "new_scene",
        description: "Request a new empty document; dirty documents require explicit confirmation using the returned pending_action_id",
        schema: schema::<MutationArgs>,
        parse: parse_new_scene,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "resolve_file_action",
        description: "Cancel, discard or save before the exact pending file action using its UUID and current scene token; source-changed Open requires Cancel and reopening",
        schema: schema::<ResolveFileArgs>,
        parse: parse_resolve_file_action,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "request_close",
        description: "Request native window closure; dirty documents require explicit confirmation, and approved remote closure waits for its successful reply to be flushed",
        schema: schema::<MutationArgs>,
        parse: parse_request_close,
        idempotent: false,
        read_only: false,
    },
];
