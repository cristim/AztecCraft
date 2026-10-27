use super::*;

const RED: [f64; 3] = [1.0, 0.0, 0.0];

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> Value {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments)
        .await
        .structured_content
        .unwrap()
}

async fn inspect_json(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn base_rgb(editor: &Editor) -> Vec<[u8; 3]> {
    ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024)
        .unwrap()
        .instances()
        .iter()
        .map(|instance| instance.base_rgb())
        .collect()
}

#[tokio::test]
async fn session_material_sdk_create_assign_recolour_inspect_undo_redo() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    let grey = [[83, 83, 83]];
    assert_eq!(base_rgb(&editor), grey);
    let before = inspect_json(&mut editor, &context, &client).await;
    assert_eq!(before["session_materials"].as_array().unwrap().len(), 1);
    assert_eq!(before["session_materials"][0]["editable"], false);
    assert_eq!(before["shapes"][0]["session_material"], "0");

    let created = tool(
        &mut editor,
        &context,
        &client,
        "create_session_material",
        json!({"color": RED}),
    )
    .await;
    assert_eq!(
        created,
        json!({"outcome":"session_material","material_id":"1"})
    );
    assert_eq!(base_rgb(&editor), grey, "creating alone assigns nothing");
    let assigned = tool(
        &mut editor,
        &context,
        &client,
        "assign_session_material",
        json!({"shape_id": shape, "material_id": "1"}),
    )
    .await;
    assert_eq!(assigned["material_id"], "1");
    assert_eq!(base_rgb(&editor), [[255, 0, 0]]);
    let again = tool(
        &mut editor,
        &context,
        &client,
        "assign_session_material",
        json!({"shape_id": shape, "material_id": "1"}),
    )
    .await;
    assert_eq!(again["outcome"], "unchanged");

    let inspection = inspect_json(&mut editor, &context, &client).await;
    assert_eq!(inspection["shapes"][0]["session_material"], "1");
    assert_eq!(inspection["session_materials"][1]["color"], json!(RED));
    assert_eq!(inspection["session_materials"][1]["editable"], true);
    assert_eq!(
        inspection["session_materials"][1]["shading_group"],
        "lambert2SG"
    );
    assert_eq!(inspection["session_materials"][1]["name"], "lambert2");

    let recoloured = tool(
        &mut editor,
        &context,
        &client,
        "set_session_material_color",
        json!({"material_id": "1", "color": [0.0, 1.0, 0.0]}),
    )
    .await;
    assert_eq!(recoloured["outcome"], "changed");
    assert_eq!(base_rgb(&editor), [[0, 255, 0]]);
    let same = tool(
        &mut editor,
        &context,
        &client,
        "set_session_material_color",
        json!({"material_id": "1", "color": [0.0, 1.0, 0.0]}),
    )
    .await;
    assert_eq!(same["outcome"], "unchanged");

    // Undo walks back colour, assignment and creation, one step each.
    for expected in [vec![[255, 0, 0]], grey.to_vec(), grey.to_vec()] {
        tool(&mut editor, &context, &client, "undo", json!({})).await;
        assert_eq!(base_rgb(&editor), expected);
    }
    assert_eq!(editor.history.scene().session_materials().len(), 1);
    for _ in 0..3 {
        tool(&mut editor, &context, &client, "redo", json!({})).await;
    }
    assert_eq!(base_rgb(&editor), [[0, 255, 0]]);

    // Assign with an inline colour is a single undo step that also creates the material.
    let inline = tool(
        &mut editor,
        &context,
        &client,
        "assign_session_material",
        json!({"shape_id": shape, "color": [0.0, 0.0, 1.0]}),
    )
    .await;
    assert_eq!(inline["material_id"], "2");
    assert_eq!(base_rgb(&editor), [[0, 0, 255]]);
    let created = &editor.history.scene().session_materials()[2];
    assert_eq!(created.name(), "lambert3");
    assert_eq!(
        editor.history.scene().session_shading_groups()[2].name(),
        "lambert3SG"
    );
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(base_rgb(&editor), [[0, 255, 0]]);
    assert_eq!(editor.history.scene().session_materials().len(), 2);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn session_material_sdk_refusals_are_typed_and_leave_state_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "create_session_material",
        json!({"color": RED}),
    )
    .await;
    let scene = editor.history.scene().clone();
    let can_undo = editor.history.can_undo();
    let can_redo = editor.history.can_redo();
    for (name, arguments, code) in [
        (
            "assign_session_material",
            json!({"shape_id": shape, "material_id": "7"}),
            "unknown_session_material",
        ),
        (
            "set_session_material_color",
            json!({"material_id": "7", "color": RED}),
            "unknown_session_material",
        ),
        (
            "set_session_material_color",
            json!({"material_id": "0", "color": RED}),
            "initial_session_material_locked",
        ),
        (
            "create_session_material",
            json!({"color": [1.5, 0.0, 0.0]}),
            "invalid_session_color",
        ),
        (
            "assign_session_material",
            json!({"shape_id": shape, "color": [0.0, -0.1, 0.0]}),
            "invalid_session_color",
        ),
        (
            "set_session_material_color",
            json!({"material_id": "1", "color": [0.0, 0.0, 2.0]}),
            "invalid_session_color",
        ),
        (
            "assign_session_material",
            json!({"shape_id": "9999", "material_id": "1"}),
            "unknown_shape",
        ),
    ] {
        let result = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(result["error"]["code"], code, "{name}");
        assert_eq!(editor.history.scene(), &scene, "{name}");
        assert_eq!(editor.history.can_undo(), can_undo, "{name}");
        assert_eq!(editor.history.can_redo(), can_redo, "{name}");
    }
    for (name, arguments) in [
        ("assign_session_material", json!({"shape_id": shape})),
        (
            "assign_session_material",
            json!({"shape_id": shape, "material_id": "1", "color": RED}),
        ),
        ("create_session_material", json!({"color": [1, 0]})),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(editor.scene_token.to_string());
        let rejected = call_response(&mut editor, &context, client.peer(), name, arguments).await;
        assert!(rejected.is_err(), "{name}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn session_material_sdk_sourced_binary_and_ascii_scenes_refuse_with_typed_codes() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("material.mb");
    std::fs::write(
        &binary,
        appearance_fixture::appearance_specimen_with_users(2),
    )
    .unwrap();
    let ascii = directory.path().join("curve.ma");
    std::fs::write(&ascii, ascii_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for (path, code) in [(&binary, "unsupported_edit"), (&ascii, "read_only_format")] {
        let opened = tool(
            &mut editor,
            &context,
            &client,
            "open_scene",
            json!({"path": path}),
        )
        .await;
        assert_eq!(opened["outcome"], "document_applied");
        let scene = editor.history.scene().clone();
        let shape = editor
            .history
            .scene()
            .shapes()
            .first()
            .map_or("1".to_owned(), |shape| shape.id().to_decimal());
        for (name, arguments) in [
            ("create_session_material", json!({"color": RED})),
            (
                "assign_session_material",
                json!({"shape_id": shape, "color": RED}),
            ),
            (
                "set_session_material_color",
                json!({"material_id": "1", "color": RED}),
            ),
        ] {
            let result = tool(&mut editor, &context, &client, name, arguments).await;
            assert_eq!(result["error"]["code"], code, "{name}");
            assert_eq!(editor.history.scene(), &scene);
            assert!(!editor.history.can_undo());
        }
        let inspection = inspect_json(&mut editor, &context, &client).await;
        assert_eq!(inspection["session_materials"], json!([]));
        for shape in inspection["shapes"].as_array().unwrap() {
            assert_eq!(shape["session_material"], Value::Null);
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn session_material_sdk_guards_stale_token_draft_modal_close_and_active_move() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "create_session_material",
        json!({"color": RED}),
    )
    .await;
    let calls = |shape: &str| {
        [
            ("create_session_material", json!({"color": RED})),
            (
                "assign_session_material",
                json!({"shape_id": shape, "material_id": "1"}),
            ),
            (
                "set_session_material_color",
                json!({"material_id": "1", "color": [0.0, 1.0, 0.0]}),
            ),
        ]
    };
    let scene = editor.history.scene().clone();

    for (name, mut arguments) in calls(&shape) {
        arguments["scene_token"] = json!(Uuid::new_v4().to_string());
        let result = call(&mut editor, &context, client.peer(), name, arguments)
            .await
            .structured_content
            .unwrap();
        assert_eq!(result["error"]["code"], "stale_scene", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);

    editor.channels[0] = "2".into();
    for (name, arguments) in calls(&shape) {
        let result = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(result["error"]["code"], "busy_editing", "draft {name}");
    }
    assert_eq!(editor.channels[0], "2");
    assert_eq!(editor.history.scene(), &scene);
    editor.channels = Default::default();
    editor.sync_fields();

    // Active move.
    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"node_id": node, "instance_path": path, "kind": "move", "axis": "x"}),
    )
    .await;
    let preview = editor.history.scene().clone();
    for (name, arguments) in calls(&shape) {
        let result = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(result["error"]["code"], "busy_manipulation", "move {name}");
    }
    assert_eq!(editor.history.scene(), &preview);
    let move_id = editor
        .history
        .active_manipulation()
        .unwrap()
        .id
        .to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "finish_manipulation",
        json!({"manipulation_id": move_id, "finish": "cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &scene);

    // Modal / closing.
    editor.request_close();
    for (name, arguments) in calls(&shape) {
        let result = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(result["error"]["code"], "busy_editing", "close {name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn session_material_sdk_table_cap_is_a_typed_limit_and_inspection_still_fits() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let cap = azteccraft::native_scene::MAX_SESSION_MATERIALS;
    for _ in 1..cap {
        editor
            .history
            .execute(azteccraft::history::Command::CreateSessionMaterial { color: RED })
            .unwrap();
    }
    let scene = editor.history.scene().clone();
    let limit = tool(
        &mut editor,
        &context,
        &client,
        "create_session_material",
        json!({"color": RED}),
    )
    .await;
    assert_eq!(limit["error"]["code"], "session_material_limit");
    assert_eq!(editor.history.scene(), &scene);
    let inspection = inspect_json(&mut editor, &context, &client).await;
    assert_eq!(
        inspection["session_materials"].as_array().unwrap().len(),
        cap
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
