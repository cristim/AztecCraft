use super::*;

const TWO_OBJECTS: &str =
    "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\nvn 0 0 1\no first\nf 1 2 3\no second\nf 2 4 3\n";

async fn import(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call(editor, context, client.peer(), "import_obj", arguments)
        .await
        .structured_content
        .unwrap()
}

fn write(directory: &tempfile::TempDir, name: &str, text: &str) -> std::path::PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, text).unwrap();
    path
}

#[tokio::test]
async fn sdk_import_obj_adds_objects_in_one_undoable_step_and_keeps_the_scene_token() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = write(&directory, "two.obj", TWO_OBJECTS);
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();

    let imported = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path}),
    )
    .await;

    assert_eq!(imported["outcome"], "imported");
    assert_eq!(imported["objects"], 2);
    assert_eq!(imported["vertices"], 6);
    assert_eq!(imported["faces"], 2);
    assert_eq!(imported["ignored_records"], 1);
    let ids = imported["node_ids"].as_array().unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.history.scene().nodes().len(), 3);
    assert_eq!(
        editor.selection.lead().map(|t| t.node.to_decimal()),
        ids.last().and_then(|id| id.as_str()).map(str::to_owned)
    );

    // The inspection lists the new nodes by name.
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap()
    .to_string();
    assert!(inspected.contains("\"first\"") && inspected.contains("\"second\""));

    // The old token is still valid: a second import works and names stay unique.
    let again = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path}),
    )
    .await;
    assert_eq!(again["outcome"], "imported");
    let names: Vec<_> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|n| n.name().to_owned())
        .collect();
    assert!(names.contains(&"first1".into()) && names.contains(&"second1".into()));

    for _ in 0..2 {
        let undone = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token": token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(undone["outcome"], "changed");
    }
    assert_eq!(editor.history.scene(), &before);

    let stale = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": Uuid::new_v4().to_string(), "path": path}),
    )
    .await;
    assert_eq!(stale["error"]["code"], "stale_scene");
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_import_obj_reports_each_refusal_with_its_code_and_changes_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();

    let objects = (0..=azteccraft::obj::import::MAX_OBJECTS)
        .fold(String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\n"), |text, i| {
            format!("{text}o n{i}\nf 1 2 3\n")
        });
    let mut too_large = String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
    while (too_large.len() as u64) <= azteccraft::obj::import::MAX_FILE_BYTES {
        too_large.push_str(&"#".repeat(1022));
        too_large.push_str("\r\n");
    }
    let cases = [
        (
            "surf.obj",
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\nsurf 1\n",
            "import_invalid",
        ),
        ("empty.obj", "# nothing\n", "import_invalid"),
        (
            "cr.obj",
            "v 0 0 0\rv 1 0 0\rv 0 1 0\rf 1 2 3\r",
            "import_invalid",
        ),
        (
            "concave.obj",
            "v 0 0 0\nv 4 0 0\nv 4 4 0\nv 2 1 0\nv 0 4 0\nf 1 2 3 4 5\n",
            "import_invalid",
        ),
        ("many.obj", objects.as_str(), "import_limit"),
        ("huge.obj", too_large.as_str(), "import_too_large"),
    ];
    for (name, text, code) in cases {
        let path = write(&directory, name, text);
        let refused = import(
            &mut editor,
            &context,
            &client,
            json!({"scene_token": token, "path": path}),
        )
        .await;
        assert_eq!(refused["error"]["code"], code, "{name}: {refused}");
        let message = refused["error"]["message"].as_str().unwrap();
        assert!(
            !message.contains(name) && !message.contains("tmp"),
            "{message}"
        );
        assert_eq!(editor.history.scene(), &before, "{name}");
    }
    let concave = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": directory.path().join("concave.obj")}),
    )
    .await;
    assert!(
        concave["error"]["message"]
            .as_str()
            .unwrap()
            .contains("triangulate")
    );

    let missing = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": directory.path().join("missing.obj")}),
    )
    .await;
    assert_eq!(missing["error"]["code"], "import_io");
    let wrong = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": directory.path().join("scene.mb")}),
    )
    .await;
    assert_eq!(wrong["error"]["code"], "invalid_input");

    for arguments in [
        json!({"scene_token": token}),
        json!({"scene_token": token, "path": ""}),
        json!({"scene_token": token, "path": "a.obj", "extra": true}),
        json!({"scene_token": token, "path": 5}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "import_obj",
                arguments
            )
            .await
            .is_err()
        );
    }
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_import_obj_is_busy_during_drafts_input_and_dialogs() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = write(&directory, "two.obj", TWO_OBJECTS);
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();

    editor.name = "typing".into();
    editor.native_input_pending = true;
    let busy = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing");
    editor.native_input_pending = false;
    editor.name = editor
        .history
        .scene()
        .nodes()
        .first()
        .unwrap()
        .name()
        .to_owned();

    assert!(matches!(
        editor.apply_operation(Operation::NewScene),
        Ok(OperationReply::PendingConfirmation { .. })
    ));
    let busy = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing");
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_import_obj_after_export_and_into_an_opened_binary_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let exported = directory.path().join("cube.obj");
    let reply = call(
        &mut editor,
        &context,
        client.peer(),
        "export_obj",
        json!({"scene_token": token, "path": exported}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(reply["outcome"], "exported");
    let imported = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": exported}),
    )
    .await;
    assert_eq!(imported["objects"], 1);
    assert_eq!(imported["vertices"], 8);
    assert_eq!(imported["faces"], 6);
    assert_eq!(editor.history.scene().nodes().len(), 2);

    // A scene opened from .mb keeps its source bytes and refuses structural edits.
    let source = directory.path().join("original.mb");
    std::fs::write(&source, binary_fixture::specimen()).unwrap();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": token, "path": source}),
    )
    .await
    .structured_content
    .unwrap();
    if opened["outcome"] == "pending_confirmation" {
        let id = opened["pending_action_id"].as_str().unwrap().to_owned();
        call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token": token, "pending_action_id": id, "decision": "discard"}),
        )
        .await;
    }
    let token = editor.scene_token.to_string();
    let nodes = editor.history.scene().clone();
    let refused = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": exported}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "scene_error", "{refused}");
    assert_eq!(editor.history.scene(), &nodes);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
