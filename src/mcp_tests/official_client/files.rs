use super::*;

#[cfg(unix)]
#[tokio::test]
async fn native_sdk_save_as_active_save_and_undo_redo_share_live_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("original.mb");
    let destination = directory.path().join("new.mb");
    let original = binary_fixture::specimen();
    std::fs::write(&source, &original).unwrap();
    let token = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": token, "path": source}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let token = editor.scene_token.to_string();
    let node = &editor.history.scene().nodes()[0];
    let id = node.id().to_decimal();
    let mut transform = editor.transform_input(node.transform()).unwrap();
    transform.translation[0] += 1.0;
    let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene_as",
        json!({"scene_token": token, "path": destination}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    assert_eq!(saved["pending_action_status"], "none");
    let first = editor.history.scene().clone();
    transform.translation[0] += 0.5;
    let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let second = editor.history.scene().clone();
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &first);
    assert!(editor.is_dirty());
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &second);
    assert!(!editor.is_dirty());
    let disk = std::fs::read(&destination).unwrap();
    call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(std::fs::read(&destination).unwrap(), disk);
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
    let inspection = &inspected["inspection"];
    assert_eq!(inspection["document_origin"], "maya_binary");
    assert_eq!(inspection["has_file"], true);
    assert_eq!(inspection["save_available"], true);
    assert_eq!(inspection["save_as_available"], true);
    assert_eq!(inspection["save_reconciliation_required"], false);
    assert_eq!(inspection["scene_token"], token);
    let conflict = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene_as",
        json!({"scene_token": token, "path": source}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(conflict["error"]["code"], "destination_exists");
    assert_eq!(editor.history.scene(), &second);
    assert!(!editor.is_dirty());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn native_sdk_saved_source_changed_is_an_acknowledged_write_and_blocks_discard() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("scene.mb");
    std::fs::write(&source, binary_fixture::specimen()).unwrap();
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": token, "path": source}),
    )
    .await;
    let token = editor.scene_token.to_string();
    let node = &editor.history.scene().nodes()[0];
    let id = node.id().to_decimal();
    let mut transform = editor.transform_input(node.transform()).unwrap();
    transform.translation[0] += 1.0;
    let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let before = editor.history.scene().clone();
    let queued = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token": token, "path": source}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(queued["outcome"], "pending_confirmation");
    let pending = queued["pending_action_id"].as_str().unwrap();
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token": token, "pending_action_id": pending, "decision": "save"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    assert_eq!(saved["pending_action_status"], "source_changed");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(
        azteccraft::document::load_native(&source)
            .unwrap()
            .scene
            .nodes()[0]
            .transform()
            .translation,
        transform.translation
    );
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
    assert_eq!(inspected["inspection"]["pending_action"]["id"], pending);
    assert_eq!(
        inspected["inspection"]["pending_action"]["source_changed"],
        true
    );
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token": token, "pending_action_id": pending, "decision": "discard"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(refused["error"]["code"], "source_changed");
    assert_eq!(editor.history.scene(), &before);
    let cancelled = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token": token, "pending_action_id": pending, "decision": "cancel"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(cancelled["outcome"], "cancelled");
    assert_eq!(editor.scene_token.to_string(), token);
    assert!(!editor.is_dirty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn native_sdk_uncertain_receipt_survives_close_reply_flush_failure_until_drop() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    use std::{cell::Cell, os::unix::fs::MetadataExt};
    let context = egui::Context::default();
    let (bridge, stream, serving, failure) =
        mcp::test_connection_with_flush_failure(context.clone());
    let server = tokio::spawn(serving);
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let (reader, writer) = tokio::io::split(stream);
    let client = ()
        .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
            reader, writer,
        ))
        .await
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("document");
    let moved = directory.path().join("moved");
    std::fs::create_dir(&parent).unwrap();
    let source = parent.join("scene.mb");
    std::fs::write(&source, binary_fixture::specimen()).unwrap();
    editor
        .apply_operation(Operation::OpenScene {
            path: source.clone(),
        })
        .unwrap();
    let token = editor.scene_token.to_string();
    let node = &editor.history.scene().nodes()[0];
    let mut transform = editor.transform_input(node.transform()).unwrap();
    let id = node.id();
    transform.translation[0] += 1.0;
    editor
        .apply_operation(Operation::SetTransform { id, transform })
        .unwrap();
    let before = editor.history.scene().clone();
    let expected = azteccraft::native_scene::binary::encode(&before).unwrap();
    let relocated = Cell::new(false);
    let guard = || {
        if !relocated.get()
            && std::fs::read_dir(&parent).unwrap().any(|entry| {
                let entry = entry.unwrap();
                entry.file_name().to_string_lossy().ends_with(".craft_tmp")
                    && std::fs::read(entry.path()).ok().as_deref() == Some(expected.as_slice())
            })
        {
            std::fs::rename(&parent, &moved).unwrap();
            relocated.set(true);
        }
        true
    };
    assert!(matches!(
        editor
            .apply_operation_guarded(
                azteccraft::history::ManipulationProvenance::Native,
                Operation::SaveScene,
                &guard
            )
            .unwrap(),
        OperationReply::SaveUncertain { .. }
    ));
    assert!(relocated.get());
    std::fs::rename(&moved, &parent).unwrap();
    let inode = std::fs::metadata(&source).unwrap().ino();
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
    assert_eq!(
        inspection["inspection"]["save_reconciliation_required"],
        true
    );
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "request_close",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    let request = ClientRequest::CallToolRequest(CallToolRequest::new(CallToolRequestParams::new("resolve_file_action").with_arguments(json!({"scene_token": token, "pending_action_id": pending["pending_action_id"], "decision":"discard"}).as_object().unwrap().clone())));
    failure.arm();
    let handle = client
        .peer()
        .send_cancellable_request(request, Default::default())
        .await
        .unwrap();
    wait_for_queue(&editor).await;
    let output = hidden_frame(&mut editor, &context, vec![]);
    assert!(
        !output
            .viewport_commands
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|commands| commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)))
    );
    assert!(editor.files.is_closing());
    assert!(editor.inspect(false).unwrap().save_reconciliation_required);
    tokio::time::timeout(Duration::from_secs(2), async {
        while !failure.observed() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
        .await
        .unwrap();
    let output = hidden_frame(&mut editor, &context, vec![]);
    assert!(
        !output
            .viewport_commands
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|commands| commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)))
    );
    assert!(!editor.files.is_closing());
    let inspection = editor.inspect(false).unwrap();
    assert!(inspection.save_reconciliation_required && inspection.dirty);
    assert!(inspection.pending_action.is_none());
    assert!(matches!(
        editor.apply_operation(Operation::ResolveFileAction {
            pending_id: Uuid::parse_str(pending["pending_action_id"].as_str().unwrap()).unwrap(),
            decision: crate::editor_operations::FileDecision::Discard,
        }),
        Err(OperationError::StalePendingAction)
    ));
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(std::fs::read(&source).unwrap(), expected);
    assert_eq!(std::fs::metadata(&source).unwrap().ino(), inode);
    let _ = tokio::time::timeout(Duration::from_secs(2), client.cancel())
        .await
        .unwrap();
    drop(editor);
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(std::fs::read(&source).unwrap(), expected);
}

#[tokio::test]
async fn remote_close_and_discard_close_flush_their_own_reply_before_hidden_exit() {
    use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
    for dirty in [false, true] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let (name, arguments) = if dirty {
            call(
                &mut editor,
                &context,
                client.peer(),
                "create_primitive",
                json!({"scene_token":token,"primitive":"cube"}),
            )
            .await;
            let pending = call(
                &mut editor,
                &context,
                client.peer(),
                "request_close",
                json!({"scene_token":token}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(pending["outcome"], "pending_confirmation");
            assert_eq!(pending["kind"], "close");
            assert_eq!(editor.mcp.as_mut().unwrap().poll_close_delivery(), None);
            let cancelled = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"cancel"})).await.structured_content.unwrap();
            assert_eq!(cancelled["outcome"], "cancelled");
            assert!(!editor.files.is_closing());
            let pending = call(
                &mut editor,
                &context,
                client.peer(),
                "request_close",
                json!({"scene_token":token}),
            )
            .await
            .structured_content
            .unwrap();
            (
                "resolve_file_action",
                json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"}),
            )
        } else {
            ("request_close", json!({"scene_token":token}))
        };
        let before = editor.history.scene().clone();
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone()),
        ));
        let handle = client
            .peer()
            .send_cancellable_request(request, Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        let first = hidden_frame(&mut editor, &context, vec![]);
        assert!(
            !first
                .viewport_commands
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|commands| commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close)))
        );
        assert_eq!(
            editor.mcp.as_mut().unwrap().poll_close_delivery(),
            Some(mcp::CloseDelivery::Waiting)
        );
        let OperationReply::Scene { inspection } = editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: None,
                    operation: Operation::Inspect,
                },
                &context,
            )
            .unwrap()
        else {
            panic!("expected inspection")
        };
        assert!(inspection.closing);
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(editor.scene_token),
                    operation: Operation::NewScene
                },
                &context
            ),
            Err(OperationError::BusyEditing)
        ));
        let response = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
            .await
            .unwrap()
            .unwrap();
        let ServerResult::CallToolResult(response) = response else {
            panic!("expected tool response")
        };
        assert_eq!(
            response.structured_content.unwrap()["outcome"],
            "close_requested"
        );
        let output = hidden_frame(&mut editor, &context, vec![]);
        assert!(
            output.viewport_commands[&egui::ViewportId::ROOT]
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token.to_string(), token);
        client.cancel().await.unwrap();
        drop(editor);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn native_mb_open_inspects_precision_and_discards_the_exact_staged_candidate() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-synthetic-scene.mb");
    let source = binary_fixture::specimen();
    std::fs::write(&path, &source).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let original_token = editor.scene_token.to_string();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":original_token,"path":path.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(result["outcome"], "document_applied");
    let token = result["scene_token"].as_str().unwrap().to_owned();
    assert_ne!(token, original_token);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        16777217.125
    );
    let id = editor.history.scene().nodes()[0].id();
    assert_eq!(
        editor
            .history
            .scene()
            .internal_transform(id)
            .unwrap()
            .rotation_radians[2],
        0.25
    );
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
        inspected["inspection"]["nodes"][0]["authoring_transform"]["translation"][0].as_f64(),
        Some(16777217.125)
    );
    assert_eq!(inspected["inspection"]["shapes"][0]["points"], 3);
    assert!(
        !serde_json::to_string(&inspected)
            .unwrap()
            .contains(path.to_str().unwrap())
    );
    assert!(
        editor
            .camera
            .target()
            .distance(Vec3::new(16777217.125_f32, -2.0, 3.0))
            < 4.0
    );
    let imported_scene = editor.history.scene().clone();
    let imported_channels = imported_scene.nodes()[0].transform();
    let imported_id = inspected["inspection"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let rotation_degrees =
        inspected["inspection"]["nodes"][0]["transform"]["rotation_degrees"].clone();
    assert_eq!(imported_channels.scale, [1.0; 3]);
    assert!(!editor.is_dirty());
    assert!(!editor.history.can_undo());
    let changed = call(
        &mut editor,
        &context,
        client.peer(),
        "set_transform",
        json!({
            "scene_token":token,
            "node_id":imported_id,
            "translation":[16777221.125,-2.0,3.0],
            "rotation_degrees":rotation_degrees,
            "scale":[1.0,1.0,1.0]
        }),
    )
    .await;
    assert_ne!(changed.is_error, Some(true));
    assert_eq!(changed.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation,
        [16777221.125, -2.0, 3.0]
    );
    assert_eq!(
        editor
            .history
            .scene()
            .internal_transform(id)
            .unwrap()
            .rotation_radians[2]
            .to_bits(),
        0.25_f64.to_bits()
    );
    assert_eq!(
        editor.history.scene().nodes()[0].transform().scale,
        [1.0; 3]
    );
    assert_eq!(editor.history.scene().shapes(), imported_scene.shapes());
    assert_eq!(editor.scene_token.to_string(), token);
    assert!(editor.is_dirty());
    assert!(editor.history.can_undo());
    let edited = call(
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
        edited["inspection"]["nodes"][0]["authoring_transform"]["translation"][0].as_f64(),
        Some(16777221.125)
    );
    assert_eq!(edited["inspection"]["dirty"], true);
    assert_eq!(edited["inspection"]["can_undo"], true);
    let undone = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_ne!(undone.is_error, Some(true));
    assert_eq!(undone.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.scene(), &imported_scene);
    assert_eq!(
        editor.history.scene().nodes()[0].transform(),
        imported_channels
    );
    assert_eq!(editor.scene_token.to_string(), token);
    assert!(!editor.is_dirty());
    assert!(!editor.history.can_undo());
    assert!(editor.history.can_redo());
    let restored = call(
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
        restored["inspection"]["nodes"],
        inspected["inspection"]["nodes"]
    );
    assert_eq!(restored["inspection"]["dirty"], false);
    assert_eq!(restored["inspection"]["can_undo"], false);
    assert_eq!(restored["inspection"]["can_redo"], true);
    call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let before = editor.history.scene().clone();
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
    assert_eq!(pending["scene_token"], token);
    assert_eq!(pending["kind"], "open");
    let pending_id = pending["pending_action_id"].as_str().unwrap();
    assert!(
        !serde_json::to_string(&pending)
            .unwrap()
            .contains(path.to_str().unwrap())
    );
    std::fs::write(&path, b"changed after admission").unwrap();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":token,"pending_action_id":pending_id,"decision":"discard"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(result["outcome"], "document_applied");
    assert_ne!(editor.history.scene(), &before);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        16777217.125
    );
    assert_eq!(
        editor
            .history
            .scene()
            .internal_transform(editor.history.scene().nodes()[0].id())
            .unwrap()
            .rotation_radians[2],
        0.25
    );
    assert!(!editor.history.can_undo());
    assert!(!editor.is_dirty());
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    let camera = editor.camera.clone();
    let failed = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(failed["error"]["code"], "file_error");
    assert!(
        !serde_json::to_string(&failed)
            .unwrap()
            .contains(path.to_str().unwrap())
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.camera, camera);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn native_file_confirmation_uses_scene_and_action_identity_without_replay() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let first_token = editor.scene_token.to_string();
    let applied = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":first_token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(applied["outcome"], "document_applied");
    let token = applied["scene_token"].as_str().unwrap().to_owned();
    assert_ne!(token, first_token);
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let before = editor.history.scene().clone();
    let camera = editor.camera.clone();
    let selected = editor.selected;
    let selected_path = editor.selected_path.clone();
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    assert_eq!(pending["scene_token"], token);
    assert_eq!(pending["kind"], "new");
    let old_pending = pending["pending_action_id"].as_str().unwrap().to_owned();
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
    assert_eq!(inspected["inspection"]["pending_action"]["id"], old_pending);
    assert_eq!(inspected["inspection"]["pending_action"]["kind"], "new");
    assert_eq!(inspected["inspection"]["save_available"], false);
    for (tool, fields) in [
        ("create_primitive", json!({"primitive":"plane"})),
        ("select_node", json!({"node_id":null})),
        (
            "rename_node",
            json!({"node_id":selected.unwrap().to_decimal(),"name":"blocked"}),
        ),
        (
            "set_transform",
            json!({"node_id":selected.unwrap().to_decimal(),"translation":[0,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]}),
        ),
        (
            "edit_node",
            json!({"node_id":selected.unwrap().to_decimal()}),
        ),
        (
            "extrude_face",
            json!({"node_id":selected.unwrap().to_decimal(),"shape_id":editor.history.scene().shapes()[0].id().to_decimal(),"instance_path":[editor.history.scene().node(selected.unwrap()).unwrap().transform_id().to_decimal(),editor.history.scene().shapes()[0].id().to_decimal()],"face_index":1,"distance":1}),
        ),
        (
            "delete_node",
            json!({"node_id":selected.unwrap().to_decimal()}),
        ),
        ("undo", json!({})),
        ("redo", json!({})),
        ("frame_selected", json!({})),
        ("orbit_camera", json!({"radians":[0.2,0]})),
        ("pan_camera", json!({"world_units":[1,0]})),
        ("dolly_camera", json!({"factor":0.5})),
        ("new_scene", json!({})),
        ("request_close", json!({})),
        ("open_scene", json!({"path":"unopened.mb"})),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "busy_editing",
            "{tool}"
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.selected, selected);
        assert_eq!(editor.selected_path, selected_path);
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":first_token,"pending_action_id":old_pending,"decision":"discard"}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    let cancelled = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":token,"pending_action_id":old_pending,"decision":"cancel"}),
    )
    .await;
    assert_eq!(
        cancelled.structured_content.unwrap()["outcome"],
        "cancelled"
    );
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.history.scene(), &before);
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    let current_pending = pending["pending_action_id"].as_str().unwrap();
    assert_ne!(current_pending, old_pending);
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":token,"pending_action_id":old_pending,"decision":"discard"}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_pending_action"
    );
    assert_eq!(editor.history.scene(), &before);
    let applied = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":token,"pending_action_id":current_pending,"decision":"discard"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(applied["outcome"], "document_applied");
    let new_token = applied["scene_token"].as_str().unwrap();
    assert_ne!(new_token, token);
    assert!(editor.history.scene().nodes().is_empty());
    assert!(!editor.history.can_undo());
    let replay = call(
        &mut editor,
        &context,
        client.peer(),
        "resolve_file_action",
        json!({"scene_token":new_token,"pending_action_id":current_pending,"decision":"discard"}),
    )
    .await;
    assert_eq!(
        replay.structured_content.unwrap()["error"]["code"],
        "stale_pending_action"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn open_scene_failure_withholds_directory_and_file_names() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::Builder::new()
        .prefix("zzprivatedir")
        .tempdir()
        .unwrap();
    let path = directory.path().join("zzsecretscene.ma");
    let token = editor.scene_token.to_string();
    let failed = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":path.to_str().unwrap()}),
    )
    .await;
    assert_eq!(failed.is_error, Some(true));
    let reply = serde_json::to_string(&failed).unwrap();
    assert_eq!(
        failed.structured_content.unwrap()["error"]["code"],
        "file_error"
    );
    assert!(reply.contains("File operation failed (details withheld)"));
    for leaked in ["zzprivatedir", "zzsecretscene"] {
        assert!(!reply.contains(leaked), "{leaked} leaked in {reply}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
