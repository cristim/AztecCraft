use super::*;

#[tokio::test]
async fn sdk_recovery_is_off_by_default_and_refuses_without_writing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let status = call(
        &mut editor,
        &context,
        client.peer(),
        "get_recovery_status",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(
        status,
        json!({"outcome": "recovery_status", "enabled": false, "max_backups": null, "files": 0})
    );
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "autosave_now",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(refused["error"]["code"], "recovery_disabled");
    for arguments in [json!({"extra": 1}), json!({"scene_token": 5})] {
        let name = if arguments.get("extra").is_some() {
            "get_recovery_status"
        } else {
            "autosave_now"
        };
        assert!(
            call_response(&mut editor, &context, client.peer(), name, arguments)
                .await
                .is_err()
        );
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn sdk_autosave_writes_a_separate_file_and_honors_the_guards() {
    let tmp = tempfile::tempdir().unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.enable_recovery(tmp.path().join("recovery")).unwrap();
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let baseline = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let scene = editor.history.scene().clone();
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "autosave_now",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(saved["outcome"], "autosaved");
    assert_eq!(saved["sequence"], 1);
    assert!(saved["bytes"].as_u64().unwrap() > 0);
    assert!(saved.get("path").is_none());
    let status = call(
        &mut editor,
        &context,
        client.peer(),
        "get_recovery_status",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(status["enabled"], true);
    assert_eq!(status["files"], 1);
    assert!(!status.to_string().contains(tmp.path().to_str().unwrap()));
    assert_eq!(
        serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
        baseline
    );
    assert_eq!(editor.history.scene(), &scene);

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "autosave_now",
        json!({"scene_token": Uuid::new_v4().to_string()}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(stale["error"]["code"], "stale_scene");
    editor.name = "pending draft".into();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "autosave_now",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(busy["error"]["code"], "busy_editing");
    assert_eq!(editor.name, "pending draft");
    let count = std::fs::read_dir(tmp.path().join("recovery"))
        .unwrap()
        .count();
    assert_eq!(count, 1, "refused calls wrote nothing");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
