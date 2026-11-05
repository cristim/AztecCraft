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

async fn read(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    arguments: Value,
) -> Value {
    call(editor, context, client.peer(), "inspect_uvs", arguments)
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

#[tokio::test]
async fn uv_sdk_project_inspect_undo_delete() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes()[0].id().to_decimal();

    let empty = read(&mut editor, &context, &client, json!({"shape_id": shape})).await;
    assert_eq!(empty["uv_set"], Value::Null);
    assert_eq!(empty["unavailable_reason"], Value::Null);

    let args = json!({"shape_id": shape, "axis": "z"});
    let first = tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        args.clone(),
    )
    .await;
    assert_eq!(first.structured_content.unwrap()["outcome"], "changed");
    let again = tool(&mut editor, &context, &client, "project_uv_planar", args).await;
    assert_eq!(again.structured_content.unwrap()["outcome"], "unchanged");

    let read_back = read(&mut editor, &context, &client, json!({"shape_id": shape})).await;
    assert_eq!(read_back["uv_set"], "map1");
    assert_eq!(read_back["faces_total"], 6);
    assert_eq!(read_back["faces"].as_array().unwrap().len(), 6);
    assert_eq!(read_back["faces"][0].as_array().unwrap().len(), 4);
    assert_eq!(read_back["next_cursor"], Value::Null);
    let scene = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    assert_eq!(
        scene.structured_content.unwrap()["inspection"]["shapes"][0]["uv_set"],
        "map1"
    );

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    let undone = read(&mut editor, &context, &client, json!({"shape_id": shape})).await;
    assert_eq!(undone["uv_set"], Value::Null);
    tool(&mut editor, &context, &client, "redo", json!({})).await;

    let args = json!({"shape_id": shape});
    let deleted = tool(&mut editor, &context, &client, "delete_uvs", args.clone()).await;
    assert_eq!(deleted.structured_content.unwrap()["outcome"], "changed");
    let absent = tool(&mut editor, &context, &client, "delete_uvs", args).await;
    assert_eq!(absent.structured_content.unwrap()["outcome"], "unchanged");

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn uv_sdk_refusals_keep_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Plane);
    let shape = editor.history.scene().shapes()[0].id();
    let id = shape.to_decimal();
    let before = editor.history.scene().clone();

    let flat = tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": id, "axis": "x"}),
    )
    .await;
    assert_eq!(flat.is_error, Some(true));
    assert_eq!(error_code(&flat), "degenerate_uv_projection");
    assert!(*editor.history.scene() == before);

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "project_uv_planar",
        json!({"scene_token": Uuid::new_v4().to_string(), "shape_id": id}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");

    let missing = tool(
        &mut editor,
        &context,
        &client,
        "delete_uvs",
        json!({"shape_id": "999999"}),
    )
    .await;
    assert_eq!(missing.is_error, Some(true));
    assert!(*editor.history.scene() == before);

    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn uv_sdk_extrude_is_refused_while_a_shape_has_uvs() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let node = editor.history.scene().nodes()[0].id().to_decimal();
    let transform = editor.history.scene().nodes()[0]
        .transform_id()
        .to_decimal();
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": shape}),
    )
    .await;
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "extrude_face",
        json!({
            "node_id": node,
            "shape_id": shape,
            "instance_path": [transform, shape],
            "face_index": 0,
            "distance": 1,
        }),
    )
    .await;
    assert_eq!(error_code(&refused), "uvs_block_topology_edit");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn uv_sdk_pages_a_large_set_and_a_cursor_goes_stale() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let created = tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({
            "primitive": "plane",
            "options": {"subdivisions_width": 128, "subdivisions_height": 128},
        }),
    )
    .await;
    assert_ne!(created.is_error, Some(true), "{created:?}");
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": shape}),
    )
    .await;
    let mut cursor = Value::Null;
    let mut seen = 0;
    let mut first_cursor = None;
    loop {
        let mut args = json!({"shape_id": shape});
        if !cursor.is_null() {
            args["cursor"] = cursor.clone();
        }
        let page = read(&mut editor, &context, &client, args).await;
        assert_eq!(page["face_offset"], seen);
        let faces = page["faces"].as_array().unwrap();
        assert!(!faces.is_empty());
        assert!(faces.iter().flat_map(|f| f.as_array().unwrap()).count() <= 4096);
        seen += faces.len();
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
        first_cursor.get_or_insert_with(|| cursor.clone());
    }
    assert_eq!(seen, 128 * 128);

    tool(
        &mut editor,
        &context,
        &client,
        "delete_uvs",
        json!({"shape_id": shape}),
    )
    .await;
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_uvs",
        json!({"shape_id": shape, "cursor": first_cursor.unwrap()}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn uv_sdk_reads_a_128_by_128_sphere_with_long_decimals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let created = tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({
            "primitive": "sphere",
            "options": {"subdivisions_axis": 128, "subdivisions_height": 128},
        }),
    )
    .await;
    assert_ne!(created.is_error, Some(true), "{created:?}");
    let shape = editor.history.scene().shapes()[0].id().to_decimal();
    let projected = tool(
        &mut editor,
        &context,
        &client,
        "project_uv_planar",
        json!({"shape_id": shape}),
    )
    .await;
    assert_ne!(projected.is_error, Some(true), "{projected:?}");
    let mut cursor = Value::Null;
    let mut seen = 0;
    loop {
        let mut args = json!({"shape_id": shape});
        if !cursor.is_null() {
            args["cursor"] = cursor.clone();
        }
        let reply = call(&mut editor, &context, client.peer(), "inspect_uvs", args).await;
        assert_ne!(reply.is_error, Some(true), "{:?}", reply.structured_content);
        let page = reply.structured_content.unwrap()["inspection"].clone();
        assert_eq!(page["scene_token"], editor.scene_token.to_string());
        seen += page["faces"].as_array().unwrap().len();
        cursor = page["next_cursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    assert_eq!(
        seen,
        editor.history.scene().shapes()[0].mesh().faces().len()
    );
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[test]
fn a_full_page_of_the_longest_uvs_fits_the_reply_limit() {
    use crate::editor_operations::{MAX_UV_CORNERS_PER_PAGE, UvInspection};
    let corner = [1.234_567_890_123_456_7e-308, 1.234_567_890_123_456_7e-308];
    let page = OperationReply::Uvs {
        inspection: Box::new(UvInspection {
            scene_token: Uuid::new_v4().to_string(),
            revision: u64::MAX,
            shape_id: u64::MAX.to_string(),
            uv_set: Some("map1"),
            unavailable_reason: None,
            faces_total: usize::MAX,
            face_offset: usize::MAX,
            // Triangles carry the most brackets per corner.
            faces: vec![vec![corner; 3]; MAX_UV_CORNERS_PER_PAGE / 3],
            next_cursor: Some(
                "v1.00000000-0000-0000-0000-000000000000.ffffffffffffffff.ffff".into(),
            ),
        }),
    };
    let bytes = serde_json::to_string(&page).unwrap().len();
    assert!(bytes < 256 * 1024, "{bytes}");
}
