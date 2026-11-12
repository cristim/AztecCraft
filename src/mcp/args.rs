use crate::editor_operations::{Operation, RemoteRequest};
use azteccraft::{history::ManipulationId, native_scene::NativeId, scene::NodeId};
use rmcp::{
    model::{ErrorData, JsonObject},
    schemars::{self, JsonSchema},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(in crate::mcp) struct MutationArgs {
    pub(in crate::mcp) scene_token: String,
}

pub(in crate::mcp) fn decode<T: DeserializeOwned>(arguments: JsonObject) -> Result<T, ErrorData> {
    serde_json::from_value(Value::Object(arguments))
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

/// A normalised coordinate pair, range-checked as sent (f64) and only then narrowed to f32, so a
/// value just outside [-1, 1] is refused instead of rounding onto the boundary.
pub(in crate::mcp) fn unit_point(name: &str, value: [f64; 2]) -> Result<glam::Vec2, ErrorData> {
    if value.iter().any(|v| !(-1.0..=1.0).contains(v)) {
        return Err(ErrorData::invalid_params(
            format!("{name} must be finite and within -1 to 1 on both axes"),
            None,
        ));
    }
    Ok(glam::Vec2::new(value[0] as f32, value[1] as f32))
}

pub(in crate::mcp) fn token(value: &str) -> Result<Uuid, ErrorData> {
    Uuid::parse_str(value)
        .map_err(|_| ErrorData::invalid_params("scene_token must be a UUID", None))
}

pub(in crate::mcp) fn node_id(value: &str) -> Result<NodeId, ErrorData> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ErrorData::invalid_params(
            "node_id must be an unsigned decimal string",
            None,
        ));
    }
    NodeId::from_decimal(value)
        .map_err(|_| ErrorData::invalid_params("node_id must fit an unsigned 64-bit integer", None))
}

pub(in crate::mcp) fn native_id(value: &str) -> Result<NativeId, ErrorData> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ErrorData::invalid_params(
            "Native IDs must be unsigned decimal strings",
            None,
        ));
    }
    NativeId::from_decimal(value).map_err(|_| {
        ErrorData::invalid_params("Native IDs must fit unsigned 64-bit integers", None)
    })
}

pub(in crate::mcp) fn manipulation_id(value: &str) -> Result<ManipulationId, ErrorData> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ErrorData::invalid_params(
            "manipulation_id must be an unsigned decimal string",
            None,
        ));
    }
    ManipulationId::from_decimal(value).map_err(|_| {
        ErrorData::invalid_params("manipulation_id must fit an unsigned 64-bit integer", None)
    })
}

pub(in crate::mcp) fn schema<T: JsonSchema>() -> JsonObject {
    let mut schema = schemars::schema_for!(T)
        .as_object()
        .expect("tool argument schemas must be objects")
        .clone();
    // A tagged enum is a oneOf without a type; MCP requires an object input schema.
    schema
        .entry("type")
        .or_insert_with(|| Value::String("object".into()));
    schema
}

/// A request bound to the scene token the caller last inspected.
pub(in crate::mcp) fn scoped(token: Uuid, operation: Operation) -> RemoteRequest {
    RemoteRequest {
        expected_scene_token: Some(token),
        operation,
    }
}

/// Decoder for tools whose only argument is the scene token.
pub(in crate::mcp) fn mutation(
    arguments: JsonObject,
    operation: Operation,
) -> Result<RemoteRequest, ErrorData> {
    let args: MutationArgs = decode(arguments)?;
    Ok(scoped(token(&args.scene_token)?, operation))
}
