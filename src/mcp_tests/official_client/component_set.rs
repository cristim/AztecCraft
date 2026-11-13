use super::*;
use crate::editor_operations::{ComponentKind, MarqueeMode};
use azteccraft::selection::ComponentOwner;

const MARQUEE_MODES: [&str; 4] = ["replace", "add", "remove", "toggle"];

async fn inspection(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> Value {
    call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

/// A cube in Face mode: the scene token and the `owner` argument naming its shape.
async fn cube_in_face_mode(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> (String, Value) {
    let token = editor.scene_token.to_string();
    call(
        editor,
        context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let node = inspection(editor, context, peer).await["nodes"][0].clone();
    let mut path = node["instance_paths"][0].as_array().unwrap().clone();
    path.push(node["shape_ids"][0].clone());
    let owner = json!({"node_id":node["id"],"shape_id":node["shape_ids"][0],"instance_path":path});
    call(
        editor,
        context,
        peer,
        "set_selection_mode",
        json!({"scene_token":token,"mode":"face"}),
    )
    .await;
    (token, owner)
}

fn arguments(
    token: &str,
    kind: &str,
    mode: Option<&str>,
    owner: &Value,
    indices: &[usize],
) -> Value {
    let mut arguments = json!({"scene_token":token,"kind":kind,"owner":owner,"indices":indices});
    if let Some(mode) = mode {
        arguments["mode"] = json!(mode);
    }
    arguments
}

fn code(result: CallToolResult) -> String {
    result.structured_content.unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn face_members(editor: &Editor) -> Vec<usize> {
    editor.component_set(ComponentKind::Face).indices().to_vec()
}

#[tokio::test]
async fn sdk_modify_component_selection_is_listed_with_a_closed_schema_and_not_idempotent() {
    let context = egui::Context::default();
    let (_editor, client, _server) = start(&context).await;
    let tools = client.peer().list_all_tools().await.unwrap();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "modify_component_selection")
        .expect("modify_component_selection is listed");
    assert_eq!(
        tool.annotations.as_ref().unwrap().idempotent_hint,
        Some(false)
    );
    assert_eq!(tool.input_schema["additionalProperties"], false);
    assert_eq!(
        tool.input_schema["required"],
        json!(["scene_token", "kind", "owner", "indices"])
    );
    assert_eq!(tool.input_schema["properties"]["indices"]["maxItems"], 6144);
    let owner = &tool.input_schema["$defs"]["ComponentOwnerArgs"];
    assert_eq!(owner["additionalProperties"], false);
    assert_eq!(owner["properties"]["instance_path"]["minItems"], 2);
    assert_eq!(owner["properties"]["instance_path"]["maxItems"], 257);
}

#[tokio::test]
async fn sdk_component_sets_hold_order_and_lead_and_inspect_reports_them_once() {
    let context = egui::Context::default();
    let (mut editor, client, _server) = start(&context).await;
    let peer = client.peer();
    let (token, owner) = cube_in_face_mode(&mut editor, &context, peer).await;
    let mut run = async |mode: Option<&str>, indices: &[usize]| {
        call(
            &mut editor,
            &context,
            peer,
            "modify_component_selection",
            arguments(&token, "face", mode, &owner, indices),
        )
        .await
        .structured_content
        .unwrap()
    };
    assert_eq!(run(None, &[3, 1, 3]).await["outcome"], "changed");
    assert_eq!(run(Some("add"), &[1, 5, 0]).await["outcome"], "changed");
    assert_eq!(run(Some("add"), &[1, 5]).await["outcome"], "unchanged");
    assert_eq!(run(Some("toggle"), &[3, 4]).await["outcome"], "changed");
    assert_eq!(run(Some("remove"), &[5, 2]).await["outcome"], "changed");
    let inspected = inspection(&mut editor, &context, peer).await;
    let faces = &inspected["component_selection"]["face"];
    assert_eq!(faces["indices"], json!([1, 0, 4]));
    assert_eq!(
        (faces["count"].as_u64(), faces["lead"].as_u64()),
        (Some(3), Some(4))
    );
    assert_eq!(faces["truncated"], false);
    assert_eq!(faces["node_id"], owner["node_id"]);
    assert_eq!(faces["instance_path"], owner["instance_path"]);
    assert_eq!(inspected["remembered_face"]["face_index"], 4);
    assert!(inspected["component_selection"]["vertex"].is_null());
    assert_eq!(face_members(&editor), [1, 0, 4]);
}

#[tokio::test]
async fn sdk_every_marquee_mode_matches_the_native_operation() {
    let context = egui::Context::default();
    for mode in MARQUEE_MODES {
        let (mut remote, client, _server) = start(&context).await;
        let peer = client.peer();
        let (token, owner) = cube_in_face_mode(&mut remote, &context, peer).await;
        let mut native = Editor::default();
        native.create(azteccraft::scene::Primitive::Cube);
        native
            .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
            .unwrap();
        let native_owner = {
            let node = native.selected().unwrap();
            let shape = native.history.scene().shapes()[0].id();
            let mut path = native.selected_path().to_vec();
            path.push(shape);
            ComponentOwner {
                transform: node,
                shape,
                path,
            }
        };
        for (step, indices) in [vec![0, 1, 2], vec![2, 3], vec![5, 0]]
            .into_iter()
            .enumerate()
        {
            let mode = if step == 0 { "replace" } else { mode };
            call(
                &mut remote,
                &context,
                peer,
                "modify_component_selection",
                arguments(&token, "face", Some(mode), &owner, &indices),
            )
            .await;
            native
                .apply_operation(Operation::ModifyComponentSelection {
                    kind: ComponentKind::Face,
                    mode: match mode {
                        "replace" => MarqueeMode::Replace,
                        "add" => MarqueeMode::Add,
                        "remove" => MarqueeMode::Remove,
                        _ => MarqueeMode::Toggle,
                    },
                    owner: native_owner.clone(),
                    indices,
                })
                .unwrap();
            assert_eq!(face_members(&remote), face_members(&native), "{mode}");
        }
    }
}

#[tokio::test]
async fn sdk_vertex_and_edge_sets_keep_their_own_members_across_modes() {
    let context = egui::Context::default();
    let (mut editor, client, _server) = start(&context).await;
    let peer = client.peer();
    let (token, owner) = cube_in_face_mode(&mut editor, &context, peer).await;
    for (kind, mode, indices) in [
        ("face", "face", vec![2, 0]),
        ("vertex", "vertex", vec![7, 1, 4]),
        ("edge", "edge", vec![11, 0]),
    ] {
        call(
            &mut editor,
            &context,
            peer,
            "set_selection_mode",
            json!({"scene_token":token,"mode":mode}),
        )
        .await;
        let result = call(
            &mut editor,
            &context,
            peer,
            "modify_component_selection",
            arguments(&token, kind, Some("add"), &owner, &indices),
        )
        .await;
        assert_ne!(result.is_error, Some(true), "{kind}");
    }
    let sets = inspection(&mut editor, &context, peer).await["component_selection"].clone();
    assert_eq!(sets["face"]["indices"], json!([2, 0]));
    assert_eq!(sets["vertex"]["indices"], json!([7, 1, 4]));
    assert_eq!(sets["edge"]["indices"], json!([11, 0]));
}

#[tokio::test]
async fn sdk_malformed_and_refused_requests_change_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, _server) = start(&context).await;
    let peer = client.peer();
    let (token, owner) = cube_in_face_mode(&mut editor, &context, peer).await;
    call(
        &mut editor,
        &context,
        peer,
        "modify_component_selection",
        arguments(&token, "face", Some("add"), &owner, &[1, 2]),
    )
    .await;
    let scene = editor.history.scene().clone();
    let mut short_owner = owner.clone();
    short_owner["instance_path"] = json!(["1"]);
    let mut long_owner = owner.clone();
    long_owner["instance_path"] = json!(vec!["1"; 258]);
    let mut extra = arguments(&token, "face", None, &owner, &[0]);
    extra["extra"] = json!(1);
    let cases = [
        (
            "wrong mode",
            arguments(&token, "vertex", Some("add"), &owner, &[0]),
            "invalid_input",
        ),
        (
            "missing face",
            arguments(&token, "face", Some("replace"), &owner, &[0, 6]),
            "scene_error",
        ),
        (
            "short path",
            arguments(&token, "face", None, &short_owner, &[0]),
            "invalid_params",
        ),
        (
            "long path",
            arguments(&token, "face", None, &long_owner, &[0]),
            "invalid_params",
        ),
        ("unknown field", extra, "invalid_params"),
        (
            "bad kind",
            arguments(&token, "uv", None, &owner, &[0]),
            "invalid_params",
        ),
        (
            "bad mode",
            arguments(&token, "face", Some("grow"), &owner, &[0]),
            "invalid_params",
        ),
        (
            "negative index",
            json!({"scene_token":token,"kind":"face","owner":owner,"indices":[-1]}),
            "invalid_params",
        ),
        (
            "stale token",
            arguments(&Uuid::nil().to_string(), "face", None, &owner, &[0]),
            "stale_scene",
        ),
    ];
    for (label, request, expected) in cases {
        let found = match call_response(
            &mut editor,
            &context,
            peer,
            "modify_component_selection",
            request,
        )
        .await
        {
            // Argument errors are protocol-level invalid_params errors, not tool results.
            Err(rmcp::ServiceError::McpError(error)) => {
                assert_eq!(
                    error.code,
                    rmcp::model::ErrorCode::INVALID_PARAMS,
                    "{label}"
                );
                "invalid_params".to_owned()
            }
            Err(other) => panic!("{label}: {other}"),
            Ok(result) => {
                assert_eq!(result.is_error, Some(true), "{label}");
                code(result)
            }
        };
        assert_eq!(found, expected, "{label}");
        assert_eq!(face_members(&editor), [1, 2], "{label}");
        assert_eq!(editor.history.scene(), &scene, "{label}");
    }
}

#[tokio::test]
async fn sdk_modify_component_selection_refuses_a_draft_and_a_modal() {
    let context = egui::Context::default();
    let (mut editor, client, _server) = start(&context).await;
    let peer = client.peer();
    let (token, owner) = cube_in_face_mode(&mut editor, &context, peer).await;
    let request = arguments(&token, "face", Some("add"), &owner, &[2]);
    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        peer,
        "modify_component_selection",
        request.clone(),
    )
    .await;
    assert_eq!(code(draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    editor.sync_fields();
    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        peer,
        "modify_component_selection",
        request,
    )
    .await;
    assert_eq!(code(modal), "busy_editing");
    assert!(face_members(&editor).is_empty());
}
