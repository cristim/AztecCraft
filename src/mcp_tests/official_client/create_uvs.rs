use super::*;

const SUPPORTED: [&str; 6] = ["cube", "plane", "sphere", "cylinder", "cone", "torus"];

async fn create(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    mut arguments: Value,
) -> rmcp::model::CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(
        editor,
        context,
        client.peer(),
        "create_primitive",
        arguments,
    )
    .await
}

fn code(reply: &rmcp::model::CallToolResult) -> &str {
    reply.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

/// The UV read of the shape of the most recent node, and that shape's face and corner counts.
async fn uvs_of_last(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> (Value, usize, usize) {
    let mesh = editor.history.scene().shapes().last().unwrap().mesh();
    let (faces, corners) = (
        mesh.faces().len(),
        mesh.faces().iter().map(|f| f.outer().len()).sum(),
    );
    let shape = editor
        .history
        .scene()
        .shapes()
        .last()
        .unwrap()
        .id()
        .to_decimal();
    let read = call(
        editor,
        context,
        client.peer(),
        "inspect_uvs",
        json!({"shape_id": shape}),
    )
    .await
    .structured_content
    .unwrap()["inspection"]
        .clone();
    (read, faces, corners)
}

#[tokio::test]
async fn create_uvs_builds_a_matching_set_and_false_or_omitted_builds_none() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for primitive in SUPPORTED {
        for arguments in [
            json!({"primitive": primitive, "create_uvs": true}),
            json!({"primitive": primitive, "create_uvs": true, "options": {}}),
        ] {
            let made = create(&mut editor, &context, &client, arguments).await;
            assert_eq!(made.is_error, Some(false), "{primitive}");
            let (read, faces, corners) = uvs_of_last(&mut editor, &context, &client).await;
            assert_eq!(read["uv_set"], "map1", "{primitive}");
            assert_eq!(read["faces_total"], faces, "{primitive}");
            let listed: usize = read["faces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_array().unwrap().len())
                .sum();
            // Small primitives fit one page; the larger ones page, so compare the first page.
            if read["next_cursor"].is_null() {
                assert_eq!(listed, corners, "{primitive}");
            }
            editor.history.scene().validate().unwrap();
        }
        for arguments in [
            json!({"primitive": primitive, "create_uvs": false}),
            json!({"primitive": primitive}),
        ] {
            let made = create(&mut editor, &context, &client, arguments).await;
            assert_eq!(made.is_error, Some(false), "{primitive}");
            let (read, _, _) = uvs_of_last(&mut editor, &context, &client).await;
            assert_eq!(read["uv_set"], Value::Null, "{primitive}");
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn bad_create_uvs_values_and_the_platonic_refusal_leave_the_editor_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let state = |e: &Editor| {
        (
            e.history.undo_len(),
            e.history.redo_len(),
            e.selected(),
            e.selected_path().to_vec(),
            e.is_dirty(),
        )
    };
    let before = state(&editor);
    let token = editor.scene_token.to_string();
    for bad in [
        json!(null),
        json!("true"),
        json!(1),
        json!(0),
        json!([true]),
        json!({}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token": token, "primitive": "cube", "create_uvs": bad}),
        )
        .await;
        assert!(
            matches!(
                rejected,
                Err(rmcp::ServiceError::McpError(ref e)) if e.code == rmcp::model::ErrorCode::INVALID_PARAMS
            ),
            "{bad}"
        );
        assert_eq!(editor.history.scene(), &scene, "{bad}");
        assert_eq!(state(&editor), before, "{bad}");
    }
    // A misplaced key (inside options) and an unknown one are rejected too.
    for arguments in [
        json!({"scene_token": token, "primitive": "cube", "options": {"create_uvs": true}}),
        json!({"scene_token": token, "primitive": "cube", "createUVs": true}),
        json!({"scene_token": token, "primitive": "cube", "create_uvs": true, "uvs": true}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(
                rejected,
                Err(rmcp::ServiceError::McpError(ref e)) if e.code == rmcp::model::ErrorCode::INVALID_PARAMS
            ),
            "{arguments}"
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(state(&editor), before);
    }
    for options in [json!({}), json!({"solid": "icosahedron", "radius": 2})] {
        let refused = create(
            &mut editor,
            &context,
            &client,
            json!({"primitive": "platonic_solid", "create_uvs": true, "options": options}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(code(&refused), "unsupported_uv_primitive");
        let message = refused.structured_content.as_ref().unwrap()["error"]["message"]
            .as_str()
            .unwrap();
        assert!(message.contains("PlatonicSolid"), "{message}");
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(state(&editor), before);
    }
    for (primitive, name) in [
        ("pyramid", "Pyramid"),
        ("prism", "Prism"),
        ("helix", "Helix"),
        ("soccer_ball", "SoccerBall"),
    ] {
        let refused = create(
            &mut editor,
            &context,
            &client,
            json!({"primitive": primitive, "create_uvs": true}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true), "{primitive}");
        assert_eq!(code(&refused), "unsupported_uv_primitive", "{primitive}");
        let message = refused.structured_content.as_ref().unwrap()["error"]["message"]
            .as_str()
            .unwrap();
        assert!(message.contains(name), "{message}");
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(state(&editor), before);
    }
    // Without create_uvs the Platonic solid still builds.
    let plain = create(
        &mut editor,
        &context,
        &client,
        json!({"primitive": "platonic_solid"}),
    )
    .await;
    assert_eq!(plain.is_error, Some(false));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn create_uvs_obeys_the_scene_token_draft_modal_and_close_guards() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    let args =
        |token: &str| json!({"scene_token": token, "primitive": "sphere", "create_uvs": true});
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        args(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(code(&stale), "stale_scene");
    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        args(&token),
    )
    .await;
    assert_eq!(code(&draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    editor.sync_fields();
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token": token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        args(&token),
    )
    .await;
    assert_eq!(code(&modal), "busy_editing");
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();

    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.request_close();
    let closing = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        args(&token),
    )
    .await;
    assert_eq!(code(&closing), "busy_editing");
    assert!(editor.history.scene().shapes().is_empty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn created_uvs_block_mb_gltf_and_usd_with_typed_errors_but_not_obj_or_ma() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let made = create(
        &mut editor,
        &context,
        &client,
        json!({"primitive": "cube", "create_uvs": true}),
    )
    .await;
    assert_eq!(made.is_error, Some(false));
    let directory = tempfile::tempdir().unwrap();
    let path = |name: &str| directory.path().join(name);
    for (tool, file, expected) in [
        ("export_gltf", "a.glb", "export_unsupported"),
        ("export_usd", "a.usda", "export_unsupported"),
    ] {
        let reply = create_tool(
            &mut editor,
            &context,
            &client,
            tool,
            json!({"path": path(file)}),
        )
        .await;
        assert_eq!(reply.is_error, Some(true), "{tool}");
        assert_eq!(code(&reply), expected, "{tool}");
        assert!(!path(file).exists(), "{tool} wrote a file");
    }
    let obj = create_tool(
        &mut editor,
        &context,
        &client,
        "export_obj",
        json!({"path": path("a.obj")}),
    )
    .await;
    assert_eq!(obj.is_error, Some(false), "{obj:?}");
    assert!(
        std::fs::read_to_string(path("a.obj"))
            .unwrap()
            .contains("\nvt ")
    );
    #[cfg(unix)]
    {
        let mb = create_tool(
            &mut editor,
            &context,
            &client,
            "save_scene_as",
            json!({"path": path("a.mb")}),
        )
        .await;
        let error = &mb.structured_content.as_ref().unwrap()["error"];
        assert_eq!(error["code"], "save_unwritable", "{mb:?}");
        assert_eq!(error["reason"], "session_uvs", "{mb:?}");
        assert!(!path("a.mb").exists());
        let ma = create_tool(
            &mut editor,
            &context,
            &client,
            "save_scene_as",
            json!({"path": path("a.ma")}),
        )
        .await;
        assert_eq!(ma.is_error, Some(false), "{ma:?}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn create_tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> rmcp::model::CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments).await
}
