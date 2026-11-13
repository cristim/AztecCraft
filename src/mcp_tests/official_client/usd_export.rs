use super::*;

async fn export(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call(editor, context, client.peer(), "export_usd", arguments)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn sdk_export_usd_writes_new_files_and_never_changes_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let glb = directory.path().join("all.usda");
    let token = editor.scene_token.to_string();
    let empty = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": glb}),
    )
    .await;
    assert_eq!(empty["error"]["code"], "nothing_to_export");
    assert!(!glb.exists());

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
        json!({"scene_token": token, "path": glb}),
    )
    .await;
    assert_eq!(exported["outcome"], "usd_exported");
    assert_eq!(exported["objects"], 2);
    assert_eq!(exported["vertices"], 8 + 121);
    assert_eq!(exported["faces"], 6 + 100);
    let bytes = std::fs::read(&glb).unwrap();
    assert!(bytes.starts_with(b"#usda 1.0\n"));
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("upAxis = \"Y\"")
    );
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), can_undo);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.selected(), Some(plane));
    assert!(editor.status.contains("to USD"), "{}", editor.status);

    let again = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": glb}),
    )
    .await;
    assert_eq!(again["error"]["code"], "destination_exists");
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);

    let gltf = directory.path().join("selection.USDA");
    let selected = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": gltf, "selection_only": true}),
    )
    .await;
    assert_eq!(selected["objects"], 1);
    let text = std::fs::read_to_string(&gltf).unwrap();
    assert_eq!(text.matches("def Mesh").count(), 1);

    editor
        .apply_operation(Operation::Select {
            node: None,
            instance_path: None,
        })
        .unwrap();
    let unselected = directory.path().join("unselected.usda");
    let refused = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": unselected, "selection_only": true}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "no_selection");
    assert!(!unselected.exists());

    for bad in ["scene.obj", "scene.usd", "scene.glb", "scene.mb"] {
        let path = directory.path().join(bad);
        let refused = export(
            &mut editor,
            &context,
            &client,
            json!({"scene_token": token, "path": path}),
        )
        .await;
        assert_eq!(refused["error"]["code"], "invalid_input");
        assert!(!path.exists());
    }

    let missing = directory.path().join("missing").join("x.usda");
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
        json!({"scene_token": token, "path": "a.usda", "extra": true}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "export_usd",
                arguments
            )
            .await
            .is_err()
        );
    }
    let fresh = directory.path().join("fresh.usda");
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
    let shape = editor.history.scene().shapes()[0].id();
    editor
        .apply_operation(Operation::ProjectUvPlanar {
            shape,
            axis: azteccraft::native_scene::UvProjectAxis::Z,
        })
        .unwrap();
    let token = editor.scene_token.to_string();
    let uvs = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": fresh}),
    )
    .await;
    assert_eq!(uvs["error"]["code"], "export_unsupported");
    assert!(!fresh.exists());
    editor
        .apply_operation(Operation::DeleteUvs { shape })
        .unwrap();
    editor
        .apply_operation(Operation::AssignSessionMaterial {
            shape,
            material: azteccraft::native_scene::MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        })
        .unwrap();
    let token = editor.scene_token.to_string();
    let unsupported = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": fresh}),
    )
    .await;
    assert_eq!(unsupported["error"]["code"], "export_unsupported");
    assert!(!fresh.exists());

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
