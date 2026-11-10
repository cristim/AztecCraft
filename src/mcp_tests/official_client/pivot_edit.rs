//! Issue #39 slice B over the official client: `set_pivots` and `reset_pivots` through the real
//! Editor protocol, with the results read back through `inspect_scene`.
use super::*;

pub(super) type Client = rmcp::service::RunningService<RoleClient, ()>;

pub(super) async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
    name: &'static str,
    mut arguments: Value,
) -> CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments).await
}

pub(super) fn reply(result: &CallToolResult) -> &Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.as_ref().unwrap()
}

pub(super) fn code(result: &CallToolResult) -> &str {
    assert_eq!(result.is_error, Some(true), "{result:?}");
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

pub(super) async fn inspected(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn assert_close(actual: &Value, expected: [f64; 3]) {
    for (axis, want) in expected.iter().enumerate() {
        let got = actual[axis].as_f64().unwrap();
        assert!((got - want).abs() < 1e-9, "{actual} != {expected:?}");
    }
}

/// An editor with `count` selected cubes; the first is returned as a decimal id.
pub(super) fn cubes(editor: &mut Editor, count: usize) -> String {
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    let first = all[0].node.to_decimal();
    editor.set_selection_members_for_test(all);
    editor.sync_fields();
    first
}

pub(super) async fn rotate_z_90_and_scale_x_2(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
    node: &str,
) {
    let edited = tool(
        editor,
        context,
        client,
        "set_transform",
        json!({"node_id":node,"translation":[0,0,0],"rotation_degrees":[0,0,90],"scale":[2,1,1]}),
    )
    .await;
    assert_eq!(reply(&edited)["outcome"], "changed");
}

#[tokio::test]
async fn set_pivots_preserves_the_matrix_through_the_pivot_translates_and_undoes_bit_identically() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    rotate_z_90_and_scale_x_2(&mut editor, &context, &client, &node).await;
    let before = editor.history.scene().clone();
    let channels = inspected(&mut editor, &context, &client).await["nodes"][0]["transform"].clone();

    // preserve is omitted: it defaults to true. With rotate about Z by 90 degrees, Rs d is
    // (-2, 1, 3), so rpt = -(I - Rs) d = (-3, -1, 0); K = diag(2, 1, 1) gives spt = (1, 0, 0).
    let set = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3],"scale_pivot":[1,2,3]}),
    )
    .await;
    assert_eq!(reply(&set)["outcome"], "changed");
    let after = inspected(&mut editor, &context, &client).await;
    let attributes = &after["nodes"][0]["transform_attributes"];
    assert_eq!(attributes["rotate_pivot"], json!([1.0, 2.0, 3.0]));
    assert_eq!(attributes["scale_pivot"], json!([1.0, 2.0, 3.0]));
    assert_close(&attributes["rotate_pivot_translate"], [-3.0, -1.0, 0.0]);
    assert_close(&attributes["scale_pivot_translate"], [1.0, 0.0, 0.0]);
    assert_eq!(after["nodes"][0]["transform"], channels);
    assert_eq!(after["undo_label"], "Pivot");
    let edited = editor.history.scene().clone();
    let repeat = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3],"scale_pivot":[1,2,3]}),
    )
    .await;
    assert_eq!(reply(&repeat)["outcome"], "unchanged");
    assert_eq!(editor.history.scene(), &edited);

    let undo = tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(reply(&undo)["outcome"], "changed");
    assert_eq!(editor.history.scene(), &before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(editor.history.scene(), &edited);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn set_pivots_without_preserve_writes_the_raw_pivots_and_both_absent_is_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    rotate_z_90_and_scale_x_2(&mut editor, &context, &client, &node).await;
    let raw = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node,"rotate_pivot":[1,2,3],"preserve":false}),
    )
    .await;
    assert_eq!(reply(&raw)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    let attributes = &seen["nodes"][0]["transform_attributes"];
    assert_eq!(attributes["rotate_pivot"], json!([1.0, 2.0, 3.0]));
    assert_eq!(attributes["non_default"], json!(["rotate_pivot"]));
    let scene = editor.history.scene().clone();
    let depth = editor.history.undo_label();
    let none = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":node}),
    )
    .await;
    assert_eq!(reply(&none)["outcome"], "unchanged");
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), depth);
    let unknown = tool(
        &mut editor,
        &context,
        &client,
        "set_pivots",
        json!({"node_id":"999999"}),
    )
    .await;
    assert_eq!(code(&unknown), "scene_error");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn reset_pivots_zero_and_center_act_on_every_selected_object_as_one_undo_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let ids: Vec<String> = editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|n| n.id().to_decimal())
        .collect();
    for id in &ids {
        let edited = tool(
            &mut editor,
            &context,
            &client,
            "set_transform",
            json!({"node_id":id,"translation":[0,0,0],"rotation_degrees":[0,0,90],"scale":[1,1,1]}),
        )
        .await;
        reply(&edited);
        let raw = tool(
            &mut editor,
            &context,
            &client,
            "set_pivots",
            json!({"node_id":id,"rotate_pivot":[5,5,5],"scale_pivot":[5,5,5],"preserve":false}),
        )
        .await;
        reply(&raw);
    }
    let pivoted = editor.history.scene().clone();
    let generation =
        inspected(&mut editor, &context, &client).await["selection_generation"].clone();

    // Center: the cube is centred on its origin, so d = -(5, 5, 5) and rpt = (I - Rs)(5, 5, 5).
    let center = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"center","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(reply(&center)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    for node in seen["nodes"].as_array().unwrap() {
        let attributes = &node["transform_attributes"];
        assert_close(&attributes["rotate_pivot"], [0.0; 3]);
        assert_close(&attributes["rotate_pivot_translate"], [10.0, 0.0, 0.0]);
        assert_eq!(node["transform"]["translation"], json!([0.0, 0.0, 0.0]));
    }
    assert_eq!(seen["undo_label"], "Pivot");
    let centered = editor.history.scene().clone();
    let again = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"center","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(reply(&again)["outcome"], "unchanged");
    assert_eq!(editor.history.scene(), &centered);
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &pivoted);

    // Zero: the object keeps its matrix by moving the compensation into the translation.
    let zero = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(reply(&zero)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    for node in seen["nodes"].as_array().unwrap() {
        assert_eq!(node["transform_attributes"], Value::Null);
        assert_close(&node["transform"]["translation"], [10.0, 0.0, 0.0]);
    }
    let zeroed = editor.history.scene().clone();
    let again = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(reply(&again)["outcome"], "unchanged");
    assert_eq!(editor.history.scene(), &zeroed);
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &pivoted);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(editor.history.scene(), &zeroed);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
