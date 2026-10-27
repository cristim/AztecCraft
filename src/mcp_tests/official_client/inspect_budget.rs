use super::*;
use crate::app::History;

pub(super) fn editor_with_cubes(editor: &mut Editor, count: usize) {
    let mut scene = azteccraft::native_scene::NativeScene::default();
    for index in 0..count {
        scene
            .create(Primitive::Cube, &format!("cube_{index:03}"))
            .unwrap();
    }
    editor.history = History::new(scene);
    editor.sync_fields();
}

/// The worst-case estimate refused from 66 cubes; 120 serialize to about 105 KB.
#[tokio::test]
async fn inspect_scene_returns_120_cubes_through_the_protocol() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_cubes(&mut editor, 120);
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    assert_ne!(
        result.is_error,
        Some(true),
        "{:?}",
        result.structured_content
    );
    let inspection = &result.structured_content.unwrap()["inspection"];
    assert_eq!(inspection["nodes"].as_array().unwrap().len(), 120);
    assert_eq!(inspection["shapes"].as_array().unwrap().len(), 120);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// The Editor has no serialized measure, so `tool_reply` refuses an oversized selection. 4096
/// members (the most a selection holds) fit alone, about 225 KB; with 250 cubes they do not.
#[tokio::test]
async fn inspect_scene_sizes_a_maximal_selection_by_its_serialized_bytes() {
    use azteccraft::native_scene::{MAX_TRANSFORM_PATHS, TransformTarget};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for (cubes, refused) in [(1, false), (250, true)] {
        editor_with_cubes(&mut editor, cubes);
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
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await;
        assert_eq!(result.is_error == Some(true), refused, "{cubes} cubes");
        if refused {
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "too_large"
            );
        } else {
            let selection = &result.structured_content.unwrap()["inspection"]["selection"];
            assert_eq!(selection.as_array().unwrap().len(), MAX_TRANSFORM_PATHS);
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn inspect_cubes(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    count: usize,
) -> CallToolResult {
    editor_with_cubes(editor, count);
    call(editor, context, peer, "inspect_scene", json!({})).await
}

/// 250 cubes is the documented capacity; 400 is refused with `too_large`, and the connection
/// stays usable. The 250-cube result and its transport frame fit the byte caps.
#[tokio::test]
async fn inspect_scene_returns_250_cubes_and_refuses_400_without_closing() {
    use crate::editor_operations::{MAX_REPLY_BYTES, MAX_RESULT_BYTES};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let peer = client.peer();
    let ok = inspect_cubes(&mut editor, &context, peer, 250).await;
    assert_ne!(ok.is_error, Some(true), "{:?}", ok.structured_content);
    let reply = serde_json::to_vec(ok.structured_content.as_ref().unwrap()).unwrap();
    let result = serde_json::to_vec(&ok).unwrap();
    assert!(reply.len() <= MAX_REPLY_BYTES && result.len() <= MAX_RESULT_BYTES);
    // The frame is the result plus a small JSON-RPC envelope.
    assert!(result.len() + 256 <= crate::mcp::MAX_EGRESS_BYTES);
    let nodes = ok.structured_content.as_ref().unwrap()["inspection"]["nodes"].clone();
    assert_eq!(nodes.as_array().unwrap().len(), 250);

    let refused = inspect_cubes(&mut editor, &context, peer, 400).await;
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "too_large"
    );
    let again = inspect_cubes(&mut editor, &context, peer, 1).await;
    assert_ne!(again.is_error, Some(true));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// A name of 45 000 quotes appears on the node and its shape. The reply (about 180 KB) passes the
/// preflight and the 256 KiB reply cap, but the text copy escapes every `\"` as `\\\"`, doubling it, so
/// the result is about 3 times the reply (about 540 KB) and only the exact result check refuses it.
#[tokio::test]
async fn inspect_scene_refuses_a_quote_heavy_name_at_the_result_bound() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut legacy = azteccraft::scene::Scene::default();
    legacy
        .create(Primitive::Cube, &"\"".repeat(45_000))
        .unwrap();
    editor.history =
        History::new(azteccraft::native_scene::NativeScene::from_legacy(legacy).unwrap());
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "too_large"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
