use super::*;

async fn vertex_positions(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Peer<RoleClient>,
    node_id: &Value,
) -> Vec<[f64; 3]> {
    let inspected = call(editor, context, client, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let node = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| &n["id"] == node_id)
        .unwrap();
    let vertices = call(
        editor,
        context,
        client,
        "inspect_vertices",
        json!({"shape_id":node["shape_ids"][0]}),
    )
    .await
    .structured_content
    .unwrap();
    vertices["inspection"]["positions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| [0, 1, 2].map(|i| p[i].as_f64().unwrap()))
        .collect()
}

const SHAPES: [(&str, &str, &str, usize); 2] = [
    (
        "pyramid",
        "pPyramid",
        r#"{"side_length":2,"number_of_sides":5}"#,
        6,
    ),
    (
        "prism",
        "pPrism",
        r#"{"side_length":2,"length":3,"number_of_sides":6,"axis":"x"}"#,
        12,
    ),
];

#[tokio::test]
async fn pyramid_and_prism_creation_naming_geometry_undo_redo_and_guards_use_the_shared_editor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    let mut ids = Vec::new();
    for (primitive, _, options, _) in SHAPES {
        let options: Value = serde_json::from_str(options).unwrap();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(created["outcome"], "created", "{primitive}");
        ids.push(created["node_id"].clone());
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
    let names: Vec<_> = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["pPyramid1", "pPrism1"]);
    for (id, (primitive, _, _, count)) in ids.iter().zip(SHAPES) {
        let p = vertex_positions(&mut editor, &context, client.peer(), id).await;
        assert_eq!(p.len(), count, "{primitive}");
        // Edge of the base polygon is the requested side length.
        let d =
            |a: [f64; 3], b: [f64; 3]| (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f64>().sqrt();
        assert!((d(p[0], p[1]) - 2.0).abs() < 1e-5, "{primitive}");
    }
    let after = editor.history.scene().clone();
    for _ in 0..2 {
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
    }
    assert_eq!(editor.history.scene(), &before);
    for _ in 0..2 {
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
    }
    assert_eq!(editor.history.scene(), &after);

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
    for (primitive, ..) in SHAPES {
        let busy = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive}),
        )
        .await;
        assert_eq!(
            busy.structured_content.unwrap()["error"]["code"],
            "busy_editing"
        );
        assert_eq!(editor.history.scene(), &after);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn pyramid_and_prism_creation_on_opened_maya_scenes_is_refused_without_mutation() {
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
        for primitive in ["pyramid", "prism"] {
            let stale = call(
                &mut editor,
                &context,
                client.peer(),
                "create_primitive",
                json!({"scene_token":original,"primitive":primitive}),
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
                json!({"scene_token":token,"primitive":primitive}),
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
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
