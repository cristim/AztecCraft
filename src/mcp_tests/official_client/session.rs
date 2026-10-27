use super::*;

#[tokio::test]
async fn strict_sdk_requests_and_core_errors_preserve_live_editor_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let id = editor.selected().unwrap().to_decimal();
    let before = editor.history.scene().clone();
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({"unknown":true})
        )
        .await
        .is_err()
    );
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "delete_node",
            json!({"scene_token":token,"node_id":1})
        )
        .await
        .is_err()
    );
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "unknown_tool",
            json!({})
        )
        .await
        .is_err()
    );
    for (name, arguments, expected) in [
        (
            "rename_node",
            json!({"scene_token":token,"node_id":id,"name":""}),
            "scene_error",
        ),
        (
            "select_node",
            json!({"scene_token":token,"node_id":"999"}),
            "scene_error",
        ),
        (
            "delete_node",
            json!({"scene_token":Uuid::new_v4().to_string(),"node_id":id}),
            "stale_scene",
        ),
    ] {
        let result = call(&mut editor, &context, client.peer(), name, arguments).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            expected
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.selected().unwrap().to_decimal(), id);
    }
    editor.name = "human_draft".into();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert_eq!(editor.history.scene(), &before);
    editor.sync_fields();
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn legacy_sdk_cancellation_and_eof_skip_hidden_queued_edits() {
    use rmcp::model::{CallToolRequest, ClientRequest};
    for tool in [
        "create_primitive",
        "edit_node",
        "extrude_face",
        "set_grid_visibility",
        "set_shading_mode",
        "set_selection_mode",
        "select_face",
    ] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.create(if tool == "extrude_face" {
            Primitive::Cube
        } else {
            Primitive::Plane
        });
        if tool == "select_face" {
            editor
                .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
                .unwrap();
        }
        let before = editor.history.scene().clone();
        let selection = (
            editor.selection_mode,
            editor.selected(),
            editor.selected_path().to_vec(),
            editor.remembered_face.clone(),
        );
        let args = if tool == "set_grid_visibility" {
            json!({"scene_token":editor.scene_token.to_string(),"visible":false})
        } else if tool == "set_shading_mode" {
            json!({"scene_token":editor.scene_token.to_string(),"mode":"wireframe"})
        } else if tool == "edit_node" {
            json!({"scene_token":editor.scene_token.to_string(),"node_id":editor.selected().unwrap().to_decimal(),"translation":[3,null,null]})
        } else if tool == "extrude_face" {
            let id = editor.selected().unwrap();
            let shape = editor.history.scene().shapes()[0].id();
            json!({"scene_token":editor.scene_token.to_string(),"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[editor.history.scene().node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":0,"distance":1})
        } else if tool == "set_selection_mode" {
            json!({"scene_token":editor.scene_token.to_string(),"mode":"face"})
        } else if tool == "select_face" {
            let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
            let instance = &view.instances()[0];
            json!({"scene_token":editor.scene_token.to_string(),"target":{"node_id":instance.transform().to_decimal(),"shape_id":instance.shape().to_decimal(),"instance_path":instance.path().iter().map(|id|id.to_decimal()).collect::<Vec<_>>(),"face_index":0}})
        } else {
            json!({"scene_token":editor.scene_token.to_string(),"primitive":"cube"})
        };
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
        ));
        let handle = client
            .peer()
            .send_cancellable_request(request.clone(), Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        handle.cancel(None).await.unwrap();
        client
            .peer()
            .send_request(ClientRequest::PingRequest(Default::default()))
            .await
            .unwrap();
        tokio::task::yield_now().await;
        hidden_frame(&mut editor, &context, vec![]);
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.grid_visible);
        assert_eq!(
            editor.shading,
            crate::editor_operations::ShadingMode::default()
        );
        assert_eq!(
            (
                editor.selection_mode,
                editor.selected(),
                editor.selected_path().to_vec(),
                editor.remembered_face.clone()
            ),
            selection
        );
        let _handle = client
            .peer()
            .send_cancellable_request(request, Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        client.cancel().await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while editor.mcp.as_ref().unwrap().test_alive() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        hidden_frame(&mut editor, &context, vec![]);
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.grid_visible);
        assert_eq!(
            editor.shading,
            crate::editor_operations::ShadingMode::default()
        );
        assert_eq!(
            (
                editor.selection_mode,
                editor.selected(),
                editor.selected_path().to_vec(),
                editor.remembered_face.clone()
            ),
            selection
        );
        assert!(editor.files.is_modal());
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn ui_deadline_drops_receiver_before_paused_editor_can_apply_request() {
    for tool in [
        "create_primitive",
        "extrude_face",
        "set_grid_visibility",
        "set_shading_mode",
    ] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let args = if tool == "set_grid_visibility" {
            json!({"scene_token":editor.scene_token.to_string(),"visible":false})
        } else if tool == "set_shading_mode" {
            json!({"scene_token":editor.scene_token.to_string(),"mode":"wireframe"})
        } else if tool == "extrude_face" {
            editor.create(Primitive::Cube);
            let id = editor.selected().unwrap();
            let shape = editor.history.scene().shapes()[0].id();
            json!({"scene_token":editor.scene_token.to_string(),"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[editor.history.scene().node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":1,"distance":1})
        } else {
            json!({"scene_token":editor.scene_token.to_string(),"primitive":"cube"})
        };
        let before = editor.history.scene().clone();
        let CallToolResponse::Complete(result) = client
            .peer()
            .call_tool_once(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
            )
            .await
            .unwrap()
        else {
            panic!("expected complete reply")
        };
        assert_eq!(result.structured_content.unwrap()["error"]["code"], "busy");
        frame(&mut editor, &context, vec![]);
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.grid_visible);
        assert_eq!(
            editor.shading,
            crate::editor_operations::ShadingMode::default()
        );
        if tool == "create_primitive" {
            assert!(editor.history.scene().nodes().is_empty());
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn official_client_negotiates_legacy_version_and_reads_live_editor() {
    use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
    let context = egui::Context::default();
    let (bridge, stream, serving) = mcp::test_connection(context.clone());
    let server = tokio::spawn(serving);
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let (reader, writer) = tokio::io::split(stream);
    let config = ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new("protocol-test", "1"),
    )
    .with_protocol_version(ProtocolVersion::V_2025_11_25);
    let client = config
        .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
            reader, writer,
        ))
        .await
        .unwrap();
    assert_eq!(
        client.peer_info().unwrap().protocol_version,
        ProtocolVersion::V_2025_11_25
    );
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    assert_eq!(
        inspected.structured_content.unwrap()["inspection"]["scene_token"],
        editor.scene_token.to_string()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_current_discover_lifecycle_mutates_the_same_editor() {
    use rmcp::{ClientLifecycleMode, ClientServiceExt};
    let context = egui::Context::default();
    let (bridge, stream, serving) = mcp::test_connection(context.clone());
    let server = tokio::spawn(serving);
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let (reader, writer) = tokio::io::split(stream);
    let client = ()
        .serve_with_lifecycle(
            rmcp::transport::async_rw::AsyncRwTransport::new(reader, writer),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        client.peer_info().unwrap().protocol_version,
        rmcp::model::ProtocolVersion::V_2026_07_28
    );
    assert_eq!(
        client.peer().list_tools(None).await.unwrap().tools.len(),
        crate::mcp::tool_names().count()
    );
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    let token = inspected.structured_content.unwrap()["inspection"]["scene_token"].clone();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"plane","name":"discover_plane"}),
    )
    .await;
    assert_eq!(result.is_error, Some(false));
    assert_eq!(editor.history.scene().nodes().len(), 1);
    assert_eq!(editor.name, "discover_plane");
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    assert_eq!(
        inspected.structured_content.unwrap()["inspection"]["nodes"][0]["name"],
        "discover_plane"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_client_discovers_every_handler_and_edits_same_native_controller() {
    let context = egui::Context::default();
    let (bridge, stream, serving) = mcp::test_connection(context.clone());
    let server_task = tokio::spawn(serving);
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
    let peer = client.peer();
    let listed = peer.list_tools(None).await.unwrap();
    assert_eq!(
        listed
            .tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>(),
        crate::mcp::tool_names().collect::<Vec<_>>()
    );
    let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({})).await;
    let token = inspected.structured_content.unwrap()["inspection"]["scene_token"]
        .as_str()
        .unwrap()
        .to_string();
    let created = call(
        &mut editor,
        &context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","name":"remote_cube"}),
    )
    .await;
    let id = created.structured_content.unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(editor.name, "remote_cube");
    call(
        &mut editor,
        &context,
        peer,
        "rename_node",
        json!({"scene_token":token,"node_id":id,"name":"renamed"}),
    )
    .await;
    call(&mut editor, &context, peer, "set_transform", json!({"scene_token":token,"node_id":id,"translation":[2,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]})).await;
    assert_eq!(editor.channels[0], "2");
    call(
        &mut editor,
        &context,
        peer,
        "frame_selected",
        json!({"scene_token":token}),
    )
    .await;
    call(
        &mut editor,
        &context,
        peer,
        "select_node",
        json!({"scene_token":token,"node_id":null}),
    )
    .await;
    assert_eq!(editor.selected(), None);
    call(
        &mut editor,
        &context,
        peer,
        "select_node",
        json!({"scene_token":token,"node_id":id}),
    )
    .await;
    editor.channels[0] = "3".into();
    assert!(editor.commit_pending());
    let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({})).await;
    assert_eq!(
        inspected.structured_content.unwrap()["inspection"]["nodes"][0]["transform"]["translation"]
            [0]
        .as_f64(),
        Some(3.0)
    );
    call(
        &mut editor,
        &context,
        peer,
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.channels[0], "2");
    editor.history_step(false);
    call(
        &mut editor,
        &context,
        peer,
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.channels[0], "2");
    call(
        &mut editor,
        &context,
        peer,
        "delete_node",
        json!({"scene_token":token,"node_id":id}),
    )
    .await;
    assert!(editor.history.scene().nodes().is_empty());
    client.cancel().await.unwrap();
    server_task.await.unwrap();
}
