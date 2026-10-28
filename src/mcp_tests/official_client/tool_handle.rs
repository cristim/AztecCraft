use super::*;
use crate::editor_operations::ToolHandle;
use azteccraft::native_scene::{RotateAxis, ScaleHandle};

async fn handles(
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
        manipulation["rotate_axis"].clone(),
        manipulation["scale_handle"].clone(),
    ]
}

#[tokio::test]
async fn sdk_tool_handle_reads_defaults_sets_each_value_and_shares_the_native_operation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let tool = editor.active_tool;
    assert_eq!(
        handles(&mut editor, &context, &client).await,
        ["x", "uniform"]
    );
    for (arguments, outcome, expected) in [
        (
            json!({"tool":"rotate","axis":"y"}),
            "changed",
            ["y", "uniform"],
        ),
        (
            json!({"tool":"rotate","axis":"y"}),
            "unchanged",
            ["y", "uniform"],
        ),
        (
            json!({"tool":"rotate","axis":"z"}),
            "changed",
            ["z", "uniform"],
        ),
        (json!({"tool":"scale","handle":"x"}), "changed", ["z", "x"]),
        (json!({"tool":"scale","handle":"y"}), "changed", ["z", "y"]),
        (json!({"tool":"scale","handle":"z"}), "changed", ["z", "z"]),
        (
            json!({"tool":"scale","handle":"uniform"}),
            "changed",
            ["z", "uniform"],
        ),
        (
            json!({"tool":"rotate","axis":"x"}),
            "changed",
            ["x", "uniform"],
        ),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(token);
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_handle",
            arguments,
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(handles(&mut editor, &context, &client).await, expected);
        assert_eq!(editor.active_tool, tool);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    // The native selector dispatches the same typed operation.
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Y,
    )));
    editor.native_operation(Operation::SetToolHandle(ToolHandle::ScaleHandle(
        ScaleHandle::Z,
    )));
    assert_eq!(handles(&mut editor, &context, &client).await, ["y", "z"]);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_tool_handle_rejects_invalid_values_and_honours_every_guard() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token":token,"tool":"move","axis":"x"}),
        json!({"scene_token":token,"tool":"select","axis":"x"}),
        json!({"scene_token":token,"tool":"rotate","handle":"x"}),
        json!({"scene_token":token,"tool":"scale","axis":"x"}),
        json!({"scene_token":token,"tool":"rotate","axis":"w"}),
        json!({"scene_token":token,"tool":"rotate","axis":"uniform"}),
        json!({"scene_token":token,"tool":"scale","handle":"xy"}),
        json!({"scene_token":token,"tool":"rotate"}),
        json!({"scene_token":token,"axis":"x"}),
        json!({"scene_token":token,"tool":"rotate","axis":"x","extra":1}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_handle",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    let defaults = ["x", "uniform"];
    assert_eq!(handles(&mut editor, &context, &client).await, defaults);

    let set = |token: &str| json!({"scene_token":token,"tool":"rotate","axis":"z"});
    let code = |result: rmcp::model::CallToolResult| {
        result.structured_content.unwrap()["error"]["code"].clone()
    };
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_handle",
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
    // begin_manipulation names its own axis and leaves the persistent selection alone.
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"y","space":"world"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    assert_eq!(handles(&mut editor, &context, &client).await, defaults);
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_handle",
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
    assert_eq!(handles(&mut editor, &context, &client).await, defaults);

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_handle",
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
        "set_tool_handle",
        set(&token),
    )
    .await;
    assert_eq!(code(modal), "busy_editing");
    assert_eq!(handles(&mut editor, &context, &client).await, defaults);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
