use super::*;
use crate::editor_operations::SidebarPanel;

async fn channel_box(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> (bool, String) {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    (
        inspected["inspection"]["sidebar_panels"]["channel_box"]
            .as_bool()
            .unwrap(),
        inspected["inspection"]["scene_token"]
            .as_str()
            .unwrap()
            .to_owned(),
    )
}

#[tokio::test]
async fn sdk_sidebar_panel_visibility_is_idempotent_and_guarded() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    assert_eq!(
        channel_box(&mut editor, &context, &client).await,
        (true, token.clone())
    );
    for (visible, outcome) in [
        (false, "changed"),
        (false, "unchanged"),
        (true, "changed"),
        (true, "unchanged"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_sidebar_panel_visibility",
            json!({"scene_token": token, "panel": "channel_box", "visible": visible}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(
            channel_box(&mut editor, &context, &client).await,
            (visible, token.clone())
        );
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
    }
    let hide =
        |token: &str| json!({"scene_token": token, "panel": "channel_box", "visible": false});

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        hide(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));

    for panel in ["tool_settings", "ChannelBox", ""] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_sidebar_panel_visibility",
            json!({"scene_token": token, "panel": panel, "visible": false}),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {panel:?}");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{panel}"
        );
        assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
    }

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        hide(&token),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        hide(&token),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_sidebar_panel_visibility_is_busy_during_an_active_move() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token": token, "primitive": "cube"}),
    )
    .await
    .structured_content
    .unwrap();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": token, "node_id": created["node_id"], "instance_path": path, "kind": "move", "axis": "x"}),
    )
    .await;
    let preview = editor.history.scene().clone();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        json!({"scene_token": token, "panel": "channel_box", "visible": false}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_manipulation"
    );
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert_eq!(editor.history.scene(), &preview);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn status_message(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> serde_json::Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["status_message"]
        .clone()
}

#[tokio::test]
async fn sdk_inspection_mirrors_native_and_remote_status_text() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    assert_eq!(
        status_message(&mut editor, &context, &client).await,
        json!(editor.status)
    );
    editor.create(Primitive::Cube);
    let native = status_message(&mut editor, &context, &client).await;
    assert_eq!(native, json!(editor.status));
    assert!(
        editor.status.starts_with("Scene edited"),
        "{}",
        editor.status
    );

    let token = editor.scene_token.to_string();
    editor.status = "sentinel".into();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token": token, "primitive": "sphere"}),
    )
    .await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "created");
    let remote = status_message(&mut editor, &context, &client).await;
    assert_eq!(remote, json!(editor.status));
    // Remote operations do not set status text (#27); the field reports whatever the Editor holds.

    editor.status.clear();
    assert!(
        status_message(&mut editor, &context, &client)
            .await
            .is_null()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_inspection_withholds_file_error_status_and_bounds_length() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let private = "open /private/scene.mb: permission denied";
    editor.set_error_status(&OperationError::File(private.into()));
    assert_eq!(
        editor.status, private,
        "the native line keeps the full text"
    );
    let withheld = status_message(&mut editor, &context, &client).await;
    assert_eq!(
        withheld,
        json!(crate::editor_operations::STATUS_MESSAGE_WITHHELD)
    );
    assert!(!withheld.to_string().contains("private"));

    editor.status = private.into();
    editor.status_withholds_path = None;
    editor.set_error_status(&OperationError::NoSelection);
    assert_eq!(
        status_message(&mut editor, &context, &client).await,
        json!(OperationError::NoSelection.to_string())
    );

    editor.status = "x".repeat(10_000);
    let long = status_message(&mut editor, &context, &client).await;
    assert_eq!(
        long.as_str().unwrap().chars().count(),
        crate::editor_operations::STATUS_MESSAGE_MAX_CHARS
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
