use super::*;

#[tokio::test]
async fn sphere_creation_inspection_selection_and_guards_use_the_shared_editor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"sphere"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(created["outcome"], "created");
    let id = created["node_id"].as_str().unwrap().to_owned();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"sphere"}),
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
    let names: Vec<_> = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["pSphere1", "pSphere2"]);
    for shape in inspected["inspection"]["shapes"].as_array().unwrap() {
        assert_eq!(
            (&shape["points"], &shape["edges"], &shape["faces"]),
            (&json!(382), &json!(780), &json!(400))
        );
        assert_eq!(shape["smoothing_known"], true);
    }
    let node = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap();
    let shape = node["shape_ids"][0].as_str().unwrap().to_owned();
    let mut path = node["instance_paths"][0].as_array().unwrap().clone();
    path.push(json!(shape));
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"face"}),
    )
    .await;
    let selected = call(
        &mut editor,
        &context,
        client.peer(),
        "select_face",
        json!({"scene_token":token,"target":{"node_id":id,"shape_id":shape,"instance_path":path,"face_index":0}}),
    )
    .await;
    assert_eq!(selected.is_error, Some(false));
    assert_eq!(
        editor
            .remembered_face()
            .as_ref()
            .map(|face| face.face_index),
        Some(0)
    );
    let before = editor.history.scene().clone();
    let pole = call(
        &mut editor,
        &context,
        client.peer(),
        "extrude_face",
        json!({"scene_token":token,"node_id":id,"shape_id":shape,"instance_path":path,"face_index":0,"distance":1}),
    )
    .await;
    assert_eq!(
        pole.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &before);
    let body = call(
        &mut editor,
        &context,
        client.peer(),
        "extrude_face",
        json!({"scene_token":token,"node_id":id,"shape_id":shape,"instance_path":path,"face_index":100,"distance":0.25}),
    )
    .await;
    assert_eq!(body.structured_content.unwrap()["outcome"], "changed");
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    assert_eq!(view.instances().len(), 2);
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
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
    assert_eq!(pending["outcome"], "pending_confirmation");
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"sphere"}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sphere_creation_on_opened_maya_scenes_is_refused_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("synthetic.mb");
    std::fs::write(&binary, binary_fixture::specimen()).unwrap();
    let ascii = directory.path().join("synthetic.ma");
    std::fs::write(&ascii, ascii_fixture::specimen()).unwrap();
    for (path, message) in [(binary, "unsupported"), (ascii, "read-only")] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let original = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":original,"path":path.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(opened["outcome"], "document_applied");
        let token = opened["scene_token"].as_str().unwrap().to_owned();
        let baseline = editor.history.scene().clone();
        let stale = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":original,"primitive":"sphere"}),
        )
        .await;
        assert_eq!(
            stale.structured_content.unwrap()["error"]["code"],
            "stale_scene"
        );
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"sphere"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(result["error"]["code"], "scene_error");
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains(message),
            "{result}"
        );
        assert_eq!(editor.history.scene(), &baseline);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
