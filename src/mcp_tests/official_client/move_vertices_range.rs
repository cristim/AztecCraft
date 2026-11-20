use super::move_vertices::{Ids, args};
use super::*;

/// A cube under a parent scaled by 1e30: a local move of 1e10 puts world coordinates past f32.
fn scaled_parent_scene(editor: &mut Editor) -> (Ids, String) {
    use azteccraft::history::History;
    use azteccraft::native_scene::{Channel, ReparentKeep};
    editor.create(Primitive::Cube);
    let parent = editor.selected().unwrap();
    editor.create(Primitive::Cube);
    let child = editor.selected().unwrap();
    let mut scene = editor.history.scene().clone();
    for axis in [Channel::ScaleX, Channel::ScaleY, Channel::ScaleZ] {
        scene.set_channel(parent, axis, 1.0e30).unwrap();
    }
    scene
        .reparent(child, Some(parent), ReparentKeep::Local)
        .unwrap();
    let shape = scene.shapes()[1].id();
    let ids = Ids {
        node: child.to_decimal(),
        shape: shape.to_decimal(),
        path: vec![
            scene.node(parent).unwrap().transform_id().to_decimal(),
            scene.node(child).unwrap().transform_id().to_decimal(),
            shape.to_decimal(),
        ],
    };
    editor.history = History::new(scene);
    (ids, editor.scene_token.to_string())
}

#[tokio::test]
async fn a_move_past_the_f32_world_range_is_world_out_of_range_and_changes_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (ids, token) = scaled_parent_scene(&mut editor);
    let before = editor.history.scene().clone();
    for space in ["object", "world"] {
        // World: 1e40 cm over the 1e30 parent scale is a 1e10 local move.
        let delta = match space {
            "world" => json!([1.0e40, 0.0, 0.0]),
            _ => json!([1.0e10, 0.0, 0.0]),
        };
        let mut a = args(&token, &ids, json!([0]), delta);
        a["space"] = json!(space);
        let refused = call(&mut editor, &context, client.peer(), "move_vertices", a).await;
        let reply = refused.structured_content.unwrap();
        assert_eq!(
            reply["error"]["code"], "world_out_of_range",
            "{space} {reply}"
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.history.can_undo());
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn a_small_move_in_an_already_undrawable_scene_is_not_refused() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let (ids, token) = scaled_parent_scene(&mut editor);
    let mut scene = editor.history.scene().clone();
    let shape = scene.shapes()[1].id();
    scene
        .move_vertices(shape, &[0], [1.0e10, 0.0, 0.0])
        .unwrap();
    editor.history = azteccraft::history::History::new(scene);
    // Vertex 0 is already past f32 world range; moving vertex 1 leaves the scene undrawable.
    let moved = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([1]), json!([1.0, 0.0, 0.0])),
    )
    .await;
    let reply = moved.structured_content.unwrap();
    assert_eq!(reply["outcome"], "changed", "{reply}");
    assert!(editor.history.can_undo());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
