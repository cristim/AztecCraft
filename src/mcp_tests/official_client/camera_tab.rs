//! #458: `select_attribute_editor_tab` with kind `camera` over the real client and Editor.
use super::image_plane::{code, outcome};
use super::*;

#[tokio::test]
async fn sdk_the_camera_tab_selects_without_a_selection_and_is_reported_by_inspect_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    // Hidden Attribute Editor: refused, nothing changes.
    let select = |token: String, tab: Value| json!({"scene_token": token, "tab": tab});
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "select_attribute_editor_tab",
        select(token.clone(), json!({"kind": "camera"})),
    )
    .await;
    assert_eq!(code(&refused), "invalid_input");
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        json!({"scene_token": token, "panel": "attribute_editor", "visible": true}),
    )
    .await;
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    // Nothing selected, no plane: the camera tab still selects, and twice is a no-op.
    for expected in ["changed", "unchanged"] {
        let chosen = call(
            &mut editor,
            &context,
            client.peer(),
            "select_attribute_editor_tab",
            select(token.clone(), json!({"kind": "camera"})),
        )
        .await;
        assert_eq!(outcome(&chosen), expected);
    }
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
        json!({"kind": "camera", "id": null})
    );
    assert_eq!((editor.is_dirty(), editor.history.scene()), (dirty, &scene));
    // An id is invalid_params.
    let rejected = call_response(
        &mut editor,
        &context,
        client.peer(),
        "select_attribute_editor_tab",
        select(token, json!({"kind": "camera", "id": "7"})),
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
