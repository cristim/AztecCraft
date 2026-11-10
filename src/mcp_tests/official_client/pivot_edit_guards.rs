//! Issue #39 slice B over the official client: the guards and input checks both pivot tools
//! share with the other mutations.
use super::pivot_edit::{code, cubes, inspected, reply, rotate_z_90_and_scale_x_2, tool};
use super::*;
use uuid::Uuid;

#[tokio::test]
async fn reset_pivots_selection_refusals_leave_everything_alone() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let scene = editor.history.scene().clone();
    let revision = editor.history.undo_label();

    let missing = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero"}),
    )
    .await;
    assert_eq!(code(&missing), "multiple_selection");
    let stale = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero","expected_selection_generation":generation + 1}),
    )
    .await;
    assert_eq!(code(&stale), "stale_selection");
    let stale_scene = call(
        &mut editor,
        &context,
        client.peer(),
        "reset_pivots",
        json!({"scene_token":Uuid::new_v4().to_string(),"mode":"zero","expected_selection_generation":generation}),
    )
    .await;
    assert_eq!(code(&stale_scene), "stale_scene");

    editor.selection_mode = SelectionMode::Face;
    let component = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"center"}),
    )
    .await;
    assert_eq!(code(&component), "invalid_input");
    editor.selection_mode = SelectionMode::Object;

    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let empty = tool(
        &mut editor,
        &context,
        &client,
        "reset_pivots",
        json!({"mode":"zero"}),
    )
    .await;
    assert_eq!(code(&empty), "no_selection");
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), revision);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn both_tools_refuse_busy_modal_and_closing_states_without_touching_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    let calls = |node: &str| {
        [
            ("set_pivots", json!({"node_id":node,"rotate_pivot":[1,2,3]})),
            ("reset_pivots", json!({"mode":"zero"})),
        ]
    };
    let scene = editor.history.scene().clone();

    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    for (name, arguments) in calls(&node) {
        let busy = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(code(&busy), "busy_editing", "{name}");
    }
    editor.channel_intents[0] = false;
    editor.sync_fields();

    // An active manipulation.
    let path: Vec<String> = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begun = tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"kind":"move","node_id":node,"instance_path":path,"axis":"x"}),
    )
    .await;
    let manipulation = reply(&begun)["manipulation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (name, arguments) in calls(&node) {
        let busy = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(code(&busy), "busy_manipulation", "{name}");
    }
    tool(
        &mut editor,
        &context,
        &client,
        "finish_manipulation",
        json!({"manipulation_id":manipulation,"finish":"cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &scene);

    // A pending close confirmation, then a close in flight.
    editor.request_close();
    for (name, arguments) in calls(&node) {
        let modal = tool(&mut editor, &context, &client, name, arguments).await;
        assert_eq!(code(&modal), "busy_editing", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn non_finite_and_overflowing_input_is_typed_and_mutates_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    rotate_z_90_and_scale_x_2(&mut editor, &context, &client, &node).await;
    let scene = editor.history.scene().clone();
    // The client serialises NaN as null, and no JSON number is infinite: the decoder refuses all
    // of these before the Editor sees them.
    for bad in [
        json!([f64::NAN, 0.0, 0.0]),
        json!("Infinity"),
        json!([1, 2]),
    ] {
        let args =
            json!({"scene_token":editor.scene_token.to_string(),"node_id":node,"rotate_pivot":bad});
        let Err(rmcp::ServiceError::McpError(error)) =
            call_response(&mut editor, &context, client.peer(), "set_pivots", args).await
        else {
            panic!("{bad}: expected a protocol error")
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{bad}");
    }
    assert_eq!(editor.history.scene(), &scene);
    for preserve in [true, false] {
        let overflow = tool(
            &mut editor,
            &context,
            &client,
            "set_pivots",
            json!({"node_id":node,"rotate_pivot":[1e308,1e308,1e308],"scale_pivot":[1e308,1e308,1e308],"preserve":preserve}),
        )
        .await;
        assert_eq!(code(&overflow), "invalid_input", "preserve={preserve}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
