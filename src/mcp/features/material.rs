use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, native_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{MaterialChoice, SessionMaterialId};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MaterialDiffuseArgs {
    scene_token: String,
    shape_id: String,
    value: f64,
}

fn parse_set_material_diffuse(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: MaterialDiffuseArgs = decode(arguments)?;
    if !args.value.is_finite() || args.value < 0.0 {
        return Err(ErrorData::invalid_params(
            "Diffuse must be finite and nonnegative",
            None,
        ));
    }
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetMaterialDiffuse {
            shape: native_id(&args.shape_id)?,
            value: args.value,
        },
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateSessionMaterialArgs {
    scene_token: String,
    /// Linear RGB, each channel 0 through 1.
    color: [f64; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AssignSessionMaterialArgs {
    scene_token: String,
    shape_id: String,
    /// An existing material id from inspection or a create reply. Give this or `color`.
    material_id: Option<String>,
    /// Creates a new lambert with this colour in the same undo step. Give this or `material_id`.
    color: Option<[f64; 3]>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetSessionMaterialColorArgs {
    scene_token: String,
    material_id: String,
    color: [f64; 3],
}

fn material_id(value: &str) -> Result<SessionMaterialId, ErrorData> {
    let invalid = || {
        ErrorData::invalid_params(
            "material_id must be a canonical unsigned decimal string",
            None,
        )
    };
    let index = value.parse::<usize>().map_err(|_| invalid())?;
    if index.to_string() != value {
        return Err(invalid());
    }
    Ok(SessionMaterialId::from_index(index))
}

fn parse_create_session_material(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: CreateSessionMaterialArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::CreateSessionMaterial { color: args.color },
    ))
}

fn parse_assign_session_material(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: AssignSessionMaterialArgs = decode(arguments)?;
    let material = match (&args.material_id, args.color) {
        (Some(id), None) => MaterialChoice::Existing(material_id(id)?),
        (None, Some(color)) => MaterialChoice::New { color },
        _ => {
            return Err(ErrorData::invalid_params(
                "Give exactly one of material_id or color",
                None,
            ));
        }
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::AssignSessionMaterial {
            shape: native_id(&args.shape_id)?,
            material,
        },
    ))
}

fn parse_set_session_material_color(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetSessionMaterialColorArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetSessionMaterialColor {
            material: material_id(&args.material_id)?,
            color: args.color,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_material_diffuse",
        description: "Edit the checked saved Phong diffuse scalar for an exact shape through shared undo history. Requires complete shared-shader users; affects all those shapes. Values are finite and nonnegative, may exceed one, and do not enable rendered materials",
        schema: schema::<MaterialDiffuseArgs>,
        parse: parse_set_material_diffuse,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "create_session_material",
        description: "Create a session-only lambert material (not saved; authored source-free scenes only, imported scenes refuse) with a linear RGB colour, as its own undo step. Replies with its material_id. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<CreateSessionMaterialArgs>,
        parse: parse_create_session_material,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "assign_session_material",
        description: "Assign a session material to a whole authored shape; every instance of the shape changes. Give material_id (existing) or color (creates a lambert in the same undo step). Replies with the material_id, or unchanged. Not saved; authored source-free scenes only, imported scenes refuse. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<AssignSessionMaterialArgs>,
        parse: parse_assign_session_material,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "set_session_material_color",
        description: "Set the linear RGB colour of a session material created in this session; the initial material 0 is locked. Not saved; unrelated to the saved Phong set_material_diffuse. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<SetSessionMaterialColorArgs>,
        parse: parse_set_session_material_color,
        idempotent: true,
        read_only: false,
    },
];
