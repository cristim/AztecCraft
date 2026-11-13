use super::*;

async fn export(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    arguments: serde_json::Value,
) -> serde_json::Value {
    call(editor, context, client.peer(), "export_gltf", arguments)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn sdk_export_gltf_writes_new_files_and_never_changes_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let glb = directory.path().join("all.glb");
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
    assert_eq!(exported["outcome"], "gltf_exported");
    assert_eq!(exported["objects"], 2);
    assert_eq!(exported["vertices"], 8 + 121);
    assert_eq!(exported["triangles"], 12 + 200);
    let bytes = std::fs::read(&glb).unwrap();
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(
        u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize,
        bytes.len()
    );
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), can_undo);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.selected(), Some(plane));
    assert!(editor.status.contains("to glTF"), "{}", editor.status);

    let again = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": glb}),
    )
    .await;
    assert_eq!(again["error"]["code"], "destination_exists");
    assert_eq!(std::fs::read(&glb).unwrap(), bytes);

    let gltf = directory.path().join("selection.GLTF");
    let selected = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": gltf, "selection_only": true}),
    )
    .await;
    assert_eq!(selected["objects"], 1);
    let text: serde_json::Value = serde_json::from_slice(&std::fs::read(&gltf).unwrap()).unwrap();
    assert_eq!(text["asset"]["version"], "2.0");

    editor
        .apply_operation(Operation::Select {
            node: None,
            instance_path: None,
        })
        .unwrap();
    let unselected = directory.path().join("unselected.glb");
    let refused = export(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": token, "path": unselected, "selection_only": true}),
    )
    .await;
    assert_eq!(refused["error"]["code"], "no_selection");
    assert!(!unselected.exists());

    for bad in ["scene.obj", "scene.mb"] {
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

    let missing = directory.path().join("missing").join("x.glb");
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
        json!({"scene_token": token, "path": "a.glb", "extra": true}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "export_gltf",
                arguments
            )
            .await
            .is_err()
        );
    }
    let fresh = directory.path().join("fresh.glb");
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
