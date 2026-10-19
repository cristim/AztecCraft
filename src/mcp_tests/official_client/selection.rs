use super::*;

#[tokio::test]
async fn sdk_face_mode_target_inspection_and_clear_share_authoritative_editor_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let mut ids = Vec::new();
    for primitive in ["cube", "plane"] {
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive}),
        )
        .await;
        ids.push(
            created.structured_content.unwrap()["node_id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":ids[0]}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "rename_node",
        json!({"scene_token":token,"node_id":ids[0],"name":"redo_payload"}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
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
    assert_eq!(inspected["inspection"]["selection_mode"], "object");
    assert!(inspected["inspection"]["remembered_face"].is_null());
    assert_eq!(
        inspected["inspection"]["face_selection_policy"],
        "visible_centers_only"
    );
    let plane = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == ids[1])
        .unwrap();
    let mut path = plane["instance_paths"][0].as_array().unwrap().clone();
    path.push(plane["shape_ids"][0].clone());
    let target = json!({"node_id":ids[1],"shape_id":plane["shape_ids"][0],"instance_path":path,"face_index":0});
    let scene = editor.history.scene().clone();
    let camera = editor.camera.clone();
    let dirty = editor.is_dirty();
    assert!(editor.history.can_redo());
    for (tool, args) in [
        (
            "set_selection_mode",
            json!({"scene_token":token,"mode":"face"}),
        ),
        ("select_face", json!({"scene_token":token,"target":target})),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_ne!(result.is_error, Some(true));
    }
    assert_eq!(editor.selected.unwrap().to_decimal(), ids[1]);
    assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 0);
    let selection = (
        editor.selected,
        editor.selected_path.clone(),
        editor.remembered_face.clone(),
    );
    for mode in ["face", "object", "face"] {
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_selection_mode",
            json!({"scene_token":token,"mode":mode}),
        )
        .await;
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
        assert_eq!(inspected["inspection"]["selection_mode"], mode);
        assert_eq!(inspected["inspection"]["remembered_face"], target);
        assert_eq!(inspected["inspection"]["selected_node"], ids[1]);
        assert_eq!(
            inspected["inspection"]["selected_instance_path"],
            plane["instance_paths"][0]
        );
    }
    for field in ["node_id", "shape_id", "instance_path", "face_index"] {
        let mut invalid = target.clone();
        invalid[field] = match field {
            "node_id" => json!(ids[0]),
            "shape_id" => json!("18446744073709551615"),
            "instance_path" => json!(["18446744073709551615", target["shape_id"]]),
            _ => json!(usize::MAX),
        };
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "select_face",
            json!({"scene_token":token,"target":invalid}),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "{field}");
        assert_eq!(
            (
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone()
            ),
            selection
        );
    }
    for (tool, args) in [
        (
            "set_selection_mode",
            json!({"scene_token":Uuid::nil().to_string(),"mode":"object"}),
        ),
        (
            "select_face",
            json!({"scene_token":Uuid::nil().to_string(),"target":null}),
        ),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "stale_scene"
        );
    }
    let cleared = call(
        &mut editor,
        &context,
        client.peer(),
        "select_face",
        json!({"scene_token":token,"target":null}),
    )
    .await;
    assert_ne!(cleared.is_error, Some(true));
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.selected, selection.0);
    assert_eq!(editor.selected_path, selection.1);
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"object"}),
    )
    .await;
    let wrong_mode = call(
        &mut editor,
        &context,
        client.peer(),
        "select_face",
        json!({"scene_token":token,"target":target}),
    )
    .await;
    assert_eq!(wrong_mode.is_error, Some(true));
    let null_target = call(
        &mut editor,
        &context,
        client.peer(),
        "select_face",
        json!({"scene_token":token,"target":null}),
    )
    .await;
    assert_eq!(null_target.is_error, Some(true));
    assert_eq!(
        null_target.structured_content.unwrap()["error"]["code"],
        "invalid_input"
    );
    let state = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(state["inspection"]["selection_mode"], "object");
    assert_eq!(state["inspection"]["selected_node"], ids[1]);
    assert_eq!(
        state["inspection"]["selected_instance_path"],
        plane["instance_paths"][0]
    );
    assert!(state["inspection"]["remembered_face"].is_null());
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.is_dirty(), dirty);
    assert!(editor.history.can_redo());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

// Mirrors `group_editor()` in src/app/tests.rs; those tests build without the mcp feature, so the builder cannot be shared.
fn grouped_scene_bytes() -> std::sync::Arc<[u8]> {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene, triple};
    scene(
        &[
            group(
                b"XFRM",
                &[creation(b"group", None), triple(b"t", [2., 0., 0.])].concat(),
                4,
            ),
            group(b"XFRM", &creation(b"child", Some(b"group")), 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"child")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    )
}

#[tokio::test]
async fn sdk_hierarchy_mode_is_idempotent_refuses_faces_and_keeps_select_node_exact() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("grouped.mb");
    std::fs::write(&source, grouped_scene_bytes()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let old_token = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":old_token,"path":source}),
    )
    .await;
    assert_eq!(
        opened.structured_content.unwrap()["outcome"],
        "document_applied"
    );
    let token = editor.scene_token.to_string();
    let paths = editor.history.scene().transform_paths().unwrap();
    assert_eq!(paths.len(), 2);
    let child = &paths[1];
    let child_node = child.node.to_decimal();
    let child_path: Vec<_> = child.path.iter().map(|id| id.to_decimal()).collect();
    assert_eq!(child_path.len(), 2);

    let first = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"hierarchy"}),
    )
    .await;
    assert_ne!(first.is_error, Some(true));
    assert_eq!(first.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
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
    assert_eq!(inspected["inspection"]["selection_mode"], "hierarchy");
    assert_eq!(editor.scene_token.to_string(), token);
    let repeat = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"hierarchy"}),
    )
    .await;
    assert_eq!(repeat.structured_content.unwrap()["outcome"], "unchanged");
    assert_eq!(editor.scene_token.to_string(), token);

    let selected = call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":child_node,"instance_path":child_path}),
    )
    .await;
    assert_ne!(selected.is_error, Some(true));
    assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
    assert_eq!(editor.selected.unwrap().to_decimal(), child_node);
    assert_eq!(
        editor
            .selected_path
            .iter()
            .map(|id| id.to_decimal())
            .collect::<Vec<_>>(),
        child_path
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
    assert_eq!(inspected["inspection"]["selected_node"], child_node);
    assert_eq!(
        inspected["inspection"]["selected_instance_path"],
        json!(child_path)
    );

    let scene = editor.history.scene().clone();
    let selection = (editor.selected, editor.selected_path.clone());
    let shape_id = scene.shapes()[0].id().to_decimal();
    let target =
        json!({"node_id":child_node,"shape_id":shape_id,"instance_path":child_path,"face_index":0});
    for target in [target, serde_json::Value::Null] {
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "select_face",
            json!({"scene_token":token,"target":target}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "invalid_input"
        );
        assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
        assert_eq!((editor.selected, editor.selected_path.clone()), selection);
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_hierarchy_mode_guards_match_other_selection_modes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    let args = |token: &str, mode: &str| json!({"scene_token":token,"mode":mode});
    let scene = editor.history.scene().clone();

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        args(&Uuid::nil().to_string(), "hierarchy"),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.selection_mode, SelectionMode::Object);

    for mode in ["root", "leaf", "template", "Hierarchy", ""] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_selection_mode",
            args(&token, mode),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {mode:?}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{mode}");
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &scene);
    }

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        args(&token, "hierarchy"),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        args(&token, "hierarchy"),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_hierarchy_mode_is_busy_during_an_active_move() {
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
    .await
    .structured_content
    .unwrap();
    let path: Vec<_> = editor
        .selected_path
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":created["node_id"],"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    let preview = editor.history.scene().clone();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"hierarchy"}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_manipulation"
    );
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.history.scene(), &preview);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_delete_node_is_refused_in_component_selection_modes() {
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
    .await
    .structured_content
    .unwrap();
    let node_id = created["node_id"].clone();
    let id = editor.selected.unwrap();
    for (mode, selection, noun) in [
        ("face", SelectionMode::Face, "Face"),
        ("vertex", SelectionMode::Vertex, "Vertex"),
        ("edge", SelectionMode::Edge, "Edge"),
    ] {
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_selection_mode",
            json!({"scene_token":token,"mode":mode}),
        )
        .await;
        let before = editor.history.scene().clone();
        let can_undo = editor.history.can_undo();
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "delete_node",
            json!({"scene_token":token,"node_id":node_id}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true), "{mode}");
        let error = refused.structured_content.unwrap();
        assert_eq!(error["error"]["code"], "invalid_input", "{mode}");
        assert_eq!(
            error["error"]["message"],
            format!("{noun} deletion is unsupported; switch to Object or Hierarchy selection"),
            "{mode}"
        );
        assert_eq!(editor.selection_mode, selection);
        assert_eq!(editor.history.scene(), &before, "{mode}");
        assert!(editor.history.scene().node(id).is_ok());
        assert_eq!(editor.history.can_undo(), can_undo, "{mode}");
        assert_eq!(editor.scene_token.to_string(), token, "{mode}");
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
        assert_eq!(inspected["inspection"]["scene_token"], token, "{mode}");
        assert!(
            inspected["inspection"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|node| node["id"] == node_id),
            "{mode}"
        );
    }
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"object"}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "delete_node",
        json!({"scene_token":token,"node_id":node_id}),
    )
    .await;
    assert!(editor.history.scene().nodes().is_empty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_delete_node_on_an_opened_hierarchy_returns_the_typed_refusal_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("grouped.mb");
    std::fs::write(&source, grouped_scene_bytes()).unwrap();
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
    let before = editor.history.scene().clone();
    for (index, message) in [
        (0, "Deleting a node with children is not supported yet"),
        (1, "Deleting a node that has a parent is not supported yet"),
    ] {
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "delete_node",
            json!({"scene_token":token,"node_id":paths[index].node.to_decimal()}),
        )
        .await;
        let error = &refused.structured_content.unwrap()["error"];
        assert_eq!(error["code"], "unsupported_delete");
        assert_eq!(error["message"], message);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
