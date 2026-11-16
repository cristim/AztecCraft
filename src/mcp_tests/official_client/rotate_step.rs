use super::*;

#[tokio::test]
async fn sdk_rotate_step_snaps_the_remote_rotate_and_honours_validation_and_guards() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    for (degrees, outcome) in [
        (json!(15), "changed"),
        (json!(15), "unchanged"),
        (json!(null), "changed"),
        (json!(15), "changed"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_rotate_step",
            json!({"scene_token":token,"degrees":degrees}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
    }
    assert_eq!(editor.history.scene(), &scene);

    for arguments in [
        json!({"scene_token":token}),
        json!({"scene_token":token,"degrees":0}),
        json!({"scene_token":token,"degrees":-5}),
        json!({"scene_token":token,"degrees":1e-7}),
        json!({"scene_token":token,"degrees":"15"}),
        json!({"scene_token":token,"degrees":15,"extra":1}),
        json!({"degrees":15}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_rotate_step",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }

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
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"z","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_rotate_step",
        json!({"scene_token":token,"degrees":null}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_manipulation"
    );
    let updated = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":37}),
    )
    .await;
    assert!(!updated.is_error.unwrap_or(false));
    let node = editor.history.scene().nodes()[0].id();
    let turned = editor
        .history
        .scene()
        .internal_transform(node)
        .unwrap()
        .rotation_radians[2];
    assert!((turned - 30f64.to_radians()).abs() < 1e-9, "{turned}");
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
