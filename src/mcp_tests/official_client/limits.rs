use super::*;

#[tokio::test]
async fn sdk_inspect_limits_serves_the_core_registry_without_a_token_or_state_change() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.name = "temporary".into();
    assert!(editor.commit_pending());
    frame(&mut editor, &context, vec![]);
    let baseline = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let scene = editor.history.scene().clone();

    let reply = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_limits",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(reply["outcome"], "scene_limits");
    let expected = serde_json::to_value(azteccraft::scene_limits::scene_limits()).unwrap();
    assert_eq!(reply["limits"], expected);
    let rows = reply["limits"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["name"] == "instance_paths" && row["value"] == 4096 && row["unit"] == "count"
    }));
    assert!(
        rows.iter()
            .any(|row| row["name"] == "view_bytes" && row["unit"] == "bytes")
    );

    for arguments in [json!({"scene_token": "x"}), json!({"extra": 1})] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "inspect_limits",
                arguments
            )
            .await
            .is_err()
        );
    }
    assert_eq!(
        serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
        baseline
    );
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
