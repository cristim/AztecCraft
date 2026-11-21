//! Issue #313 slice 1 over the official client: `reset_transformations` and
//! `freeze_transformations` through the real Editor protocol, with the results read back through
//! `inspect_scene` and the scene itself.
use super::pivot_edit::{cubes, inspected, reply, tool};
use super::*;
use azteccraft::native_scene::NativeScene;

pub(super) const TOOLS: [&str; 2] = ["reset_transformations", "freeze_transformations"];

/// Every mesh point of every shape in world centimetres, in shape order.
pub(super) fn world_points(scene: &NativeScene) -> Vec<Vec<[f64; 3]>> {
    let targets = scene.transform_paths().unwrap();
    scene
        .shapes()
        .iter()
        .map(|shape| {
            let parent = scene
                .dag_edges()
                .iter()
                .find(|edge| edge.child() == shape.id())
                .and_then(|edge| edge.parent());
            let target = targets
                .iter()
                .find(|t| t.path.last().copied() == parent)
                .expect("shape under a transform");
            let frame = scene.component_move_frame(target).unwrap();
            (0..shape.mesh().point_count())
                .map(|i| {
                    frame
                        .world_point_cm(shape.mesh().effective_point_cm(i).unwrap())
                        .to_array()
                })
                .collect()
        })
        .collect()
}

pub(super) fn assert_same_world(before: &[Vec<[f64; 3]>], after: &[Vec<[f64; 3]>]) {
    assert_eq!(before.len(), after.len());
    for (a, b) in before.iter().flatten().zip(after.iter().flatten()) {
        for axis in 0..3 {
            assert!((a[axis] - b[axis]).abs() < 1e-6, "{a:?} != {b:?}");
        }
    }
}

pub(super) async fn pose(
    editor: &mut Editor,
    context: &egui::Context,
    client: &super::pivot_edit::Client,
    node: &str,
) {
    let edited = tool(
        editor,
        context,
        client,
        "set_transform",
        json!({"node_id":node,"translation":[1,2,3],"rotation_degrees":[0,0,90],"scale":[2,2,2]}),
    )
    .await;
    assert_eq!(reply(&edited)["outcome"], "changed");
}

#[tokio::test]
async fn both_tools_are_one_named_undo_step_with_an_exact_restore_and_repeat_as_unchanged() {
    for (name, label) in [
        ("reset_transformations", "Reset Transformations"),
        ("freeze_transformations", "Freeze Transformations"),
    ] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let node = cubes(&mut editor, 1);
        pose(&mut editor, &context, &client, &node).await;
        let posed = editor.history.scene().clone();
        let world = world_points(&posed);

        let done = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(reply(&done)["outcome"], "changed", "{name}");
        let seen = inspected(&mut editor, &context, &client).await;
        assert_eq!(seen["undo_label"], label);
        assert_eq!(
            seen["nodes"][0]["transform"]["scale"],
            json!([1.0, 1.0, 1.0])
        );
        assert_eq!(
            seen["nodes"][0]["transform"]["translation"],
            json!([0.0, 0.0, 0.0])
        );
        if name == "freeze_transformations" {
            assert_same_world(&world, &world_points(editor.history.scene()));
        }
        let after = editor.history.scene().clone();
        let again = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(reply(&again)["outcome"], "unchanged", "{name}");
        assert_eq!(editor.history.scene(), &after);

        tool(&mut editor, &context, &client, "undo", json!({})).await;
        assert_eq!(editor.history.scene(), &posed, "{name}");
        tool(&mut editor, &context, &client, "redo", json!({})).await;
        assert_eq!(editor.history.scene(), &after, "{name}");
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn a_partial_reset_keeps_the_other_channels() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    pose(&mut editor, &context, &client, &node).await;
    let done = tool(
        &mut editor,
        &context,
        &client,
        "reset_transformations",
        json!({"translate":true,"rotate":false,"scale":false}),
    )
    .await;
    assert_eq!(reply(&done)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    let transform = &seen["nodes"][0]["transform"];
    assert_eq!(transform["translation"], json!([0.0, 0.0, 0.0]));
    assert_eq!(transform["rotation_degrees"], json!([0.0, 0.0, 90.0]));
    assert_eq!(transform["scale"], json!([2.0, 2.0, 2.0]));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn freezing_a_parent_keeps_every_child_point_where_it_was_in_the_world() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut ids = Vec::new();
    for name in ["parentCube", "childCube"] {
        let created = tool(
            &mut editor,
            &context,
            &client,
            "create_primitive",
            json!({"primitive":"cube","name":name}),
        )
        .await;
        ids.push(reply(&created)["node_id"].as_str().unwrap().to_owned());
    }
    for (id, translation, rotation, scale) in [
        (&ids[0], [4, -2, 1], [10, 20, 90], [2.0, 2.0, 2.0]),
        (&ids[1], [1, 0, 0], [0, 0, 30], [1.0, 3.0, 1.0]),
    ] {
        let edited = tool(
            &mut editor,
            &context,
            &client,
            "edit_node",
            json!({"node_id":id,"translation":translation,"rotation_degrees":rotation,"scale":scale}),
        )
        .await;
        reply(&edited);
    }
    let parented = tool(
        &mut editor,
        &context,
        &client,
        "parent_node",
        json!({"node_id":ids[1],"parent_id":ids[0]}),
    )
    .await;
    assert_eq!(reply(&parented)["outcome"], "changed");
    let hierarchy = editor.history.scene().clone();
    let world = world_points(&hierarchy);
    let parent_path = hierarchy
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|t| t.node.to_decimal() == ids[0])
        .unwrap();
    editor.set_selection_members_for_test(vec![parent_path]);
    editor.sync_fields();

    let frozen = tool(
        &mut editor,
        &context,
        &client,
        "freeze_transformations",
        json!({}),
    )
    .await;
    assert_eq!(reply(&frozen)["outcome"], "changed");
    assert_same_world(&world, &world_points(editor.history.scene()));
    let seen = inspected(&mut editor, &context, &client).await;
    for node in seen["nodes"].as_array().unwrap() {
        assert_eq!(node["transform"]["scale"], json!([1.0, 1.0, 1.0]));
        assert_eq!(node["transform"]["translation"], json!([0.0, 0.0, 0.0]));
    }
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &hierarchy);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
