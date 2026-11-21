//! #492: the Rotate tool's view ring and free rotation over the official MCP client.

use super::*;
use glam::{DQuat, DVec3, EulerRot};

async fn manipulation(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["manipulation"]
        .clone()
}

fn orientation(editor: &Editor) -> DQuat {
    let values = editor.history.scene().nodes()[0].transform().values();
    let [x, y, z] = [values[3], values[4], values[5]].map(f64::to_radians);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

fn assert_turned(editor: &Editor, axis: DVec3, degrees: f64) {
    let expected = DQuat::from_axis_angle(axis.normalize(), degrees.to_radians());
    let off = orientation(editor).angle_between(expected).to_degrees();
    assert!(off < 1e-6, "{off} degrees off");
}

#[tokio::test]
async fn sdk_free_rotate_trackball_and_view_ring_turn_about_world_directions() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token": token, "primitive": "cube"}),
    )
    .await
    .structured_content
    .unwrap();
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = |handle: &str| {
        json!({"scene_token": token, "kind": "free_rotate", "node_id": node,
               "instance_path": path, "handle": handle})
    };

    // Free rotation has its own availability (World-space refusals).
    let kinds = manipulation(&mut editor, &context, &client).await["kinds"].clone();
    let free = kinds
        .as_array()
        .unwrap()
        .iter()
        .find(|kind| kind["kind"] == "free_rotate")
        .unwrap()
        .clone();
    assert_eq!(free["available"], true, "{free}");
    // Trackball: each update names a world axis; values are absolute from begin.
    let started = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        begin("trackball"),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(started["outcome"], "manipulation_started");
    let id = started["manipulation_id"].as_str().unwrap().to_owned();
    let inspected = manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["active"]["kind"], "free_rotate");
    assert_eq!(inspected["active"]["handle"], "trackball");
    assert_eq!(inspected["active"]["space"], "world");
    assert_eq!(inspected["active"]["channels"], "rotate");
    assert!(inspected["active"].get("free_rotate_axis").is_none());
    for (axis, angle) in [([1.0, 2.0, 0.5], 40.0), ([0.0, 0.0, 3.0], -25.0)] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token": token, "manipulation_id": id, "angle": angle, "world_axis": axis}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
        assert_turned(&editor, DVec3::from_array(axis), angle);
        let inspected = manipulation(&mut editor, &context, &client).await;
        assert_eq!(inspected["active"]["free_rotate_axis"], json!(axis));
        assert_eq!(inspected["active"]["value"], angle);
    }
    let preview = editor.history.scene().clone();
    // A plain angle has no axis for a trackball: a typed kind mismatch, the preview kept.
    let plain = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "angle": 10.0}),
    )
    .await;
    assert_eq!(
        plain.structured_content.unwrap()["error"]["code"],
        "manipulation_kind_mismatch"
    );
    assert_eq!(editor.history.scene(), &preview);
    // A zero axis is refused by the core and keeps the last preview and axis.
    let zero = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "angle": 10.0, "world_axis": [0.0, 0.0, 0.0]}),
    )
    .await;
    assert_eq!(
        zero.structured_content.unwrap()["error"]["code"],
        "invalid_input"
    );
    assert_eq!(editor.history.scene(), &preview);
    let inspected = manipulation(&mut editor, &context, &client).await;
    // Malformed input is not a scene refusal of the drag.
    assert!(inspected["refusal"].is_null());
    assert_eq!(inspected["active"]["parameter"], "angle_and_world_axis");
    assert_eq!(
        inspected["active"]["free_rotate_axis"],
        json!([0.0, 0.0, 3.0])
    );
    // Malformed axes never reach the editor.
    for axis in [json!([1.0, 2.0]), json!([1.0, 2.0, 3.0, 4.0]), json!("y")] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token": token, "manipulation_id": id, "angle": 10.0, "world_axis": axis}),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {axis}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{axis}");
    }
    let finished = call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "finish": "commit"}),
    )
    .await;
    assert_eq!(finished.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.history.undo_label().as_deref(), Some("Rotate"));
    assert_turned(&editor, DVec3::Z, -25.0);

    // View ring: the axis is the direction toward the viewer through the pivot at begin, and
    // updates take a plain angle.
    let toward = (editor.camera.eye().as_dvec3()
        - editor
            .history
            .scene()
            .manipulator_frame(&editor.selected_move_target().unwrap())
            .unwrap()
            .handle_origin(azteccraft::native_scene::ManipulatorTool::Rotate))
    .normalize();
    let before = orientation(&editor);
    let started = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        begin("view"),
    )
    .await
    .structured_content
    .unwrap();
    let id = started["manipulation_id"].as_str().unwrap().to_owned();
    let inspected = manipulation(&mut editor, &context, &client).await;
    assert_eq!(inspected["active"]["handle"], "view");
    let axis = DVec3::from_array(
        serde_json::from_value(inspected["active"]["free_rotate_axis"].clone()).unwrap(),
    );
    assert!(axis.dot(toward) > 1.0 - 1e-12, "{axis} vs {toward}");
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "angle": 30.0}),
    )
    .await;
    let expected = DQuat::from_axis_angle(toward, 30f64.to_radians()) * before;
    assert!(orientation(&editor).angle_between(expected).to_degrees() < 1e-6);
    // A view ring takes no world_axis.
    let mismatch = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "angle": 5.0, "world_axis": [0.0, 1.0, 0.0]}),
    )
    .await;
    assert_eq!(
        mismatch.structured_content.unwrap()["error"]["code"],
        "manipulation_kind_mismatch"
    );
    let cancelled = call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token": token, "manipulation_id": id, "finish": "cancel"}),
    )
    .await;
    assert_eq!(cancelled.structured_content.unwrap()["outcome"], "changed");
    assert!(orientation(&editor).angle_between(before) < 1e-12);
    // A ring rotate's angle update cannot carry an axis either.
    let ring = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": token, "kind": "rotate", "node_id": node,
               "instance_path": path, "axis": "x", "space": "object"}),
    )
    .await
    .structured_content
    .unwrap();
    let ring_id = ring["manipulation_id"].as_str().unwrap().to_owned();
    let mismatch = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token": token, "manipulation_id": ring_id, "angle": 5.0, "world_axis": [0.0, 1.0, 0.0]}),
    )
    .await;
    assert_eq!(
        mismatch.structured_content.unwrap()["error"]["code"],
        "manipulation_kind_mismatch"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_free_rotate_refuses_an_unknown_handle_and_a_stale_token() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let created = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token": token, "primitive": "cube"}),
    )
    .await
    .structured_content
    .unwrap();
    let node = created["node_id"].as_str().unwrap().to_owned();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let scene = editor.history.scene().clone();
    for handle in ["free", "View", ""] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            json!({"scene_token": token, "kind": "free_rotate", "node_id": node,
                   "instance_path": path, "handle": handle}),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {handle:?}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": Uuid::new_v4().to_string(), "kind": "free_rotate", "node_id": node,
               "instance_path": path, "handle": "trackball"}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
