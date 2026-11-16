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
        json!({"node_id": node, "channel": "translate_x", "frame": 11, "value": 10.0, "pre_infinity": "constant", "post_infinity": "constant"})
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
    let directory = tempfile::tempdir().unwrap();
    let binary = directory.path().join("keys.mb");
    std::fs::write(
        &binary,
        appearance_fixture::appearance_specimen_with_users(2),
    )
    .unwrap();
    let ascii = directory.path().join("keys.ma");
    std::fs::write(&ascii, ascii_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for (path, code) in [(&binary, "unsupported_edit"), (&ascii, "read_only_format")] {
        let opened = tool(
            &mut editor,
            &context,
            &client,
            "open_scene",
            json!({"path": path}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let scene = editor.history.scene().clone();
        let id = scene.nodes()[0].id().to_decimal();
        for name in ["set_keyframe", "delete_keys"] {
            let refused = tool(
                &mut editor,
                &context,
                &client,
                name,
                json!({"node_ids": [id]}),
            )
            .await;
            assert_eq!(refused.is_error, Some(true), "{name}");
            assert_eq!(error_code(&refused), code, "{name}");
            assert_eq!(editor.history.scene(), &scene, "{name}");
            assert!(!editor.history.can_undo(), "{name}");
        }
    }
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
            camera_keys: Vec::new(),
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
                    pre_infinity: "cycle_relative",
                    post_infinity: "cycle_relative",
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

#[tokio::test]
async fn animation_sdk_refuses_a_channel_list_the_schema_does_not_allow() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let all = [
        "translate_x",
        "translate_y",
        "translate_z",
        "rotate_x",
        "rotate_y",
        "rotate_z",
        "scale_x",
        "scale_y",
        "scale_z",
    ];
    let mut too_many = all.to_vec();
    too_many.push("scale_z");
    for (name, channels) in [
        ("set_keyframe", json!(too_many)),
        ("set_keyframe", json!(["translate_x", "translate_x"])),
        ("set_keyframe", json!([])),
        ("delete_keys", json!(too_many)),
        ("delete_keys", json!(["rotate_y", "rotate_y"])),
    ] {
        let mut arguments = json!({"channels": channels});
        arguments["scene_token"] = json!(editor.scene_token.to_string());
        let refused = call_response(&mut editor, &context, client.peer(), name, arguments).await;
        let Err(rmcp::ServiceError::McpError(error)) = refused else {
            panic!("{name} {channels}: expected a protocol error");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{name} {channels}"
        );
        assert_eq!(editor.history.scene().session_key_count(), 0, "{name}");
    }
    assert!(*editor.history.scene() == before);
    let ok = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"channels": all}),
    )
    .await;
    assert_ne!(ok.is_error, Some(true));
    assert_eq!(editor.history.scene().session_key_count(), 9);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_refuses_more_node_ids_than_the_cap() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap().to_decimal();
    for (name, keys) in [("set_keyframe", 9), ("delete_keys", 0)] {
        let mut arguments = json!({"node_ids": vec![id.clone(); 4097]});
        arguments["scene_token"] = json!(editor.scene_token.to_string());
        let refused = call_response(&mut editor, &context, client.peer(), name, arguments).await;
        assert!(refused.is_err(), "{name}");
        let mut at_the_cap = json!({"node_ids": vec![id.clone(); 4096]});
        at_the_cap["scene_token"] = json!(editor.scene_token.to_string());
        let accepted = call_response(&mut editor, &context, client.peer(), name, at_the_cap).await;
        assert!(accepted.is_ok(), "{name}");
        assert_eq!(editor.history.scene().session_key_count(), keys, "{name}");
    }
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_a_full_key_table_answers_key_limit_and_changes_nothing() {
    use azteccraft::history::History;
    use azteccraft::native_scene::NativeScene;
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut scene = NativeScene::default();
    let ids: Vec<String> = (0..1000)
        .map(|n| {
            scene
                .create(Primitive::Cube, &format!("cube{n}"))
                .unwrap()
                .to_decimal()
        })
        .collect();
    editor.history = History::new(scene);
    // 1000 nodes keep 9000 keys per frame, so seven frames fit under the 65536 bound.
    for frame in 0..7 {
        let keyed = tool(
            &mut editor,
            &context,
            &client,
            "set_keyframe",
            json!({"node_ids": ids, "time": frame}),
        )
        .await;
        assert_ne!(keyed.is_error, Some(true), "frame {frame}");
    }
    assert_eq!(editor.history.scene().session_key_count(), 63_000);
    let before = editor.history.scene().clone();
    let full = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids": ids, "time": 7}),
    )
    .await;
    assert_eq!(full.is_error, Some(true));
    assert_eq!(error_code(&full), "key_limit");
    assert!(*editor.history.scene() == before);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_a_scene_in_another_time_unit_refuses_every_time_tool() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    // Patch the one TUNI token of a committed Maya Binary fixture from film to ntsc (the same
    // four bytes); an ASCII ntsc scene is covered by the same `check_time_unit` refusal.
    let mut bytes =
        include_bytes!("../../../tests/fixtures/mayascenekit/file_owner_delete.mb").to_vec();
    let at = bytes.windows(4).position(|word| word == b"film").unwrap();
    bytes[at..at + 4].copy_from_slice(b"ntsc");
    let scene = azteccraft::native_scene::binary::decode(bytes.into()).unwrap();
    assert_eq!(scene.units().time, "ntsc");
    editor.history = azteccraft::history::History::new(scene);
    let id = editor.history.scene().nodes()[0].id().to_decimal();
    for (name, args) in [
        ("set_current_time", json!({"time": 2})),
        ("set_playback_range", json!({"start": 1, "end": 5})),
        ("play_timeline", json!({})),
        ("set_keyframe", json!({"node_ids": [id.clone()]})),
        ("delete_keys", json!({"node_ids": [id.clone()]})),
        ("euler_filter", json!({"node_ids": [id.clone()]})),
    ] {
        let refused = tool(&mut editor, &context, &client, name, args).await;
        assert_eq!(refused.is_error, Some(true), "{name}");
        assert_eq!(error_code(&refused), "unsupported_time_unit", "{name}");
    }
    assert_eq!(editor.timeline.current, 1);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_euler_filter_is_one_undo_step_and_refuses_bad_input() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap().to_decimal();
    for (time, degrees) in [(1, 10.0), (2, 350.0)] {
        let rotation = json!([0.0, degrees, 0.0]);
        let args = json!({"node_id": id, "translation": [0, 0, 0], "rotation_degrees": rotation, "scale": [1, 1, 1]});
        let set = tool(&mut editor, &context, &client, "set_transform", args).await;
        assert_ne!(set.is_error, Some(true), "{set:?}");
        let key = json!({"time": time, "channels": ["rotate_y"]});
        tool(&mut editor, &context, &client, "set_keyframe", key).await;
    }
    let rotate_y = |keys: &Value| -> Vec<f64> {
        keys["keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|key| key["channel"] == "rotate_y")
            .map(|key| key["value"].as_f64().unwrap())
            .collect()
    };
    let before = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(rotate_y(&before), vec![10.0, 350.0]);

    let none = tool(
        &mut editor,
        &context,
        &client,
        "euler_filter",
        json!({"node_ids": [id.clone()], "channels": ["rotate_x"]}),
    )
    .await;
    assert_eq!(error_code(&none), "no_rotation_keys");
    for channels in [json!(["translate_x"]), json!(["rotate_y", "scale_z"])] {
        let refused = tool(
            &mut editor,
            &context,
            &client,
            "euler_filter",
            json!({"channels": channels}),
        )
        .await;
        assert_eq!(error_code(&refused), "not_rotation_channel");
    }
    let scene_token = editor.scene_token.to_string();
    for mut args in [
        json!({"channels": []}),
        json!({"channels": ["rotate_y", "rotate_y"]}),
        json!({"channels": ["visibility"]}),
    ] {
        args["scene_token"] = json!(scene_token);
        let refused =
            call_response(&mut editor, &context, client.peer(), "euler_filter", args).await;
        assert!(refused.is_err());
    }
    let empty = tool(
        &mut editor,
        &context,
        &client,
        "euler_filter",
        json!({"node_ids": []}),
    )
    .await;
    assert_eq!(error_code(&empty), "invalid_input");
    let untouched = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(rotate_y(&untouched), vec![10.0, 350.0]);

    let filtered = tool(&mut editor, &context, &client, "euler_filter", json!({})).await;
    assert_eq!(outcome(filtered), "changed");
    let after = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(rotate_y(&after), vec![10.0, -10.0]);
    let again = tool(&mut editor, &context, &client, "euler_filter", json!({})).await;
    assert_eq!(outcome(again), "unchanged");

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    let undone = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(rotate_y(&undone), vec![10.0, 350.0]);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    let redone = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(rotate_y(&redone), vec![10.0, -10.0]);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_set_infinity_extends_the_curve_reads_back_and_undoes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    let node = id.to_decimal();
    let key = json!({"node_ids": [node.clone()], "channels": ["translate_x"]});
    tool(&mut editor, &context, &client, "set_keyframe", key.clone()).await;
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 5}),
    )
    .await;
    let edit = json!({"node_id": node, "translation": [8, null, null]});
    tool(&mut editor, &context, &client, "edit_node", edit).await;
    tool(&mut editor, &context, &client, "set_keyframe", key).await;
    let settings = |keys: &Value| -> Vec<(String, String)> {
        keys["keys"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|key| key["channel"] == "translate_x")
            .map(|key| {
                (
                    key["pre_infinity"].as_str().unwrap().to_string(),
                    key["post_infinity"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    };
    let constant = ("constant".to_string(), "constant".to_string());
    let read = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(settings(&read), vec![constant.clone(); 2]);

    let set = tool(
        &mut editor,
        &context,
        &client,
        "set_infinity",
        json!({"channels": ["translate_x"], "pre": "linear", "post": "cycle_relative"}),
    )
    .await;
    assert_eq!(outcome(set), "changed");
    let cycle = ("linear".to_string(), "cycle_relative".to_string());
    let read = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(settings(&read), vec![cycle.clone(); 2]);
    // Frame 9 is a whole span past the last key: 8 plus the 8 the span rises.
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 9}),
    )
    .await;
    assert_eq!(tx(&editor, id), 16.0);
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": -3}),
    )
    .await;
    assert_eq!(tx(&editor, id), -8.0);
    let again = tool(
        &mut editor,
        &context,
        &client,
        "set_infinity",
        json!({"channels": ["translate_x"], "pre": "linear"}),
    )
    .await;
    assert_eq!(outcome(again), "unchanged");

    tool(&mut editor, &context, &client, "undo", json!({})).await;
    let undone = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(settings(&undone), vec![constant.clone(); 2]);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    let redone = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(settings(&redone), vec![cycle; 2]);

    tool(
        &mut editor,
        &context,
        &client,
        "delete_keys",
        json!({"channels": ["translate_x"]}),
    )
    .await;
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time": 1}),
    )
    .await;
    tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"channels": ["translate_x"]}),
    )
    .await;
    let fresh = keys(&mut editor, &context, &client, json!({})).await;
    assert_eq!(settings(&fresh), vec![constant]);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_set_infinity_refusals_change_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap().to_decimal();
    tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"channels": ["translate_x"]}),
    )
    .await;
    let before = keys(&mut editor, &context, &client, json!({})).await;
    let neither = tool(&mut editor, &context, &client, "set_infinity", json!({})).await;
    assert_eq!(error_code(&neither), "invalid_input");
    let unkeyed = tool(
        &mut editor,
        &context,
        &client,
        "set_infinity",
        json!({"channels": ["rotate_x"], "post": "cycle"}),
    )
    .await;
    assert_eq!(error_code(&unkeyed), "no_keys");
    let empty = tool(
        &mut editor,
        &context,
        &client,
        "set_infinity",
        json!({"node_ids": [], "post": "cycle"}),
    )
    .await;
    assert_eq!(error_code(&empty), "invalid_input");
    let unknown = tool(
        &mut editor,
        &context,
        &client,
        "set_infinity",
        json!({"node_ids": ["999"], "post": "cycle"}),
    )
    .await;
    assert_eq!(unknown.is_error, Some(true));
    let scene_token = editor.scene_token.to_string();
    for mut args in [
        json!({"post": "bounce"}),
        json!({"pre": "Cycle"}),
        json!({"channels": [], "post": "cycle"}),
        json!({"channels": ["translate_x", "translate_x"], "post": "cycle"}),
        json!({"node_ids": [id.clone()], "post": "cycle", "extra": 1}),
    ] {
        args["scene_token"] = json!(scene_token);
        let shown = args.to_string();
        let refused =
            call_response(&mut editor, &context, client.peer(), "set_infinity", args).await;
        assert!(refused.is_err(), "{shown}");
    }
    assert_eq!(
        keys(&mut editor, &context, &client, json!({})).await,
        before
    );
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn animation_sdk_euler_filter_refuses_a_key_beyond_the_turn_limit() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap().to_decimal();
    for (time, degrees) in [(1, 0.0), (2, 3.6e8 + 1000.0)] {
        let args = json!({"node_id": id, "translation": [0, 0, 0], "rotation_degrees": [0.0, degrees, 0.0], "scale": [1, 1, 1]});
        let set = tool(&mut editor, &context, &client, "set_transform", args).await;
        assert_ne!(set.is_error, Some(true), "{set:?}");
        let key = json!({"time": time, "channels": ["rotate_y"]});
        tool(&mut editor, &context, &client, "set_keyframe", key).await;
    }
    let before = keys(&mut editor, &context, &client, json!({})).await;
    let refused = tool(&mut editor, &context, &client, "euler_filter", json!({})).await;
    assert_eq!(error_code(&refused), "euler_value_too_large");
    assert_eq!(
        keys(&mut editor, &context, &client, json!({})).await,
        before
    );
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn camera_focal_sdk_keys_interpolates_reads_history_and_rejects_mixed_targets() {
    use azteccraft::native_scene::SceneCamera;
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for (time, value) in [(1, 35.123456789012345), (11, 75.123_456_789_012_35)] {
        let result = tool(
            &mut editor,
            &context,
            &client,
            "set_keyframe",
            json!({"camera":"persp","attribute":"focal_length","time":time,"value":value}),
        )
        .await;
        assert_eq!(outcome(result), "changed");
    }
    let read = keys(
        &mut editor,
        &context,
        &client,
        json!({"camera":"persp","attribute":"focal_length"}),
    )
    .await;
    assert_eq!(read["keys_total"], 2);
    assert_eq!(
        read["camera_keys"][0],
        json!({"camera":"persp","attribute":"focal_length","frame":1,"value":35.123456789012345})
    );
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time":6}),
    )
    .await;
    assert!(
        (editor
            .history
            .scene()
            .camera_optics(SceneCamera::Persp)
            .focal_length_mm
            - 55.123456789012345)
            .abs()
            < 1e-12
    );
    let before = editor.history.scene().clone();
    for args in [
        json!({"camera":"persp","attribute":"focal_length","value":0}),
        json!({"camera":"persp","attribute":"focal_length","time":1.5}),
    ] {
        let refused = tool(&mut editor, &context, &client, "set_keyframe", args).await;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.timeline.current, 6);
    }
    for args in [
        json!({"camera":"persp","attribute":"focal_length","channels":["translate_x"]}),
        json!({"camera":"persp"}),
        json!({"attribute":"focal_length"}),
        json!({"value":50}),
        json!({"camera":null,"attribute":null}),
        json!({"camera":"persp","attribute":"focal_length","node_ids":null}),
        json!({"camera":"persp","attribute":"focal_length","channels":null}),
        json!({"camera":"persp","attribute":"focal_length","value":null}),
    ] {
        let token = editor.scene_token.to_string();
        let mut args = args;
        args["scene_token"] = json!(token);
        let refused =
            call_response(&mut editor, &context, client.peer(), "set_keyframe", args).await;
        assert!(refused.is_err());
        assert_eq!(editor.history.scene(), &before);
    }
    let result = tool(
        &mut editor,
        &context,
        &client,
        "delete_keys",
        json!({"camera":"persp","attribute":"focal_length","time":11}),
    )
    .await;
    assert_eq!(outcome(result), "changed");
    assert_eq!(
        editor
            .history
            .scene()
            .camera_optics(SceneCamera::Persp)
            .focal_length_mm
            .to_bits(),
        35.123456789012345_f64.to_bits()
    );
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(
        keys(
            &mut editor,
            &context,
            &client,
            json!({"camera":"persp","attribute":"focal_length"})
        )
        .await["keys_total"],
        1
    );
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn camera_focal_sdk_save_open_both_formats_repeated_save_and_source_guards() {
    use azteccraft::native_scene::SceneCamera;
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    for (time, value) in [(1, 35.123456789012345), (11, 75.123_456_789_012_35)] {
        assert_eq!(
            outcome(
                tool(
                    &mut editor,
                    &context,
                    &client,
                    "set_keyframe",
                    json!({"camera":"persp","attribute":"focal_length","time":time,"value":value})
                )
                .await
            ),
            "changed"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    for extension in ["ma", "mb"] {
        tool(
            &mut editor,
            &context,
            &client,
            "set_current_time",
            json!({"time":6}),
        )
        .await;
        let path = dir.path().join(format!("animated.{extension}"));
        assert_eq!(
            outcome(
                tool(
                    &mut editor,
                    &context,
                    &client,
                    "save_scene_as",
                    json!({"path":path})
                )
                .await
            ),
            "saved"
        );
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            outcome(tool(&mut editor, &context, &client, "save_scene", json!({})).await),
            "saved"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(
            outcome(
                tool(
                    &mut editor,
                    &context,
                    &client,
                    "open_scene",
                    json!({"path":path})
                )
                .await
            ),
            "document_applied"
        );
        assert_eq!(editor.timeline.current, 1);
        assert!(!editor.is_dirty());
        assert_eq!(
            editor
                .history
                .scene()
                .camera_optics(SceneCamera::Persp)
                .focal_length_mm
                .to_bits(),
            35.123456789012345_f64.to_bits()
        );
        assert_eq!(
            keys(
                &mut editor,
                &context,
                &client,
                json!({"camera":"persp","attribute":"focal_length"})
            )
            .await["keys_total"],
            2
        );
        tool(
            &mut editor,
            &context,
            &client,
            "set_current_time",
            json!({"time":6}),
        )
        .await;
        assert!(
            (editor
                .history
                .scene()
                .camera_optics(SceneCamera::Persp)
                .focal_length_mm
                - 55.123456789012345)
                .abs()
                < 1e-12
        );
        assert!(
            !editor.is_dirty(),
            "scrubbing persisted focal curves must stay clean"
        );
        assert_eq!(
            outcome(
                tool(
                    &mut editor,
                    &context,
                    &client,
                    "set_keyframe",
                    json!({"camera":"persp","attribute":"focal_length","time":11,"value":90})
                )
                .await
            ),
            "changed"
        );
        assert!(editor.is_dirty());
        tool(&mut editor, &context, &client, "undo", json!({})).await;
        assert!(!editor.is_dirty());
        let mut optics = editor.history.scene().camera_optics(SceneCamera::Persp);
        optics.overscan = 1.2;
        tool(
            &mut editor,
            &context,
            &client,
            "set_camera_optics",
            json!({"camera":"persp","optics":optics}),
        )
        .await;
        assert!(editor.is_dirty());
        tool(&mut editor, &context, &client, "undo", json!({})).await;
        assert!(!editor.is_dirty());
        let occupied = dir.path().join(format!("occupied.{extension}"));
        std::fs::write(&occupied, b"keep").unwrap();
        let refused = tool(
            &mut editor,
            &context,
            &client,
            "save_scene_as",
            json!({"path":occupied}),
        )
        .await;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
    }
    let path = dir.path().join("animated.mb");
    std::fs::write(&path, b"changed externally").unwrap();
    let refused = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(refused.is_error, Some(true));
    assert_eq!(std::fs::read(path).unwrap(), b"changed externally");
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn camera_focal_sdk_paging_scopes_and_stale_cursor_preserve_state() {
    use azteccraft::native_scene::{NativeScene, SceneCamera};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut scene = NativeScene::default();
    for frame in 1..=1025 {
        scene
            .set_camera_focal_key(SceneCamera::Persp, frame, 35., 1)
            .unwrap();
    }
    editor.history = History::new(scene);
    let page = keys(
        &mut editor,
        &context,
        &client,
        json!({"camera":"persp","attribute":"focal_length"}),
    )
    .await;
    assert_eq!(page["keys_total"], 1025);
    assert_eq!(page["camera_keys"].as_array().unwrap().len(), 1024);
    let cursor = page["next_cursor"].as_str().unwrap();
    let last = keys(
        &mut editor,
        &context,
        &client,
        json!({"camera":"persp","attribute":"focal_length","cursor":cursor}),
    )
    .await;
    assert_eq!(last["key_offset"], 1024);
    assert_eq!(last["camera_keys"].as_array().unwrap().len(), 1);
    assert_eq!(last["next_cursor"], Value::Null);
    let before = editor.history.scene().clone();
    for args in [
        json!({"camera":"top","attribute":"focal_length","cursor":cursor}),
        json!({"cursor":cursor}),
    ] {
        let refused = call(&mut editor, &context, client.peer(), "inspect_keys", args).await;
        assert_eq!(refused.is_error, Some(true));
        assert_eq!(editor.history.scene(), &before);
    }
    tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"camera":"persp","attribute":"focal_length","time":1,"value":40}),
    )
    .await;
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_keys",
        json!({"camera":"persp","attribute":"focal_length","cursor":cursor}),
    )
    .await;
    assert_eq!(error_code(&refused), "stale_scene");
    for (name, args) in [
        (
            "delete_keys",
            json!({"camera":"persp","attribute":"focal_length","value":null}),
        ),
        (
            "inspect_keys",
            json!({"camera":"persp","attribute":"focal_length","node_id":null}),
        ),
    ] {
        let mut args = args;
        if name == "delete_keys" {
            args["scene_token"] = json!(editor.scene_token.to_string());
        }
        assert!(
            call_response(&mut editor, &context, client.peer(), name, args)
                .await
                .is_err()
        );
    }
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn focal_curves_and_transform_infinity_coexist_without_losing_either_table() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    for (frame, x, focal) in [(1, 0., 35.), (11, 10., 75.)] {
        tool(
            &mut editor,
            &context,
            &client,
            "set_current_time",
            json!({"time":frame}),
        )
        .await;
        tool(
            &mut editor,
            &context,
            &client,
            "edit_node",
            json!({"node_id":id.to_decimal(),"translation":[x,null,null]}),
        )
        .await;
        tool(
            &mut editor,
            &context,
            &client,
            "set_keyframe",
            json!({"channels":["translate_x"]}),
        )
        .await;
        tool(
            &mut editor,
            &context,
            &client,
            "set_keyframe",
            json!({"camera":"persp","attribute":"focal_length","value":focal}),
        )
        .await;
    }
    assert_eq!(
        outcome(
            tool(
                &mut editor,
                &context,
                &client,
                "set_infinity",
                json!({"channels":["translate_x"],"post":"cycle"})
            )
            .await
        ),
        "changed"
    );
    tool(
        &mut editor,
        &context,
        &client,
        "set_current_time",
        json!({"time":26}),
    )
    .await;
    assert_eq!(tx(&editor, id), 5.);
    assert_eq!(
        editor
            .history
            .scene()
            .camera_optics(azteccraft::native_scene::SceneCamera::Persp)
            .focal_length_mm,
        75.
    );
    tool(
        &mut editor,
        &context,
        &client,
        "delete_keys",
        json!({"camera":"persp","attribute":"focal_length"}),
    )
    .await;
    assert_eq!(
        keys(&mut editor, &context, &client, json!({})).await["keys"][0]["post_infinity"],
        "cycle"
    );
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(
        keys(
            &mut editor,
            &context,
            &client,
            json!({"camera":"persp","attribute":"focal_length"})
        )
        .await["keys_total"],
        2
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.ma");
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path":path}),
    )
    .await;
    assert_eq!(refused.is_error, Some(true));
    assert!(!path.exists());
    client.cancel().await.unwrap();
    server.await.unwrap();
}
