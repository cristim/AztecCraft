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
