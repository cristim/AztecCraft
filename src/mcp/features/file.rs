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
#[serde(deny_unknown_fields)]
struct ExportArgs {
    scene_token: String,
    /// New .obj destination; an existing file is never replaced.
    #[schemars(length(min = 1, max = 4096))]
    path: String,
    /// Export only every selected object and its descendants (Export Selection); default false
    /// exports every displayed polygon instance (Export All).
    #[serde(default)]
    selection_only: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportArgs {
    scene_token: String,
    /// Existing .obj file to add to the scene.
    #[schemars(length(min = 1, max = 4096))]
    path: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FileDecisionInput {
    Cancel,
    Discard,
    Save,
    SaveAs,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ResolveFileArgs {
    scene_token: String,
    pending_action_id: String,
    decision: FileDecisionInput,
    /// Fresh destination, required for and only allowed with decision save_as (1 to 4096 bytes).
    #[schemars(length(min = 1, max = 4096))]
    path: Option<String>,
}

fn check_path(path: &str) -> Result<(), ErrorData> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return Err(ErrorData::invalid_params(
            "path must contain 1 to 4096 UTF-8 bytes and no NUL",
            None,
        ));
    }
    Ok(())
}

fn parse_path(arguments: JsonObject) -> Result<(Uuid, String), ErrorData> {
    let args: OpenArgs = decode(arguments)?;
    check_path(&args.path)?;
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

fn parse_export_obj(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ExportArgs = decode(arguments)?;
    check_path(&args.path)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ExportObj {
            path: args.path.into(),
            selection_only: args.selection_only,
        },
    ))
}

fn parse_import_obj(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ImportArgs = decode(arguments)?;
    check_path(&args.path)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::ImportObj {
            path: args.path.into(),
        },
    ))
}

fn parse_new_scene(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::NewScene)
}

fn parse_resolve_file_action(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ResolveFileArgs = decode(arguments)?;
    let pending_id = Uuid::parse_str(&args.pending_action_id)
        .map_err(|_| ErrorData::invalid_params("pending_action_id must be a UUID", None))?;
    let decision = match (args.decision, args.path) {
        (FileDecisionInput::Cancel, None) => FileDecision::Cancel,
        (FileDecisionInput::Discard, None) => FileDecision::Discard,
        (FileDecisionInput::Save, None) => FileDecision::Save,
        (FileDecisionInput::SaveAs, Some(path)) => {
            if path.is_empty() || path.len() > 4096 || path.contains('\0') {
                return Err(ErrorData::invalid_params(
                    "path must contain 1 to 4096 UTF-8 bytes and no NUL",
                    None,
                ));
            }
            FileDecision::SaveAs { path: path.into() }
        }
        (FileDecisionInput::SaveAs, None) => {
            return Err(ErrorData::invalid_params(
                "decision save_as requires path",
                None,
            ));
        }
        _ => {
            return Err(ErrorData::invalid_params(
                "path is only allowed with decision save_as",
                None,
            ));
        }
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
        description: "Open a Maya binary .mb, supported read-only static curve Maya ASCII .ma, or legacy .aztec scene from a local UTF-8 path (1 to 4096 bytes, no NUL). ASCII scripts stay inert; unsupported commands/effects refuse the whole candidate. Dirty documents require confirmation. Imported ASCII scenes remain read-only; a mesh .ma written unchanged by this editor's ASCII Save opens on macOS and Linux as an editable, saveable authored document, and any other mesh .ma is refused",
        schema: schema::<OpenArgs>,
        parse: parse_open_scene,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "save_scene",
        description: "Save the active supported Maya binary document, or on Unix an authored Maya ASCII document previously saved by this editor, or reconcile an earlier uncertain installation without another write. On Unix, a new Untitled scene returns save_needs_path: use save_scene_as for a fresh .mb or .ma. Imported ASCII scenes remain read-only. Untitled saves are unsupported on Windows. PROVISIONAL: Maya acceptance of new-scene files is unproven; an authored .ma carries each shape's session map1 UVs (the .mb writer does not and refuses them: use Delete UVs or Undo to save a .mb), and the UV layout in the file is unchecked against Maya",
        schema: schema::<MutationArgs>,
        parse: parse_save_scene,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "save_scene_as",
        description: "Save a supported Maya binary scene to a fresh .mb path without overwriting. On Unix only, a new Untitled scene, or an authored document previously saved by this editor as either .ma or .mb, can be written as a Maya 2025-format .mb or as Maya ASCII .ma, chosen by the extension; an opened Maya binary scene refuses .ma with save_ascii_write. Unsupported content is refused (save_unwritable for .mb, with a stable error.reason such as session_materials or session_uvs, save_ascii_write for .ma, for example any session material, created or assigned: undo it to save; session UVs are not a .ma refusal). An uncertain prior save is reconciled instead. PROVISIONAL: Maya acceptance of new-scene files is unproven, an authored .ma carries each shape's session map1 UVs and reopens with them as an authored document (the UV layout in the file is unchecked against Maya), and a scene with UVs is refused as .mb with save_unwritable reason session_uvs",
        schema: schema::<OpenArgs>,
        parse: parse_save_scene_as,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "export_obj",
        description: "Export displayed polygon geometry as a new Wavefront OBJ file (File > Export All, or Export Selection with selection_only), the same operation as the native menu. World-space f64 positions in centimeters, one object per instance, faces only: no normals, UVs, groups or materials. Never overwrites; the scene is not changed. Replies with object, vertex and face counts",
        schema: schema::<ExportArgs>,
        parse: parse_export_obj,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "import_obj",
        description: "Import a Wavefront OBJ file into the current scene (File > Import), the same operation as the native menu. Adds one new root object per o/g group (faces before any group take the file name) as one undo step, selects them, and keeps the scene token. Meshes only: v and f records (triangles, quads, n-gons, negative indices); normals, UVs, materials, smoothing and .mtl are counted in ignored_records and dropped. Refuses free-form geometry, unknown records, overlapping groups, CR-only line endings and faces the viewport cannot display (concave or holed faces with 5 or more corners: triangulate and export again). Refused in a scene opened from .mb or .ma. PROVISIONAL: Maya's help is silent on units, naming, winding and limits. Replies with object, vertex and face counts and the new node_ids",
        schema: schema::<ImportArgs>,
        parse: parse_import_obj,
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
        description: "Cancel, discard, save or save_as (with a fresh path; the only save for a pending Untitled scene) before the exact pending file action using its UUID and current scene token; a failed save leaves the prompt pending; source-changed Open requires Cancel and reopening",
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
