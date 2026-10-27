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

/// The Editor has no serialized measure, so an oversized selection is refused by `tool_reply`.
#[tokio::test]
async fn inspect_scene_refuses_a_maximal_selection_through_the_protocol() {
    use azteccraft::native_scene::{MAX_TRANSFORM_PATHS, TransformTarget};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor_with_cubes(&mut editor, 1);
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
    assert_eq!(result.is_error, Some(true));
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "too_large"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
