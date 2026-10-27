use super::*;

fn shape_names(editor: &Editor) -> Vec<String> {
    editor
        .history
        .scene()
        .shapes()
        .iter()
        .map(|shape| shape.name().to_owned())
        .collect()
}

#[tokio::test]
async fn created_and_renamed_shapes_use_maya_names_over_the_protocol() {
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
    .await;
    let id = created.structured_content.unwrap()["node_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(shape_names(&editor), ["pCubeShape1"]);
    assert!(editor.history.undo());
    assert!(shape_names(&editor).is_empty());
    assert!(editor.history.redo());
    assert_eq!(shape_names(&editor), ["pCubeShape1"]);
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","name":"pCube7"}),
    )
    .await;
    assert_eq!(shape_names(&editor), ["pCubeShape1", "pCube7Shape"]);
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
    assert_eq!(inspected["inspection"]["shapes"][0]["name"], "pCubeShape1");
    assert_eq!(inspected["inspection"]["shapes"][1]["name"], "pCube7Shape");

    let renamed = call(
        &mut editor,
        &context,
        client.peer(),
        "rename_node",
        json!({"scene_token":token,"node_id":id,"name":"box"}),
    )
    .await;
    assert_eq!(renamed.is_error, Some(false));
    assert_eq!(shape_names(&editor), ["boxShape", "pCube7Shape"]);
    let edited = call(
        &mut editor,
        &context,
        client.peer(),
        "edit_node",
        json!({"scene_token":token,"node_id":id,"name":"crate7"}),
    )
    .await;
    assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(shape_names(&editor), ["crate7Shape", "pCube7Shape"]);
    assert!(editor.history.undo());
    assert_eq!(shape_names(&editor), ["boxShape", "pCube7Shape"]);

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
