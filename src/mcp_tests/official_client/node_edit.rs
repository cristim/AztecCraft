use super::*;

#[tokio::test]
async fn sparse_node_edit_combines_history_preserves_selection_and_rejects_invalid_intent() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","name":"first"}),
    )
    .await;
    let id = created.structured_content.unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"plane","name":"selected"}),
    )
    .await;
    let before = editor.history.scene().clone();
    let selection = (editor.selected(), editor.selected_path().to_vec());
    let camera = editor.camera.clone();
    let title = editor.document_title();
    let result = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id,"name":"combined","translation":[16777217.125,null,null],"rotation_degrees":[null,90,null],"scale":[null,null,-1]})).await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
    let edited = editor.history.scene().clone();
    let node = edited.node(NodeId::from_decimal(&id).unwrap()).unwrap();
    assert_eq!(node.name(), "combined");
    assert_eq!(node.transform().translation, [16777217.125, 0.0, 0.0]);
    assert_eq!(node.transform().rotation, [0.0, 90.0, 0.0]);
    assert_eq!(node.transform().scale, [1.0, 1.0, -1.0]);
    assert_eq!(
        (editor.selected(), editor.selected_path()),
        (selection.0, selection.1.as_slice())
    );
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.scene_token.to_string(), token);
    assert_eq!(editor.document_title(), title);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.can_redo());
    let fields = (editor.name.clone(), editor.channels.clone());
    for args in [
        json!({"scene_token":token,"node_id":id}),
        json!({"scene_token":token,"node_id":id,"name":null,"translation":[null,null,null]}),
        json!({"scene_token":token,"node_id":id,"translation":[0,null,null]}),
    ] {
        let result = call(&mut editor, &context, client.peer(), "edit_node", args).await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "unchanged");
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
    }
    for args in [
        json!({"scene_token":token,"node_id":id,"name":"","translation":[3,null,null]}),
        json!({"scene_token":token,"node_id":"999999"}),
    ] {
        let result = call(&mut editor, &context, client.peer(), "edit_node", args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
    }
    for fields in [
        json!({"translation":null}),
        json!({"scale":[1,2]}),
        json!({"rotation_degrees":[null,"bad",null]}),
        json!({"extra":true}),
        json!({"node_id":1}),
    ] {
        let mut args = json!({"scene_token":token,"node_id":id});
        args.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        assert!(
            call_response(&mut editor, &context, client.peer(), "edit_node", args)
                .await
                .is_err()
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
    }
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &edited);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sparse_imported_edit_preserves_raw_radians_and_only_changes_requested_words() {
    use azteccraft::native_scene::binary;
    let source = binary_fixture::scene_with_units(
        &[
            binary_fixture::group(
                b"XFRM",
                &[
                    binary_fixture::creation(b"object", None),
                    binary_fixture::triple(b"t", [16777217.125, -2.0, 3.0]),
                    binary_fixture::triple(b"r", [0.1, -0.0, 0.25]),
                    binary_fixture::triple(b"rp", [0.25, 0.0, 0.0]),
                    binary_fixture::triple(b"sp", [0.0, 0.5, 0.0]),
                ]
                .concat(),
                4,
            ),
            binary_fixture::group(
                b"DMSH",
                &[
                    binary_fixture::creation(b"shape", Some(b"object")),
                    binary_fixture::chunk(
                        b"MESH",
                        &binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]),
                        8,
                    ),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"cm",
        b"rad",
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("radian-scene.mb");
    std::fs::write(&path, &source).unwrap();
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
    let token = opened.structured_content.unwrap()["scene_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let original = editor.history.scene().clone();
    let id = original.nodes()[0].id();
    let original_internal = original.internal_transform(id).unwrap();
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
    let normalized = inspected["inspection"]["nodes"][0]["transform"]["rotation_degrees"].clone();
    let result = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[16777219.375,-2,3],"rotation_degrees":normalized,"scale":[1,1,1]})).await;
    assert_eq!(result.is_error, Some(false));
    assert_ne!(
        editor
            .history
            .scene()
            .internal_transform(id)
            .unwrap()
            .rotation_radians[0]
            .to_bits(),
        original_internal.rotation_radians[0].to_bits(),
        "normalized whole-transform workflow must expose precision loss"
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &original);
    let fields = (editor.name.clone(), editor.channels.clone());
    let empty = call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":id.to_decimal()}),
    )
    .await;
    assert_eq!(empty.structured_content.unwrap()["outcome"], "unchanged");
    assert!(editor.history.can_redo());
    assert!(!editor.is_dirty());
    assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
    let selection = (editor.selected(), editor.selected_path().to_vec());
    let camera = editor.camera.clone();
    let title = editor.document_title();
    let edited = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[16777219.375,null,null]})).await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    let scene = editor.history.scene();
    let internal = scene.internal_transform(id).unwrap();
    assert_eq!(
        internal.rotation_radians.map(f64::to_bits),
        original_internal.rotation_radians.map(f64::to_bits)
    );
    assert_eq!(
        internal.rotate_pivot_cm.map(f64::to_bits),
        original_internal.rotate_pivot_cm.map(f64::to_bits)
    );
    assert_eq!(
        internal.scale_pivot_cm.map(f64::to_bits),
        original_internal.scale_pivot_cm.map(f64::to_bits)
    );
    assert_eq!(scene.shapes(), original.shapes());
    assert_eq!(scene.dag_edges(), original.dag_edges());
    let encoded = binary::encode(scene).unwrap();
    let offset = source
        .windows(8)
        .position(|word| word == 16777217.125_f64.to_be_bytes())
        .unwrap();
    assert_eq!(encoded.len(), source.len());
    assert!(encoded[..offset] == source[..offset]);
    assert!(encoded[offset..offset + 8] == 16777219.375_f64.to_be_bytes());
    assert!(encoded[offset + 8..] == source[offset + 8..]);
    assert_eq!(
        (editor.selected(), editor.selected_path()),
        (selection.0, selection.1.as_slice())
    );
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.document_title(), format!("{title} *"));
    assert_eq!(editor.scene_token.to_string(), token);
    let translated = editor.history.scene().clone();
    let refused = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id.to_decimal(),"name":"unsupported","translation":[7,null,null]})).await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &translated);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &original);
    assert!(binary::encode(editor.history.scene()).unwrap().as_slice() == source.as_ref());
    assert!(!editor.is_dirty());
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &translated);
    let rotated = call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":id.to_decimal(),"rotation_degrees":[90,null,null]}),
    )
    .await;
    assert_eq!(rotated.structured_content.unwrap()["outcome"], "changed");
    let rotation = editor
        .history
        .scene()
        .internal_transform(id)
        .unwrap()
        .rotation_radians;
    assert_eq!(rotation[0].to_bits(), std::f64::consts::FRAC_PI_2.to_bits());
    assert_eq!(rotation[1].to_bits(), (-0.0_f64).to_bits());
    assert_eq!(rotation[2].to_bits(), 0.25_f64.to_bits());
    assert!(std::fs::read(path).unwrap().as_slice() == source.as_ref());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn native_sdk_transforms_preserve_double_precision_and_inspect_metadata_only() {
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
    let id = created["node_id"].as_str().unwrap();
    let result = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id,"translation":[16777217.125,0,0],"rotation_degrees":[90,0,0],"scale":[1,1,1]})).await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        16777217.125
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
    let inspection = &inspected["inspection"];
    let node = &inspection["nodes"][0];
    assert_eq!(
        node["transform"]["translation"][0].as_f64(),
        Some(16777217.125)
    );
    assert_eq!(node["transform"]["rotation_degrees"][0], 90.0);
    assert_eq!(node["authoring_transform"]["rotation"][0], 90.0);
    assert_eq!(inspection["units"]["angular"], "deg");
    assert_eq!(inspection["units"]["linear"], "cm");
    assert!(node.get("mesh").is_none());
    assert!(node.get("source").is_none());
    assert_eq!(inspection["shapes"][0]["points"], 8);
    assert_eq!(inspection["shapes"][0]["faces"], 6);
    assert_eq!(node["shape_ids"][0], inspection["shapes"][0]["id"]);
    assert_eq!(inspection["viewport_backend"], "uninitialized");
    assert_eq!(inspection["viewport_error"], "uninitialized");
    assert_eq!(inspection["depth_complete"], false);
    assert_eq!(inspection["clipping_complete"], false);
    assert!(inspection["shapes"][0].get("vertices").is_none());
    let paths = node["instance_paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    let selected = call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":id,"instance_path":paths[0]}),
    )
    .await;
    assert_eq!(selected.is_error, Some(false));
    let before = editor.history.scene().clone();
    let selected_path = editor.selected_path().to_vec();
    let wrong = call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":id,"instance_path":["18446744073709551615"]}),
    )
    .await;
    assert_eq!(
        wrong.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.selected_path(), selected_path);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
