//! #459 P9b-1: `select_attribute_editor_tab` with kind `image_plane` over the real client and Editor.
use super::image_plane::{code, message, outcome, set, write_png};
use super::*;

async fn select(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    tab: Value,
) -> rmcp::model::CallToolResult {
    let token = editor.scene_token.to_string();
    call(
        editor,
        context,
        client.peer(),
        "select_attribute_editor_tab",
        json!({"scene_token": token, "tab": tab}),
    )
    .await
}

#[tokio::test]
async fn sdk_the_image_plane_tab_is_refused_without_a_plane_and_selected_with_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "plate.png", 4, 4);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let text = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        json!({"scene_token": text, "panel": "attribute_editor", "visible": true}),
    )
    .await;
    let (tab, dirty, scene, selection) = (
        editor.attribute_editor_tab,
        editor.is_dirty(),
        editor.history.scene().clone(),
        editor.selected(),
    );

    // No plane: refused with the tab's own reason, and nothing changes.
    let refused = select(
        &mut editor,
        &context,
        &client,
        json!({"kind": "image_plane"}),
    )
    .await;
    assert_eq!(code(&refused), "invalid_input");
    assert!(
        message(&refused).contains("needs an image plane"),
        "{}",
        message(&refused)
    );
    assert_eq!(editor.attribute_editor_tab, tab);
    assert_eq!((editor.is_dirty(), editor.selected()), (dirty, selection));
    assert_eq!(editor.history.scene(), &scene);

    // With a plane the tab is selected and inspect_scene reports it.
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": path.to_str().unwrap()}),
    )
    .await;
    let chosen = select(
        &mut editor,
        &context,
        &client,
        json!({"kind": "image_plane"}),
    )
    .await;
    assert_eq!(outcome(&chosen), "changed");
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(
        inspected["inspection"]["attribute_editor_tab"],
        json!({"kind": "image_plane", "id": null})
    );
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);

    // An id is refused as invalid_params, whatever the plane state.
    let rejected = call_response(
        &mut editor,
        &context,
        client.peer(),
        "select_attribute_editor_tab",
        json!({"scene_token": text, "tab": {"kind": "image_plane", "id": "7"}}),
    )
    .await;
    let Err(rmcp::ServiceError::McpError(error)) = rejected else {
        panic!("expected a JSON-RPC error");
    };
    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
