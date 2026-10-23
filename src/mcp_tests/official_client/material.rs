use super::*;

#[tokio::test]
async fn material_sdk_edit_inspect_history_and_repeated_save_share_the_native_word() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("material-original.mb");
    let original = appearance_fixture::appearance_specimen_with_users(2);
    std::fs::write(&source, &original).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let before_open = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":before_open,"path":source}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let token = editor.scene_token.to_string();
    let shape = editor.history.scene().shapes()[0].id();
    let initial = editor.history.scene().clone();
    let selected = editor.selected;
    let camera = editor.camera.clone();
    let edited = call(
        &mut editor,
        &context,
        client.peer(),
        "set_material_diffuse",
        json!({"scene_token":token,"shape_id":shape.to_decimal(),"value":1.2345678901234567}),
    )
    .await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let first = editor.history.scene().clone();
    assert_eq!(editor.selected, selected);
    assert_eq!(editor.camera, camera);
    let inspection = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(inspection["inspection"]["appearance_incomplete"], true);
    assert_eq!(inspection["inspection"]["dirty"], true);
    for material in inspection["inspection"]["shapes"].as_array().unwrap() {
        assert_eq!(material["material"]["diffuse"], 1.2345678901234567);
        assert_eq!(material["material"]["shared_users"], 2);
        assert_eq!(material["material"]["rendering_available"], false);
    }
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &initial);
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &first);
    #[cfg(any(unix, windows))]
    {
        let destination = directory.path().join("material-working.mb");
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token":token,"path":destination}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        assert!(!editor.is_dirty());
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":destination}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(opened["outcome"], "document_applied");
        let previous_token = token;
        let token = editor.scene_token.to_string();
        assert_ne!(token, previous_token);
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
        assert_eq!(inspected["inspection"]["scene_token"], token);
        assert_eq!(inspected["inspection"]["dirty"], false);
        assert_eq!(inspected["inspection"]["save_available"], true);
        assert_eq!(
            inspected["inspection"]["shapes"].as_array().unwrap().len(),
            2
        );
        for entry in inspected["inspection"]["shapes"].as_array().unwrap() {
            let material = &entry["material"];
            assert_eq!(
                material["diffuse"].as_f64().unwrap().to_bits(),
                1.2345678901234567f64.to_bits()
            );
            assert_eq!(material["editing_available"], true);
            assert_eq!(material["rendering_available"], false);
            assert_eq!(material["shared_users"], 2);
        }
        let edited = call(
            &mut editor,
            &context,
            client.peer(),
            "set_material_diffuse",
            json!({"scene_token":token,"shape_id":shape.to_decimal(),"value":-0.0}),
        )
        .await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene",
            json!({"scene_token":token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        let installed = std::fs::read(&destination).unwrap();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":destination}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(opened["outcome"], "document_applied");
        let previous_token = token;
        let token = editor.scene_token.to_string();
        assert_ne!(token, previous_token);
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
        assert_eq!(inspected["inspection"]["scene_token"], token);
        assert_eq!(
            inspected["inspection"]["shapes"].as_array().unwrap().len(),
            2
        );
        for entry in inspected["inspection"]["shapes"].as_array().unwrap() {
            let material = &entry["material"];
            assert_eq!(
                material["diffuse"].as_f64().unwrap().to_bits(),
                (-0.0f64).to_bits()
            );
            assert_eq!(material["editing_available"], true);
            assert_eq!(material["rendering_available"], false);
        }
        let repeated = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene",
            json!({"scene_token":token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(repeated["outcome"], "saved");
        assert_eq!(repeated["dirty"], false);
        assert_eq!(std::fs::read(&destination).unwrap(), installed);
    }
    let baseline = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "set_material_diffuse",
        json!({"scene_token":Uuid::new_v4().to_string(),"shape_id":shape.to_decimal(),"value":0.5}),
    )
    .await;
    assert!(refused.is_error.unwrap_or(false));
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn checked_appearance_open_inspection_and_document_replacement_share_live_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic-appearance.mb");
    std::fs::write(&path, appearance_fixture::appearance_specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let expected = editor.history.scene().appearance_readiness();
    assert_eq!(expected.checked_source_bindings, 1);
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
    assert_eq!(
        inspected["inspection"]["source_appearance"],
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        inspected["inspection"]["shapes"][0]["source_appearance"],
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(
        editor.appearance_scope().unwrap(),
        ("Scene totals", expected)
    );
    let token = editor.scene_token.to_string();
    let id = inspected["inspection"]["nodes"][0]["id"].clone();
    let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id,"translation":[4.0,2.0,3.0],"rotation_degrees":[0.0,0.0,0.0],"scale":[1.0,1.0,1.0]})).await;
    assert_ne!(edited.is_error, Some(true));
    let edited_scene = editor.history.scene().clone();
    assert_eq!(edited_scene.appearance_readiness(), expected);
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    let cancelled = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"cancel"})).await.structured_content.unwrap();
    assert_eq!(cancelled["outcome"], "cancelled");
    assert_eq!(editor.history.scene(), &edited_scene);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene().appearance_readiness(), expected);
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &edited_scene);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    let before = editor.history.scene().clone();
    let token = editor.scene_token.to_string();
    std::fs::write(&path, b"malformed").unwrap();
    let failed = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(failed.is_error, Some(true));
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token.to_string(), token);
    let fresh = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(
        fresh.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    assert_eq!(editor.history.scene().appearance_readiness().shapes, 0);
    assert_eq!(
        editor.history.scene().appearance_readiness().state,
        azteccraft::native_scene::AppearanceState::Unbound
    );
    client.cancel().await.unwrap();
    drop(editor);
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}
