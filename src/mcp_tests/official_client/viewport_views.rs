use super::*;

async fn camera(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["camera"]
        .clone()
}

#[tokio::test]
async fn sdk_viewport_view_is_idempotent_session_only_and_guarded() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    assert_eq!(
        camera(&mut editor, &context, &client).await["view"],
        "persp"
    );
    for (view, outcome, projection) in [
        ("persp", "unchanged", "perspective"),
        ("top", "changed", "orthographic"),
        ("top", "unchanged", "orthographic"),
        ("front", "changed", "orthographic"),
        ("side", "changed", "orthographic"),
        ("persp", "changed", "perspective"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_viewport_view",
            json!({"scene_token": token, "view": view}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        let state = camera(&mut editor, &context, &client).await;
        assert_eq!(
            (state["view"].as_str(), state["projection"].as_str()),
            (Some(view), Some(projection))
        );
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
    }

    // Orbit is refused in an orthographic view and the camera is preserved.
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token": token, "view": "front"}),
    )
    .await;
    let before = camera(&mut editor, &context, &client).await;
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "orbit_camera",
        json!({"scene_token": token, "radians": [0.1, 0.0]}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "invalid_input"
    );
    assert_eq!(camera(&mut editor, &context, &client).await, before);

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token": Uuid::new_v4().to_string(), "view": "top"}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    for arguments in [
        json!({"scene_token": token}),
        json!({"view": "top"}),
        json!({"scene_token": token, "view": "back"}),
        json!({"scene_token": token, "view": "Top"}),
        json!({"scene_token": token, "view": null}),
        json!({"scene_token": token, "view": "top", "extra": true}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_viewport_view",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert_eq!(camera(&mut editor, &context, &client).await, before);
    }

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token": token, "view": "top"}),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(camera(&mut editor, &context, &client).await, before);
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token": token, "view": "top"}),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
