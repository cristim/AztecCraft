use super::*;
use crate::editor_operations::ActiveTool;
use azteccraft::move_handles::ManipulatorSize;

async fn size(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> f64 {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    inspected["inspection"]["manipulation"]["manipulator_size"]
        .as_f64()
        .unwrap()
}

fn plus() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Plus,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[tokio::test]
async fn sdk_manipulator_size_reads_the_default_sets_values_and_shares_the_native_operation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let tool = editor.active_tool;
    assert_eq!(size(&mut editor, &context, &client).await, 1.0);
    for (value, outcome, expected) in [
        (0.1, "changed", 0.1),
        (0.1, "unchanged", 0.1),
        (10.0, "changed", 10.0),
        (2.5, "changed", 2.5),
        (2.5555, "changed", 2.556),
        (2.556, "unchanged", 2.556),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_manipulator_size",
            json!({"scene_token":token,"size":value}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(
            size(&mut editor, &context, &client).await.to_bits(),
            f64::to_bits(expected)
        );
        assert_eq!(editor.active_tool, tool);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    // A native hotkey step reads back, and an MCP set feeds the next hotkey step, which moves to the adjacent 1.25^k grid value.
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Move));
    editor.native_operation(Operation::SetManipulatorSize(
        ManipulatorSize::new(2.0).unwrap(),
    ));
    frame(&mut editor, &context, vec![plus()]);
    assert_eq!(size(&mut editor, &context, &client).await, 1.25_f64.powi(4));
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_manipulator_size",
        json!({"scene_token":token,"size":4.0}),
    )
    .await;
    frame(&mut editor, &context, vec![plus()]);
    assert_eq!(size(&mut editor, &context, &client).await, 1.25_f64.powi(7));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_manipulator_size_rejects_invalid_values_and_honours_every_guard() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token":token,"size":"1"}),
        json!({"scene_token":token,"size":"NaN"}),
        json!({"scene_token":token,"size":null}),
        json!({"scene_token":token,"size":0.0999}),
        json!({"scene_token":token,"size":10.0001}),
        json!({"scene_token":token,"size":-1}),
        json!({"scene_token":token}),
        json!({"scene_token":token,"size":1,"extra":1}),
        json!({"size":1}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_manipulator_size",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    assert_eq!(size(&mut editor, &context, &client).await, 1.0);

    let set = |token: &str| json!({"scene_token":token,"size":3.0});
    let code = |result: rmcp::model::CallToolResult| {
        result.structured_content.unwrap()["error"]["code"].clone()
    };
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_manipulator_size",
        set(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(code(stale), "stale_scene");

    let node = editor.selected.unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path
        .iter()
        .map(|id| id.to_decimal())
        .collect();
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
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_manipulator_size",
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
    assert_eq!(size(&mut editor, &context, &client).await, 1.0);

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_manipulator_size",
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
        "set_manipulator_size",
        set(&token),
    )
    .await;
    assert_eq!(code(modal), "busy_editing");
    assert_eq!(size(&mut editor, &context, &client).await, 1.0);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
