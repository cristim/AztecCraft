use super::*;

async fn export(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call(editor, context, client.peer(), "export_obj", arguments)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn sdk_export_obj_writes_new_files_and_never_changes_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let all = directory.path().join("all.obj");
    let token = editor.scene_token.to_string();
    let empty = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": all}),
    )
    .await;
    assert_eq!(empty["error"]["code"], "nothing_to_export");
    assert!(!all.exists());

    editor.create(Primitive::Cube);
    editor.create(Primitive::Plane);
    let plane = editor.selected().unwrap();
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let can_undo = editor.history.can_undo();
    let exported = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": all}),
    )
    .await;
    assert_eq!(exported["outcome"], "exported");
    assert_eq!(exported["objects"], 2);
    let text = std::fs::read_to_string(&all).unwrap();
    assert_eq!(
        text.lines().filter(|line| line.starts_with("o ")).count(),
        2
    );
    assert_eq!(
        exported["vertices"],
        text.lines().filter(|line| line.starts_with("v ")).count()
    );
    assert_eq!(
        exported["faces"],
        text.lines().filter(|line| line.starts_with("f ")).count()
    );
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), can_undo);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.selected(), Some(plane));

    let again = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": all}),
    )
    .await;
    assert_eq!(again["error"]["code"], "destination_exists");
    assert_eq!(std::fs::read_to_string(&all).unwrap(), text);

    let selection = directory.path().join("selection.OBJ");
    let selected = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": selection, "selection_only": true}),
    )
    .await;
    assert_eq!(selected["outcome"], "exported");
    assert_eq!(selected["objects"], 1);
    let name = editor
        .history
        .scene()
        .node(plane)
        .unwrap()
        .name()
        .to_string();
    assert!(
        std::fs::read_to_string(&selection)
            .unwrap()
            .contains(&format!("o {name}\n"))
    );

    let objects = editor.history.scene().transform_paths().unwrap().len();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_by_rule",
        json!({"scene_token": token, "rule": "all"}),
    )
    .await;
    let every = directory.path().join("every.obj");
    let selected = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": every, "selection_only": true}),
    )
    .await;
    assert_eq!(selected["objects"], objects);

    editor
        .apply_operation(Operation::Select {
            node: None,
            instance_path: None,
        })
        .unwrap();
    let unselected = directory.path().join("unselected.obj");
    let refused = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": unselected, "selection_only": true}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "no_selection");
    assert!(!unselected.exists());

    let wrong = directory.path().join("scene.mb");
    let refused = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": wrong}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "invalid_input");
    assert!(!wrong.exists());

    let missing = directory.path().join("missing").join("x.obj");
    let refused = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": missing}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "export_io");
    assert!(
        !refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("missing")
    );

    for arguments in [
        json!({"scene_token": token}),
        json!({"scene_token": token, "path": ""}),
        json!({"scene_token": token, "path": "a.obj", "extra": true}),
        json!({"scene_token": token, "path": "a.obj", "selection_only": "yes"}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "export_obj",
                arguments
            )
            .await
            .is_err()
        );
    }
    let fresh = directory.path().join("fresh.obj");
    let stale = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": Uuid::new_v4().to_string(), "path": fresh}),
    )
    .await;
    assert_eq!(stale["error"]["code"], "stale_scene");
    editor
        .apply_operation(Operation::Select {
            node: Some(plane),
            instance_path: None,
        })
        .unwrap();
    editor.name = "typing".into();
    editor.native_input_pending = true;
    let busy = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": fresh}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing", "{busy}");
    assert!(!fresh.exists());
    editor.native_input_pending = false;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
