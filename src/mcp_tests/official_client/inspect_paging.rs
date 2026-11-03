use super::*;
use crate::app::History;
use crate::editor_operations::MAX_RESULT_BYTES;
use azteccraft::{native_scene::NativeScene, scene::Scene};

/// Cheap to build, unlike `NativeScene::create` over a whole scene (cubic).
fn editor_with_legacy_cubes(editor: &mut Editor, count: usize) {
    let mut legacy = Scene::default();
    for index in 0..count {
        legacy
            .create(Primitive::Cube, &format!("cube_{index:04}"))
            .unwrap();
    }
    editor.history = History::new(NativeScene::from_legacy(legacy).unwrap());
    editor.sync_fields();
}

async fn inspect(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    arguments: Value,
) -> CallToolResult {
    call(editor, context, peer, "inspect_scene", arguments).await
}

fn error_code(result: &CallToolResult) -> &str {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

#[tokio::test]
async fn paging_reads_a_450_cube_scene_in_3_pages() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 450);
    let (mut nodes, mut shapes, mut pages) = (Vec::new(), Vec::new(), 0);
    let mut arguments = json!({"node_limit": 200});
    loop {
        let result = inspect(&mut editor, &context, client.peer(), arguments.clone()).await;
        assert_ne!(
            result.is_error,
            Some(true),
            "{:?}",
            result.structured_content
        );
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_RESULT_BYTES);
        assert!(serde_json::to_vec(&result).unwrap().len() + 256 <= crate::mcp::MAX_EGRESS_BYTES);
        let inspection = &result.structured_content.unwrap()["inspection"];
        let page = &inspection["page"];
        assert_eq!(page["nodes_total"], 450);
        assert_eq!(page["node_offset"], pages * 200);
        for (key, ids) in [("nodes", &mut nodes), ("shapes", &mut shapes)] {
            for item in inspection[key].as_array().unwrap() {
                ids.push(item["id"].as_str().unwrap().to_owned());
            }
        }
        pages += 1;
        assert!(pages <= 3, "cursor never ends");
        if page["truncated"] == false {
            assert!(page["next_cursor"].is_null());
            break;
        }
        arguments = json!({"node_limit": 200, "cursor": page["next_cursor"]});
    }
    assert_eq!(pages, 3);
    let order: Vec<String> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|node| node.id().to_decimal())
        .collect();
    assert_eq!(nodes, order);
    let mut unique = shapes.clone();
    unique.sort();
    unique.dedup();
    assert_eq!((shapes.len(), unique.len()), (450, 450));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn the_whole_450_cube_scene_is_still_too_large_unpaged_and_as_one_page() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 450);
    for arguments in [json!({}), json!({"node_limit": 450})] {
        let result = inspect(&mut editor, &context, client.peer(), arguments).await;
        assert_eq!(result.is_error, Some(true));
        assert_eq!(error_code(&result), "too_large");
    }
    editor_with_legacy_cubes(&mut editor, 1);
    let again = inspect(&mut editor, &context, client.peer(), json!({})).await;
    assert_ne!(again.is_error, Some(true));
    assert!(again.structured_content.unwrap()["inspection"]["page"].is_null());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn a_cursor_goes_stale_when_the_scene_changes_and_a_restart_works() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 6);
    let peer = client.peer();
    let first = inspect(&mut editor, &context, peer, json!({"node_limit": 2})).await;
    let inspection = first.structured_content.unwrap()["inspection"].clone();
    let cursor = inspection["page"]["next_cursor"].clone();
    let token = inspection["scene_token"].clone();
    let created = call(
        &mut editor,
        &context,
        peer,
        "create_primitive",
        json!({"scene_token": token, "primitive": "cube"}),
    )
    .await;
    assert_eq!(created.structured_content.unwrap()["outcome"], "created");
    let stale = inspect(
        &mut editor,
        &context,
        peer,
        json!({"node_limit": 2, "cursor": cursor}),
    )
    .await;
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(error_code(&stale), "stale_scene");
    let fresh = inspect(&mut editor, &context, peer, json!({"node_limit": 2})).await;
    assert_ne!(fresh.is_error, Some(true));
    // A new document token also invalidates it.
    let cursor = fresh.structured_content.unwrap()["inspection"]["page"]["next_cursor"].clone();
    editor.scene_token = uuid::Uuid::new_v4();
    let stale = inspect(
        &mut editor,
        &context,
        peer,
        json!({"node_limit": 2, "cursor": cursor}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn unpaged_output_has_no_page_key_and_keeps_its_nodes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 1);
    let result = inspect(&mut editor, &context, client.peer(), json!({})).await;
    let inspection = &result.structured_content.unwrap()["inspection"];
    assert!(inspection.as_object().unwrap().get("page").is_none());
    assert_eq!(inspection["nodes"].as_array().unwrap().len(), 1);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn bad_paging_arguments_are_invalid_params_or_typed_errors() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 6);
    let peer = client.peer();
    let valid = "v1.00000000-0000-0000-0000-000000000000.0.0";
    for arguments in [
        json!({"node_limit": 0}),
        json!({"node_limit": -1}),
        json!({"node_limit": 4097}),
        json!({"node_limit": 1.5}),
        json!({"node_limit": "10"}),
        json!({"node_limit": u32::MAX}),
        json!({"cursor": valid}),
        json!({"node_limit": 2, "cursor": "garbage"}),
        json!({"node_limit": 2, "cursor": ""}),
        json!({"node_limit": 2, "extra": 1}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            peer,
            "inspect_scene",
            arguments.clone(),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {arguments}");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{arguments}"
        );
    }
    // A cursor for this document and revision but an offset past the last node.
    let first = inspect(&mut editor, &context, peer, json!({"node_limit": 2})).await;
    let cursor = first.structured_content.unwrap()["inspection"]["page"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let (head, _) = cursor.rsplit_once('.').unwrap();
    let forged = format!("{head}.7");
    let result = inspect(
        &mut editor,
        &context,
        peer,
        json!({"node_limit": 2, "cursor": forged}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(error_code(&result), "invalid_input");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// A fresh client can obtain a token from a 4096-member selection by asking for a small page.
/// The members are fabricated over 250 cubes (as in `inspect_budget`) so the UI frames the
/// protocol loop runs stay cheap.
#[tokio::test]
async fn a_full_selection_is_inspectable_with_a_small_node_limit() {
    use azteccraft::native_scene::{MAX_TRANSFORM_PATHS, TransformTarget};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_legacy_cubes(&mut editor, 250);
    let path = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .clone();
    let members: Vec<_> = (1..=MAX_TRANSFORM_PATHS as u64)
        .map(|id| TransformTarget {
            node: azteccraft::scene::NodeId::from_decimal(&id.to_string()).unwrap(),
            path: path.clone(),
        })
        .collect();
    editor.set_selection_members_for_test(members);
    let peer = client.peer();
    let small = inspect(&mut editor, &context, peer, json!({"node_limit": 1})).await;
    assert_ne!(small.is_error, Some(true), "{:?}", small.structured_content);
    let inspection = &small.structured_content.unwrap()["inspection"];
    assert_eq!(inspection["selection"].as_array().unwrap().len(), 4096);
    assert!(inspection["scene_token"].is_string());
    for arguments in [json!({}), json!({"node_limit": 250})] {
        let big = inspect(&mut editor, &context, peer, arguments).await;
        assert_eq!(error_code(&big), "too_large");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
