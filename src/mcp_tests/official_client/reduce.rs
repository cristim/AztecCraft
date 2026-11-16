use super::*;

fn error_code(result: &rmcp::model::CallToolResult) -> &str {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> rmcp::model::CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments).await
}

fn face_count(editor: &Editor, shape: NativeId) -> usize {
    editor
        .history
        .scene()
        .shape(shape)
        .unwrap()
        .mesh()
        .faces()
        .len()
}

#[tokio::test]
async fn reduce_mesh_is_one_undo_step_and_triangulates() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let shape = editor.history.scene().shapes()[0].id();
    let before = editor.history.scene().clone();

    let args = json!({"shape_id": shape.to_decimal(), "percent": 50});
    let reduced = tool(&mut editor, &context, &client, "reduce_mesh", args).await;
    assert_eq!(reduced.structured_content.unwrap()["outcome"], "changed");
    let after = editor.history.scene().clone();
    let mesh = after.shape(shape).unwrap().mesh();
    assert!(mesh.faces().len() < face_count_of(&before, shape));
    assert!(mesh.faces().iter().all(|f| f.outer().len() == 3));

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(*editor.history.scene() == before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert!(*editor.history.scene() == after);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

fn face_count_of(scene: &azteccraft::native_scene::NativeScene, shape: NativeId) -> usize {
    scene.shape(shape).unwrap().mesh().faces().len()
}

#[tokio::test]
async fn reduce_mesh_counts_reach_their_target() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();

    tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "target_vertices": 60}),
    )
    .await;
    let mesh = editor.history.scene().shape(shape).unwrap().mesh().clone();
    assert_eq!(mesh.point_count(), 60);
    tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "target_triangles": 50}),
    )
    .await;
    assert!(face_count(&editor, shape) <= 50);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_refusals_keep_the_scene_and_history() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();
    let before = editor.history.scene().clone();

    for (args, code) in [
        (
            json!({"shape_id": id, "percent": 0}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "percent": 100}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_vertices": 2}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_triangles": 0}),
            "invalid_reduce_target",
        ),
        (
            json!({"shape_id": id, "target_triangles": 100000}),
            "nothing_to_reduce",
        ),
        (
            json!({"shape_id": id, "target_triangles": 1}),
            "reduce_target_unreachable",
        ),
        (json!({"shape_id": "999999", "percent": 50}), "scene_error"),
    ] {
        let refused = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
        assert_eq!(refused.is_error, Some(true), "{args}");
        assert_eq!(error_code(&refused), code, "{args}");
        assert!(*editor.history.scene() == before, "{args}");
    }

    for args in [
        json!({"shape_id": id}),
        json!({"shape_id": id, "percent": 50, "target_vertices": 10}),
        json!({"shape_id": id, "percent": "half"}),
        json!({"shape_id": id, "percent": 50, "extra": 1}),
        json!({"shape_id": id, "target_triangles": -3}),
    ] {
        let mut args = args;
        args["scene_token"] = json!(editor.scene_token.to_string());
        let malformed =
            call_response(&mut editor, &context, client.peer(), "reduce_mesh", args).await;
        assert!(malformed.is_err());
        assert!(*editor.history.scene() == before);
    }

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "reduce_mesh",
        json!({"scene_token": Uuid::new_v4().to_string(), "shape_id": id, "percent": 50}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_is_refused_while_a_shape_has_uvs() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let id = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": id}),
    )
    .await;
    let before = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "reduce_mesh",
        json!({"shape_id": id, "percent": 50}),
    )
    .await;
    assert_eq!(error_code(&refused), "uvs_block_topology_edit");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reduce_mesh_respects_drafts_and_modals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Sphere);
    let id = editor.history.scene().shapes()[0].id().to_decimal();
    let before = editor.history.scene().clone();

    editor.channels[0] = "7".into();
    let args = json!({"shape_id": id, "percent": 50});
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args.clone()).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert_eq!(editor.channels[0], "7");
    assert!(*editor.history.scene() == before);
    editor.sync_fields();

    editor
        .apply_operation(Operation::NewScene)
        .expect("dirty scene asks first");
    assert!(editor.files.is_modal());
    let busy = tool(&mut editor, &context, &client, "reduce_mesh", args).await;
    assert_eq!(error_code(&busy), "busy_editing");
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}
