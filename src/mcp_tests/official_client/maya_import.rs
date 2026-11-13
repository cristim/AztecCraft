use super::*;

async fn import(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call(
        editor,
        context,
        client.peer(),
        "import_maya_scene",
        arguments,
    )
    .await
    .structured_content
    .unwrap()
}

/// A `.ma` file saved by the real authored writer from one cube per name.
#[cfg(unix)]
fn saved_ma(directory: &tempfile::TempDir, file: &str, names: &[&str]) -> std::path::PathBuf {
    let path = directory.path().join(file);
    let mut seed = Editor::default();
    for name in names {
        seed.create(Primitive::Cube);
        let id = seed.selected().unwrap();
        seed.apply_operation(Operation::Rename {
            id,
            name: (*name).into(),
        })
        .unwrap();
    }
    seed.apply_operation(Operation::SaveSceneAs { path: path.clone() })
        .unwrap();
    path
}

#[cfg(unix)]
#[tokio::test]
async fn sdk_import_maya_scene_adds_prefixed_objects_in_one_undoable_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = saved_ma(&directory, "two.ma", &["wall", "floor"]);
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();

    let imported = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path, "prefix": "ns"}),
    )
    .await;

    assert_eq!(imported["outcome"], "imported", "{imported}");
    assert_eq!(imported["objects"], 2);
    assert_eq!(imported["vertices"], 16);
    assert_eq!(imported["faces"], 12);
    assert_eq!(imported["ignored_records"], 0);
    let ids = imported["node_ids"].as_array().unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(editor.scene_token.to_string(), token);
    let names: Vec<_> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|node| node.name().to_owned())
        .collect();
    assert_eq!(names, ["pCube1", "ns_wall", "ns_floor"]);
    assert_eq!(
        editor.selection.lead().map(|t| t.node.to_decimal()),
        ids.last().and_then(|id| id.as_str()).map(str::to_owned)
    );
    assert!(editor.status.starts_with("Imported 2 object(s)"));
    assert!(!editor.status.contains("not imported"), "{}", editor.status);

    // The scene token survives, so a second import works and names stay unique.
    let again = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": path, "prefix": "ns"}),
    )
    .await;
    assert_eq!(again["outcome"], "imported", "{again}");
    let names: Vec<_> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|node| node.name().to_owned())
        .collect();
    assert_eq!(names.len(), 5);
    assert!(names.contains(&"ns_wall1".to_owned()), "{names:?}");

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
async fn sdk_import_maya_scene_reports_each_refusal_with_its_code_and_changes_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();

    let binary = directory.path().join("scene.mb");
    std::fs::write(&binary, binary_fixture::specimen()).unwrap();
    let garbage = directory.path().join("garbage.ma");
    std::fs::write(&garbage, "this is not maya\n").unwrap();
    let huge = directory.path().join("huge.ma");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(azteccraft::document::MAX_DOCUMENT_BYTES as u64 + 1)
        .unwrap();
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut cases = vec![
        (
            binary,
            json!(null),
            "import_refused",
            Some("unrecognized_content"),
        ),
        (garbage, json!(null), "import_invalid", None),
        (huge, json!(null), "import_too_large", None),
        (
            directory.path().join("missing.ma"),
            json!(null),
            "import_io",
            None,
        ),
    ];
    #[cfg(unix)]
    {
        let wall = saved_ma(&directory, "wall.ma", &["wall"]);
        for prefix in ["1bad", "has space", "a:b", "é"] {
            cases.push((
                wall.clone(),
                json!(prefix),
                "import_refused",
                Some("invalid_prefix"),
            ));
        }
        let empty = directory.path().join("empty.ma");
        Editor::default()
            .apply_operation(Operation::SaveSceneAs {
                path: empty.clone(),
            })
            .unwrap();
        cases.push((empty, json!(null), "import_refused", Some("empty_scene")));
    }
    for (path, prefix, code, reason) in cases {
        let mut arguments = json!({"scene_token": token, "path": path});
        if !prefix.is_null() {
            arguments["prefix"] = prefix.clone();
        }
        let refused = import(&mut editor, &context, &client, arguments).await;
        assert_eq!(refused["error"]["code"], code, "{path:?}: {refused}");
        match reason {
            Some(reason) => assert_eq!(refused["error"]["reason"], reason, "{refused}"),
            None => assert!(refused["error"].get("reason").is_none(), "{refused}"),
        }
        let message = refused["error"]["message"].as_str().unwrap();
        let file = path.file_name().unwrap().to_str().unwrap();
        assert!(
            !message.contains(file) && !message.contains("tmp"),
            "{message}"
        );
        assert_eq!(editor.history.scene(), &before, "{path:?}");
    }

    let wrong = import(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": directory.path().join("scene.obj")}),
    )
    .await;
    assert_eq!(wrong["error"]["code"], "invalid_input");

    for arguments in [
        json!({"scene_token": token}),
        json!({"scene_token": token, "path": ""}),
        json!({"scene_token": token, "path": "a.ma", "prefix": ""}),
        json!({"scene_token": token, "path": "a.ma", "prefix": "x".repeat(65)}),
        json!({"scene_token": token, "path": "a.ma", "extra": true}),
        json!({"scene_token": token, "path": 5}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "import_maya_scene",
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

#[cfg(unix)]
#[tokio::test]
async fn sdk_import_maya_scene_is_busy_during_input_and_dialogs_and_refused_in_a_sourced_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = saved_ma(&directory, "wall.ma", &["wall"]);
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
    editor.name = before.nodes().first().unwrap().name().to_owned();

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

    let pending: Uuid = editor
        .files
        .pending_inspection()
        .unwrap()
        .id
        .parse()
        .unwrap();
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: pending,
            decision: crate::editor_operations::FileDecision::Cancel,
        })
        .unwrap();

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
        json!({"scene_token": token, "path": path}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "scene_error", "{refused}");
    assert_eq!(editor.history.scene(), &nodes);

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
