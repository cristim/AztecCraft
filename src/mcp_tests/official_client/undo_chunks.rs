use super::*;

async fn undo_state(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> serde_json::Value {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    inspected["inspection"]["undo_state"].clone()
}

async fn run(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    tool: &'static str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let mut arguments = arguments;
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), tool, arguments)
        .await
        .structured_content
        .unwrap()
}

async fn cube(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) {
    let reply = run(
        editor,
        context,
        client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    assert!(reply.get("error").is_none(), "{reply}");
}

#[tokio::test]
async fn sdk_undo_chunk_groups_several_tool_calls_into_one_named_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let before = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        before,
        json!({"enabled":true,"infinite":false,"queue_size":50,"undo_length":0,"redo_length":0,"open_chunk":null})
    );
    let opened = run(
        &mut editor,
        &context,
        &client,
        "open_undo_chunk",
        json!({"name":"Build"}),
    )
    .await;
    assert_eq!(opened["outcome"], "changed");
    cube(&mut editor, &context, &client).await;
    cube(&mut editor, &context, &client).await;
    let open = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (open["undo_length"].clone(), open["open_chunk"].clone()),
        (json!(0), json!("Build"))
    );
    let again = run(&mut editor, &context, &client, "open_undo_chunk", json!({})).await;
    assert_eq!(again["error"]["code"], "undo_state");
    let closed = run(
        &mut editor,
        &context,
        &client,
        "close_undo_chunk",
        json!({}),
    )
    .await;
    assert_eq!(closed["outcome"], "changed");
    let after = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (after["undo_length"].clone(), after["open_chunk"].clone()),
        (json!(1), json!(null))
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Build"));
    run(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene().nodes().len(), 0);
    let nothing = run(
        &mut editor,
        &context,
        &client,
        "close_undo_chunk",
        json!({}),
    )
    .await;
    assert_eq!(nothing["error"]["code"], "undo_state");
    run(&mut editor, &context, &client, "open_undo_chunk", json!({})).await;
    let empty = run(
        &mut editor,
        &context,
        &client,
        "close_undo_chunk",
        json!({}),
    )
    .await;
    assert_eq!(empty["outcome"], "unchanged");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_native_undo_closes_an_abandoned_chunk() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    run(&mut editor, &context, &client, "open_undo_chunk", json!({})).await;
    cube(&mut editor, &context, &client).await;
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.history.scene().nodes().len(), 0);
    assert_eq!(editor.history.open_chunk_name(), None);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_undo_queue_and_enabled_settings_read_back_and_survive_new_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for _ in 0..4 {
        cube(&mut editor, &context, &client).await;
    }
    let set = run(
        &mut editor,
        &context,
        &client,
        "set_undo_queue",
        json!({"queue":"finite","size":2}),
    )
    .await;
    assert_eq!(set["outcome"], "changed");
    let state = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (state["queue_size"].clone(), state["undo_length"].clone()),
        (json!(2), json!(2))
    );
    let same = run(
        &mut editor,
        &context,
        &client,
        "set_undo_queue",
        json!({"queue":"finite","size":2}),
    )
    .await;
    assert_eq!(same["outcome"], "unchanged");
    run(
        &mut editor,
        &context,
        &client,
        "set_undo_queue",
        json!({"queue":"infinite"}),
    )
    .await;
    let state = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (state["infinite"].clone(), state["queue_size"].clone()),
        (json!(true), json!(null))
    );
    let off = run(
        &mut editor,
        &context,
        &client,
        "set_undo_enabled",
        json!({"enabled":false}),
    )
    .await;
    assert_eq!(off["outcome"], "changed");
    cube(&mut editor, &context, &client).await;
    let state = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (state["enabled"].clone(), state["undo_length"].clone()),
        (json!(false), json!(0))
    );
    let undone = run(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(undone["outcome"], "unchanged");
    assert_eq!(editor.history.scene().nodes().len(), 5);
    run(&mut editor, &context, &client, "open_undo_chunk", json!({})).await;
    let refused = run(
        &mut editor,
        &context,
        &client,
        "set_undo_queue",
        json!({"queue":"infinite"}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "undo_state");
    let refused = run(
        &mut editor,
        &context,
        &client,
        "set_undo_enabled",
        json!({"enabled":true}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "undo_state");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_undo_settings_survive_new_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    run(
        &mut editor,
        &context,
        &client,
        "set_undo_queue",
        json!({"queue":"infinite"}),
    )
    .await;
    run(
        &mut editor,
        &context,
        &client,
        "set_undo_enabled",
        json!({"enabled":false}),
    )
    .await;
    editor.native_operation(Operation::NewScene);
    let state = undo_state(&mut editor, &context, &client).await;
    assert_eq!(
        (state["enabled"].clone(), state["infinite"].clone()),
        (json!(false), json!(true))
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_undo_tools_reject_invalid_arguments_and_stale_tokens() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    for (tool, arguments) in [
        (
            "set_undo_queue",
            json!({"scene_token":token,"queue":"finite"}),
        ),
        (
            "set_undo_queue",
            json!({"scene_token":token,"queue":"finite","size":0}),
        ),
        (
            "set_undo_queue",
            json!({"scene_token":token,"queue":"finite","size":1001}),
        ),
        (
            "set_undo_queue",
            json!({"scene_token":token,"queue":"infinite","size":5}),
        ),
        (
            "set_undo_queue",
            json!({"scene_token":token,"queue":"bounded","size":5}),
        ),
        ("set_undo_queue", json!({"queue":"infinite"})),
        ("set_undo_enabled", json!({"scene_token":token})),
        (
            "set_undo_enabled",
            json!({"scene_token":token,"enabled":"yes"}),
        ),
        (
            "open_undo_chunk",
            json!({"scene_token":token,"name":"x".repeat(65)}),
        ),
        (
            "open_undo_chunk",
            json!({"scene_token":token,"name":"a\nb"}),
        ),
        ("open_undo_chunk", json!({"scene_token":token,"extra":1})),
        ("close_undo_chunk", json!({})),
    ] {
        let rejected = call_response(&mut editor, &context, client.peer(), tool, arguments).await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("{tool}: expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    let stale = Uuid::new_v4().to_string();
    for (tool, arguments) in [
        (
            "set_undo_queue",
            json!({"scene_token":stale,"queue":"infinite"}),
        ),
        (
            "set_undo_enabled",
            json!({"scene_token":stale,"enabled":false}),
        ),
        ("open_undo_chunk", json!({"scene_token":stale})),
        ("close_undo_chunk", json!({"scene_token":stale})),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, arguments).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "stale_scene"
        );
    }
    let state = undo_state(&mut editor, &context, &client).await;
    assert_eq!(state["queue_size"], 50);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
