use super::{
    EditorEnvelope, UI_REPLY_TIMEOUT,
    transport::{Control, Shared, check_serialized_size},
};
use crate::editor_operations::{
    MAX_NAME_BYTES, MAX_RESULT_BYTES, Operation, OperationError, OperationReply, RemoteRequest,
    TOOL_NAMES,
};
use azteccraft::scene::{NodeId, Primitive, Transform};
use rmcp::{
    RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelledNotificationParam,
        ErrorData, Implementation, JsonObject, ListToolsResult, PaginatedRequestParams,
        ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    schemars::{self, JsonSchema},
    service::{NotificationContext, RequestContext},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{future::Future, sync::Arc};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct InspectArgs {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MutationArgs {
    scene_token: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum PrimitiveInput {
    Cube,
    Plane,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    scene_token: String,
    primitive: PrimitiveInput,
    name: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectArgs {
    scene_token: String,
    node_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    scene_token: String,
    node_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RenameArgs {
    scene_token: String,
    node_id: String,
    name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TransformArgs {
    scene_token: String,
    node_id: String,
    translation: [f32; 3],
    rotation_degrees: [f32; 3],
    scale: [f32; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OrbitArgs {
    scene_token: String,
    radians: [f32; 2],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PanArgs {
    scene_token: String,
    world_units: [f32; 2],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DollyArgs {
    scene_token: String,
    factor: f32,
}

fn decode<T: DeserializeOwned>(arguments: JsonObject) -> Result<T, ErrorData> {
    serde_json::from_value(Value::Object(arguments))
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

fn token(value: &str) -> Result<Uuid, ErrorData> {
    Uuid::parse_str(value)
        .map_err(|_| ErrorData::invalid_params("scene_token must be a UUID", None))
}

fn node_id(value: &str) -> Result<NodeId, ErrorData> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ErrorData::invalid_params(
            "node_id must be an unsigned decimal string",
            None,
        ));
    }
    NodeId::from_decimal(value)
        .map_err(|_| ErrorData::invalid_params("node_id must fit an unsigned 64-bit integer", None))
}

fn parse_request(name: &str, arguments: JsonObject) -> Result<RemoteRequest, ErrorData> {
    if arguments
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| name.len() > MAX_NAME_BYTES)
    {
        return Err(ErrorData::invalid_params(
            "name exceeds 4096 UTF-8 bytes",
            None,
        ));
    }
    let (expected_scene_token, operation) = match name {
        "inspect_scene" => {
            decode::<InspectArgs>(arguments)?;
            return Ok(RemoteRequest {
                expected_scene_token: None,
                operation: Operation::Inspect,
            });
        }
        "create_primitive" => {
            let args: CreateArgs = decode(arguments)?;
            let primitive = match args.primitive {
                PrimitiveInput::Cube => Primitive::Cube,
                PrimitiveInput::Plane => Primitive::Plane,
            };
            (
                token(&args.scene_token)?,
                Operation::Create {
                    primitive,
                    name: args.name,
                },
            )
        }
        "select_node" => {
            let args: SelectArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Select(args.node_id.as_deref().map(node_id).transpose()?),
            )
        }
        "rename_node" => {
            let args: RenameArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Rename {
                    id: node_id(&args.node_id)?,
                    name: args.name,
                },
            )
        }
        "set_transform" => {
            let args: TransformArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::SetTransform {
                    id: node_id(&args.node_id)?,
                    transform: Transform {
                        translation: args.translation.into(),
                        rotation_degrees: args.rotation_degrees.into(),
                        scale: args.scale.into(),
                    },
                },
            )
        }
        "delete_node" => {
            let args: NodeArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Delete(node_id(&args.node_id)?),
            )
        }
        "orbit_camera" => {
            let args: OrbitArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Orbit {
                    radians: args.radians,
                },
            )
        }
        "pan_camera" => {
            let args: PanArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Pan {
                    world_units: args.world_units,
                },
            )
        }
        "dolly_camera" => {
            let args: DollyArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::Dolly {
                    factor: args.factor,
                },
            )
        }
        "undo" | "redo" | "frame_selected" => {
            let args: MutationArgs = decode(arguments)?;
            let operation = match name {
                "undo" => Operation::Undo,
                "redo" => Operation::Redo,
                _ => Operation::FrameSelected,
            };
            (token(&args.scene_token)?, operation)
        }
        _ => {
            return Err(ErrorData::new(
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                "Unknown AztecCraft tool",
                None,
            ));
        }
    };
    Ok(RemoteRequest {
        expected_scene_token: Some(expected_scene_token),
        operation,
    })
}

fn schema<T: JsonSchema>() -> JsonObject {
    schemars::schema_for!(T)
        .as_object()
        .expect("tool argument schemas must be objects")
        .clone()
}

fn catalog() -> Vec<Tool> {
    TOOL_NAMES
        .into_iter()
        .map(|name| {
            let (description, input_schema) = match name {
                "inspect_scene" => (
                    "Inspect committed editor state and report pending human edits",
                    schema::<InspectArgs>(),
                ),
                "create_primitive" => (
                    "Create a cube or plane in the live editor",
                    schema::<CreateArgs>(),
                ),
                "select_node" => (
                    "Select an object, or clear selection with null node_id",
                    schema::<SelectArgs>(),
                ),
                "rename_node" => ("Rename an existing object", schema::<RenameArgs>()),
                "set_transform" => (
                    "Set translation, rotation in XYZ degrees, and scale",
                    schema::<TransformArgs>(),
                ),
                "delete_node" => (
                    "Delete an object through editor undo history",
                    schema::<NodeArgs>(),
                ),
                "undo" => (
                    "Undo the last editor authoring operation",
                    schema::<MutationArgs>(),
                ),
                "redo" => (
                    "Redo the last undone authoring operation",
                    schema::<MutationArgs>(),
                ),
                "frame_selected" => (
                    "Frame the selected object using the live camera",
                    schema::<MutationArgs>(),
                ),
                "orbit_camera" => (
                    "Orbit the live camera by [yaw, pitch] radians; pitch clamps near the poles",
                    schema::<OrbitArgs>(),
                ),
                "pan_camera" => (
                    "Translate the live camera target by [right, up] camera-plane world units",
                    schema::<PanArgs>(),
                ),
                "dolly_camera" => (
                    "Multiply live camera distance by a positive finite factor; below one moves closer",
                    schema::<DollyArgs>(),
                ),
                _ => unreachable!("catalogue names must have schemas"),
            };
            Tool::new(name, description, input_schema).with_annotations(
                ToolAnnotations::new()
                    .read_only(name == "inspect_scene")
                    .idempotent(false)
                    .open_world(false),
            )
        })
        .collect()
}

pub(super) struct EditorServer {
    sender: mpsc::Sender<EditorEnvelope>,
    shared: Arc<Shared>,
    tools: Vec<Tool>,
}

impl EditorServer {
    pub(super) fn new(sender: mpsc::Sender<EditorEnvelope>, shared: Arc<Shared>) -> Self {
        Self {
            sender,
            shared,
            tools: catalog(),
        }
    }
}

fn tool_error(code: &str, message: impl ToString) -> CallToolResult {
    CallToolResult::structured_error(
        json!({ "error": { "code": code, "message": message.to_string() } }),
    )
}

fn tool_reply(reply: Result<OperationReply, OperationError>) -> CallToolResult {
    match reply {
        Ok(reply) => {
            if check_serialized_size(&reply, MAX_RESULT_BYTES / 4).is_err() {
                return tool_error("too_large", OperationError::TooLarge);
            }
            let value = serde_json::to_value(reply).expect("bounded editor replies must serialize");
            let result = CallToolResult::structured(value);
            if check_serialized_size(&result, MAX_RESULT_BYTES).is_err() {
                return tool_error("too_large", OperationError::TooLarge);
            }
            result
        }
        Err(error) => {
            let code = match &error {
                OperationError::BusyEditing => "busy_editing",
                OperationError::StaleScene => "stale_scene",
                OperationError::TooLarge => "too_large",
                OperationError::NoSelection => "no_selection",
                OperationError::InvalidInput(_) => "invalid_input",
                OperationError::Scene(_) => "scene_error",
                OperationError::Camera(_) => "camera_error",
            };
            tool_error(code, error)
        }
    }
}

impl ServerHandler for EditorServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("AztecCraft", env!("CARGO_PKG_VERSION")))
            .with_instructions("Controls this newly launched native editor window. Inspect first; mutations require its scene token. Human text edits and dialogs block remote mutation.")
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools.iter().find(|tool| tool.name == name).cloned()
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools.clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let request = parse_request(&request.name, request.arguments.unwrap_or_default())?;
        if !self.shared.alive() {
            return Ok(tool_error("unavailable", "Editor protocol connection is closed").into());
        }
        let (reply, receiver) = oneshot::channel();
        if self
            .sender
            .try_send(EditorEnvelope {
                request,
                reply,
                cancellation: context.ct.clone(),
            })
            .is_err()
        {
            return Ok(tool_error("busy", "Editor request queue is full or closed").into());
        }
        self.shared.repaint();
        let result = tokio::select! {
            _ = context.ct.cancelled() => tool_error("cancelled", "Request cancelled; inspect to reconcile any completed operation"),
            outcome = tokio::time::timeout(UI_REPLY_TIMEOUT, receiver) => match outcome {
                Ok(Ok(reply)) => tool_reply(reply),
                Ok(Err(_)) => tool_error("unavailable", "Editor did not acknowledge the request"),
                Err(_) => tool_error("busy", "Editor reply deadline exceeded; inspect before retrying"),
            },
        };
        Ok(result.into())
    }

    fn on_initialized(
        &self,
        _context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + Send + '_ {
        let completion = self.shared.complete_control(Control::Initialized);
        async move {
            drop(completion);
        }
    }

    fn on_cancelled(
        &self,
        notification: CancelledNotificationParam,
        _context: NotificationContext<RoleServer>,
    ) -> impl Future<Output = ()> + Send + '_ {
        let completion = notification
            .request_id
            .map(|id| self.shared.complete_control(Control::Cancelled(id)));
        async move {
            drop(completion);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_catalogue_schemas_and_handlers_match_exactly() {
        let tools = catalog();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            TOOL_NAMES
        );
        let token = Uuid::new_v4().to_string();
        for tool in tools {
            assert_eq!(tool.input_schema["additionalProperties"], false);
            assert_eq!(
                tool.annotations.as_ref().unwrap().read_only_hint,
                Some(tool.name == "inspect_scene")
            );
            let args = match tool.name.as_ref() {
                "inspect_scene" => json!({}),
                "create_primitive" => json!({"scene_token":token,"primitive":"cube"}),
                "select_node" => json!({"scene_token":token,"node_id":null}),
                "rename_node" => json!({"scene_token":token,"node_id":"1","name":"cube"}),
                "set_transform" => {
                    json!({"scene_token":token,"node_id":"1","translation":[0,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]})
                }
                "delete_node" => json!({"scene_token":token,"node_id":"1"}),
                "orbit_camera" => json!({"scene_token":token,"radians":[0,0]}),
                "pan_camera" => json!({"scene_token":token,"world_units":[0,0]}),
                "dolly_camera" => json!({"scene_token":token,"factor":1}),
                _ => json!({"scene_token":token}),
            };
            assert!(parse_request(&tool.name, args.as_object().unwrap().clone()).is_ok());
        }
    }

    #[test]
    fn strict_parameters_reject_unknown_fields_invalid_ids_and_name_byte_overflow() {
        let token = Uuid::new_v4().to_string();
        for invalid in [json!({"extra":true}), json!({"scene_token":token})] {
            assert!(parse_request("inspect_scene", invalid.as_object().unwrap().clone()).is_err());
        }
        for id in [
            json!(1),
            json!("+1"),
            json!(""),
            json!("18446744073709551616"),
        ] {
            let args = json!({"scene_token":token,"node_id":id});
            assert!(parse_request("delete_node", args.as_object().unwrap().clone()).is_err());
        }
        let fitting = "é".repeat(MAX_NAME_BYTES / 2);
        let mut args = json!({"scene_token":token,"node_id":"1","name":fitting});
        assert!(parse_request("rename_node", args.as_object().unwrap().clone()).is_ok());
        args["name"] = format!("{}a", args["name"].as_str().unwrap()).into();
        assert!(parse_request("rename_node", args.as_object().unwrap().clone()).is_err());
        assert!(parse_request("extrude_face", JsonObject::new()).is_err());
    }

    #[test]
    fn result_bound_covers_structured_and_textual_copies_and_surfaces_size_error() {
        let result = tool_reply(Ok(OperationReply::Created {
            node_id: "x".repeat(MAX_RESULT_BYTES),
        }));
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.as_ref().unwrap()["error"]["code"],
            "too_large"
        );
        assert!(check_serialized_size(&result, MAX_RESULT_BYTES).is_ok());
    }
}
