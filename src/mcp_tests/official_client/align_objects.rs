//! Issue #313 slice 4 over the official client: `align_objects` through the real Editor
//! protocol, its guards and typed refusals, with results read back through `inspect_scene`.
use super::pivot_edit::{code, cubes, inspected, reply, tool};
use super::*;
use uuid::Uuid;

const TOOL: &str = "align_objects";

/// Two selected cubes (the second is the last selected) with different transforms.
async fn two_posed(
    editor: &mut Editor,
    context: &egui::Context,
    client: &super::pivot_edit::Client,
) -> [String; 2] {
    cubes(editor, 2);
    let nodes: Vec<String> = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .iter()
        .map(|t| t.node.to_decimal())
        .collect();
    for (node, (t, r, s)) in nodes.iter().zip([
        ([1, 2, 3], [10, 20, 30], [1, 2, 3]),
        ([4, -5, 6], [0, 0, 90], [2, 2, 2]),
    ]) {
        let edited = tool(
            editor,
            context,
            client,
            "set_transform",
            json!({"node_id":node,"translation":t,"rotation_degrees":r,"scale":s}),
        )
        .await;
        assert_eq!(reply(&edited)["outcome"], "changed");
    }
    [nodes[0].clone(), nodes[1].clone()]
}

/// The transform of the node created `index`-th (0 first, 1 last selected).
fn transform_of(seen: &serde_json::Value, index: usize) -> serde_json::Value {
    seen["nodes"][index]["transform"].clone()
}

#[tokio::test]
async fn min_x_against_the_last_selected_is_one_named_undo_step_with_an_exact_restore() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let _ = two_posed(&mut editor, &context, &client).await;
    let posed = editor.history.scene().clone();
    let before = inspected(&mut editor, &context, &client).await;
    let generation = before["selection_generation"].as_u64().unwrap();
    let done = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"x":"min","align_to":"last_selected","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(reply(&done)["outcome"], "changed");
    let seen = inspected(&mut editor, &context, &client).await;
    assert_eq!(seen["undo_label"], "Align Objects");
    // Only the first object moves, only along X, and its rotation and scale are untouched.
    let (old, new) = (transform_of(&before, 0), transform_of(&seen, 0));
    assert_ne!(old["translation"][0], new["translation"][0]);
    assert_eq!(old["translation"][1], new["translation"][1]);
    assert_eq!(old["translation"][2], new["translation"][2]);
    assert_eq!(old["rotation_degrees"], new["rotation_degrees"]);
    assert_eq!(old["scale"], new["scale"]);
    assert_eq!(transform_of(&before, 1), transform_of(&seen, 1));
    let after = editor.history.scene().clone();
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &posed);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(editor.history.scene(), &after);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn selection_token_and_input_refusals_leave_the_scene_untouched() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    two_posed(&mut editor, &context, &client).await;
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let scene = editor.history.scene().clone();
    let revision = editor.history.undo_label();
    let missing = tool(&mut editor, &context, &client, TOOL, json!({"x":"min"})).await;
    assert_eq!(code(&missing), "multiple_selection");
    let stale = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"x":"min","expected_selection_generation":generation + 1}),
    )
    .await;
    assert_eq!(code(&stale), "stale_selection");
    let stale_scene = call(
        &mut editor,
        &context,
        client.peer(),
        TOOL,
        json!({"scene_token":Uuid::new_v4().to_string(),"x":"min","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&stale_scene), "stale_scene");
    let none = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&none), "invalid_transform_edit");
    for bad in [
        json!({"x":"sideways"}),
        json!({"align_to":"world_origin"}),
        json!({"unknown":true}),
    ] {
        let mut args = bad.clone();
        args["scene_token"] = json!(editor.scene_token.to_string());
        let Err(rmcp::ServiceError::McpError(error)) =
            call_response(&mut editor, &context, client.peer(), TOOL, args).await
        else {
            panic!("{bad}: expected a protocol error")
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    editor.selection_mode = SelectionMode::Face;
    let component = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"x":"min","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&component), "invalid_input");
    editor.selection_mode = SelectionMode::Object;
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), revision);

    let lead = editor.history.scene().transform_paths().unwrap()[1].clone();
    editor.set_selection_members_for_test(vec![lead]);
    editor.sync_fields();
    let one = tool(&mut editor, &context, &client, TOOL, json!({"x":"min"})).await;
    assert_eq!(code(&one), "invalid_transform_edit");
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let empty = tool(&mut editor, &context, &client, TOOL, json!({"x":"min"})).await;
    assert_eq!(code(&empty), "no_selection");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn draft_manipulation_modal_and_closing_states_refuse_the_tool() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let [first, _] = two_posed(&mut editor, &context, &client).await;
    let scene = editor.history.scene().clone();
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let args = json!({"x":"min","expected_selection_generation":generation});

    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    let busy = tool(&mut editor, &context, &client, TOOL, args.clone()).await;
    assert_eq!(code(&busy), "busy_editing");
    editor.channel_intents[0] = false;
    editor.sync_fields();

    let path: Vec<String> = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let lead = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![lead]);
    editor.sync_fields();
    let begun = tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"kind":"move","node_id":first,"instance_path":path,"axis":"x"}),
    )
    .await;
    let manipulation = reply(&begun)["manipulation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let busy = tool(&mut editor, &context, &client, TOOL, json!({"x":"min"})).await;
    assert_eq!(code(&busy), "busy_manipulation");
    tool(
        &mut editor,
        &context,
        &client,
        "finish_manipulation",
        json!({"manipulation_id":manipulation,"finish":"cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &scene);

    editor.request_close();
    let modal = tool(&mut editor, &context, &client, TOOL, json!({"x":"min"})).await;
    assert_eq!(code(&modal), "busy_editing");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn a_keyed_object_is_a_typed_refusal_and_changes_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let [first, _] = two_posed(&mut editor, &context, &client).await;
    let keyed = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids":[first]}),
    )
    .await;
    assert_eq!(reply(&keyed)["outcome"], "changed");
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let scene = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"x":"min","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&refused), "invalid_transform_edit");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_ascii_scene_is_read_only() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("specimen.ma");
    std::fs::write(&source, ascii_fixture::specimen()).unwrap();
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":source}),
    )
    .await;
    assert_eq!(reply(&opened)["outcome"], "document_applied");
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.into_iter().take(2).collect());
    editor.sync_fields();
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let scene = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        TOOL,
        json!({"x":"min","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&refused), "read_only_format");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
