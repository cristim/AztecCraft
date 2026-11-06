use super::*;

async fn inspected(editor: &mut Editor, context: &egui::Context, peer: &Peer<RoleClient>) -> Value {
    call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

/// Maya 2025.1's SelectNone (Alt+D) clears the selection and returns to Object mode.
#[tokio::test]
async fn deselect_all_clears_the_selection_and_returns_to_object_mode() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let id = created.structured_content.unwrap()["node_id"].clone();
    let node = inspected(&mut editor, &context, client.peer()).await["nodes"][0].clone();
    let mut path = node["instance_paths"][0].as_array().unwrap().clone();
    path.push(node["shape_ids"][0].clone());
    let target =
        json!({"node_id":id,"shape_id":node["shape_ids"][0],"instance_path":path,"face_index":0});
    for (tool, args) in [
        (
            "set_selection_mode",
            json!({"scene_token":token,"mode":"face"}),
        ),
        ("select_face", json!({"scene_token":token,"target":target})),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_ne!(result.is_error, Some(true), "{tool}");
    }
    let before = inspected(&mut editor, &context, client.peer()).await;
    assert_eq!(before["selection_mode"], "face");
    assert_eq!(before["selected_node"], id);
    assert!(!before["remembered_face"].is_null());

    let stale = Uuid::new_v4().to_string();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "deselect_all",
        json!({"scene_token":stale}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    editor.channels[0] = "2".into();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "deselect_all",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    editor.sync_fields();
    assert_eq!(
        inspected(&mut editor, &context, client.peer()).await,
        before
    );

    for outcome in ["changed", "unchanged"] {
        let reply = call(
            &mut editor,
            &context,
            client.peer(),
            "deselect_all",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(reply.structured_content.unwrap()["outcome"], outcome);
        let after = inspected(&mut editor, &context, client.peer()).await;
        assert_eq!(after["selection_mode"], "object");
        assert!(after["selected_node"].is_null());
        for remembered in ["remembered_face", "remembered_vertex", "remembered_edge"] {
            assert!(after[remembered].is_null(), "{remembered}");
        }
        assert_eq!(after["nodes"], before["nodes"]);
    }

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
