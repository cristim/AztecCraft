//! #459 P6a: `set_image_plane` and `remove_image_plane` over the real client and Editor.
use super::camera::inspected_camera;
use super::*;
use std::path::{Path, PathBuf};

pub(super) fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![0u8; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    path
}

pub(super) fn code(result: &rmcp::model::CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .map_or_else(|| format!("no error: {result:?}"), str::to_owned)
}

pub(super) fn message(result: &rmcp::model::CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["message"]
        .as_str()
        .unwrap()
        .to_owned()
}

pub(super) fn outcome(result: &rmcp::model::CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["outcome"]
        .as_str()
        .unwrap_or("no outcome")
        .to_owned()
}

pub(super) async fn set(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    arguments: Value,
) -> rmcp::model::CallToolResult {
    call(editor, context, client.peer(), "set_image_plane", arguments).await
}

pub(super) async fn plane(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    inspected_camera(editor, context, client).await["image_plane"].clone()
}

#[tokio::test]
async fn sdk_set_image_plane_sets_each_argument_and_inspect_scene_agrees() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "plate.0001.png", 64, 32);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    let (dirty, scene) = (editor.is_dirty(), editor.history.scene().clone());
    assert!(plane(&mut editor, &context, &client).await.is_null());

    let first = set(
        &mut editor,
        &context,
        &client,
        json!({
            "scene_token": text, "image_name": path.to_str().unwrap(),
            "use_frame_extension": true, "frame_offset": 4, "frame_cache": 12,
            "fit": "to_size", "depth": 250.5, "alpha_gain": 0.5, "display_mode": "luminance",
            "coverage_x": 16, "coverage_y": 8, "coverage_origin_x": 4, "coverage_origin_y": 2,
        }),
    )
    .await;
    assert_eq!(outcome(&first), "changed");
    let state = plane(&mut editor, &context, &client).await;
    assert_eq!(
        state["settings"],
        json!({
            "use_frame_extension": true, "frame_offset": 4, "frame_cache": 12,
            "fit": "to_size", "depth_cm": 250.5, "alpha_gain": 0.5,
            "display_mode": "luminance",
            "coverage": {"origin_x": 4, "origin_y": 2, "width": 16, "height": 8}
        })
    );
    assert_eq!(state["image_size"], json!({"width": 64, "height": 32}));
    // Current time 1 plus offset 4.
    assert_eq!(state["resolved_frame"], 5);
    assert_eq!(state["resolved_file_name"], "plate.0005.png");

    let repeat = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": path.to_str().unwrap(), "depth": 250.5}),
    )
    .await;
    assert_eq!(outcome(&repeat), "unchanged");

    // One argument at a time changes only that setting.
    for (arguments, key, value) in [
        (json!({"depth": 10.0}), "depth_cm", json!(10.0)),
        (json!({"alpha_gain": 1000.0}), "alpha_gain", json!(1000.0)),
        (json!({"frame_cache": 1000}), "frame_cache", json!(1000)),
        (
            json!({"frame_offset": -1_000_000}),
            "frame_offset",
            json!(-1_000_000),
        ),
        (json!({"fit": "fill"}), "fit", json!("fill")),
        (
            json!({"display_mode": "none"}),
            "display_mode",
            json!("none"),
        ),
        (
            json!({"use_frame_extension": false}),
            "use_frame_extension",
            json!(false),
        ),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(text);
        let result = set(&mut editor, &context, &client, arguments.clone()).await;
        assert_eq!(outcome(&result), "changed", "{arguments}");
        let settings = plane(&mut editor, &context, &client).await["settings"].clone();
        assert_eq!(settings[key], value, "{arguments}");
    }
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.scene_token.to_string(), text);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_image_plane_merges_partial_coverage_and_resets_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_png(dir.path(), "p.png", 64, 32);
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    let name = path.to_str().unwrap();
    let coverage = |editor: &mut Editor| {
        let editor = &editor.image_plane;
        editor.as_ref().unwrap().plane.coverage
    };
    // None stored: the numbers you give replace those of the full image (0, 0, 64 x 32).
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "image_name": name}),
    )
    .await;
    let r = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "coverage_x": 32}),
    )
    .await;
    assert_eq!(outcome(&r), "changed");
    let c = coverage(&mut editor).unwrap();
    assert_eq!((c.origin_x, c.origin_y, c.width, c.height), (0, 0, 32, 32));
    // Some stored: only the named numbers move.
    set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "coverage_origin_x": 10}),
    )
    .await;
    let c = coverage(&mut editor).unwrap();
    assert_eq!((c.origin_x, c.origin_y, c.width, c.height), (10, 0, 32, 32));
    // A result outside the image is refused and nothing moves.
    let before = editor.image_plane.clone();
    for arguments in [
        json!({"coverage_origin_x": 33}),
        json!({"coverage_y": 33}),
        json!({"coverage_x": 0}),
        json!({"coverage_origin_y": 1}),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(text);
        let r = set(&mut editor, &context, &client, arguments.clone()).await;
        assert_eq!(code(&r), "image_plane_error", "{arguments}");
        assert!(message(&r).contains("coverage"), "{arguments}");
        assert_eq!(editor.image_plane, before, "{arguments}");
    }
    // Reset restores the full image; again it is unchanged.
    let r = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "reset_coverage": true}),
    )
    .await;
    assert_eq!(outcome(&r), "changed");
    assert!(coverage(&mut editor).is_none());
    assert!(plane(&mut editor, &context, &client).await["settings"]["coverage"].is_null());
    let r = set(
        &mut editor,
        &context,
        &client,
        json!({"scene_token": text, "reset_coverage": true}),
    )
    .await;
    assert_eq!(outcome(&r), "unchanged");
    // Reset excludes the numbers; reset false alone names nothing.
    for arguments in [
        json!({"reset_coverage": true, "coverage_x": 4}),
        json!({"reset_coverage": false}),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(text);
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
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
