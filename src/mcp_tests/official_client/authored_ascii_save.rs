//! Exercises the authored ASCII path through the actual Editor and official SDK.
use super::*;

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

#[tokio::test]
async fn authored_ascii_sdk_save_as_edit_save_and_history_preserve_document() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.ma");
    let created = tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    assert_eq!(created["outcome"], "created");
    let saved = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path":path}),
    )
    .await;
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    let first = std::fs::read(&path).unwrap();
    assert!(first.starts_with(b"//Maya ASCII 2026 scene"));
    let id = editor.history.scene().nodes()[0].id().to_decimal();
    let edited = tool(&mut editor, &context, &client, "set_transform", json!({"node_id":id,"translation":[2.5,0.0,0.0],"rotation_degrees":[0.0,0.0,0.0],"scale":[1.0,1.0,1.0]})).await;
    assert_eq!(edited["outcome"], "changed");
    let saved = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(saved["outcome"], "saved");
    assert!(!editor.is_dirty());
    let second = std::fs::read(&path).unwrap();
    assert_ne!(first, second);
    assert!(String::from_utf8(second.clone()).unwrap().contains("2.5"));
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(editor.is_dirty());
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert!(!editor.is_dirty());
    let saved = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(std::fs::read(&path).unwrap(), second);
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
    assert_eq!(inspection["inspection"]["document_origin"], "maya_ascii");
    assert_eq!(inspection["inspection"]["has_file"], true);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn authored_ascii_sdk_material_refusal_preserves_dirty_scene_and_destinations() {
    for already_saved in [false, true] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.ma");
        let created = tool(
            &mut editor,
            &context,
            &client,
            "create_primitive",
            json!({"primitive":"cube"}),
        )
        .await;
        assert_eq!(created["outcome"], "created");
        let original = if already_saved {
            let saved = tool(
                &mut editor,
                &context,
                &client,
                "save_scene_as",
                json!({"path":path}),
            )
            .await;
            assert_eq!(saved["outcome"], "saved");
            Some(std::fs::read(&path).unwrap())
        } else {
            None
        };
        let shape = editor.history.scene().shapes()[0].id().to_decimal();
        let assigned = tool(
            &mut editor,
            &context,
            &client,
            "assign_session_material",
            json!({"shape_id":shape,"color":[1.0,0.0,0.0]}),
        )
        .await;
        assert_eq!(assigned["material_id"], "1");
        assert!(editor.is_dirty());
        let scene = editor.history.scene().clone();
        let undo = editor.history.can_undo();
        let redo = editor.history.can_redo();
        let token = editor.scene_token;
        let error = if already_saved {
            tool(&mut editor, &context, &client, "save_scene", json!({})).await
        } else {
            tool(
                &mut editor,
                &context,
                &client,
                "save_scene_as",
                json!({"path":path}),
            )
            .await
        };
        assert_eq!(error["error"]["code"], "save_ascii_write");
        assert!(editor.is_dirty());
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.history.can_undo(), undo);
        assert_eq!(editor.history.can_redo(), redo);
        assert_eq!(editor.scene_token, token);
        match original {
            Some(bytes) => assert_eq!(std::fs::read(&path).unwrap(), bytes),
            None => assert!(!path.exists()),
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
