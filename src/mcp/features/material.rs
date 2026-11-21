use crate::{
    editor_operations::{Operation, RemoteRequest},
    mcp::{
        args::{decode, native_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::native_scene::{LambertAttribute, MaterialChoice, SessionMaterialId};
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
    /// One shape. Give this or `shape_ids`.
    shape_id: Option<String>,
    /// Several shapes assigned in one undo step: non-empty, no duplicates, at most 1024. All or
    /// nothing. Give this or `shape_id`.
    shape_ids: Option<Vec<String>>,
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

/// The lambert colour attributes besides `color`, by Maya's long names.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
enum SessionMaterialAttribute {
    #[serde(rename = "transparency")]
    Transparency,
    #[serde(rename = "ambientColor")]
    AmbientColor,
    #[serde(rename = "incandescence")]
    Incandescence,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SetSessionMaterialAttributeArgs {
    scene_token: String,
    material_id: String,
    attribute: SessionMaterialAttribute,
    /// Linear RGB, each channel 0 through 1.
    rgb: [f64; 3],
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
    let shapes = match (&args.shape_id, &args.shape_ids) {
        (Some(id), None) => vec![native_id(id)?],
        (None, Some(ids)) => ids
            .iter()
            .map(|id| native_id(id))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(ErrorData::invalid_params(
                "Give exactly one of shape_id or shape_ids",
                None,
            ));
        }
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::AssignSessionMaterial { shapes, material },
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

fn parse_set_session_material_attribute(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: SetSessionMaterialAttributeArgs = decode(arguments)?;
    let attribute = match args.attribute {
        SessionMaterialAttribute::Transparency => LambertAttribute::Transparency,
        SessionMaterialAttribute::AmbientColor => LambertAttribute::AmbientColor,
        SessionMaterialAttribute::Incandescence => LambertAttribute::Incandescence,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetSessionMaterialAttribute {
            material: material_id(&args.material_id)?,
            attribute,
            rgb: args.rgb,
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
        description: "Create a session lambert material (saved to an authored .ma and reopened from it; refused as .mb with save_unwritable reason session_materials until it is undone; a scene with a retained source, such as an opened Maya file, refuses, while new and legacy JSON scenes accept) with a linear RGB colour (stored in single precision: each channel is rounded to the nearest f32), as its own undo step. Replies with its material_id. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<CreateSessionMaterialArgs>,
        parse: parse_create_session_material,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "assign_session_material",
        description: "Assign a session material to whole authored shapes; every instance of each shape changes. Give shape_id (one shape) or shape_ids (several, assigned all-or-nothing as one undo step; an empty, duplicated, over-1024 or unknown-shape list is refused with invalid_shape_list or unknown_shape and changes nothing). Give material_id (existing) or color (creates a lambert in the same undo step). Replies with the material_id, or unchanged. Saved to an authored .ma with the shape's membership; refused as .mb with save_unwritable reason session_materials until it is undone. A scene with a retained source, such as an opened Maya file, refuses; new and legacy JSON scenes accept. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<AssignSessionMaterialArgs>,
        parse: parse_assign_session_material,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "set_session_material_color",
        description: "Set the linear RGB colour of a session material created in this session (stored in single precision: each channel is rounded to the nearest f32); the initial material 0 is locked. Saved to an authored .ma like the other session material state; unrelated to the saved Phong set_material_diffuse. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<SetSessionMaterialColorArgs>,
        parse: parse_set_session_material_color,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_session_material_attribute",
        description: "Set the linear RGB Transparency (transparency), Ambient Color (ambientColor) or Incandescence (incandescence) of a session material created in this session, default black, each channel 0 through 1 (stored in single precision: rounded to the nearest f32); the initial material 0 is locked. Stored, undoable and saved to an authored .ma when not black, but they do not yet change the viewport (PROVISIONAL). Use set_session_material_color for the colour. Material ids are decimal table indices valid only at the current history position: undo or redo can free and reuse them, so never cache one across edits",
        schema: schema::<SetSessionMaterialAttributeArgs>,
        parse: parse_set_session_material_attribute,
        idempotent: true,
        read_only: false,
    },
];
