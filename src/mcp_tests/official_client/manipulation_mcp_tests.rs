use super::*;

#[tokio::test]
async fn move_sdk_absolute_preview_cancel_commit_busy_and_tokens_share_one_history() {
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
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let before = editor.history.scene().clone();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await
    .structured_content
    .unwrap();
    let move_id = begin["manipulation_id"].as_str().unwrap().to_owned();
    assert_eq!(begin["outcome"], "manipulation_started");
    assert_eq!(
        editor.active_tool,
        crate::editor_operations::ActiveTool::Select
    );
    for distance in [1.0, -2.0, 3.0] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":move_id,"distance":distance}),
        )
        .await;
        assert!(!result.is_error.unwrap_or(false));
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            distance
        );
        assert!(!editor.has_draft());
    }
    let preview = editor.history.scene().clone();
    let camera = editor.camera.clone();
    for (name, arguments) in [
        ("undo", json!({"scene_token":token})),
        ("redo", json!({"scene_token":token})),
        ("new_scene", json!({"scene_token":token})),
        ("request_close", json!({"scene_token":token})),
        ("save_scene", json!({"scene_token":token})),
        (
            "save_scene_as",
            json!({"scene_token":token,"path":"unused-move.mb"}),
        ),
        (
            "open_scene",
            json!({"scene_token":token,"path":"unused-move.mb"}),
        ),
        (
            "set_active_tool",
            json!({"scene_token":token,"tool":"move"}),
        ),
        ("select_node", json!({"scene_token":token,"node_id":null})),
        (
            "orbit_camera",
            json!({"scene_token":token,"radians":[0.1,0.1]}),
        ),
        (
            "set_material_diffuse",
            json!({"scene_token":token,"shape_id":"1","value":0.25}),
        ),
        (
            "create_session_material",
            json!({"scene_token":token,"color":[1,0,0]}),
        ),
        (
            "assign_session_material",
            json!({"scene_token":token,"shape_id":"1","color":[1,0,0]}),
        ),
        (
            "set_session_material_color",
            json!({"scene_token":token,"material_id":"1","color":[1,0,0]}),
        ),
        (
            "set_selection_mode",
            json!({"scene_token":token,"mode":"face"}),
        ),
        ("select_face", json!({"scene_token":token,"target":null})),
        (
            "set_grid_visibility",
            json!({"scene_token":token,"visible":false}),
        ),
        (
            "set_shading_mode",
            json!({"scene_token":token,"mode":"wireframe"}),
        ),
    ] {
        let grid = editor.grid_visible;
        let shading = editor.shading;
        let mode = editor.selection_mode;
        let result = call(&mut editor, &context, client.peer(), name, arguments).await;
        assert_eq!((editor.grid_visible, editor.selection_mode), (grid, mode));
        assert_eq!(editor.shading, shading);
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "busy_manipulation",
            "{name}"
        );
        assert_eq!(editor.history.scene(), &preview);
        assert_eq!(editor.camera, camera);
    }
    let stale_id = call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":"18446744073709551615","finish":"commit"}),
    )
    .await;
    assert_eq!(
        stale_id.structured_content.unwrap()["error"]["code"],
        "stale_manipulation"
    );
    let stale_token = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":Uuid::new_v4().to_string(),"manipulation_id":move_id,"distance":7}),
    )
    .await;
    assert_eq!(
        stale_token.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.history.scene(), &preview);
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
        inspected["inspection"]["manipulation"]["active_tool"],
        "select"
    );
    assert_eq!(
        inspected["inspection"]["manipulation"]["active"]["provenance"],
        "remote"
    );
    assert_eq!(
        inspected["inspection"]["manipulation"]["active"]["value"],
        3.0
    );
    assert_eq!(
        inspected["inspection"]["nodes"][0]["authoring_transform"]["translation"][0],
        3.0
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":move_id,"finish":"cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &before);
    let activated = call(
        &mut editor,
        &context,
        client.peer(),
        "set_active_tool",
        json!({"scene_token":token,"tool":"move"}),
    )
    .await;
    assert!(!activated.is_error.unwrap_or(false));
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"y"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap();
    assert_ne!(id, move_id);
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"distance":2.5}),
    )
    .await;
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    let after = editor.history.scene().clone();
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &before);
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &after);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_sdk_declared_inches_cancel_and_saved_translation_preserve_other_source_words() {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene_with_units, triple};
    let original = scene_with_units(
        &[
            group(
                b"XFRM",
                &[creation(b"object", None), triple(b"t", [25.4, -0.0, 0.0])].concat(),
                4,
            ),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"in",
        b"deg",
    );
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("move-original.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let baseline = editor.history.scene().clone();
    for finish in ["cancel", "commit"] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
        )
        .await
        .structured_content
        .unwrap();
        let id = result["manipulation_id"].as_str().unwrap();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"distance":2}),
        )
        .await;
        assert!(!result.is_error.unwrap_or(false));
        let transform = editor
            .history
            .scene()
            .node(target.node)
            .unwrap()
            .transform();
        assert_eq!(transform.translation[0], 12.0);
        assert_eq!(transform.translation[1].to_bits(), (-0.0f64).to_bits());
        call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"finish":finish}),
        )
        .await;
        if finish == "cancel" {
            assert_eq!(editor.history.scene(), &baseline);
        }
    }
    let committed = editor.history.scene().clone();
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &baseline);
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &committed);
    #[cfg(any(unix, windows))]
    {
        let destination = directory.path().join("move-saved.mb");
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token":token,"path":destination}),
        )
        .await;
        assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
        let written = std::fs::read(&destination).unwrap();
        let offset = original
            .windows(8)
            .position(|word| word == 25.4f64.to_be_bytes())
            .unwrap();
        let mut expected = original.to_vec();
        expected[offset..offset + 8].copy_from_slice(&30.48f64.to_be_bytes());
        assert_eq!(written, expected);
        let reopened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":destination}),
        )
        .await;
        assert_eq!(
            reopened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        assert_ne!(editor.scene_token.to_string(), token);
        assert_eq!(
            editor.active_tool,
            crate::editor_operations::ActiveTool::Select
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            12.0
        );
    }
    assert_eq!(
        std::fs::read(&source).unwrap().as_slice(),
        original.as_ref()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_sdk_never_commits_real_native_text_drafts_or_takes_their_focus() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    context.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(&mut editor, &context, vec![]);
    frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("human".into())],
    );
    assert!(editor.has_draft());
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let before = editor.history.scene().clone();
    let name = editor.name.clone();
    for (tool, args) in [
        (
            "set_active_tool",
            json!({"scene_token":token,"tool":"move"}),
        ),
        (
            "begin_manipulation",
            json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
        ),
        (
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":"1","distance":1}),
        ),
        (
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":"1","finish":"commit"}),
        ),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "busy_editing"
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.name, name);
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(name_edit_id())
        );
        assert!(editor.history.active_manipulation().is_none());
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_sdk_refuses_begin_in_face_mode_and_reports_it_in_inspection() {
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
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let mode = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"face"}),
    )
    .await;
    assert!(!mode.is_error.unwrap_or(false));
    let before = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "invalid_input"
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
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
    let tool = &inspected["inspection"]["manipulation"];
    assert_eq!(tool["kinds"][0]["available"], false);
    assert!(
        tool["kinds"][0]["unavailable_reason"]
            .as_str()
            .unwrap()
            .contains("Face selection")
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"object"}),
    )
    .await;
    let begun = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    assert_eq!(
        begun.structured_content.unwrap()["outcome"],
        "manipulation_started"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[tokio::test]
async fn move_sdk_refuses_begin_on_a_read_only_ascii_curve_scene_with_a_typed_error() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    editor.history = azteccraft::history::History::new(scene);
    let node = editor.history.scene().nodes()[0].id();
    editor.select(Some(node));
    let token = editor.scene_token.to_string();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let before = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node.to_decimal(),"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    let error = &refused.structured_content.unwrap()["error"];
    assert_eq!(error["code"], "scene_error");
    assert!(error["message"].as_str().unwrap().contains("read-only"));
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
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
        inspected["inspection"]["manipulation"]["kinds"][0]["available"],
        false
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_sdk_refuses_a_binary_scene_lacking_the_translate_leaf_with_a_typed_error() {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene_with_units};
    let original = scene_with_units(
        &[
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"cm",
        b"deg",
    );
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("absent-leaf.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let before = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    let error = &refused.structured_content.unwrap()["error"];
    assert_eq!(error["code"], "absent_channel_leaf");
    assert!(error["message"].as_str().unwrap().contains("translateX"));
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
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
    let tool = &inspected["inspection"]["manipulation"];
    assert_eq!(tool["kinds"][0]["available"], false);
    assert!(
        tool["kinds"][0]["unavailable_reason"]
            .as_str()
            .unwrap()
            .contains("translateX")
    );
    assert_eq!(
        std::fs::read(&source).unwrap().as_slice(),
        original.as_ref()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn inspect_manipulation(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["manipulation"]
        .clone()
}

fn error_code(result: CallToolResult) -> String {
    result.structured_content.unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn every_kind_sdk_shares_guards_busy_tokens_cancel_undo_and_typed_mismatch() {
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
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let before = editor.history.scene().clone();
    // (kind, handle fields, parameter, value, channel index, expected stored value)
    let cases = [
        ("move", json!({"axis":"y"}), "distance", 3.0, 1, 3.0),
        (
            "rotate",
            json!({"axis":"z","space":"object"}),
            "angle",
            30.0,
            5,
            30.0,
        ),
        (
            "scale",
            json!({"handle":"y","space":"object"}),
            "factor",
            2.5,
            7,
            2.5,
        ),
    ];
    let mut committed = before.clone();
    for (kind, handle, parameter, value, channel, expected) in cases {
        let mut arguments =
            json!({"scene_token":token,"kind":kind,"node_id":node,"instance_path":path});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(handle.as_object().unwrap().clone());
        let begin = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            arguments.clone(),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(begin["outcome"], "manipulation_started", "{kind}");
        assert_eq!(
            editor.active_tool,
            crate::editor_operations::ActiveTool::Select
        );
        let id = begin["manipulation_id"].as_str().unwrap().to_owned();
        let update = |value: f64| json!({"scene_token":token,"manipulation_id":id,parameter:value});
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            update(value),
        )
        .await;
        assert!(!result.is_error.unwrap_or(false), "{kind}");
        let stored = editor.history.scene().nodes()[0].transform().values();
        assert!(
            (stored[channel] - expected).abs() < 1e-9,
            "{kind} {stored:?}"
        );
        assert!(!editor.has_draft());
        let preview = editor.history.scene().clone();

        // A parameter of another kind is a typed error and leaves the preview alone.
        for (wrong, bad) in [("distance", 1.0), ("angle", 5.0), ("factor", 2.0)] {
            if wrong == parameter {
                continue;
            }
            let mismatch = call(
                &mut editor,
                &context,
                client.peer(),
                "update_manipulation",
                json!({"scene_token":token,"manipulation_id":id,wrong:bad}),
            )
            .await;
            let error = &mismatch.structured_content.unwrap()["error"];
            assert_eq!(
                error["code"], "manipulation_kind_mismatch",
                "{kind} {wrong}"
            );
            assert!(error["message"].as_str().unwrap().contains(kind));
            assert_eq!(editor.history.scene(), &preview);
        }

        let camera = editor.camera.clone();
        for (name, arguments) in [
            ("undo", json!({"scene_token":token})),
            ("redo", json!({"scene_token":token})),
            ("new_scene", json!({"scene_token":token})),
            ("request_close", json!({"scene_token":token})),
            ("save_scene", json!({"scene_token":token})),
            (
                "open_scene",
                json!({"scene_token":token,"path":"unused-manipulation.mb"}),
            ),
            (
                "set_active_tool",
                json!({"scene_token":token,"tool":"rotate"}),
            ),
            ("select_node", json!({"scene_token":token,"node_id":null})),
            (
                "orbit_camera",
                json!({"scene_token":token,"radians":[0.1,0.1]}),
            ),
            ("begin_manipulation", arguments.clone()),
        ] {
            let result = call(&mut editor, &context, client.peer(), name, arguments).await;
            assert_eq!(error_code(result), "busy_manipulation", "{kind} {name}");
            assert_eq!(editor.history.scene(), &preview);
            assert_eq!(editor.camera, camera);
        }
        let stale_id = call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":"18446744073709551615","finish":"commit"}),
        )
        .await;
        assert_eq!(error_code(stale_id), "stale_manipulation");
        let stale_token = call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":Uuid::new_v4().to_string(),"manipulation_id":id,parameter:value}),
        )
        .await;
        assert_eq!(error_code(stale_token), "stale_scene");
        assert_eq!(editor.history.scene(), &preview);

        let inspected = inspect_manipulation(&mut editor, &context, &client).await;
        let active = &inspected["active"];
        assert_eq!(active["kind"], kind);
        assert_eq!(active["provenance"], "remote");
        assert_eq!(active["parameter"], parameter);
        assert_eq!(active["value"], value);
        assert_eq!(active["manipulation_id"], id.as_str());
        assert_eq!(inspected["units"]["factor"], "unitless");
        assert_eq!(inspected["refusal"], Value::Null);
        let (first, channels, label) = match kind {
            "move" => (0, "translate", "Move"),
            "rotate" => (3, "rotate", "Rotate"),
            _ => (6, "scale", "Scale"),
        };
        assert_eq!(active["channels"], channels);
        let baseline: Vec<f64> = before.nodes()[0].transform().values()[first..first + 3].to_vec();
        assert_eq!(active["baseline_channels"], json!(baseline));
        let current: Vec<f64> = stored[first..first + 3].to_vec();
        assert_eq!(active["current_channels"], json!(current));

        // Cancel restores the exact begin state, as does the draft-free commit path below.
        call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
        )
        .await;
        assert_eq!(editor.history.scene(), &committed);
        assert!(editor.history.active_manipulation().is_none());

        let begin = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            arguments,
        )
        .await
        .structured_content
        .unwrap();
        let second = begin["manipulation_id"].as_str().unwrap();
        assert_ne!(second, id);
        call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":second,parameter:value}),
        )
        .await;
        call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":second,"finish":"commit"}),
        )
        .await;
        let after = editor.history.scene().clone();
        assert_ne!(after, committed);
        assert_eq!(editor.history.undo_label(), Some(label.to_string()));
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &committed);
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &after);
        committed = after;
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn rotate_sdk_refusal_by_the_scene_keeps_the_preview_and_is_inspectable() {
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
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"y","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":30}),
    )
    .await;
    let preview = editor.history.scene().clone();
    // Turning the Y channel to 90 degrees is gimbal lock: the scene refuses the sample.
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":90}),
    )
    .await;
    let error = &refused.structured_content.unwrap()["error"];
    assert_eq!(error["code"], "manipulation_error");
    assert_eq!(editor.history.scene(), &preview);
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["refusal"], error["message"]);
    assert_eq!(inspected["active"]["value"], 30.0);
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":45}),
    )
    .await;
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["refusal"], Value::Null);
    assert_eq!(inspected["active"]["value"], 45.0);
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    assert!(editor.history.active_manipulation().is_none());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn unit_scene(
    linear: &[u8],
    angular: &[u8],
    rotate: [f64; 3],
    scale: [f64; 3],
) -> std::sync::Arc<[u8]> {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene_with_units, triple};
    scene_with_units(
        &[
            group(
                b"XFRM",
                &[
                    creation(b"object", None),
                    triple(b"t", [2.0, 0.0, 0.0]),
                    triple(b"r", rotate),
                    triple(b"s", scale),
                ]
                .concat(),
                4,
            ),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        linear,
        angular,
    )
}

#[tokio::test]
async fn rotate_and_scale_sdk_use_angular_and_unitless_values_in_a_non_cm_radian_scene() {
    let original = unit_scene(b"in", b"rad", [0.25, 0.0, 0.0], [1.5, 1.0, 1.0]);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("units-original.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let baseline = editor.history.scene().clone();
    let stored = |editor: &Editor| {
        editor
            .history
            .scene()
            .node(target.node)
            .unwrap()
            .transform()
            .values()
    };
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["units"]["distance"], "in");
    assert_eq!(inspected["units"]["angle"], "rad");

    // Rotate: 0.5 radians is added to the stored radian channel, with no inch conversion.
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":0.5}),
    )
    .await;
    let values = stored(&editor);
    assert!((values[3] - 0.75).abs() < 1e-12, "{values:?}");
    assert_eq!(values[4].to_bits(), 0.0f64.to_bits());
    let start = baseline.nodes()[0].transform().values();
    for index in [0, 1, 2, 4, 5, 6, 7, 8] {
        assert_eq!(values[index].to_bits(), start[index].to_bits(), "{index}");
    }
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["active"]["value"], 0.5);
    assert_eq!(inspected["active"]["unit"], "rad");
    assert_eq!(
        inspected["active"]["baseline_channels"],
        json!([0.25, 0.0, 0.0])
    );
    // 4.5 rad / (180/pi) * (180/pi) does not round-trip; the echo is the requested value itself.
    for requested in [4.5_f64, 0.1, 1.0 / 3.0] {
        call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"angle":requested}),
        )
        .await;
        let inspected = inspect_manipulation(&mut editor, &context, &client).await;
        assert_eq!(
            inspected["active"]["value"].as_f64().unwrap().to_bits(),
            requested.to_bits(),
            "{requested}"
        );
    }
    // Degrees are not accepted as a second spelling: the same number in degrees is tiny.
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":0.0}),
    )
    .await;
    assert_eq!(stored(&editor)[3].to_bits(), 0.25f64.to_bits());
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(
        azteccraft::native_scene::binary::encode(editor.history.scene()).unwrap(),
        original.as_ref()
    );

    // Scale: the factor is unitless, so 2 doubles the stored 1.5 and ignores inches.
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"x","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"factor":2}),
    )
    .await;
    assert_eq!(stored(&editor)[6], 3.0);
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["active"]["unit"], "unitless");
    assert_eq!(inspected["active"]["handle"], "x");
    assert_eq!(inspected["active"]["axis"], Value::Null);
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    #[cfg(any(unix, windows))]
    {
        let destination = directory.path().join("units-saved.mb");
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token":token,"path":destination}),
        )
        .await;
        assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
        let written = std::fs::read(&destination).unwrap();
        let offset = original
            .windows(8)
            .position(|word| word == 1.5f64.to_be_bytes())
            .unwrap();
        let mut expected = original.to_vec();
        expected[offset..offset + 8].copy_from_slice(&3.0f64.to_be_bytes());
        assert_eq!(written, expected);
    }
    assert_eq!(
        std::fs::read(&source).unwrap().as_slice(),
        original.as_ref()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn rotate_commit_in_a_radian_scene_saves_the_expected_radians() {
    // An angle whose degree round trip is not exact, so only a typed radian path stores 0.25 + it.
    let angle = (1..100_000)
        .map(|i| f64::from(i) / 1000.0)
        .find(|angle| 0.25 + (angle * (180.0 / std::f64::consts::PI)).to_radians() != 0.25 + angle)
        .unwrap();
    let original = unit_scene(b"in", b"rad", [0.25, 0.0, 0.0], [1.5, 1.0, 1.0]);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("rotate-original.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"angle":angle}),
    )
    .await;
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    assert_eq!(
        inspected["active"]["value"].as_f64().unwrap().to_bits(),
        angle.to_bits()
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    let destination = directory.path().join("rotate-saved.mb");
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene_as",
        json!({"scene_token":token,"path":destination}),
    )
    .await;
    assert_eq!(saved.structured_content.unwrap()["outcome"], "saved");
    let written = std::fs::read(&destination).unwrap();
    let offset = original
        .windows(8)
        .position(|word| word == 0.25f64.to_be_bytes())
        .unwrap();
    let mut expected = original.to_vec();
    expected[offset..offset + 8].copy_from_slice(&(0.25 + angle).to_be_bytes());
    assert_eq!(written, expected);
    assert_eq!(
        std::fs::read(&source).unwrap().as_slice(),
        original.as_ref()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn declared_echo_does_not_survive_a_new_scene_that_reuses_the_manipulation_id() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let mut ids = Vec::new();
    for round in 0..2 {
        let token = editor.scene_token.to_string();
        let node = editor.selected().unwrap().to_decimal();
        let path: Vec<_> = editor
            .selected_path()
            .iter()
            .map(|id| id.to_decimal())
            .collect();
        let begin = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
        )
        .await
        .structured_content
        .unwrap();
        let id = begin["manipulation_id"].as_str().unwrap().to_owned();
        ids.push(id.clone());
        let inspected = inspect_manipulation(&mut editor, &context, &client).await;
        assert_eq!(inspected["active"]["value"], 0.0, "round {round}");
        if round == 1 {
            break;
        }
        call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"angle":33}),
        )
        .await;
        let inspected = inspect_manipulation(&mut editor, &context, &client).await;
        assert_eq!(inspected["active"]["value"], 33.0);
        call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
        )
        .await;
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
        call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"}),
        )
        .await;
        editor.create(Primitive::Cube);
        frame(&mut editor, &context, vec![]);
    }
    assert_eq!(ids[0], ids[1], "the repro needs the reused manipulation id");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// Rotates the first transform of a cm/degree scene about `axis` in `space` through the SDK and
/// returns the rotate channels before and after each requested angle.
async fn rotate_samples(
    rotate: [f64; 3],
    axis: &str,
    space: &str,
    angles: &[f64],
) -> (Vec<f64>, Vec<[f64; 3]>) {
    let original = unit_scene(b"cm", b"deg", rotate, [1.0, 1.0, 1.0]);
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("degrees.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let channels = |editor: &Editor| {
        let values = editor
            .history
            .scene()
            .node(target.node)
            .unwrap()
            .transform()
            .values();
        [values[3], values[4], values[5]]
    };
    let baseline = channels(&editor).to_vec();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":axis,"space":space}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let mut samples = Vec::new();
    for &angle in angles {
        call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"angle":angle}),
        )
        .await;
        samples.push(channels(&editor));
    }
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
    (baseline, samples)
}

#[tokio::test]
async fn object_x_ring_rotate_stores_exactly_baseline_plus_delta_in_a_degree_scene() {
    let angles: Vec<f64> = (1..=720).map(|step| f64::from(step) * 0.5).collect();
    let (baseline, samples) = rotate_samples([10.3, 0.0, 0.0], "x", "object", &angles).await;
    for (angle, values) in angles.iter().zip(&samples) {
        let expected = baseline[0] + angle;
        assert_eq!(values[0].to_bits(), expected.to_bits(), "{angle}");
        assert_eq!(values[1].to_bits(), baseline[1].to_bits(), "{angle}");
        assert_eq!(values[2].to_bits(), baseline[2].to_bits(), "{angle}");
    }
}

#[tokio::test]
async fn y_ring_and_world_rotates_are_exact_only_when_one_channel_changes() {
    let angles = [15.0, 30.0, 45.0, 60.0];
    // From zero, a Y ring turns rotateY alone, so it snaps to the exact delta.
    let (baseline, samples) = rotate_samples([0.0; 3], "y", "object", &angles).await;
    for (angle, values) in angles.iter().zip(&samples) {
        assert_eq!(
            values[1].to_bits(),
            (baseline[1] + angle).to_bits(),
            "{angle}"
        );
        assert_eq!(values[0].to_bits(), baseline[0].to_bits());
        assert_eq!(values[2].to_bits(), baseline[2].to_bits());
    }
    // The same from zero in world space, with an unrotated parent.
    let (baseline, samples) = rotate_samples([0.0; 3], "y", "world", &angles).await;
    for (angle, values) in angles.iter().zip(&samples) {
        assert_eq!(
            values[1].to_bits(),
            (baseline[1] + angle).to_bits(),
            "{angle}"
        );
    }
    // From a rotated start a Y ring changes all three channels: the result is computed, correct
    // to rounding but not claimed exact.
    let (baseline, samples) = rotate_samples([10.3, 20.0, 5.0], "y", "object", &angles).await;
    for (angle, values) in angles.iter().zip(&samples) {
        let changed = (0..3)
            .filter(|&i| values[i].to_bits() != baseline[i].to_bits())
            .count();
        assert!(changed >= 2, "{angle}: {values:?}");
    }
}

#[tokio::test]
async fn rotate_and_scale_sdk_never_commit_real_native_text_drafts() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    context.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(&mut editor, &context, vec![]);
    frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("human".into())],
    );
    assert!(editor.has_draft());
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let before = editor.history.scene().clone();
    let name = editor.name.clone();
    for (tool, args) in [
        (
            "set_active_tool",
            json!({"scene_token":token,"tool":"rotate"}),
        ),
        (
            "set_active_tool",
            json!({"scene_token":token,"tool":"scale"}),
        ),
        (
            "begin_manipulation",
            json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
        ),
        (
            "begin_manipulation",
            json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"uniform","space":"world"}),
        ),
        (
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":"1","angle":1}),
        ),
        (
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":"1","factor":2}),
        ),
        (
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":"1","finish":"commit"}),
        ),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_eq!(error_code(result), "busy_editing", "{tool}");
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.name, name);
        assert_eq!(
            context.memory(|memory| memory.focused()),
            Some(name_edit_id())
        );
        assert!(editor.history.active_manipulation().is_none());
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn rotate_and_scale_sdk_refuse_face_mode_and_absent_leaves_with_typed_errors() {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene_with_units};
    // A cube in face mode.
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
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"face"}),
    )
    .await;
    let before = editor.history.scene().clone();
    for arguments in [
        json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"x","space":"object"}),
    ] {
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            arguments,
        )
        .await;
        assert_eq!(error_code(refused), "invalid_input");
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
    }
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    for index in 0..3 {
        let kind = &inspected["kinds"][index];
        assert_eq!(kind["available"], false);
        assert!(
            kind["unavailable_reason"]
                .as_str()
                .unwrap()
                .contains("Face selection")
        );
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();

    // A binary scene with no rotate or scale leaves cannot be saved after an edit, so it is refused.
    let original = scene_with_units(
        &[
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"cm",
        b"deg",
    );
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("absent-leaves.mb");
    std::fs::write(&source, &original).unwrap();
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
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "select_node",
        json!({"scene_token":token,"node_id":node,"instance_path":path}),
    )
    .await;
    let before = editor.history.scene().clone();
    for (arguments, leaf) in [
        (
            json!({"scene_token":token,"kind":"rotate","node_id":node,"instance_path":path,"axis":"x","space":"object"}),
            "rotateX",
        ),
        (
            json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"x","space":"object"}),
            "scaleX",
        ),
    ] {
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            arguments,
        )
        .await;
        let error = &refused.structured_content.unwrap()["error"];
        assert_eq!(error["code"], "absent_channel_leaf");
        assert!(error["message"].as_str().unwrap().contains(leaf));
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
    }
    let inspected = inspect_manipulation(&mut editor, &context, &client).await;
    for index in 0..3 {
        assert_eq!(inspected["kinds"][index]["available"], false);
    }
    assert_eq!(
        std::fs::read(&source).unwrap().as_slice(),
        original.as_ref()
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
