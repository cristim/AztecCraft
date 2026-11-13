use super::*;

struct Ids {
    node: String,
    shape: String,
    path: Vec<String>,
}

fn ids(editor: &Editor) -> Ids {
    let scene = editor.history.scene();
    let node = scene.nodes()[0].id();
    let shape = scene.shapes()[0].id();
    let transform = scene.node(node).unwrap().transform_id();
    Ids {
        node: node.to_decimal(),
        shape: shape.to_decimal(),
        path: vec![transform.to_decimal(), shape.to_decimal()],
    }
}

fn args(token: &str, ids: &Ids, vertices: Value, delta: Value) -> Value {
    json!({"scene_token":token,"node_id":ids.node,"shape_id":ids.shape,"instance_path":ids.path,"vertex_indices":vertices,"delta":delta})
}

fn point(editor: &Editor, index: usize) -> [f64; 3] {
    editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(index)
        .unwrap()
}

#[tokio::test]
async fn move_vertices_moves_only_the_named_points_and_undo_redo_are_exact() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    let before = editor.history.scene().clone();
    let original: Vec<_> = (0..8).map(|i| point(&editor, i)).collect();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([1, 4]), json!([0.25, -1.0, 2.0])),
    )
    .await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
    let moved = editor.history.scene().clone();
    for (index, old) in original.iter().enumerate() {
        let new = point(&editor, index);
        if [1, 4].contains(&index) {
            for (k, delta) in [0.25, -1.0, 2.0].into_iter().enumerate() {
                assert_eq!(new[k], f64::from((old[k] + delta) as f32));
            }
        } else {
            assert_eq!(&new, old);
        }
    }
    assert_eq!(
        moved.shapes()[0].mesh().edges(),
        before.shapes()[0].mesh().edges()
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Move"));
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
    assert_eq!(editor.history.scene(), &moved);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_vertices_refuses_bad_input_and_keeps_the_baseline() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    let before = editor.history.scene().clone();
    let undo_len = editor.history.undo_len();
    let cases = [
        args(&token, &ids, json!([8]), json!([1, 0, 0])),
        args(&token, &ids, json!([0, 0]), json!([1, 0, 0])),
        args(&token, &ids, json!([usize::MAX]), json!([1, 0, 0])),
        args(&token, &ids, json!([0]), json!([f64::MAX, 0, 0])),
        args(&token, &ids, json!([0]), json!([1e300, 0, 0])),
        {
            let mut a = args(&token, &ids, json!([0]), json!([1, 0, 0]));
            a["shape_id"] = json!("999999");
            a
        },
        {
            let mut a = args(&token, &ids, json!([0]), json!([1, 0, 0]));
            a["instance_path"] = json!([ids.path[1], ids.path[0]]);
            a
        },
    ];
    for case in cases {
        let result = call(&mut editor, &context, client.peer(), "move_vertices", case).await;
        let reply = result.structured_content.unwrap();
        assert!(reply.get("error").is_some(), "{reply}");
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.history.undo_len(), undo_len);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_vertices_is_refused_on_a_scene_opened_from_a_file() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("opened.mb");
    let source = binary_fixture::specimen();
    std::fs::write(&file, &source).unwrap();
    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":file.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    let opened = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"}))
        .await
        .structured_content
        .unwrap();
    let token = opened["scene_token"].as_str().unwrap().to_string();
    let ids = ids(&editor);
    let imported = editor.history.scene().clone();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([0]), json!([1, 0, 0])),
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &imported);
    assert!(!editor.is_dirty());
    assert!(std::fs::read(file).unwrap() == source);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_vertices_saved_to_binary_and_ascii_reopens_with_the_moved_points() {
    for extension in ["mb", "ma"] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.create(Primitive::Cube);
        let ids = ids(&editor);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(format!("scene.{extension}"));
        let token = editor.scene_token.to_string();
        let moved = call(
            &mut editor,
            &context,
            client.peer(),
            "move_vertices",
            args(&token, &ids, json!([2, 5]), json!([0.5, 0.0, -3.0])),
        )
        .await;
        assert_eq!(moved.structured_content.unwrap()["outcome"], "changed");
        let expected: Vec<_> = (0..8).map(|i| point(&editor, i)).collect();
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token":token,"path":path}),
        )
        .await;
        assert_eq!(
            saved.structured_content.unwrap()["outcome"],
            "saved",
            "{extension}"
        );
        let (mut other, other_client, other_server) = start(&context).await;
        let other_token = other.scene_token.to_string();
        let result = call(
            &mut other,
            &context,
            other_client.peer(),
            "open_scene",
            json!({"scene_token":other_token,"path":path.to_str().unwrap()}),
        )
        .await;
        assert!(result.structured_content.is_some());
        let reopened = &other.history.scene().shapes()[0];
        for (index, want) in expected.iter().enumerate() {
            assert_eq!(
                &reopened.mesh().effective_point_cm(index).unwrap(),
                want,
                "{extension} {index}"
            );
        }
        other_client.cancel().await.unwrap();
        drop(other);
        other_server.await.unwrap();
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
