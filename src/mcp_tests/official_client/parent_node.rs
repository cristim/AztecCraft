use super::*;

async fn cube(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &str,
) -> String {
    let token = editor.scene_token.to_string();
    let created = call(
        editor,
        context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","name":name}),
    )
    .await;
    created.structured_content.unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn parent(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    args: serde_json::Value,
) -> serde_json::Value {
    call(editor, context, client.peer(), "parent_node", args)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn sdk_parent_node_keeps_world_undoes_follows_selection_and_refuses_bad_requests() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let a = cube(&mut editor, &context, &client, "groupA").await;
    let b = cube(&mut editor, &context, &client, "childB").await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":a,"translation":[10,0,0],"rotation_degrees":[0,0,90],"scale":[2,2,2]}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":b,"translation":[1,0,0]}),
    )
    .await;
    let flat = editor.history.scene().clone();
    // childB is selected (the last created); its path changes with the parent.
    let reply = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"parent_id":a}),
    )
    .await;
    assert_eq!(reply["outcome"], "changed", "{reply}");
    let node = editor
        .history
        .scene()
        .node(NodeId::from_decimal(&b).unwrap())
        .unwrap()
        .transform();
    // World (1, 0, 0) under translate (10, 0, 0), Rz(90), scale 2 is (0, 4.5, 0) with Rz(-90) and scale 0.5.
    for (got, want) in node
        .translation
        .iter()
        .chain(&node.rotation)
        .chain(&node.scale)
        .zip([0.0, 4.5, 0.0, 0.0, 0.0, -90.0, 0.5, 0.5, 0.5])
    {
        assert!((got - want).abs() < 1e-9, "{node:?}");
    }
    let paths = editor.history.scene().transform_paths().unwrap();
    let moved = paths.iter().find(|t| t.node.to_decimal() == b).unwrap();
    assert_eq!(moved.path.len(), 2);
    assert_eq!(editor.selection.members(), std::slice::from_ref(moved));
    assert_eq!(editor.history.undo_label().as_deref(), Some("Parent"));

    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &flat);
    assert_eq!(editor.selection.members().len(), 1);
    assert_eq!(editor.selection.members()[0].path.len(), 1);
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.selection.members()[0].path.len(), 2);

    // Same parent is unchanged; unparent restores the original channels.
    let same = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"parent_id":a}),
    )
    .await;
    assert_eq!(same["outcome"], "unchanged");
    let world = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"world":true}),
    )
    .await;
    assert_eq!(world["outcome"], "changed");
    assert_eq!(editor.history.undo_label().as_deref(), Some("Unparent"));

    // Typed refusals keep the baseline.
    parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"parent_id":a,"keep":"local"}),
    )
    .await;
    let baseline = editor.history.scene().clone();
    for (args, code) in [
        (
            json!({"scene_token":token,"node_id":a,"parent_id":b}),
            "invalid_reparent",
        ),
        (
            json!({"scene_token":token,"node_id":a,"parent_id":a}),
            "invalid_reparent",
        ),
        (
            json!({"scene_token":token,"node_id":b,"parent_id":"999999"}),
            "scene_error",
        ),
        (
            json!({"scene_token":Uuid::new_v4().to_string(),"node_id":b,"world":true}),
            "stale_scene",
        ),
    ] {
        let result = parent(&mut editor, &context, &client, args).await;
        assert_eq!(result["error"]["code"], code, "{result}");
        assert_eq!(editor.history.scene(), &baseline);
    }
    // A transform named like the parent's shape clashes among the new siblings.
    let c = cube(&mut editor, &context, &client, "clashC").await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "rename_node",
        json!({"scene_token":token,"node_id":c,"name":"groupAShape"}),
    )
    .await;
    let baseline = editor.history.scene().clone();
    let result = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":c,"parent_id":a}),
    )
    .await;
    assert_eq!(result["error"]["code"], "name_collision", "{result}");
    assert_eq!(editor.history.scene(), &baseline);
    for args in [
        json!({"scene_token":token,"node_id":b}),
        json!({"scene_token":token,"node_id":b,"parent_id":a,"world":true}),
        json!({"scene_token":token,"node_id":b,"world":false}),
        json!({"scene_token":token,"node_id":b,"parent_id":a,"keep":"both"}),
        json!({"scene_token":token,"node_id":b,"parent_id":1}),
        json!({"scene_token":token,"node_id":b,"parent_id":a,"extra":1}),
    ] {
        assert!(
            call_response(&mut editor, &context, client.peer(), "parent_node", args)
                .await
                .is_err()
        );
        assert_eq!(editor.history.scene(), &baseline);
    }

    // Human draft, then modal and closing, refuse with busy_editing.
    let untouched = editor.history.scene().clone();
    editor.channels[0] = "7".into();
    let busy = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"world":true}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing");
    assert_eq!(editor.channels[0], "7");
    editor.channels = Default::default();
    editor.sync_fields();
    editor.request_close();
    let busy = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":b,"world":true}),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing");
    assert_eq!(editor.history.scene(), &untouched);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_parent_node_refuses_an_opened_ascii_scene_with_read_only_format() {
    let context = egui::Context::default();
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("scene.ma");
    std::fs::write(&source, ascii_fixture::specimen()).unwrap();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":source}),
    )
    .await;
    let token = editor.scene_token.to_string();
    let node = editor.history.scene().nodes()[0].id().to_decimal();
    let before = editor.history.scene().clone();
    let result = parent(
        &mut editor,
        &context,
        &client,
        json!({"scene_token":token,"node_id":node,"world":true}),
    )
    .await;
    assert_eq!(result["error"]["code"], "read_only_format", "{result}");
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
