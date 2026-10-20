use super::{
    EditorEnvelope, UI_REPLY_TIMEOUT,
    registry::{catalog, parse_request},
    transport::{Control, Shared, check_serialized_size},
};
use crate::editor_operations::{MAX_RESULT_BYTES, OperationError, OperationReply};
use azteccraft::{history::ManipulationError, native_scene::NativeError, scene::SceneError};
use rmcp::{
    RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelledNotificationParam,
        ErrorData, Implementation, ListToolsResult, PaginatedRequestParams, ServerCapabilities,
        ServerConfig, Tool,
    },
    service::{NotificationContext, RequestContext},
};
use serde_json::json;
use std::{future::Future, sync::Arc};
use tokio::sync::{mpsc, oneshot};

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
                OperationError::BusyManipulation => "busy_manipulation",
                OperationError::Move(error) => match error {
                    ManipulationError::StaleScene => "stale_scene",
                    ManipulationError::StaleManipulation => "stale_manipulation",
                    ManipulationError::WrongOwner => "manipulation_owner",
                    ManipulationError::Busy => "busy_manipulation",
                    ManipulationError::Native(NativeError::AbsentChannelLeaf { .. }) => {
                        "absent_channel_leaf"
                    }
                    _ => "manipulation_error",
                },
                OperationError::StaleScene => "stale_scene",
                OperationError::TooLarge => "too_large",
                OperationError::SessionMaterial(error) => match error {
                    NativeError::UnknownSessionMaterial(_) => "unknown_session_material",
                    NativeError::SessionColorOutOfRange | NativeError::NonFinite => {
                        "invalid_session_color"
                    }
                    NativeError::InitialSessionMaterialLocked => "initial_session_material_locked",
                    NativeError::UnsupportedEdit => "unsupported_edit",
                    NativeError::ReadOnlyFormat => "read_only_format",
                    NativeError::MissingShape(_) => "unknown_shape",
                    NativeError::ResourceLimit => "session_material_limit",
                    _ => "scene_error",
                },
                OperationError::NoSelection => "no_selection",
                OperationError::NothingToFrame => "nothing_to_frame",
                OperationError::InvalidInput(_) => "invalid_input",
                OperationError::Scene(NativeError::AbsentChannelLeaf { .. }) => {
                    "absent_channel_leaf"
                }
                OperationError::Scene(NativeError::Legacy(
                    SceneError::InvalidPrimitiveOptions(_),
                )) => "invalid_primitive_options",
                OperationError::Scene(_) => "scene_error",
                OperationError::Camera(_) => "camera_error",
                OperationError::File(_) => "file_error",
                OperationError::View(_) => "view_error",
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
                OperationError::KindMismatch { .. } => "manipulation_kind_mismatch",
                OperationError::AmbiguousSelection => "ambiguous_selection",
                OperationError::StalePendingAction => "stale_pending_action",
            };
            match error {
                // File text comes from DocumentError/ObjError Display, which embeds the path; the
                // typed variant is the only reliable signal, so withhold it.
                OperationError::File(_) => {
                    tool_error(code, crate::editor_operations::STATUS_MESSAGE_WITHHELD)
                }
                error => tool_error(code, error),
            }
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
    use crate::editor_operations::{
        AttributeEditorTab, FileDecision, MAX_NAME_BYTES, ManipulationSpace, ManipulationValue,
        MoveDistance, Operation, SelectionMode, ShadingMode, SidebarPanel,
    };
    use crate::mcp::tool_names;
    use azteccraft::native_scene::{RotateAxis, ScaleHandle};
    use rmcp::model::{ErrorCode, JsonObject};
    use uuid::Uuid;

    #[test]
    fn file_errors_withhold_private_paths_and_keep_the_code() {
        let io = azteccraft::document::DocumentError::Io {
            operation: "read",
            path: "/tmp/secret-dir/scene.ma".into(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        };
        let text = io.to_string();
        assert!(
            text.contains("/tmp/secret-dir"),
            "fixture must carry the path"
        );
        let result = tool_reply(Err(OperationError::File(text)));
        assert_eq!(result.is_error, Some(true));
        let value = serde_json::to_value(&result).unwrap();
        let rendered = value.to_string();
        assert!(!rendered.contains("secret-dir") && !rendered.contains("scene.ma"));
        let error = &value["structuredContent"]["error"];
        assert_eq!(error["code"], "file_error");
        assert_eq!(
            error["message"],
            crate::editor_operations::STATUS_MESSAGE_WITHHELD
        );
    }

    #[test]
    fn view_errors_keep_their_actionable_text_under_their_own_code() {
        let text = "derived view exceeds its resource budget or allocation capacity";
        let result = tool_reply(Err(OperationError::View(text.into())));
        let value = serde_json::to_value(&result).unwrap();
        let error = &value["structuredContent"]["error"];
        assert_eq!(error["code"], "view_error");
        assert_eq!(error["message"], text);
    }

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
    fn manipulation_schemas_reject_unsupported_kinds_axes_ids_and_extra_fields() {
        let token = Uuid::new_v4().to_string();
        let move_args = |extra: serde_json::Value| {
            let mut args = json!({"scene_token":token,"kind":"move","node_id":"1","instance_path":["1"],"axis":"x"});
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            args
        };
        for (name, args) in [
            (
                "set_active_tool",
                json!({"scene_token":token,"tool":"lasso"}),
            ),
            (
                "set_active_tool",
                json!({"scene_token":token,"tool":"move","owner":"native"}),
            ),
            (
                "set_tool_space",
                json!({"scene_token":token,"tool":"move","space":"world"}),
            ),
            (
                "set_tool_space",
                json!({"scene_token":token,"tool":"rotate","space":"gimbal"}),
            ),
            (
                "set_tool_space",
                json!({"scene_token":token,"tool":"rotate"}),
            ),
            ("begin_manipulation", move_args(json!({"instance_path":[]}))),
            ("begin_manipulation", move_args(json!({"axis":"xy"}))),
            (
                "begin_manipulation",
                move_args(json!({"instance_path":vec!["1";257]})),
            ),
            ("begin_manipulation", move_args(json!({"node_id":1}))),
            ("begin_manipulation", move_args(json!({"kind":"shear"}))),
            ("begin_manipulation", move_args(json!({"kind":null}))),
            // Move takes no space, handle or unknown field.
            ("begin_manipulation", move_args(json!({"space":"world"}))),
            ("begin_manipulation", move_args(json!({"handle":"x"}))),
            ("begin_manipulation", move_args(json!({"owner":"native"}))),
            // Rotate needs an axis and an explicit space and no handle.
            (
                "begin_manipulation",
                json!({"scene_token":token,"kind":"rotate","node_id":"1","instance_path":["1"],"axis":"x"}),
            ),
            (
                "begin_manipulation",
                json!({"scene_token":token,"kind":"rotate","node_id":"1","instance_path":["1"],"axis":"x","space":"parent"}),
            ),
            (
                "begin_manipulation",
                json!({"scene_token":token,"kind":"rotate","node_id":"1","instance_path":["1"],"handle":"x","space":"object"}),
            ),
            // Scale needs a handle (including uniform) and an explicit space and no axis.
            (
                "begin_manipulation",
                json!({"scene_token":token,"kind":"scale","node_id":"1","instance_path":["1"],"axis":"x","space":"object"}),
            ),
            (
                "begin_manipulation",
                json!({"scene_token":token,"kind":"scale","node_id":"1","instance_path":["1"],"handle":"xy","space":"object"}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"+1","distance":1}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"18446744073709551616","distance":1}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":1,"distance":1}),
            ),
            // Exactly one parameter, finite, and a factor above zero.
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1"}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","distance":1,"angle":1}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","angle":1,"factor":2}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","angle":"1"}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","degrees":1}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","factor":0}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","factor":-1}),
            ),
            (
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","distance":1,"owner":"native"}),
            ),
            (
                "finish_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","finish":"release"}),
            ),
            (
                "finish_manipulation",
                json!({"scene_token":token,"manipulation_id":"1","finish":"commit","owner":"native"}),
            ),
            // The removed Move-only tools are gone.
            ("begin_move", move_args(json!({}))),
            (
                "update_move",
                json!({"scene_token":token,"move_id":"1","distance":1}),
            ),
            (
                "finish_move",
                json!({"scene_token":token,"move_id":"1","finish":"commit"}),
            ),
        ] {
            assert!(
                parse_request(name, args.as_object().unwrap().clone()).is_err(),
                "{name}: {args}"
            );
        }
        let valid = json!({"scene_token":token,"manipulation_id":"18446744073709551615","distance":-1.0000000000000002});
        let request =
            parse_request("update_manipulation", valid.as_object().unwrap().clone()).unwrap();
        let Operation::UpdateMove {
            id,
            distance: MoveDistance::Declared(value),
        } = request.operation
        else {
            panic!()
        };
        assert_eq!(id.to_decimal(), "18446744073709551615");
        assert_eq!(value.to_bits(), (-1.0000000000000002f64).to_bits());
        for (parameter, value) in [("angle", 1e-300_f64), ("angle", -720.5), ("factor", 1e-300)] {
            let args = json!({"scene_token":token,"manipulation_id":"7",parameter:value});
            let request =
                parse_request("update_manipulation", args.as_object().unwrap().clone()).unwrap();
            let Operation::UpdateManipulation { id, value: actual } = request.operation else {
                panic!("{parameter}")
            };
            assert_eq!(id.to_decimal(), "7");
            match (parameter, actual) {
                ("angle", ManipulationValue::Angle(actual))
                | ("factor", ManipulationValue::Factor(actual)) => {
                    assert_eq!(actual.to_bits(), value.to_bits())
                }
                other => panic!("{other:?}"),
            }
        }
        let begin = |args: serde_json::Value| {
            parse_request("begin_manipulation", args.as_object().unwrap().clone())
                .unwrap()
                .operation
        };
        assert!(matches!(
            begin(
                json!({"scene_token":token,"kind":"rotate","node_id":"1","instance_path":["1"],"axis":"z","space":"world"})
            ),
            Operation::BeginRotate {
                axis: RotateAxis::Z,
                space: ManipulationSpace::World,
                ..
            }
        ));
        assert!(matches!(
            begin(
                json!({"scene_token":token,"kind":"scale","node_id":"1","instance_path":["1"],"handle":"uniform","space":"object"})
            ),
            Operation::BeginScale {
                handle: ScaleHandle::Uniform,
                space: ManipulationSpace::Object,
                ..
            }
        ));
    }

    #[test]
    fn begin_and_update_advertise_one_of_with_closed_branches() {
        let tools = catalog();
        for (name, tagged) in [("begin_manipulation", true), ("update_manipulation", false)] {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            let one_of = tool.input_schema["oneOf"].as_array().unwrap();
            assert_eq!(one_of.len(), 3, "{name}");
            for branch in one_of {
                assert_eq!(branch["additionalProperties"], false, "{name}: {branch}");
                if tagged {
                    assert!(
                        branch["properties"]["kind"]["const"].is_string(),
                        "{branch}"
                    );
                }
            }
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
            tool_names().collect::<Vec<_>>()
        );
        let token = Uuid::new_v4().to_string();
        for tool in tools {
            // The oneOf tools close each branch instead (begin_and_update_advertise_one_of...).
            if tool.input_schema.contains_key("oneOf") {
                assert!(!tool.input_schema.contains_key("additionalProperties"));
            } else {
                assert_eq!(tool.input_schema["additionalProperties"], false);
            }
            assert_eq!(
                tool.annotations.as_ref().unwrap().read_only_hint,
                Some(tool.name == "inspect_scene")
            );
            let args = match tool.name.as_ref() {
                "inspect_scene" => json!({}),
                "set_active_tool" => json!({"scene_token":token,"tool":"move"}),
                "set_tool_space" => {
                    json!({"scene_token":token,"tool":"scale","space":"object"})
                }
                "begin_manipulation" => {
                    json!({"scene_token":token,"kind":"move","node_id":"1","instance_path":["1"],"axis":"x"})
                }
                "update_manipulation" => {
                    json!({"scene_token":token,"manipulation_id":"1","distance":1.25})
                }
                "finish_manipulation" => {
                    json!({"scene_token":token,"manipulation_id":"1","finish":"cancel"})
                }
                "create_primitive" => json!({"scene_token":token,"primitive":"cube"}),
                "select_node" => json!({"scene_token":token,"node_id":null}),
                "set_selection_mode" => json!({"scene_token":token,"mode":"face"}),
                "select_face" | "select_vertex" | "select_edge" => {
                    json!({"scene_token":token,"target":null})
                }
                "rename_node" => json!({"scene_token":token,"node_id":"1","name":"cube"}),
                "edit_node" => json!({"scene_token":token,"node_id":"1"}),
                "set_material_diffuse" => json!({"scene_token":token,"shape_id":"2","value":1.25}),
                "create_session_material" => {
                    json!({"scene_token":token,"color":[1.0,0.0,0.0]})
                }
                "assign_session_material" => {
                    json!({"scene_token":token,"shape_id":"2","material_id":"1"})
                }
                "set_session_material_color" => {
                    json!({"scene_token":token,"material_id":"1","color":[0.0,1.0,0.0]})
                }
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
                "set_shading_mode" => json!({"scene_token":token,"mode":"wireframe"}),
                "set_sidebar_panel_visibility" => {
                    json!({"scene_token":token,"panel":"channel_box","visible":true})
                }
                "select_attribute_editor_tab" => {
                    json!({"scene_token":token,"tab":{"kind":"transform","id":"1"}})
                }
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
    fn session_material_arguments_are_strict_and_assign_takes_exactly_one_source() {
        let token = Uuid::new_v4().to_string();
        let bad = [
            (
                "assign_session_material",
                json!({"scene_token":token,"shape_id":"2"}),
            ),
            (
                "assign_session_material",
                json!({"scene_token":token,"shape_id":"2","material_id":"1","color":[0,0,0]}),
            ),
            (
                "assign_session_material",
                json!({"scene_token":token,"shape_id":"2","material_id":"-1"}),
            ),
            (
                "set_session_material_color",
                json!({"scene_token":token,"material_id":"1","color":[0.0,1.0]}),
            ),
            (
                "set_session_material_color",
                json!({"scene_token":token,"material_id":"01x","color":[0,0,0]}),
            ),
            (
                "set_session_material_color",
                json!({"scene_token":token,"material_id":"007","color":[0,0,0]}),
            ),
            (
                "assign_session_material",
                json!({"scene_token":token,"shape_id":"2","material_id":"+1"}),
            ),
            (
                "create_session_material",
                json!({"scene_token":token,"color":[0,0,0],"extra":1}),
            ),
            (
                "create_session_material",
                json!({"scene_token":token,"color":["a",0,0]}),
            ),
        ];
        for (name, args) in bad {
            assert!(
                parse_request(name, args.as_object().unwrap().clone()).is_err(),
                "{name} {args}"
            );
        }
        let request = parse_request(
            "assign_session_material",
            json!({"scene_token":token,"shape_id":"2","color":[1,0,0]})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(matches!(
            request.operation,
            Operation::AssignSessionMaterial {
                material: azteccraft::native_scene::MaterialChoice::New { .. },
                ..
            }
        ));
    }

    #[test]
    fn only_absolute_state_setters_are_annotated_idempotent() {
        for tool in catalog() {
            let expected = matches!(
                tool.name.as_ref(),
                "set_grid_visibility"
                    | "set_shading_mode"
                    | "set_sidebar_panel_visibility"
                    | "select_attribute_editor_tab"
                    | "set_active_tool"
                    | "set_tool_space"
                    | "select_node"
                    | "set_selection_mode"
                    | "select_face"
                    | "set_session_material_color"
                    | "select_vertex"
                    | "select_edge"
            );
            assert_eq!(
                tool.annotations.unwrap().idempotent_hint == Some(true),
                expected,
                "{}",
                tool.name
            );
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
        for (mode, expected) in [
            ("object", SelectionMode::Object),
            ("face", SelectionMode::Face),
            ("hierarchy", SelectionMode::Hierarchy),
            ("vertex", SelectionMode::Vertex),
            ("edge", SelectionMode::Edge),
        ] {
            let args = json!({"scene_token":token,"mode":mode});
            let Operation::SetSelectionMode(parsed) =
                parse_request("set_selection_mode", args.as_object().unwrap().clone())
                    .unwrap()
                    .operation
            else {
                panic!("{mode} did not parse to SetSelectionMode");
            };
            assert_eq!(parsed, expected);
        }
        for mode in [
            json!(null),
            json!(false),
            json!("uv"),
            json!("Edge"),
            json!("Face"),
        ] {
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

    type PickedComponent = Option<(
        azteccraft::scene::NodeId,
        azteccraft::native_scene::NativeId,
        Vec<azteccraft::native_scene::NativeId>,
        usize,
    )>;

    #[test]
    fn vertex_selection_schema_and_parser_require_complete_target_or_null() {
        component_selection_contract("vertex", "Vertex", |operation| {
            let Operation::SelectVertex { target } = operation else {
                panic!("expected SelectVertex")
            };
            target.map(|t| (t.transform, t.shape, t.path, t.vertex_index))
        });
    }

    #[test]
    fn edge_selection_schema_and_parser_require_complete_target_or_null() {
        component_selection_contract("edge", "Edge", |operation| {
            let Operation::SelectEdge { target } = operation else {
                panic!("expected SelectEdge")
            };
            target.map(|t| (t.transform, t.shape, t.path, t.edge_index))
        });
    }

    fn component_selection_contract(
        noun: &str,
        def: &str,
        unpack: fn(Operation) -> PickedComponent,
    ) {
        let tool_name = format!("select_{noun}");
        let index_field = format!("{noun}_index");
        let token = Uuid::new_v4().to_string();
        let tools = catalog();
        let tool = tools.iter().find(|tool| tool.name == tool_name).unwrap();
        assert_eq!(
            tool.input_schema["required"],
            json!(["scene_token", "target"])
        );
        let target_schema = &tool.input_schema["$defs"][format!("{def}SelectionTarget")];
        assert_eq!(target_schema["additionalProperties"], false);
        assert_eq!(target_schema["properties"]["instance_path"]["minItems"], 2);
        assert_eq!(
            target_schema["properties"]["instance_path"]["maxItems"],
            257
        );
        assert!(
            tool.input_schema["properties"]["target"]
                .to_string()
                .contains("null")
        );
        let parse =
            |args: &serde_json::Value| parse_request(&tool_name, args.as_object().unwrap().clone());
        assert_eq!(
            unpack(
                parse(&json!({"scene_token":token,"target":null}))
                    .unwrap()
                    .operation
            ),
            None
        );
        let valid = json!({"scene_token":token,"target":{"node_id":"1","shape_id":"2","instance_path":["1","2"],index_field.clone():7}});
        let (transform, shape, path, index) = unpack(parse(&valid).unwrap().operation).unwrap();
        assert_eq!(transform.to_decimal(), "1");
        assert_eq!(shape.to_decimal(), "2");
        assert_eq!(
            path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
            ["1", "2"]
        );
        assert_eq!(index, 7);
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
                index_field.as_str(),
                vec![json!(-1), json!(0.5), json!("0"), json!(null), json!(1e20)],
            ),
        ] {
            for value in values {
                let mut args = valid.clone();
                args["target"][field] = value;
                assert!(parse(&args).is_err(), "{field}");
            }
        }
        for field in ["node_id", "shape_id", "instance_path", index_field.as_str()] {
            let mut args = valid.clone();
            args["target"].as_object_mut().unwrap().remove(field);
            assert!(parse(&args).is_err(), "missing {field}");
        }
        let mut face_field = valid.clone();
        face_field["target"]["face_index"] = json!(0);
        for args in [
            json!({"scene_token":token}),
            json!({"scene_token":token,"target":null,"extra":true}),
            json!({"scene_token":token,"target":{"node_id":"1","shape_id":"2","instance_path":["1","2"],index_field.clone():0,"extra":true}}),
            face_field,
        ] {
            assert!(parse(&args).is_err());
        }
        let mut boundary = valid;
        boundary["target"]["instance_path"] = json!(vec!["1"; 257]);
        assert!(parse(&boundary).is_ok());
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
    fn shading_schema_and_decoder_require_token_and_a_known_mode() {
        let token = Uuid::new_v4().to_string();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "set_shading_mode")
            .unwrap();
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let required = tool.input_schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 2);
        assert!(required.contains(&json!("scene_token")));
        assert!(required.contains(&json!("mode")));
        assert_eq!(tool.annotations.unwrap().idempotent_hint, Some(true));
        for arguments in [
            json!({"scene_token": token}),
            json!({"mode": "wireframe"}),
            json!({"scene_token": token, "mode": null}),
            json!({"scene_token": token, "mode": "Wireframe"}),
            json!({"scene_token": token, "mode": "shaded"}),
            json!({"scene_token": token, "mode": "default_material"}),
            json!({"scene_token": token, "mode": 5}),
            json!({"scene_token": token, "mode": "wireframe", "extra": true}),
            json!({"scene_token": "invalid", "mode": "wireframe"}),
        ] {
            assert!(
                parse_request("set_shading_mode", arguments.as_object().unwrap().clone()).is_err()
            );
        }
        for (name, expected) in [
            ("wireframe", ShadingMode::Wireframe),
            ("smooth_shade_all", ShadingMode::SmoothShadeAll),
        ] {
            let request = parse_request(
                "set_shading_mode",
                json!({"scene_token": token, "mode": name})
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
                matches!(request.operation, Operation::SetShadingMode(actual) if actual == expected)
            );
        }
    }

    #[test]
    fn sidebar_panel_schema_and_decoder_require_token_panel_and_explicit_boolean() {
        let token = Uuid::new_v4().to_string();
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "set_sidebar_panel_visibility")
            .unwrap();
        assert_eq!(
            tool.input_schema["properties"]["visible"]["type"],
            "boolean"
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let required = tool.input_schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 3);
        for field in ["scene_token", "panel", "visible"] {
            assert!(required.contains(&json!(field)), "{field}");
        }
        let annotations = tool.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(false));
        for arguments in [
            json!({"panel": "channel_box", "visible": false}),
            json!({"scene_token": token, "visible": false}),
            json!({"scene_token": token, "panel": "channel_box"}),
            json!({"scene_token": token, "panel": "channel_box", "visible": null}),
            json!({"scene_token": token, "panel": "channel_box", "visible": "true"}),
            json!({"scene_token": token, "panel": "channel_box", "visible": false, "extra": 1}),
            json!({"scene_token": token, "panel": "attributeEditor", "visible": false}),
            json!({"scene_token": token, "panel": "tool_settings", "visible": false}),
            json!({"scene_token": token, "panel": "ChannelBox", "visible": false}),
            json!({"scene_token": token, "panel": null, "visible": false}),
            json!({"scene_token": "invalid", "panel": "channel_box", "visible": false}),
        ] {
            let Err(error) = parse_request(
                "set_sidebar_panel_visibility",
                arguments.as_object().unwrap().clone(),
            ) else {
                panic!("accepted {arguments}");
            };
            assert_eq!(error.code, ErrorCode::INVALID_PARAMS, "{arguments}");
        }
        for visible in [false, true] {
            let request = parse_request(
                "set_sidebar_panel_visibility",
                json!({"scene_token": token, "panel": "channel_box", "visible": visible})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            assert_eq!(
                request.expected_scene_token,
                Some(Uuid::parse_str(&token).unwrap())
            );
            assert!(matches!(
                request.operation,
                Operation::SetSidebarPanelVisible { panel: SidebarPanel::ChannelBox, visible: actual }
                    if actual == visible
            ));
        }
    }

    #[test]
    fn attribute_editor_panel_and_tab_decode_to_typed_operations() {
        let token = Uuid::new_v4().to_string();
        let request = parse_request(
            "set_sidebar_panel_visibility",
            json!({"scene_token": token, "panel": "attribute_editor", "visible": true})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap();
        assert!(matches!(
            request.operation,
            Operation::SetSidebarPanelVisible {
                panel: SidebarPanel::AttributeEditor,
                visible: true
            }
        ));
        let tool = catalog()
            .into_iter()
            .find(|tool| tool.name == "select_attribute_editor_tab")
            .unwrap();
        assert_eq!(tool.input_schema["additionalProperties"], false);
        let annotations = tool.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));
        for (kind, check) in [("transform", 0), ("shape", 1), ("material", 2)] {
            let request = parse_request(
                "select_attribute_editor_tab",
                json!({"scene_token": token, "tab": {"kind": kind, "id": "7"}})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .unwrap();
            let Operation::SelectAttributeEditorTab { tab } = request.operation else {
                panic!("wrong operation");
            };
            assert_eq!(
                check,
                match tab {
                    AttributeEditorTab::Transform(_) => 0,
                    AttributeEditorTab::Shape(_) => 1,
                    AttributeEditorTab::Material(_) => 2,
                }
            );
        }
        for arguments in [
            json!({"tab": {"kind": "shape", "id": "7"}}),
            json!({"scene_token": token}),
            json!({"scene_token": token, "tab": null}),
            json!({"scene_token": token, "tab": {"kind": "shape"}}),
            json!({"scene_token": token, "tab": {"id": "7"}}),
            json!({"scene_token": token, "tab": {"kind": "pin", "id": "7"}}),
            json!({"scene_token": token, "tab": {"kind": "Shape", "id": "7"}}),
            json!({"scene_token": token, "tab": {"kind": "shape", "id": "x"}}),
            json!({"scene_token": token, "tab": {"kind": "shape", "id": ""}}),
            json!({"scene_token": token, "tab": {"kind": "shape", "id": 7}}),
            json!({"scene_token": token, "tab": {"kind": "shape", "id": "7", "extra": 1}}),
            json!({"scene_token": token, "tab": {"kind": "shape", "id": "7"}, "extra": 1}),
            json!({"scene_token": "invalid", "tab": {"kind": "shape", "id": "7"}}),
        ] {
            let Err(error) = parse_request(
                "select_attribute_editor_tab",
                arguments.as_object().unwrap().clone(),
            ) else {
                panic!("accepted {arguments}");
            };
            assert_eq!(error.code, ErrorCode::INVALID_PARAMS, "{arguments}");
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

    #[test]
    fn invalid_primitive_options_have_their_own_error_code() {
        let cause = azteccraft::primitive_options::PrimitiveOptions::default_for(
            azteccraft::scene::Primitive::Cube,
        )
        .validate(azteccraft::scene::Primitive::Cone)
        .unwrap_err();
        let result = tool_reply(Err(OperationError::Scene(NativeError::Legacy(
            SceneError::InvalidPrimitiveOptions(cause),
        ))));
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.as_ref().unwrap()["error"]["code"],
            "invalid_primitive_options"
        );
    }
}
