use super::*;

#[tokio::test]
async fn torus_creation_naming_inspection_extrusion_and_guards_use_the_shared_editor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let mut ids = Vec::new();
    for name in [None, Some("pTorus3"), None] {
        let mut arguments = json!({"scene_token":token,"primitive":"torus"});
        if let Some(name) = name {
            arguments["name"] = json!(name);
        }
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            arguments,
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(created["outcome"], "created");
        ids.push(created["node_id"].as_str().unwrap().to_owned());
    }
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
    let nodes = inspected["inspection"]["nodes"].as_array().unwrap();
    let names: Vec<_> = nodes.iter().map(|n| n["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["pTorus1", "pTorus3", "pTorus2"]);
    for shape in inspected["inspection"]["shapes"].as_array().unwrap() {
        assert_eq!(
            (&shape["points"], &shape["edges"], &shape["faces"]),
            (&json!(400), &json!(800), &json!(400))
        );
        assert_eq!(shape["smoothing_known"], true);
    }
    let node = nodes.iter().find(|n| n["id"] == ids[0]).unwrap();
    let shape = node["shape_ids"][0].as_str().unwrap().to_owned();
    let mut path = node["instance_paths"][0].as_array().unwrap().clone();
    path.push(json!(shape));
    // Every torus face is a quad, so extrusion works on the first one.
    let before = editor.history.scene().clone();
    let extruded = call(
        &mut editor,
        &context,
        client.peer(),
        "extrude_face",
        json!({"scene_token":token,"node_id":ids[0],"shape_id":shape,"instance_path":path,"face_index":0,"distance":0.25}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(extruded["outcome"], "changed");
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
        json!({"scene_token":token,"primitive":"torus"}),
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
async fn torus_creation_on_opened_maya_scenes_is_refused_without_mutation() {
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
        let token = opened["scene_token"].as_str().unwrap().to_owned();
        let baseline = editor.history.scene().clone();
        let stale = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":original,"primitive":"torus"}),
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
            json!({"scene_token":token,"primitive":"torus"}),
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
