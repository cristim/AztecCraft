//! #459 P6a: `set_image_plane` refusals, argument errors, `remove_image_plane` and the guards.
use super::camera::inspected_camera;
use super::image_plane::{code, message, outcome, plane, set, write_png};
use super::*;

#[tokio::test]
async fn sdk_set_image_plane_refuses_bad_files_with_a_path_free_error_and_no_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("SECRETSHOW");
    std::fs::create_dir(&secret).unwrap();
    let good = write_png(&secret, "plate.png", 8, 8);
    std::fs::write(secret.join("junk.png"), b"not an image").unwrap();
    std::fs::create_dir(secret.join("dir.png")).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    let mut checks = vec![
        (
            secret.join("missing.png").to_str().unwrap().to_owned(),
            "could not be read",
        ),
        (secret.join("junk.png").to_str().unwrap().to_owned(), ""),
        (
            secret.join("dir.png").to_str().unwrap().to_owned(),
            "could not be read",
        ),
        // The extension rule runs before any open: a missing .txt is an extension error.
        (
            secret.join("missing.txt").to_str().unwrap().to_owned(),
            ".png, .jpg or .jpeg",
        ),
        (
            secret.join("a.exr").to_str().unwrap().to_owned(),
            ".png, .jpg or .jpeg",
        ),
        (format!("{}.png", "a".repeat(4093)), "1 to 4096"),
    ];
    checks.push((format!("{}\0.png", secret.display()), "NUL"));
    for (name, expected) in checks {
        let r = set(
            &mut editor,
            &context,
            &client,
            json!({"scene_token": text, "image_name": name}),
        )
        .await;
        assert_eq!(code(&r), "image_plane_error", "{name}");
        let text_out = message(&r);
        assert!(text_out.contains(expected), "{name}: {text_out}");
        assert!(!text_out.contains("SECRETSHOW"), "{text_out}");
        assert!(editor.image_plane.is_none());
    }
    // 4097 bytes and an empty name are protocol errors (the schema bounds), not state changes.
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": good.to_str().unwrap()}),
    )
    .await;
    let kept = editor.image_plane.clone();
    let r = set(&mut editor, &context, &client, json!({"scene_token": text, "image_name": secret.join("missing.png").to_str().unwrap(), "depth": 5.0})).await;
    assert_eq!(code(&r), "image_plane_error");
    assert_eq!(editor.image_plane, kept);
    // The reply to a success carries no path either.
    let ok = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "depth": 7.0}),
    )
    .await;
    assert!(!format!("{ok:?}").contains("SECRETSHOW"));
    let all = inspected_camera(&mut editor, &context, &client).await;
    assert!(!all.to_string().contains("SECRETSHOW"));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_image_plane_surfaces_the_reset_hint_for_a_smaller_image() {
    let dir = tempfile::tempdir().unwrap();
    let big = write_png(dir.path(), "big.png", 64, 64);
    let small = write_png(dir.path(), "small.png", 16, 16);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    set(&mut editor, &context, &client, json!({"scene_token": text, "image_name": big.to_str().unwrap(), "coverage_x": 40, "coverage_y": 40})).await;
    let before = editor.image_plane.clone();
    let r = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": small.to_str().unwrap()}),
    )
    .await;
    assert_eq!(code(&r), "image_plane_error");
    assert!(
        message(&r).contains("reset it in the same edit"),
        "{}",
        message(&r)
    );
    assert_eq!(editor.image_plane, before);
    let r = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": small.to_str().unwrap(), "reset_coverage": true}),
    )
    .await;
    assert_eq!(outcome(&r), "changed");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_image_plane_argument_errors_are_protocol_errors_with_no_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 8, 8);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": path.to_str().unwrap()}),
    )
    .await;
    let before = editor.image_plane.clone();
    for arguments in [
        json!({"scene_token": text}),
        json!({"scene_token": text, "depth": null}),
        json!({"scene_token": text, "bogus": 1}),
        json!({"depth": 5.0}),
        // One past each integer limit of the model's i32 and u32 fields, and fractions.
        json!({"scene_token": text, "frame_offset": 2_147_483_648_i64}),
        json!({"scene_token": text, "frame_offset": -2_147_483_649_i64}),
        json!({"scene_token": text, "frame_offset": 1.5}),
        json!({"scene_token": text, "frame_cache": 4_294_967_296_i64}),
        json!({"scene_token": text, "frame_cache": -1}),
        json!({"scene_token": text, "frame_cache": 2.5}),
        json!({"scene_token": text, "coverage_x": 4_294_967_296_i64}),
        json!({"scene_token": text, "coverage_origin_y": 1.5}),
        json!({"scene_token": text, "depth": 1e300}),
        json!({"scene_token": text, "alpha_gain": "1"}),
        json!({"scene_token": text, "fit": "Best"}),
        json!({"scene_token": text, "display_mode": "color"}),
        json!({"scene_token": text, "image_name": 5}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_image_plane",
            arguments.clone(),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {arguments}");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{arguments}"
        );
        assert_eq!(editor.image_plane, before, "{arguments}");
    }
    // Model range errors name the field and leave the plane alone, even beside a valid field.
    for (arguments, field) in [
        (json!({"frame_offset": 1_000_001}), "frame_offset"),
        (json!({"frame_offset": -1_000_001}), "frame_offset"),
        (json!({"frame_cache": 0}), "frame_cache"),
        (json!({"frame_cache": 1001}), "frame_cache"),
        (json!({"depth": 0}), "depth"),
        (json!({"depth": -1}), "depth"),
        (json!({"alpha_gain": -0.001}), "alpha_gain"),
        (json!({"alpha_gain": 1000.001}), "alpha_gain"),
        (json!({"fit": "fill", "alpha_gain": 2000}), "alpha_gain"),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(text);
        let r = set(&mut editor, &context, &client, arguments.clone()).await;
        assert_eq!(code(&r), "image_plane_error", "{arguments}");
        assert!(message(&r).contains(field), "{arguments}: {}", message(&r));
        assert_eq!(editor.image_plane, before, "{arguments}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_remove_image_plane_removes_and_reports_unchanged_when_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 8, 8);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    let none = call(
        &mut editor,
        &context,
        client.peer(),
        "remove_image_plane",
        json!({"scene_token": text}),
    )
    .await;
    assert_eq!(outcome(&none), "unchanged");
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": path.to_str().unwrap()}),
    )
    .await;
    let removed = call(
        &mut editor,
        &context,
        client.peer(),
        "remove_image_plane",
        json!({"scene_token": text}),
    )
    .await;
    assert_eq!(outcome(&removed), "changed");
    assert!(plane(&mut editor, &context, &client).await.is_null());
    let extra = call_response(
        &mut editor,
        &context,
        client.peer(),
        "remove_image_plane",
        json!({"scene_token": text, "x": 1}),
    )
    .await;
    assert!(matches!(extra, Err(rmcp::ServiceError::McpError(_))));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_image_plane_tools_obey_the_token_draft_modal_and_manipulation_guards() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 8, 8);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let text = editor.scene_token.to_string();
    let edit = json!({"scene_token": text, "image_name": path.to_str().unwrap()});
    let stale =
        json!({"scene_token": Uuid::new_v4().to_string(), "image_name": path.to_str().unwrap()});
    for (tool, arguments) in [
        ("set_image_plane", stale.clone()),
        (
            "remove_image_plane",
            json!({"scene_token": Uuid::new_v4().to_string()}),
        ),
    ] {
        let r = call(&mut editor, &context, client.peer(), tool, arguments).await;
        assert_eq!(code(&r), "stale_scene", "{tool}");
        assert!(editor.image_plane.is_none());
    }
    editor.name = "human_draft".into();
    let r = call(
        &mut editor,
        &context,
        client.peer(),
        "set_image_plane",
        edit.clone(),
    )
    .await;
    assert_eq!(code(&r), "busy_editing", "draft");
    assert_eq!(editor.name, "human_draft");
    assert!(editor.image_plane.is_none());
    editor.sync_fields();

    let node = editor.selected().unwrap().to_decimal();
    let ipath: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(&mut editor, &context, client.peer(), "begin_manipulation",
        json!({"scene_token": text, "node_id": node, "instance_path": ipath, "kind": "move", "axis": "x"})).await.structured_content.unwrap();
    let held = call(
        &mut editor,
        &context,
        client.peer(),
        "set_image_plane",
        edit.clone(),
    )
    .await;
    assert_eq!(code(&held), "busy_manipulation", "held");
    assert!(editor.image_plane.is_none());
    call(&mut editor, &context, client.peer(), "finish_manipulation",
        json!({"scene_token": text, "manipulation_id": begin["manipulation_id"], "finish": "cancel"})).await;

    let ok = call(
        &mut editor,
        &context,
        client.peer(),
        "set_image_plane",
        edit.clone(),
    )
    .await;
    assert_eq!(outcome(&ok), "changed");
    editor.request_close();
    for (tool, arguments) in [
        (
            "set_image_plane",
            json!({"scene_token": text, "depth": 9.0}),
        ),
        ("remove_image_plane", json!({"scene_token": text})),
    ] {
        let before = editor.image_plane.clone();
        let r = call(&mut editor, &context, client.peer(), tool, arguments).await;
        assert_eq!(code(&r), "busy_editing", "modal {tool}");
        assert_eq!(editor.image_plane, before);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
