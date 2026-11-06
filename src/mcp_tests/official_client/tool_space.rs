use super::*;
use crate::editor_operations::{ActiveTool, ManipulationSpace};

async fn spaces(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> [Value; 2] {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let manipulation = &inspected["inspection"]["manipulation"];
    [
        manipulation["rotate_space"].clone(),
        manipulation["scale_space"].clone(),
    ]
}

#[tokio::test]
async fn sdk_tool_space_reads_defaults_sets_per_tool_and_shares_the_native_operation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let tool = editor.active_tool;
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["object", "world"]
    );
    for (name, space, outcome, expected) in [
        ("scale", "object", "changed", ["object", "object"]),
        ("scale", "object", "unchanged", ["object", "object"]),
        ("rotate", "world", "changed", ["world", "object"]),
        ("scale", "world", "changed", ["world", "world"]),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_space",
            json!({"scene_token":token,"tool":name,"space":space}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(spaces(&mut editor, &context, &client).await, expected);
        assert_eq!(editor.active_tool, tool);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    // The native selector dispatches the same typed operation.
    editor.native_operation(Operation::SetToolSpace {
        tool: ActiveTool::Rotate,
        space: ManipulationSpace::Object,
    });
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["object", "world"]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_tool_space_rejects_invalid_values_and_honours_every_guard() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token":token,"tool":"move","space":"world"}),
        json!({"scene_token":token,"tool":"select","space":"world"}),
        json!({"scene_token":token,"tool":"rotate","space":"gimbal"}),
        json!({"scene_token":token,"tool":"rotate"}),
        json!({"scene_token":token,"space":"world"}),
        json!({"scene_token":token,"tool":"rotate","space":"world","extra":1}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_space",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    // The core operation also refuses a tool without an Axis Orientation, with a typed error.
    assert!(matches!(
        editor.apply_operation(Operation::SetToolSpace {
            tool: ActiveTool::Move,
            space: ManipulationSpace::Object,
        }),
        Err(crate::editor_operations::OperationError::InvalidInput(_))
    ));
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["object", "world"]
    );

    let set = |token: &str| json!({"scene_token":token,"tool":"scale","space":"object"});
    let code = |result: rmcp::model::CallToolResult| {
        result.structured_content.unwrap()["error"]["code"].clone()
    };
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(code(stale), "stale_scene");

    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"uniform","space":"world"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(busy), "busy_manipulation");
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(modal), "busy_editing");
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["object", "world"]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
