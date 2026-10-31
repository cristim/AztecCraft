use super::*;
use azteccraft::native_scene::NativeError;

async fn inspected(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Peer<RoleClient>,
) -> Value {
    call(editor, context, client, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn error_code(result: &CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn sdk_builds_inspects_undoes_and_deletes_a_chain_through_the_editor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let root = call(
        &mut editor,
        &context,
        client.peer(),
        "create_joint",
        json!({"scene_token":token,"position":[1.0,2.0,3.0]}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(root["outcome"], "created");
    let root = root["node_id"].as_str().unwrap().to_owned();
    let child = call(
        &mut editor,
        &context,
        client.peer(),
        "create_joint",
        json!({"scene_token":token,"parent_id":root,"position":[4.0,2.0,3.0],"radius":2.5,"name":"elbow"}),
    )
    .await
    .structured_content
    .unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let relative = call(
        &mut editor,
        &context,
        client.peer(),
        "create_joint",
        json!({"scene_token":token,"parent_id":child,"position":[1.0,0.0,0.0],"relative":true}),
    )
    .await;
    assert_eq!(relative.structured_content.unwrap()["outcome"], "created");

    let scene = inspected(&mut editor, &context, client.peer()).await;
    let nodes = scene["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0]["name"], "joint1");
    assert_eq!(nodes[0]["joint"]["radius"], 1.0);
    assert_eq!(nodes[1]["name"], "elbow");
    assert_eq!(nodes[1]["joint"]["radius"], 2.5);
    assert_eq!(nodes[1]["transform"]["translation"], json!([3.0, 0.0, 0.0]));
    assert_eq!(nodes[2]["transform"]["translation"], json!([1.0, 0.0, 0.0]));
    assert_eq!(nodes[2]["instance_paths"][0].as_array().unwrap().len(), 3);
    assert_eq!(nodes[1]["shape_ids"], json!([]));
    assert_eq!(scene["selected_node"], nodes[2]["id"]);
    assert_eq!(
        scene["selected_instance_path"],
        nodes[2]["instance_paths"][0]
    );
    assert_eq!(scene["undo_label"], "Create");

    let framed = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(framed.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.scene().nodes().len(), 2);
    assert!(editor.selected().is_none());

    let deleted = call(
        &mut editor,
        &context,
        client.peer(),
        "delete_node",
        json!({"scene_token":token,"node_id":root}),
    )
    .await;
    assert_eq!(deleted.structured_content.unwrap()["outcome"], "changed");
    assert!(editor.history.scene().nodes().is_empty());
    assert!(!editor.history.scene().has_joints());
    let undone = call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(undone.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.scene().nodes().len(), 2);
    assert!(editor.history.scene().has_joints());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_refusals_are_typed_and_leave_the_scene_alone() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let mesh = editor.history.scene().nodes()[0].id().to_decimal();
    let scene = editor.history.scene().clone();
    for (arguments, code) in [
        (
            json!({"scene_token":token,"parent_id":mesh}),
            "unsupported_joint",
        ),
        (
            json!({"scene_token":token,"radius":0.0}),
            "unsupported_joint",
        ),
        (json!({"scene_token":token,"name":"1bad"}), "scene_error"),
        (
            json!({"scene_token":token,"parent_id":"999999"}),
            "scene_error",
        ),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "create_joint",
            arguments,
        )
        .await;
        assert_eq!(error_code(&result), code);
        assert_eq!(editor.history.scene(), &scene);
    }
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_joint",
            json!({"scene_token":token,"rotation":[0,0,0]}),
        )
        .await
        .is_err()
    );
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "create_joint",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(error_code(&refused), "invalid_input");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_stale_token_is_refused_for_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let stale = Uuid::new_v4().to_string();
    for (tool, arguments) in [
        ("create_joint", json!({"scene_token":stale})),
        ("set_menu_set", json!({"scene_token":stale,"set":"rigging"})),
    ] {
        let result = call(&mut editor, &context, client.peer(), tool, arguments).await;
        assert_eq!(error_code(&result), "stale_scene");
    }
    assert!(editor.history.scene().nodes().is_empty());
    assert_eq!(editor.menu_set, MenuSet::Modeling);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_menu_set_is_session_state_that_survives_new_and_leaves_the_document_alone() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    for (set, outcome) in [
        ("rigging", "changed"),
        ("rigging", "unchanged"),
        ("modeling", "changed"),
        ("rigging", "changed"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_menu_set",
            json!({"scene_token":token,"set":set}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        let state = inspected(&mut editor, &context, client.peer()).await;
        assert_eq!(state["menu_set"], set);
        assert_eq!(state["scene_token"], token);
        assert_eq!(state["dirty"], false);
        assert_eq!(state["can_undo"], false);
        assert_eq!(editor.history.scene(), &scene);
    }
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_menu_set",
            json!({"scene_token":token,"set":"animation"}),
        )
        .await
        .is_err()
    );
    let renewed = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(renewed["outcome"], "document_applied");
    let state = inspected(&mut editor, &context, client.peer()).await;
    assert_eq!(state["menu_set"], "rigging");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_save_refuses_a_scene_holding_joints() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_joint",
        json!({"scene_token":token}),
    )
    .await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("joints.mb");
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene_as",
        json!({"scene_token":token,"path":path}),
    )
    .await;
    assert!(result.structured_content.unwrap()["error"].is_object());
    assert!(!path.exists());
    assert_eq!(
        editor.history.scene().supported_view().err(),
        Some(NativeError::UnsupportedView("joints in legacy mesh view"))
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
