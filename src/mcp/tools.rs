use super::{
    EditorEnvelope, UI_REPLY_TIMEOUT,
    transport::{Control, Shared, check_serialized_size},
};
use crate::editor_operations::{
    FaceSelectionTarget, FileDecision, MAX_NAME_BYTES, MAX_RESULT_BYTES, Operation, OperationError,
    OperationReply, RemoteRequest, SelectionMode, TOOL_NAMES, TransformInput,
};
use azteccraft::{
    native_scene::{FaceTarget, NativeId},
    scene::{NodeId, Primitive},
};
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
    #[schemars(length(max = 256))]
    instance_path: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    scene_token: String,
    node_id: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectionModeArgs {
    scene_token: String,
    mode: SelectionMode,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectFaceArgs {
    scene_token: String,
    target: Option<FaceSelectionTarget>,
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
    translation: [f64; 3],
    rotation_degrees: [f64; 3],
    scale: [f64; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct EditNodeArgs {
    scene_token: String,
    node_id: String,
    name: Option<String>,
    #[serde(default)]
    translation: [Option<f64>; 3],
    #[serde(default)]
    rotation_degrees: [Option<f64>; 3],
    #[serde(default)]
    scale: [Option<f64>; 3],
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ExtrudeArgs {
    scene_token: String,
    node_id: String,
    shape_id: String,
    #[schemars(length(min = 2, max = 257))]
    instance_path: Vec<String>,
    face_index: usize,
    distance: f64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MaterialDiffuseArgs {
    scene_token: String,
    shape_id: String,
    value: f64,
}

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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GridArgs {
    scene_token: String,
    visible: bool,
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

fn native_id(value: &str) -> Result<NativeId, ErrorData> {
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
            if args.node_id.is_none() && args.instance_path.is_some() {
                return Err(ErrorData::invalid_params(
                    "Clearing selection cannot include an instance_path",
                    None,
                ));
            }
            let instance_path = args
                .instance_path
                .map(|path| {
                    if path.len() > 256 {
                        return Err(ErrorData::invalid_params(
                            "instance_path exceeds 256 IDs",
                            None,
                        ));
                    }
                    path.into_iter()
                        .map(|value| native_id(&value))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            (
                token(&args.scene_token)?,
                Operation::Select {
                    node: args.node_id.as_deref().map(node_id).transpose()?,
                    instance_path,
                },
            )
        }
        "set_selection_mode" => {
            let args: SelectionModeArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::SetSelectionMode(args.mode),
            )
        }
        "select_face" => {
            if !arguments.contains_key("target") {
                return Err(ErrorData::invalid_params(
                    "target must be a complete face target or null",
                    None,
                ));
            }
            let args: SelectFaceArgs = decode(arguments)?;
            let target = args
                .target
                .map(|target| {
                    if !(2..=257).contains(&target.instance_path.len()) {
                        return Err(ErrorData::invalid_params(
                            "Face selection requires 2 to 257 path IDs",
                            None,
                        ));
                    }
                    Ok(FaceTarget {
                        transform: node_id(&target.node_id)?,
                        shape: native_id(&target.shape_id)?,
                        path: target
                            .instance_path
                            .iter()
                            .map(|value| native_id(value))
                            .collect::<Result<_, _>>()?,
                        face_index: target.face_index,
                    })
                })
                .transpose()?;
            (token(&args.scene_token)?, Operation::SelectFace { target })
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
                    transform: TransformInput {
                        translation: args.translation,
                        rotation_degrees: args.rotation_degrees,
                        scale: args.scale,
                    },
                },
            )
        }
        "edit_node" => {
            let args: EditNodeArgs = decode(arguments)?;
            if args
                .translation
                .iter()
                .chain(&args.rotation_degrees)
                .chain(&args.scale)
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(ErrorData::invalid_params(
                    "Channel values must be finite",
                    None,
                ));
            }
            (
                token(&args.scene_token)?,
                Operation::EditNode {
                    id: node_id(&args.node_id)?,
                    name: args.name,
                    translation: args.translation,
                    rotation_degrees: args.rotation_degrees,
                    scale: args.scale,
                },
            )
        }
        "set_material_diffuse" => {
            let args: MaterialDiffuseArgs = decode(arguments)?;
            if !args.value.is_finite() || args.value < 0.0 {
                return Err(ErrorData::invalid_params(
                    "Diffuse must be finite and nonnegative",
                    None,
                ));
            }
            (
                token(&args.scene_token)?,
                Operation::SetMaterialDiffuse {
                    shape: native_id(&args.shape_id)?,
                    value: args.value,
                },
            )
        }
        "extrude_face" => {
            let args: ExtrudeArgs = decode(arguments)?;
            if !(2..=257).contains(&args.instance_path.len()) || !args.distance.is_finite() {
                return Err(ErrorData::invalid_params(
                    "Extrusion requires 2 to 257 path IDs and a finite distance",
                    None,
                ));
            }
            (
                token(&args.scene_token)?,
                Operation::ExtrudeFace {
                    target: FaceTarget {
                        transform: node_id(&args.node_id)?,
                        shape: native_id(&args.shape_id)?,
                        path: args
                            .instance_path
                            .iter()
                            .map(|value| native_id(value))
                            .collect::<Result<_, _>>()?,
                        face_index: args.face_index,
                    },
                    distance: args.distance,
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
        "new_scene" => {
            let args: MutationArgs = decode(arguments)?;
            (token(&args.scene_token)?, Operation::NewScene)
        }
        "request_close" => {
            let args: MutationArgs = decode(arguments)?;
            (token(&args.scene_token)?, Operation::RequestClose)
        }
        "save_scene" => {
            let args: MutationArgs = decode(arguments)?;
            (token(&args.scene_token)?, Operation::SaveScene)
        }
        "open_scene" | "save_scene_as" => {
            let args: OpenArgs = decode(arguments)?;
            if args.path.is_empty() || args.path.len() > 4096 || args.path.contains('\0') {
                return Err(ErrorData::invalid_params(
                    "path must contain 1 to 4096 UTF-8 bytes and no NUL",
                    None,
                ));
            }
            (
                token(&args.scene_token)?,
                if name == "open_scene" {
                    Operation::OpenScene {
                        path: args.path.into(),
                    }
                } else {
                    Operation::SaveSceneAs {
                        path: args.path.into(),
                    }
                },
            )
        }
        "resolve_file_action" => {
            let args: ResolveFileArgs = decode(arguments)?;
            let pending_id = Uuid::parse_str(&args.pending_action_id)
                .map_err(|_| ErrorData::invalid_params("pending_action_id must be a UUID", None))?;
            let decision = match args.decision {
                FileDecisionInput::Cancel => FileDecision::Cancel,
                FileDecisionInput::Discard => FileDecision::Discard,
                FileDecisionInput::Save => FileDecision::Save,
            };
            (
                token(&args.scene_token)?,
                Operation::ResolveFileAction {
                    pending_id,
                    decision,
                },
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
        "set_grid_visibility" => {
            let args: GridArgs = decode(arguments)?;
            (
                token(&args.scene_token)?,
                Operation::SetGridVisible {
                    visible: args.visible,
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
                    "Inspect committed editor state, pending human edits and bounded scene/shape source-appearance readiness. Checked source associations do not imply evaluated or rendered materials",
                    schema::<InspectArgs>(),
                ),
                "create_primitive" => (
                    "Create a cube or plane in the live editor",
                    schema::<CreateArgs>(),
                ),
                "select_node" => (
                    "Select an object using an optional full transform instance_path; ambiguous objects require a path. Null node_id clears selection without a path",
                    schema::<SelectArgs>(),
                ),
                "set_selection_mode" => (
                    "Set Object or Face selection mode without editing the scene; a valid remembered face stays inactive in Object mode",
                    schema::<SelectionModeArgs>(),
                ),
                "select_face" => (
                    "Select a complete canonical face target in Face mode, or clear it with null; explicit targets need not be visible and select their owning instance",
                    {
                        let mut input = schema::<SelectFaceArgs>();
                        input.insert("required".into(), json!(["scene_token", "target"]));
                        input
                    },
                ),
                "rename_node" => ("Rename an existing object", schema::<RenameArgs>()),
                "set_transform" => (
                    "Set f64 translation in declared scene units, rotation in XYZ degrees, and scale",
                    schema::<TransformArgs>(),
                ),
                "edit_node" => (
                    "Edit optional name and individual f64 channels in one undo entry. Omitted arrays and null components are untouched; translation uses declared scene units and supplied rotations use degrees. Does not change selection",
                    schema::<EditNodeArgs>(),
                ),
                "extrude_face" => (
                    "Extrude one current canonical quad along its local normal by distance in declared scene units. Requires node, shape and full root-to-shape path; shared-shape edits affect every instance. Zero distance changes topology; imported structural edits remain unsupported",
                    schema::<ExtrudeArgs>(),
                ),
                "set_material_diffuse" => (
                    "Edit the checked saved Phong diffuse scalar for an exact shape through shared undo history. Requires complete shared-shader users; affects all those shapes. Values are finite and nonnegative, may exceed one, and do not enable rendered materials",
                    schema::<MaterialDiffuseArgs>(),
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
                "set_grid_visibility" => (
                    "Set the current viewport's grid visibility explicitly. Session-only and idempotent; does not edit the scene or report GPU health",
                    schema::<GridArgs>(),
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
                "new_scene" => (
                    "Request a new empty document; dirty documents require explicit confirmation using the returned pending_action_id",
                    schema::<MutationArgs>(),
                ),
                "request_close" => (
                    "Request native window closure; dirty documents require explicit confirmation, and approved remote closure waits for its successful reply to be flushed",
                    schema::<MutationArgs>(),
                ),
                "open_scene" => (
                    "Open a supported scene from a local UTF-8 path (1 to 4096 bytes, no NUL); dirty documents require confirmation",
                    schema::<OpenArgs>(),
                ),
                "save_scene" => (
                    "Save the active supported Maya binary document, or reconcile an earlier uncertain installation without another write",
                    schema::<MutationArgs>(),
                ),
                "save_scene_as" => (
                    "Save a supported Maya binary scene to a fresh .mb path without overwriting; an uncertain prior save is reconciled instead",
                    schema::<OpenArgs>(),
                ),
                "resolve_file_action" => (
                    "Cancel, discard or save before the exact pending file action using its UUID and current scene token; source-changed Open requires Cancel and reopening",
                    schema::<ResolveFileArgs>(),
                ),
                _ => unreachable!("catalogue names must have schemas"),
            };
            Tool::new(name, description, input_schema).with_annotations(
                ToolAnnotations::new()
                    .read_only(name == "inspect_scene")
                    .idempotent(name == "set_grid_visibility")
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
                OperationError::File(_) => "file_error",
                OperationError::NativeSave(error) => match error {
                    azteccraft::document::NativeSaveError::Unsupported => "save_unsupported",
                    azteccraft::document::NativeSaveError::InvalidDestination => {
                        "invalid_destination"
                    }
                    azteccraft::document::NativeSaveError::SourceChanged => "source_changed",
                    azteccraft::document::NativeSaveError::DestinationExists => {
                        "destination_exists"
                    }
                    azteccraft::document::NativeSaveError::Cancelled => "save_cancelled",
                    azteccraft::document::NativeSaveError::Io(_) => "save_io",
                    azteccraft::document::NativeSaveError::Encode(_) => "save_encode",
                },
                OperationError::AmbiguousSelection => "ambiguous_selection",
                OperationError::StalePendingAction => "stale_pending_action",
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
        let deadline = std::time::Instant::now() + UI_REPLY_TIMEOUT;
        if self
            .sender
            .try_send(EditorEnvelope {
                request,
                reply,
                cancellation: context.ct.clone(),
                request_id: context.id.clone(),
                deadline,
                #[cfg(test)]
                install_probe: None,
            })
            .is_err()
        {
            return Ok(tool_error("busy", "Editor request queue is full or closed").into());
        }
        self.shared.repaint();
        let result = tokio::select! {
            _ = context.ct.cancelled() => {
                self.shared.fail_close(&context.id);
                tool_error("cancelled", "Request cancelled; inspect to reconcile any completed operation")
            },
            outcome = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), receiver) => match outcome {
                Ok(Ok(reply)) => tool_reply(reply),
                Ok(Err(_)) => {
                    self.shared.fail_close(&context.id);
                    tool_error("unavailable", "Editor did not acknowledge the request")
                },
                Err(_) => {
                    self.shared.fail_close(&context.id);
                    tool_error("busy", "Editor reply deadline exceeded; inspect before retrying")
                },
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
    fn material_arguments_are_strict_f64_and_exact_native_identity() {
        let token = Uuid::new_v4().to_string();
        for args in [
            json!({"scene_token":token,"shape_id":"2","value":-1}),
            json!({"scene_token":token,"shape_id":2,"value":0.5}),
            json!({"scene_token":token,"shape_id":"2"}),
            json!({"scene_token":token,"shape_id":"2","value":"NaN"}),
            json!({"scene_token":token,"shape_id":"2","value":null}),
            json!({"scene_token":token,"shape_id":"2","value":0.5,"extra":true}),
        ] {
            assert!(
                parse_request("set_material_diffuse", args.as_object().unwrap().clone()).is_err()
            );
        }
        for value in [-0.0f64, 1.2345678901234567, 4.5] {
            let request = parse_request(
                "set_material_diffuse",
                json!({"scene_token":token,"shape_id":"18446744073709551615","value":value})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            let Operation::SetMaterialDiffuse {
                shape,
                value: actual,
            } = request.operation
            else {
                panic!("wrong typed operation");
            };
            assert_eq!(shape.to_decimal(), "18446744073709551615");
            assert_eq!(actual.to_bits(), value.to_bits());
        }
    }

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
                "set_selection_mode" => json!({"scene_token":token,"mode":"face"}),
                "select_face" => json!({"scene_token":token,"target":null}),
                "rename_node" => json!({"scene_token":token,"node_id":"1","name":"cube"}),
                "edit_node" => json!({"scene_token":token,"node_id":"1"}),
                "set_material_diffuse" => json!({"scene_token":token,"shape_id":"2","value":1.25}),
                "extrude_face" => {
                    json!({"scene_token":token,"node_id":"1","shape_id":"2","instance_path":["1","2"],"face_index":0,"distance":1})
                }
                "set_transform" => {
                    json!({"scene_token":token,"node_id":"1","translation":[0,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]})
                }
                "delete_node" => json!({"scene_token":token,"node_id":"1"}),
                "orbit_camera" => json!({"scene_token":token,"radians":[0,0]}),
                "pan_camera" => json!({"scene_token":token,"world_units":[0,0]}),
                "dolly_camera" => json!({"scene_token":token,"factor":1}),
                "set_grid_visibility" => json!({"scene_token":token,"visible":true}),
                "open_scene" | "save_scene_as" => json!({"scene_token":token,"path":"scene.mb"}),
                "resolve_file_action" => {
                    json!({"scene_token":token,"pending_action_id":Uuid::new_v4().to_string(),"decision":"cancel"})
                }
                _ => json!({"scene_token":token}),
            };
            assert!(parse_request(&tool.name, args.as_object().unwrap().clone()).is_ok());
        }
    }

    #[test]
    fn face_selection_schemas_and_parser_require_explicit_mode_and_complete_target_or_null() {
        let token = Uuid::new_v4().to_string();
        let tools = catalog();
        let face = tools
            .iter()
            .find(|tool| tool.name == "select_face")
            .unwrap();
        assert_eq!(
            face.input_schema["required"],
            json!(["scene_token", "target"])
        );
        let target_schema = &face.input_schema["$defs"]["FaceSelectionTarget"];
        assert_eq!(target_schema["additionalProperties"], false);
        assert_eq!(target_schema["properties"]["instance_path"]["minItems"], 2);
        assert_eq!(
            target_schema["properties"]["instance_path"]["maxItems"],
            257
        );
        assert!(
            face.input_schema["properties"]["target"]
                .to_string()
                .contains("null")
        );
        for mode in ["object", "face"] {
            let args = json!({"scene_token":token,"mode":mode});
            assert!(matches!(
                parse_request("set_selection_mode", args.as_object().unwrap().clone())
                    .unwrap()
                    .operation,
                Operation::SetSelectionMode(_)
            ));
        }
        for mode in [json!(null), json!(false), json!("vertex"), json!("Face")] {
            let args = json!({"scene_token":token,"mode":mode});
            assert!(
                parse_request("set_selection_mode", args.as_object().unwrap().clone()).is_err()
            );
        }
        let clear = json!({"scene_token":token,"target":null});
        assert!(matches!(
            parse_request("select_face", clear.as_object().unwrap().clone())
                .unwrap()
                .operation,
            Operation::SelectFace { target: None }
        ));
        let valid = json!({"scene_token":token,"target":{"node_id":"1","shape_id":"2","instance_path":["1","2"],"face_index":0}});
        let Operation::SelectFace {
            target: Some(target),
        } = parse_request("select_face", valid.as_object().unwrap().clone())
            .unwrap()
            .operation
        else {
            panic!("expected face target")
        };
        assert_eq!(target.transform.to_decimal(), "1");
        assert_eq!(target.shape.to_decimal(), "2");
        assert_eq!(
            target
                .path
                .iter()
                .map(|id| id.to_decimal())
                .collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(target.face_index, 0);
        for (field, values) in [
            (
                "node_id",
                vec![json!(1), json!("+1"), json!("18446744073709551616")],
            ),
            ("shape_id", vec![json!(null), json!(""), json!("-1")]),
            (
                "instance_path",
                vec![
                    json!([]),
                    json!(["1"]),
                    json!(vec!["1"; 258]),
                    json!(["1", 2]),
                    json!(["1", "+2"]),
                ],
            ),
            (
                "face_index",
                vec![json!(-1), json!(0.5), json!("0"), json!(null)],
            ),
        ] {
            for value in values {
                let mut args = valid.clone();
                args["target"][field] = value;
                assert!(
                    parse_request("select_face", args.as_object().unwrap().clone()).is_err(),
                    "{field}"
                );
            }
        }
        for field in ["node_id", "shape_id", "instance_path", "face_index"] {
            let mut args = valid.clone();
            args["target"].as_object_mut().unwrap().remove(field);
            assert!(parse_request("select_face", args.as_object().unwrap().clone()).is_err());
        }
        for args in [
            json!({"scene_token":token}),
            json!({"scene_token":token,"target":null,"extra":true}),
            json!({"scene_token":token,"target":{"node_id":"1","shape_id":"2","instance_path":["1","2"],"face_index":0,"extra":true}}),
        ] {
            assert!(parse_request("select_face", args.as_object().unwrap().clone()).is_err());
        }
        let mut boundary = valid;
        boundary["target"]["instance_path"] = json!(vec!["1"; 257]);
        assert!(parse_request("select_face", boundary.as_object().unwrap().clone()).is_ok());
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
    }

    #[test]
    fn grid_schema_and_decoder_require_an_explicit_boolean_and_current_token() {
        let token = Uuid::new_v4().to_string();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "set_grid_visibility")
            .unwrap();
        assert_eq!(
            tool.input_schema["properties"]["visible"]["type"],
            "boolean"
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let required = tool.input_schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("scene_token")));
        assert!(required.contains(&json!("visible")));
        assert_eq!(tool.annotations.unwrap().idempotent_hint, Some(true));
        for arguments in [
            json!({"scene_token": token}),
            json!({"visible": false}),
            json!({"scene_token": token, "visible": null}),
            json!({"scene_token": token, "visible": "false"}),
            json!({"scene_token": token, "visible": 0}),
            json!({"scene_token": token, "visible": []}),
            json!({"scene_token": token, "visible": {},}),
            json!({"scene_token": token, "visible": false, "extra": true}),
            json!({"scene_token": "invalid", "visible": false}),
        ] {
            assert!(
                parse_request(
                    "set_grid_visibility",
                    arguments.as_object().unwrap().clone()
                )
                .is_err()
            );
        }
        for visible in [false, true] {
            let request = parse_request(
                "set_grid_visibility",
                json!({"scene_token": token, "visible": visible})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            assert_eq!(
                request.expected_scene_token,
                Some(Uuid::parse_str(&token).unwrap())
            );
            assert!(
                matches!(request.operation, Operation::SetGridVisible { visible: actual } if actual == visible)
            );
        }
    }

    #[test]
    fn native_file_and_instance_arguments_enforce_bounds_and_exact_decisions() {
        let token = Uuid::new_v4().to_string();
        let pending = Uuid::new_v4().to_string();
        for path in [String::new(), "x\0.mb".into(), "é".repeat(2049)] {
            let args = json!({"scene_token":token,"path":path});
            for tool in ["open_scene", "save_scene_as"] {
                assert!(parse_request(tool, args.as_object().unwrap().clone()).is_err());
            }
        }
        let args = json!({"scene_token":token,"path":"é".repeat(2048)});
        assert!(parse_request("open_scene", args.as_object().unwrap().clone()).is_ok());
        for path in [
            json!(["+1"]),
            json!([1]),
            json!(["18446744073709551616"]),
            json!(vec!["1"; 257]),
        ] {
            let args = json!({"scene_token":token,"node_id":"1","instance_path":path});
            assert!(parse_request("select_node", args.as_object().unwrap().clone()).is_err());
        }
        let args = json!({"scene_token":token,"node_id":null,"instance_path":[]});
        assert!(parse_request("select_node", args.as_object().unwrap().clone()).is_err());
        for args in [
            json!({"scene_token":token,"pending_action_id":pending,"decision":"save_as"}),
            json!({"scene_token":token,"pending_action_id":"bad","decision":"cancel"}),
            json!({"scene_token":token,"pending_action_id":pending,"decision":"cancel","path":"ignored.mb"}),
            json!({"scene_token":token,"path":"scene.mb","extra":true}),
        ] {
            let name = if args.get("pending_action_id").is_some() {
                "resolve_file_action"
            } else {
                "open_scene"
            };
            assert!(parse_request(name, args.as_object().unwrap().clone()).is_err());
        }
        let save = json!({"scene_token":token,"pending_action_id":pending,"decision":"save"});
        assert!(matches!(
            parse_request("resolve_file_action", save.as_object().unwrap().clone())
                .unwrap()
                .operation,
            Operation::ResolveFileAction {
                decision: FileDecision::Save,
                ..
            }
        ));
        let args = json!({"scene_token":token,"node_id":"1","translation":[16777217.125,0,0],"rotation_degrees":[90,0,0],"scale":[1,1,1]});
        let request = parse_request("set_transform", args.as_object().unwrap().clone()).unwrap();
        let Operation::SetTransform { transform, .. } = request.operation else {
            panic!("expected transform")
        };
        assert_eq!(transform.translation[0], 16777217.125);
        assert_eq!(transform.rotation_degrees[0], 90.0);
    }

    #[test]
    fn sparse_node_schema_and_decoder_preserve_component_omissions() {
        let token = Uuid::new_v4().to_string();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "edit_node")
            .unwrap();
        for field in ["translation", "rotation_degrees", "scale"] {
            let field = &tool.input_schema["properties"][field];
            assert_eq!(field["type"], "array");
            assert_eq!(field["minItems"], 3);
            assert_eq!(field["maxItems"], 3);
        }
        let args = json!({"scene_token":token,"node_id":"1","name":null,"translation":[16777217.125,null,null]});
        let request = parse_request("edit_node", args.as_object().unwrap().clone()).unwrap();
        let Operation::EditNode {
            name,
            translation,
            rotation_degrees,
            scale,
            ..
        } = request.operation
        else {
            panic!("expected sparse edit")
        };
        assert!(name.is_none());
        assert_eq!(translation, [Some(16777217.125), None, None]);
        assert_eq!(rotation_degrees, [None; 3]);
        assert_eq!(scale, [None; 3]);
        for field in ["translation", "rotation_degrees", "scale"] {
            for invalid in [
                json!(null),
                json!([]),
                json!([1, 2]),
                json!([1, 2, 3, 4]),
                json!([false, null, null]),
                json!(["1", null, null]),
            ] {
                let mut args = json!({"scene_token":token,"node_id":"1"});
                args[field] = invalid;
                assert!(parse_request("edit_node", args.as_object().unwrap().clone()).is_err());
            }
        }
        for fields in [
            json!({"unknown":true}),
            json!({"node_id":1}),
            json!({"node_id":"+1"}),
            json!({"name":"x".repeat(MAX_NAME_BYTES + 1)}),
        ] {
            let mut args = json!({"scene_token":token,"node_id":"1"});
            args.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert!(parse_request("edit_node", args.as_object().unwrap().clone()).is_err());
        }
        assert!(
            serde_json::from_str::<JsonObject>(&format!(
                r#"{{"scene_token":"{token}","node_id":"1","translation":[1e400,null,null]}}"#
            ))
            .is_err()
        );
    }

    #[test]
    fn extrusion_schema_and_runtime_require_explicit_bounded_targets() {
        let token = Uuid::new_v4().to_string();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "extrude_face")
            .unwrap();
        assert_eq!(
            tool.input_schema["properties"]["instance_path"]["minItems"],
            2
        );
        assert_eq!(
            tool.input_schema["properties"]["instance_path"]["maxItems"],
            257
        );
        let valid = json!({"scene_token":token,"node_id":"1","shape_id":"2","instance_path":["1","2"],"face_index":1,"distance":-0.75});
        let request = parse_request("extrude_face", valid.as_object().unwrap().clone()).unwrap();
        let Operation::ExtrudeFace { target, distance } = request.operation else {
            panic!("expected extrusion")
        };
        assert_eq!(target.transform.to_decimal(), "1");
        assert_eq!(target.shape.to_decimal(), "2");
        assert_eq!(target.face_index, 1);
        assert_eq!(distance, -0.75);
        for (field, values) in [
            (
                "instance_path",
                vec![
                    json!(null),
                    json!([]),
                    json!(["1"]),
                    json!(vec!["1"; 258]),
                    json!([1, "2"]),
                    json!(["+1", "2"]),
                    json!(["1", "18446744073709551616"]),
                ],
            ),
            (
                "node_id",
                vec![
                    json!(1),
                    json!(""),
                    json!("+1"),
                    json!("18446744073709551616"),
                ],
            ),
            (
                "shape_id",
                vec![
                    json!(2),
                    json!(""),
                    json!("+2"),
                    json!("18446744073709551616"),
                ],
            ),
            (
                "face_index",
                vec![json!(-1), json!(1.5), json!("1"), json!(null)],
            ),
            ("distance", vec![json!(null), json!("1"), json!(false)]),
        ] {
            for value in values {
                let mut args = valid.clone();
                args[field] = value;
                assert!(
                    parse_request("extrude_face", args.as_object().unwrap().clone()).is_err(),
                    "{field}"
                );
            }
        }
        for required in [
            "node_id",
            "shape_id",
            "instance_path",
            "face_index",
            "distance",
        ] {
            let mut args = valid.clone();
            args.as_object_mut().unwrap().remove(required);
            assert!(parse_request("extrude_face", args.as_object().unwrap().clone()).is_err());
        }
        let mut args = valid.clone();
        args["extra"] = json!(true);
        assert!(parse_request("extrude_face", args.as_object().unwrap().clone()).is_err());
        let mut args = valid;
        args["instance_path"] = json!(vec!["1"; 257]);
        assert!(parse_request("extrude_face", args.as_object().unwrap().clone()).is_ok());
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
