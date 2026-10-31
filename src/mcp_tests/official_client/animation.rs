use super::*;
use crate::editor_operations::{KeyInspection, KeysInspection, MAX_KEYS_PER_PAGE};

fn error_code(result: &rmcp::model::CallToolResult) -> &str {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

fn outcome(result: rmcp::model::CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    result.structured_content.unwrap()["outcome"].clone()
}

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> rmcp::model::CallToolResult {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments).await
}

async fn keys(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    arguments: Value,
) -> Value {
    call(editor, context, client.peer(), "inspect_keys", arguments)
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

async fn timeline(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["timeline"]
        .clone()
}

fn tx(editor: &Editor, id: NodeId) -> f64 {
    editor
        .history
        .scene()
        .node(id)
        .unwrap()
        .transform()
        .translation[0]
}

#[tokio::test]
async fn animation_sdk_keys_scrubs_reads_undoes_and_deletes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    let node = id.to_decimal();

    let state = timeline(&mut editor, &context, &client).await;
    assert_eq!(state["current_time"], 1);
    assert_eq!(state["playback_start"], 1);
    assert_eq!(state["playback_end"], 48);
    assert_eq!(state["playing"], false);
    assert_eq!(state["frames_per_second"], 24.0);
    assert_eq!(state["loop"], "continuous");
    assert_eq!(state["key_count"], 0);

    let first = tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;
    assert_eq!(outcome(first), "changed");
    let again = tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;
    assert_eq!(outcome(again), "unchanged");

    let moved = tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 11}),
    )
    .await;
    assert_eq!(outcome(moved), "changed");
    tool(
        &mut editor,
        &context,
        &client,
        "edit_node",
        json!({"node_id": node, "translation": [10, null, null]}),
    )
    .await;
    tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids": [node], "channels": ["translate_x"]}),
    )
    .await;

    let read = keys(&mut editor, &context, &client, json!({"node_id": node})).await;
    assert_eq!(read["keys_total"], 10);
    assert_eq!(read["unavailable_reason"], Value::Null);
    assert_eq!(
        read["keys"][1],
        json!({"node_id": node, "channel": "translate_x", "frame": 11, "value": 10.0})
    );

    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 6}),
    )
    .await;
    assert_eq!(tx(&editor, id), 5.0);
    assert_eq!(
        timeline(&mut editor, &context, &client).await["current_time"],
        6
    );

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(tx(&editor, id), 0.0);
    assert_eq!(
        keys(&mut editor, &context, &client, json!({})).await["keys_total"],
        9
    );
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(tx(&editor, id), 5.0);

    let one = tool(
        &mut editor,
        &context,
        &client,
        "delete_keys",
        json!({"time": 11, "channels": ["translate_x"]}),
    )
    .await;
    assert_eq!(outcome(one), "changed");
    assert_eq!(tx(&editor, id), 0.0);
    let all = tool(&mut editor, &context, &client, "delete_keys", json!({})).await;
    assert_eq!(outcome(all), "changed");
    let none = tool(&mut editor, &context, &client, "delete_keys", json!({})).await;
    assert_eq!(outcome(none), "unchanged");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_refusals_keep_the_scene_and_the_time() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap().to_decimal();
    tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;
    let before = editor.history.scene().clone();

    for (name, args, code) in [
        ("set_current_time", json!({"time": 1.5}), "invalid_frame"),
        ("set_current_time", json!({"time": 2e6}), "invalid_frame"),
        (
            "set_keyframe",
            json!({"time": 0.25, "node_ids": [id]}),
            "invalid_frame",
        ),
        (
            "set_playback_range",
            json!({"start": 9, "end": 2}),
            "invalid_playback_range",
        ),
        (
            "set_keyframe",
            json!({"node_ids": ["999999"]}),
            "scene_error",
        ),
        ("delete_keys", json!({"node_ids": []}), "invalid_input"),
    ] {
        let refused = tool(&mut editor, &context, &client, name, args).await;
        assert_eq!(refused.is_error, Some(true), "{name}");
        assert_eq!(error_code(&refused), code, "{name}");
        assert!(*editor.history.scene() == before, "{name}");
    }
    assert_eq!(editor.timeline.current, 1);

    let scene_token = editor.scene_token.to_string();
    let unknown = call_response(
        &mut editor,
        &context,
        client.peer(),
        "set_keyframe",
        json!({"scene_token": scene_token, "channels": ["visibility"]}),
    )
    .await;
    assert!(unknown.is_err());

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_current_time",
        json!({"scene_token": Uuid::new_v4().to_string(), "time": 3}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_respects_drafts_modals_and_component_modes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;

    editor.channels[0] = "7".into();
    for name in ["set_current_time", "set_keyframe", "play_timeline"] {
        let args = if name == "set_current_time" {
            json!({"time": 4})
        } else {
            json!({})
        };
        let busy = tool(&mut editor, &context, &client, name, args).await;
        assert_eq!(error_code(&busy), "busy_editing", "{name}");
    }
    assert_eq!(editor.channels[0], "7");
    editor.sync_fields();

    editor
        .apply_operation(Operation::NewScene)
        .expect("dirty scene asks first");
    assert!(editor.files.is_modal());
    let busy = tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 4}),
    )
    .await;
    assert_eq!(error_code(&busy), "busy_editing");
    client.cancel().await.unwrap();
    server.await.unwrap();

    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    let refused = tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;
    assert_eq!(error_code(&refused), "invalid_input");
    // Naming the object does not depend on the selection mode.
    let id = editor.history.scene().nodes()[0].id().to_decimal();
    let named = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids": [id]}),
    )
    .await;
    assert_eq!(outcome(named), "changed");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_plays_and_a_mutation_stops_playback() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let range = tool(
        &mut editor,
        &context,
        &client,
        "set_playback_range",
        json!({"start": 1, "end": 12}),
    )
    .await;
    assert_eq!(outcome(range), "changed");
    let play = tool(&mut editor, &context, &client, "play_timeline", json!({})).await;
    assert_eq!(outcome(play), "changed");
    let state = timeline(&mut editor, &context, &client).await;
    assert_eq!(state["playing"], true);
    assert_eq!(state["playback_end"], 12);
    let again = tool(&mut editor, &context, &client, "play_timeline", json!({})).await;
    assert_eq!(outcome(again), "unchanged");
    let stop = tool(&mut editor, &context, &client, "stop_timeline", json!({})).await;
    assert_eq!(outcome(stop), "changed");
    let again = tool(&mut editor, &context, &client, "stop_timeline", json!({})).await;
    assert_eq!(outcome(again), "unchanged");

    tool(&mut editor, &context, &client, "play_timeline", json!({})).await;
    tool(&mut editor, &context, &client, "set_keyframe", json!({})).await;
    assert_eq!(
        timeline(&mut editor, &context, &client).await["playing"],
        false
    );
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_pages_a_large_table_and_a_time_change_stales_a_cursor() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    for frame in 0..=(MAX_KEYS_PER_PAGE / 9) as i32 {
        editor
            .apply_operation(Operation::EditNode {
                id,
                name: None,
                translation: [Some(f64::from(frame) / 7.0), None, None],
                rotation_degrees: [None; 3],
                scale: [None; 3],
            })
            .unwrap();
        editor
            .apply_operation(Operation::SetKeyframe {
                nodes: None,
                time: Some(f64::from(frame)),
                channels: azteccraft::native_scene::ALL_CHANNELS,
            })
            .unwrap();
    }
    let first = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(first["keys"].as_array().unwrap().len(), MAX_KEYS_PER_PAGE);
    let cursor = first["next_cursor"].clone();
    assert!(cursor.is_string());
    let last = keys(&mut editor, &context, &client, json!({"cursor": cursor})).await;
    assert_eq!(last["key_offset"], MAX_KEYS_PER_PAGE);
    assert_eq!(last["next_cursor"], Value::Null);

    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 40}),
    )
    .await;
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_keys",
        json!({"cursor": cursor}),
    )
    .await;
    assert_eq!(error_code(&stale), "stale_scene");
    let malformed = call_response(
        &mut editor,
        &context,
        client.peer(),
        "inspect_keys",
        json!({"cursor": "nope"}),
    )
    .await;
    assert!(malformed.is_err());
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_a_sourced_scene_refuses_keys_and_says_curves_are_not_read() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    editor.history = azteccraft::history::History::new(scene);
    let id = editor.history.scene().nodes()[0].id().to_decimal();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids": [id]}),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));
    let read = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(read["keys_total"], 0);
    assert!(read["unavailable_reason"].is_string());
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[test]
fn a_full_page_of_the_longest_keys_fits_the_reply_limits() {
    let page = OperationReply::Keys {
        inspection: Box::new(KeysInspection {
            scene_token: Uuid::new_v4().to_string(),
            revision: u64::MAX,
            unavailable_reason: None,
            keys_total: usize::MAX,
            key_offset: usize::MAX,
            keys: (0..MAX_KEYS_PER_PAGE)
                .map(|_| KeyInspection {
                    node_id: u64::MAX.to_string(),
                    channel: "translate_x",
                    frame: -azteccraft::native_scene::MAX_FRAME,
                    value: -1.234_567_890_123_456_7e-308,
                })
                .collect(),
            next_cursor: Some(
                "v1.00000000-0000-0000-0000-000000000000.ffffffffffffffff.ffffffff".into(),
            ),
        }),
    };
    let text = serde_json::to_string(&page).unwrap();
    assert!(
        text.len() < crate::editor_operations::MAX_REPLY_BYTES,
        "{}",
        text.len()
    );
    // The result carries the reply twice: the value and an escaped text copy.
    let copy = serde_json::to_string(&text).unwrap();
    assert!(
        text.len() + copy.len() < crate::editor_operations::MAX_RESULT_BYTES,
        "{}",
        text.len() + copy.len()
    );
}
