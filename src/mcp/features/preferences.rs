use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{MutationArgs, decode, mutation, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::compatibility::MayaVersion;
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetCompatibilityVersionArgs {
    scene_token: String,
    /// Maya release year to follow, 2011 to 2027.
    #[schemars(range(min = 2011, max = 2027))]
    version: u64,
}

fn parse_set_compatibility_version(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetCompatibilityVersionArgs = decode(arguments)?;
    let version = MayaVersion::try_from(args.version)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetCompatibilityVersion { version },
    ))
}

fn parse_reset_preferences(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    mutation(arguments, Operation::ResetPreferences)
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_compatibility_version",
        description: "Set the Maya release the editor follows (Preferences > Settings > Maya compatibility), a year from 2011 to 2027. Persisted when the preferences file is writable, otherwise applied to this session only; see inspect_scene preferences.persistence. Never changes the scene or undo history. Refused with busy_editing while the Preferences window holds an unsaved value",
        schema: schema::<SetCompatibilityVersionArgs>,
        parse: parse_set_compatibility_version,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "reset_preferences",
        description: "Move an unreadable or changed preferences file aside as a numbered backup and return to defaults. Reports unchanged unless inspect_scene preferences.persistence.reset_available is true. Refused with busy_editing while the Preferences window holds an unsaved value",
        schema: schema::<MutationArgs>,
        parse: parse_reset_preferences,
        idempotent: true,
        read_only: false,
    },
];
