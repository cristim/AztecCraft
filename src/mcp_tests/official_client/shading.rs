use super::*;
use crate::editor_operations::ShadingMode;

async fn shading(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["shading"]
        .clone()
}

#[tokio::test]
async fn sdk_shading_mode_is_idempotent_session_only_and_guarded() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    assert_eq!(
        shading(&mut editor, &context, &client).await,
        "smooth_shade_all"
    );
    for (mode, outcome) in [
        ("smooth_shade_all", "unchanged"),
        ("wireframe", "changed"),
        ("wireframe", "unchanged"),
        ("smooth_shade_all", "changed"),
        ("wireframe", "changed"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_shading_mode",
            json!({"scene_token": token, "mode": mode}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(shading(&mut editor, &context, &client).await, mode);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
    }

    let wire = ShadingMode::Wireframe;
    let set = |token: &str| json!({"scene_token": token, "mode": "smooth_shade_all"});

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_shading_mode",
        set(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.shading, wire);

    for arguments in [
        json!({"scene_token": token}),
        json!({"mode": "wireframe"}),
        json!({"scene_token": token, "mode": "default_material"}),
        json!({"scene_token": token, "mode": "Wireframe"}),
        json!({"scene_token": token, "mode": null}),
        json!({"scene_token": token, "mode": "wireframe", "extra": true}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_shading_mode",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert_eq!(editor.shading, wire);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
    }

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_shading_mode",
        set(&token),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert_eq!(editor.shading, wire);
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_shading_mode",
        set(&token),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.shading, wire);
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// A cube extruded 200 times so a Wireframe occlusion check over its faces or vertices exceeds
/// the 1,000,000 intersection budget.
fn over_budget_editor(editor: &mut Editor) {
    editor.renderer = Some(crate::viewport_renderer::tests::renderer_handle());
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id();
    let node = editor.selected().unwrap();
    let path = vec![
        editor.history.scene().node(node).unwrap().transform_id(),
        shape,
    ];
    for _ in 0..200 {
        let target = azteccraft::native_scene::FaceTarget {
            transform: node,
            shape,
            path: path.clone(),
            face_index: 1,
        };
        editor
            .apply_operation(Operation::ExtrudeFace {
                target,
                distance: 0.01,
            })
            .unwrap();
    }
}

fn hidden_flags(editor: &mut Editor) -> [serde_json::Value; 2] {
    let inspection = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    ["face_centres_hidden", "vertex_markers_hidden"].map(|key| inspection[key].clone())
}

#[tokio::test]
async fn sdk_inspection_reports_face_centres_hidden_for_an_over_budget_wireframe_frame() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    over_budget_editor(&mut editor);
    editor.selection_mode = SelectionMode::Face;
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [false, false]);
    editor
        .apply_operation(Operation::SetShadingMode(ShadingMode::Wireframe))
        .unwrap();
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [true, false]);
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
    assert_eq!(inspected["inspection"]["face_centres_hidden"], true);
    assert_eq!(inspected["inspection"]["vertex_markers_hidden"], false);
    editor
        .apply_operation(Operation::SetShadingMode(ShadingMode::SmoothShadeAll))
        .unwrap();
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [false, false]);
    drop(client);
    let _ = server.await;
}

#[tokio::test]
async fn sdk_inspection_reports_vertex_markers_hidden_for_an_over_budget_wireframe_frame() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    over_budget_editor(&mut editor);
    editor.selection_mode = SelectionMode::Vertex;
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [false, false]);
    editor
        .apply_operation(Operation::SetShadingMode(ShadingMode::Wireframe))
        .unwrap();
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [false, true]);
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
    assert_eq!(inspected["inspection"]["face_centres_hidden"], false);
    assert_eq!(inspected["inspection"]["vertex_markers_hidden"], true);
    editor.selection_mode = SelectionMode::Face;
    frame(&mut editor, &context, vec![]);
    assert_eq!(hidden_flags(&mut editor), [true, false]);
    drop(client);
    let _ = server.await;
}
