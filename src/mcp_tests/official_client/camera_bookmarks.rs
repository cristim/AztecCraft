use super::*;

fn code(result: &CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn outcome(result: &CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["outcome"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn sdk_bookmarks_round_trip_the_inspected_camera_exactly() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.history_step(false);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let peer = client.peer();
    let pose = json!({"scene_token":token,"target":[1.5,-2.25,0.125],"distance":6.5,"yaw_degrees":123.456,"pitch_degrees":-33.3});
    call(&mut editor, &context, peer, "set_camera_pose", pose).await;
    let saved = editor.camera.clone();
    let created = call(
        &mut editor,
        &context,
        peer,
        "create_camera_bookmark",
        json!({"scene_token":token,"name":"hero"}),
    )
    .await;
    assert_eq!(outcome(&created), "changed");
    call(
        &mut editor,
        &context,
        peer,
        "orbit_camera",
        json!({"scene_token":token,"radians":[0.7,0.2]}),
    )
    .await;
    assert!(!editor.camera.same_capture(&saved));
    let applied = call(
        &mut editor,
        &context,
        peer,
        "apply_camera_bookmark",
        json!({"scene_token":token,"name":"hero"}),
    )
    .await;
    assert_eq!(outcome(&applied), "changed");
    assert!(editor.camera.same_capture(&saved));
    let again = call(
        &mut editor,
        &context,
        peer,
        "apply_camera_bookmark",
        json!({"scene_token":token,"name":"hero"}),
    )
    .await;
    assert_eq!(outcome(&again), "unchanged");

    let listed = call(
        &mut editor,
        &context,
        peer,
        "list_camera_bookmarks",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    let inspection = call(&mut editor, &context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let camera = &inspection["inspection"]["camera"];
    let entry = &listed["bookmarks"][0];
    assert_eq!(entry["name"], "hero");
    for field in ["target", "distance", "yaw_degrees", "pitch_degrees"] {
        assert_eq!(entry[field], camera[field], "{field}");
    }
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.scene_token.to_string(), token);

    let renamed = call(
        &mut editor,
        &context,
        peer,
        "rename_camera_bookmark",
        json!({"scene_token":token,"name":"hero","new_name":"villain"}),
    )
    .await;
    assert_eq!(outcome(&renamed), "changed");
    let deleted = call(
        &mut editor,
        &context,
        peer,
        "delete_camera_bookmark",
        json!({"scene_token":token,"name":"villain"}),
    )
    .await;
    assert_eq!(outcome(&deleted), "changed");
    let listed = call(
        &mut editor,
        &context,
        peer,
        "list_camera_bookmarks",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(listed["bookmarks"], json!([]));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_bookmark_errors_schemas_and_guards() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let peer = client.peer();
    call(
        &mut editor,
        &context,
        peer,
        "create_camera_bookmark",
        json!({"scene_token":token,"name":"a"}),
    )
    .await;
    for (tool, arguments, expected) in [
        (
            "create_camera_bookmark",
            json!({"scene_token":token,"name":"a"}),
            "bookmark_exists",
        ),
        (
            "create_camera_bookmark",
            json!({"scene_token":token,"name":" a"}),
            "invalid_bookmark_name",
        ),
        (
            "create_camera_bookmark",
            json!({"scene_token":token,"name":"a\u{202e}"}),
            "invalid_bookmark_name",
        ),
        (
            "apply_camera_bookmark",
            json!({"scene_token":token,"name":"zz"}),
            "bookmark_not_found",
        ),
        (
            "delete_camera_bookmark",
            json!({"scene_token":token,"name":"zz"}),
            "bookmark_not_found",
        ),
        (
            "rename_camera_bookmark",
            json!({"scene_token":token,"name":"zz","new_name":"q"}),
            "bookmark_not_found",
        ),
        (
            "create_camera_bookmark",
            json!({"scene_token":Uuid::new_v4().to_string(),"name":"b"}),
            "stale_scene",
        ),
    ] {
        let result = call(&mut editor, &context, peer, tool, arguments).await;
        assert_eq!(code(&result), expected, "{tool}");
    }
    for (tool, arguments) in [
        ("create_camera_bookmark", json!({"scene_token":token})),
        ("create_camera_bookmark", json!({"name":"b"})),
        (
            "create_camera_bookmark",
            json!({"scene_token":token,"name":"b","extra":1}),
        ),
        (
            "rename_camera_bookmark",
            json!({"scene_token":token,"name":"a"}),
        ),
        ("list_camera_bookmarks", json!({"extra":1})),
    ] {
        assert!(
            call_response(&mut editor, &context, peer, tool, arguments)
                .await
                .is_err(),
            "{tool}"
        );
    }
    let mut long = "x".repeat(65);
    long.push('y');
    let result = call(
        &mut editor,
        &context,
        peer,
        "create_camera_bookmark",
        json!({"scene_token":token,"name":long}),
    )
    .await;
    assert_eq!(code(&result), "invalid_bookmark_name");

    // Human draft, then a pending file action (modal): every mutating tool is busy_editing.
    editor.channels[0] = "7".into();
    let all = [
        (
            "create_camera_bookmark",
            json!({"scene_token":token,"name":"b"}),
        ),
        (
            "rename_camera_bookmark",
            json!({"scene_token":token,"name":"a","new_name":"c"}),
        ),
        (
            "delete_camera_bookmark",
            json!({"scene_token":token,"name":"a"}),
        ),
        (
            "apply_camera_bookmark",
            json!({"scene_token":token,"name":"a"}),
        ),
    ];
    for (tool, arguments) in all.clone() {
        let result = call(&mut editor, &context, peer, tool, arguments).await;
        assert_eq!(code(&result), "busy_editing", "draft {tool}");
    }
    assert_eq!(editor.channels[0], "7");
    editor.sync_fields();
    editor.apply_operation(Operation::NewScene).unwrap();
    assert!(editor.files.is_modal());
    for (tool, arguments) in all {
        let result = call(&mut editor, &context, peer, tool, arguments).await;
        assert_eq!(code(&result), "busy_editing", "modal {tool}");
    }
    let listed = call(
        &mut editor,
        &context,
        peer,
        "list_camera_bookmarks",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(listed["bookmarks"].as_array().unwrap().len(), 1);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_bookmarks_are_bounded_and_cleared_by_new() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let peer = client.peer();
    for i in 0..azteccraft::camera_bookmarks::MAX_BOOKMARKS {
        let result = call(
            &mut editor,
            &context,
            peer,
            "create_camera_bookmark",
            json!({"scene_token":token,"name":format!("v{i}")}),
        )
        .await;
        assert_eq!(outcome(&result), "changed");
    }
    let over = call(
        &mut editor,
        &context,
        peer,
        "create_camera_bookmark",
        json!({"scene_token":token,"name":"extra"}),
    )
    .await;
    assert_eq!(code(&over), "bookmark_limit");
    let new = call(
        &mut editor,
        &context,
        peer,
        "new_scene",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(outcome(&new), "document_applied");
    let listed = call(
        &mut editor,
        &context,
        peer,
        "list_camera_bookmarks",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(listed["bookmarks"], json!([]));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
