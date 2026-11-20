use super::*;

#[tokio::test]
async fn sdk_scale_step_snaps_the_remote_scale_and_honours_validation_and_guards() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    for (step, outcome) in [
        (json!(0.25), "changed"),
        (json!(0.25), "unchanged"),
        (json!(null), "changed"),
        (json!(0.25), "changed"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_scale_step",
            json!({"scene_token":token,"step":step}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
    }
    assert_eq!(editor.history.scene(), &scene);
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
    assert_eq!(inspected["inspection"]["manipulation"]["scale_step"], 0.25);
    assert_eq!(
        inspected["inspection"]["manipulation"]["rotate_step_degrees"],
        json!(null)
    );

    for arguments in [
        json!({"scene_token":token}),
        json!({"scene_token":token,"step":0}),
        json!({"scene_token":token,"step":-5}),
        json!({"scene_token":token,"step":1e-7}),
        json!({"scene_token":token,"step":"15"}),
        json!({"scene_token":token,"step":15,"extra":1}),
        json!({"step":15}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_scale_step",
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
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"uniform","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_scale_step",
        json!({"scene_token":token,"step":null}),
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
        json!({"scene_token":token,"manipulation_id":id,"factor":1.8}),
    )
    .await;
    assert!(!updated.is_error.unwrap_or(false));
    let node = editor.history.scene().nodes()[0].id();
    let scaled = editor.history.scene().node(node).unwrap().transform().scale;
    assert!(scaled.iter().all(|v| (v - 1.75).abs() < 1e-9), "{scaled:?}");
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
