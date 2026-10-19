use crate::{
    editor_operations::{
        ActiveTool, ManipulationSpace, ManipulationValue, MoveDistance, Operation, RemoteRequest,
    },
    mcp::{
        args::{decode, manipulation_id, native_id, node_id, schema, scoped, token},
        registry::ToolSpec,
    },
};
use azteccraft::{
    history::ManipulationFinish,
    native_scene::{RotateAxis, ScaleHandle, TransformTarget, WorldAxis},
};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ActiveToolArgs {
    scene_token: String,
    tool: ActiveTool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum AxisInput {
    X,
    Y,
    Z,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum HandleInput {
    X,
    Y,
    Z,
    Uniform,
}

/// Rotate and Scale work in the node's own axes (`object`) or the world axes (`world`).
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SpaceInput {
    Object,
    World,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SpaceToolInput {
    Rotate,
    Scale,
}

/// Sets the Axis Orientation the Rotate or Scale tool settings use for the next manipulation.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToolSpaceArgs {
    scene_token: String,
    tool: SpaceToolInput,
    space: SpaceInput,
}

/// `kind` selects the manipulation; each kind takes its own handle fields and nothing else. Move
/// works along World axes only and takes no `space`.
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum BeginManipulationArgs {
    Move {
        scene_token: String,
        node_id: String,
        #[schemars(length(min = 1, max = 256))]
        instance_path: Vec<String>,
        axis: AxisInput,
    },
    Rotate {
        scene_token: String,
        node_id: String,
        #[schemars(length(min = 1, max = 256))]
        instance_path: Vec<String>,
        axis: AxisInput,
        space: SpaceInput,
    },
    Scale {
        scene_token: String,
        node_id: String,
        #[schemars(length(min = 1, max = 256))]
        instance_path: Vec<String>,
        handle: HandleInput,
        space: SpaceInput,
    },
}

/// Exactly one of `distance`, `angle` or `factor`, matching the kind of the manipulation.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateManipulationArgs {
    scene_token: String,
    manipulation_id: String,
    distance: Option<f64>,
    angle: Option<f64>,
    factor: Option<f64>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum FinishInput {
    Commit,
    Cancel,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FinishManipulationArgs {
    scene_token: String,
    manipulation_id: String,
    finish: FinishInput,
}

fn parse_set_active_tool(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ActiveToolArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetActiveTool(args.tool),
    ))
}

fn parse_set_tool_space(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: ToolSpaceArgs = decode(arguments)?;
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::SetToolSpace {
            tool: match args.tool {
                SpaceToolInput::Rotate => ActiveTool::Rotate,
                SpaceToolInput::Scale => ActiveTool::Scale,
            },
            space: space(args.space),
        },
    ))
}

fn target(node: &str, instance_path: Vec<String>) -> Result<TransformTarget, ErrorData> {
    if !(1..=256).contains(&instance_path.len()) {
        return Err(ErrorData::invalid_params(
            "instance_path must contain 1 to 256 IDs",
            None,
        ));
    }
    Ok(TransformTarget {
        node: node_id(node)?,
        path: instance_path
            .into_iter()
            .map(|value| native_id(&value))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn world_axis(axis: AxisInput) -> WorldAxis {
    match axis {
        AxisInput::X => WorldAxis::X,
        AxisInput::Y => WorldAxis::Y,
        AxisInput::Z => WorldAxis::Z,
    }
}

fn space(space: SpaceInput) -> ManipulationSpace {
    match space {
        SpaceInput::Object => ManipulationSpace::Object,
        SpaceInput::World => ManipulationSpace::World,
    }
}

fn parse_begin_manipulation(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let (scene_token, operation) = match decode(arguments)? {
        BeginManipulationArgs::Move {
            scene_token,
            node_id,
            instance_path,
            axis,
        } => (
            scene_token,
            Operation::BeginMove {
                target: target(&node_id, instance_path)?,
                axis: world_axis(axis),
            },
        ),
        BeginManipulationArgs::Rotate {
            scene_token,
            node_id,
            instance_path,
            axis,
            space: input,
        } => (
            scene_token,
            Operation::BeginRotate {
                target: target(&node_id, instance_path)?,
                axis: match axis {
                    AxisInput::X => RotateAxis::X,
                    AxisInput::Y => RotateAxis::Y,
                    AxisInput::Z => RotateAxis::Z,
                },
                space: space(input),
            },
        ),
        BeginManipulationArgs::Scale {
            scene_token,
            node_id,
            instance_path,
            handle,
            space: input,
        } => (
            scene_token,
            Operation::BeginScale {
                target: target(&node_id, instance_path)?,
                handle: match handle {
                    HandleInput::X => ScaleHandle::X,
                    HandleInput::Y => ScaleHandle::Y,
                    HandleInput::Z => ScaleHandle::Z,
                    HandleInput::Uniform => ScaleHandle::Uniform,
                },
                space: space(input),
            },
        ),
    };
    Ok(scoped(token(&scene_token)?, operation))
}

fn parse_update_manipulation(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: UpdateManipulationArgs = decode(arguments)?;
    let id = manipulation_id(&args.manipulation_id)?;
    let finite = |name: &str, value: f64| {
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ErrorData::invalid_params(
                format!("{name} must be finite"),
                None,
            ))
        }
    };
    let operation = match (args.distance, args.angle, args.factor) {
        (Some(distance), None, None) => Operation::UpdateMove {
            id,
            distance: MoveDistance::Declared(finite("distance", distance)?),
        },
        (None, Some(angle), None) => Operation::UpdateManipulation {
            id,
            value: ManipulationValue::Angle(finite("angle", angle)?),
        },
        (None, None, Some(factor)) => {
            if finite("factor", factor)? <= 0.0 {
                return Err(ErrorData::invalid_params(
                    "factor must be greater than zero",
                    None,
                ));
            }
            Operation::UpdateManipulation {
                id,
                value: ManipulationValue::Factor(factor),
            }
        }
        _ => {
            return Err(ErrorData::invalid_params(
                "give exactly one of distance, angle or factor",
                None,
            ));
        }
    };
    Ok(scoped(token(&args.scene_token)?, operation))
}

fn update_schema() -> JsonObject {
    let common = |name: &str, parameter: Value| {
        json!({
            "type": "object",
            "properties": {
                "scene_token": {"type": "string"},
                "manipulation_id": {"type": "string"},
                name: parameter,
            },
            "required": ["scene_token", "manipulation_id", name],
            "additionalProperties": false,
        })
    };
    let Value::Object(schema) = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "UpdateManipulationArgs",
        "type": "object",
        "description": "Exactly one of distance (Move), angle (Rotate) or factor (Scale), matching the kind of the manipulation; any other is a typed manipulation_kind_mismatch error",
        "oneOf": [
            common("distance", json!({
                "type": "number",
                "description": "Absolute Move distance from begin along the World axis, in the scene linear unit (inspection units.distance)",
            })),
            common("angle", json!({
                "type": "number",
                "description": "Absolute Rotate angle from begin about the axis, in the scene angular unit (inspection units.angle)",
            })),
            common("factor", json!({
                "type": "number",
                "exclusiveMinimum": 0,
                "description": "Absolute Scale factor from begin, unitless and greater than zero",
            })),
        ],
    }) else {
        unreachable!("the update schema is an object");
    };
    schema
}

fn parse_finish_manipulation(arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    let args: FinishManipulationArgs = decode(arguments)?;
    let finish = match args.finish {
        FinishInput::Commit => ManipulationFinish::Commit,
        FinishInput::Cancel => ManipulationFinish::Cancel,
    };
    Ok(scoped(
        token(&args.scene_token)?,
        Operation::FinishMove {
            id: manipulation_id(&args.manipulation_id)?,
            finish,
        },
    ))
}

pub(in crate::mcp) const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "set_active_tool",
        description: "Select the native Select, Move, Rotate or Scale tool; unsupported tools are not exposed",
        schema: schema::<ActiveToolArgs>,
        parse: parse_set_active_tool,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "set_tool_space",
        description: "Set the Axis Orientation (object or world) of the Rotate or Scale tool settings, the same setting as the native Tool Settings selector; read it from inspect_scene manipulation.rotate_space and scale_space. Does not change the active tool",
        schema: schema::<ToolSpaceArgs>,
        parse: parse_set_tool_space,
        idempotent: true,
        read_only: false,
    },
    ToolSpec {
        name: "begin_manipulation",
        description: "Begin one remote-owned Move (World axis), Rotate or Scale (Object or World space) of the exact selected transform instance. Numeric manipulation may begin while any tool is active; this does not change tools",
        schema: schema::<BeginManipulationArgs>,
        parse: parse_begin_manipulation,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "update_manipulation",
        description: "Preview an absolute value from begin: distance in the scene linear unit for Move, angle in the scene angular unit for Rotate, a unitless factor greater than zero for Scale. Units are echoed in inspection.manipulation.units. Matching scene token and remote-owned manipulation ID are required; a parameter of the wrong kind is a typed error",
        schema: update_schema,
        parse: parse_update_manipulation,
        idempotent: false,
        read_only: false,
    },
    ToolSpec {
        name: "finish_manipulation",
        description: "Commit one undoable Move, Rotate or Scale edit or cancel to the exact begin authoring state. Only the matching remote-owned manipulation ID may finish",
        schema: schema::<FinishManipulationArgs>,
        parse: parse_finish_manipulation,
        idempotent: false,
        read_only: false,
    },
];

#[cfg(test)]
mod schema_drift_tests {
    use super::*;
    use std::collections::BTreeSet;

    fn names(properties: &Value) -> BTreeSet<String> {
        properties.as_object().unwrap().keys().cloned().collect()
    }

    // The published schema is a hand-written oneOf (exactly one value parameter), which the type
    // cannot express; this keeps its property names equal to the decoded struct's fields.
    #[test]
    fn update_schema_properties_match_the_decoded_fields() {
        let derived = schema::<UpdateManipulationArgs>();
        let derived = names(&derived["properties"]);
        let published = update_schema();
        let branches = published["oneOf"].as_array().unwrap();
        let union: BTreeSet<String> = branches
            .iter()
            .flat_map(|branch| names(&branch["properties"]))
            .collect();
        assert_eq!(union, derived);
        for branch in branches {
            let own = names(&branch["properties"]);
            assert_eq!(own.len(), 3, "each branch has the two ids and one value");
            let required: BTreeSet<String> = branch["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|name| name.as_str().unwrap().to_owned())
                .collect();
            assert_eq!(required, own);
        }
    }
}
