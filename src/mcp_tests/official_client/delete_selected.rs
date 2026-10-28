use super::*;

async fn delete_selected(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> Value {
    let token = editor.scene_token.to_string();
    call(
        editor,
        context,
        peer,
        "delete_selected",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap()
}

async fn inspected(editor: &mut Editor, context: &egui::Context, peer: &Peer<RoleClient>) -> Value {
    call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

#[tokio::test]
async fn sdk_delete_selected_removes_every_selected_object_as_one_undo_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(vec![all[2].clone(), all[0].clone()]);
    editor.sync_fields();
    frame(&mut editor, &context, vec![]);
    let reply = delete_selected(&mut editor, &context, client.peer()).await;
    assert_eq!(reply["outcome"], "changed");
    let seen = inspected(&mut editor, &context, client.peer()).await;
    assert_eq!(seen["selection_count"], 0);
    let left: Vec<_> = seen["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["id"].clone())
        .collect();
    assert_eq!(left, [json!(all[1].node.to_decimal())]);
    assert_eq!(seen["undo_label"], "Delete");
    let undo_token = editor.scene_token.to_string();
    let undo = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":undo_token}),
    )
    .await;
    assert_ne!(undo.is_error, Some(true));
    assert_eq!(editor.history.scene().nodes().len(), 3);
    assert!(editor.selection.is_empty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_delete_selected_refusals_keep_scene_selection_and_history() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let baseline = |editor: &Editor| {
        (
            editor.history.scene().clone(),
            editor.selection.clone(),
            editor.history.undo_label(),
            editor.is_dirty(),
        )
    };
    let before = baseline(&editor);
    let code = |result: Value| result["error"]["code"].as_str().unwrap().to_owned();

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "delete_selected",
        json!({"scene_token":Uuid::new_v4().to_string()}),
    )
    .await;
    assert_eq!(code(stale.structured_content.unwrap()), "stale_scene");
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "delete_selected",
            json!({"scene_token":token,"node_id":"1"})
        )
        .await
        .is_err()
    );
    editor.channels[0] = "9".into();
    editor.channel_intents[0] = true;
    let busy = delete_selected(&mut editor, &context, client.peer()).await;
    assert_eq!(code(busy), "busy_editing");
    editor.channel_intents[0] = false;
    editor.sync_fields();
    assert_eq!(baseline(&editor), before);

    let mode = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"hierarchy"}),
    )
    .await;
    assert_ne!(mode.is_error, Some(true));
    let hierarchy_baseline = baseline(&editor);
    editor.selection_mode = SelectionMode::Face;
    editor.selection.replace(all[0].clone());
    editor.sync_fields();
    let refused = delete_selected(&mut editor, &context, client.peer()).await;
    assert_eq!(code(refused), "invalid_input");
    assert_eq!(editor.history.scene(), &hierarchy_baseline.0);

    editor.selection_mode = SelectionMode::Object;
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let empty = delete_selected(&mut editor, &context, client.peer()).await;
    assert_eq!(code(empty), "no_selection");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_delete_selected_on_an_opened_hierarchy_is_refused_whole_like_delete_node() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("grouped.mb");
    std::fs::write(&source, super::selection::grouped_scene_bytes()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let old_token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":old_token,"path":source}),
    )
    .await;
    let token = editor.scene_token.to_string();
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.clone());
    editor.sync_fields();
    frame(&mut editor, &context, vec![]);
    let scene = editor.history.scene().clone();
    let single = call(
        &mut editor,
        &context,
        client.peer(),
        "delete_node",
        json!({"scene_token":token,"node_id":paths[0].node.to_decimal()}),
    )
    .await
    .structured_content
    .unwrap();
    let many = delete_selected(&mut editor, &context, client.peer()).await;
    assert_eq!(many["error"], single["error"]);
    assert_eq!(many["error"]["code"], "unsupported_delete");
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.selection.members(), paths);
    assert!(!editor.history.can_undo());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
