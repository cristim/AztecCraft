use super::*;
use azteccraft::primitive_options::{Axis, PrimitiveOptions};

pub(super) struct Ids {
    pub(super) node: String,
    pub(super) shape: String,
    pub(super) path: Vec<String>,
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

pub(super) fn args(token: &str, ids: &Ids, vertices: Value, delta: Value) -> Value {
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

#[cfg(unix)]
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

#[tokio::test]
async fn a_zero_delta_move_is_unchanged_and_adds_no_undo_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    let before = editor.history.scene().clone();
    let undo_len = editor.history.undo_len();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([0, 1]), json!([0.0, -0.0, 0.0])),
    )
    .await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "unchanged");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.undo_len(), undo_len);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn inspect_vertices_pages_with_a_revision_bound_cursor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.native_operation(Operation::Create {
        primitive: Primitive::Plane,
        options: Some(PrimitiveOptions::Plane {
            width: 1.0,
            height: 1.0,
            sub_width: 40,
            sub_height: 40,
            axis: Axis::Y,
        }),
        name: None,
    });
    let ids = ids(&editor);
    let total = editor.history.scene().shapes()[0].mesh().point_count();
    assert!(total > 1024);
    let first = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_vertices",
        json!({"shape_id":ids.shape}),
    )
    .await
    .structured_content
    .unwrap();
    let first = &first["inspection"];
    assert_eq!(first["vertices_total"], total);
    assert_eq!(first["vertex_offset"], 0);
    assert_eq!(first["positions"].as_array().unwrap().len(), 1024);
    assert_eq!(first["positions"][5], json!(point(&editor, 5)));
    let cursor = first["next_cursor"].as_str().unwrap().to_string();
    let second = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_vertices",
        json!({"shape_id":ids.shape,"cursor":cursor}),
    )
    .await
    .structured_content
    .unwrap();
    let second = &second["inspection"];
    assert_eq!(second["vertex_offset"], 1024);
    assert_eq!(second["positions"].as_array().unwrap().len(), total - 1024);
    assert_eq!(second["positions"][0], json!(point(&editor, 1024)));
    assert!(second["next_cursor"].is_null());
    // The scene changes: the cursor goes stale and nothing is read.
    let moved = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([0]), json!([1, 0, 0])),
    )
    .await;
    assert_eq!(moved.structured_content.unwrap()["outcome"], "changed");
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_vertices",
        json!({"shape_id":ids.shape,"cursor":cursor}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(stale["error"]["code"], "stale_scene");
    let wrong_shape = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_vertices",
        json!({"shape_id":"999999","cursor":cursor}),
    )
    .await
    .structured_content
    .unwrap();
    assert!(wrong_shape.get("error").is_some());
    let fresh = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_vertices",
        json!({"shape_id":ids.shape}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(
        fresh["inspection"]["positions"][0],
        json!(point(&editor, 0))
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn move_vertices_on_a_transformed_node_moves_along_the_mesh_axes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    let set = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":ids.node,"translation":[4.0,-2.0,1.0],"rotation_degrees":[30.0,45.0,60.0],"scale":[2.0,3.0,0.5]})).await;
    assert_eq!(set.structured_content.unwrap()["outcome"], "changed");
    let before = point(&editor, 2);
    let moved = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([2]), json!([1.0, 0.0, 0.0])),
    )
    .await;
    assert_eq!(moved.structured_content.unwrap()["outcome"], "changed");
    let after = point(&editor, 2);
    assert_eq!(after[0], f64::from((before[0] + 1.0) as f32));
    assert_eq!(&after[1..], &before[1..]);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn world_space_moves_by_the_world_vector_under_a_rotated_scaled_node_and_object_matches_omitted()
 {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    let set = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":ids.node,"translation":[4.0,-2.0,1.0],"rotation_degrees":[30.0,45.0,60.0],"scale":[2.0,3.0,0.5]})).await;
    assert_eq!(set.structured_content.unwrap()["outcome"], "changed");
    let world = |editor: &Editor, index: usize| {
        let scene = editor.history.scene();
        let node = scene.nodes()[0].id();
        let target = azteccraft::native_scene::TransformTarget {
            node,
            path: vec![scene.node(node).unwrap().transform_id()],
        };
        scene
            .component_move_frame(&target)
            .unwrap()
            .world_point_cm(point(editor, index))
    };
    let (before_moved, before_still) = (world(&editor, 2), world(&editor, 5));
    let before_local = point(&editor, 2);
    let mut world_args = args(&token, &ids, json!([2]), json!([1.0, 2.0, -3.0]));
    world_args["space"] = json!("world");
    let moved = call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        world_args,
    )
    .await;
    assert_eq!(moved.structured_content.unwrap()["outcome"], "changed");
    let delta = world(&editor, 2) - before_moved;
    for (got, want) in delta.to_array().into_iter().zip([1.0, 2.0, -3.0]) {
        assert!((got - want).abs() < 1e-3, "{delta:?}");
    }
    assert_eq!(world(&editor, 5), before_still);
    assert!(editor.history.undo());
    assert_eq!(point(&editor, 2), before_local);
    assert!(editor.history.redo());
    // Explicit `object` equals the omitted default, bit for bit.
    let after_world = editor.history.scene().clone();
    assert!(editor.history.undo());
    let mut explicit = args(&token, &ids, json!([2]), json!([1.0, 0.0, 0.0]));
    explicit["space"] = json!("object");
    call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        explicit,
    )
    .await;
    let object_explicit = point(&editor, 2);
    assert!(editor.history.undo());
    call(
        &mut editor,
        &context,
        client.peer(),
        "move_vertices",
        args(&token, &ids, json!([2]), json!([1.0, 0.0, 0.0])),
    )
    .await;
    assert_eq!(
        point(&editor, 2).map(f64::to_bits),
        object_explicit.map(f64::to_bits)
    );
    assert_ne!(&after_world, editor.history.scene());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn set_scale(editor: &mut Editor, scale: [f64; 3]) {
    use azteccraft::native_scene::Channel;
    let node = editor.history.scene().nodes()[0].id();
    let mut scene = editor.history.scene().clone();
    for (axis, value) in [Channel::ScaleX, Channel::ScaleY, Channel::ScaleZ]
        .into_iter()
        .zip(scale)
    {
        scene.set_channel(node, axis, value).unwrap();
    }
    editor.history = azteccraft::history::History::new(scene);
    editor.sync_fields();
}

async fn world_move(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::Peer<rmcp::RoleClient>,
    token: &str,
    ids: &Ids,
    delta: Value,
) -> Value {
    let mut a = args(token, ids, json!([2]), delta);
    a["space"] = json!("world");
    call(editor, context, client, "move_vertices", a)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn world_moves_refuse_a_singular_or_ill_conditioned_frame_and_a_non_finite_result() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    // Singular, and condition number 1e13 past the 1e12 limit: scene_error carries the reason.
    for scale in [[0.0, 1.0, 1.0], [1.0, 1.0, 1.0e-13]] {
        set_scale(&mut editor, scale);
        let before = editor.history.scene().clone();
        let (revision, undo_len) = (editor.history.revision(), editor.history.undo_len());
        let reply = world_move(
            &mut editor,
            &context,
            client.peer(),
            &token,
            &ids,
            json!([1.0, 0.0, 0.0]),
        )
        .await;
        assert_eq!(reply["error"]["code"], "scene_error", "{scale:?} {reply}");
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.history.revision(), revision);
        assert_eq!(editor.history.undo_len(), undo_len);
    }
    // A uniform 1e-13 scale is well conditioned, but 1e300 / 1e-13 overflows f64.
    set_scale(&mut editor, [1.0e-13; 3]);
    let before = editor.history.scene().clone();
    let (revision, undo_len) = (editor.history.revision(), editor.history.undo_len());
    let reply = world_move(
        &mut editor,
        &context,
        client.peer(),
        &token,
        &ids,
        json!([1.0e300, 0.0, 0.0]),
    )
    .await;
    assert_eq!(reply["error"]["code"], "scene_error", "{reply}");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.revision(), revision);
    assert_eq!(editor.history.undo_len(), undo_len);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn a_world_move_under_a_mirrored_scale_works() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    let ids = ids(&editor);
    set_scale(&mut editor, [-1.0, 1.0, 1.0]);
    let before = point(&editor, 2);
    let reply = world_move(
        &mut editor,
        &context,
        client.peer(),
        &token,
        &ids,
        json!([1.0, 0.0, 0.0]),
    )
    .await;
    assert_eq!(reply["outcome"], "changed", "{reply}");
    // Mirrored in X: a world +1 is a local -1.
    let after = point(&editor, 2);
    assert_eq!(after[0], f64::from((before[0] - 1.0) as f32));
    assert_eq!(&after[1..], &before[1..]);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
