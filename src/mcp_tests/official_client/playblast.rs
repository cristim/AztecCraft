use super::*;
use crate::app::features::playblast::fake::{self, Asked, Seen};
use rmcp::model::ErrorCode;
use rmcp::service::RunningService;

type Client = RunningService<RoleClient, ()>;

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
    name: &'static str,
    arguments: Value,
) -> Value {
    call(editor, context, client.peer(), name, arguments)
        .await
        .structured_content
        .unwrap()
}

async fn progress(editor: &mut Editor, context: &egui::Context, client: &Client) -> Value {
    let inspected = tool(editor, context, client, "inspect_scene", json!({})).await;
    inspected["inspection"]["playblast"].clone()
}

fn arguments(token: &str, directory: &std::path::Path, end: u32) -> Value {
    json!({"scene_token": token, "start_frame": 1, "end_frame": end, "width": 4,
        "height": 4, "output_directory": directory, "name": "shot"})
}

async fn rig() -> (Editor, egui::Context, Client, tokio::task::JoinHandle<()>) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.playblast.factory = Some(fake::factory(Seen::default()));
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    (editor, context, client, server)
}

#[tokio::test]
async fn sdk_playblast_writes_numbered_pngs_and_reports_progress() {
    let (mut editor, context, client, server) = rig().await;
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Asked::default()));
    editor.playblast.factory = Some(fake::recording_factory(
        Seen::default(),
        std::sync::Arc::clone(&asked),
    ));
    let dir = tempfile::tempdir().unwrap();
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let started = tool(
        &mut editor,
        &context,
        &client,
        "playblast",
        json!({"scene_token": token, "start_frame": 1, "end_frame": 3, "width": 64,
            "height": 32, "percent": 50, "output_directory": dir.path(), "name": "shot",
            "format": "png", "show_ornaments": false}),
    )
    .await;
    assert_eq!(started["outcome"], "playblast_started");
    assert_eq!(
        (
            started["frames"].clone(),
            started["width"].clone(),
            started["height"].clone()
        ),
        (json!(3), json!(32), json!(16))
    );
    assert!(!started.to_string().contains(dir.path().to_str().unwrap()));
    let mut state = progress(&mut editor, &context, &client).await;
    for _ in 0..200 {
        if state["state"] != "running" {
            break;
        }
        state = progress(&mut editor, &context, &client).await;
    }
    assert_eq!(state["state"], "finished");
    assert_eq!(
        (state["frames_done"].clone(), state["frames_total"].clone()),
        (json!(3), json!(3))
    );
    assert_eq!(
        state["directory_name"],
        dir.path().file_name().unwrap().to_str().unwrap()
    );
    for frame in 1..=3u8 {
        let path = dir.path().join(format!("shot.{frame:04}.png"));
        let mut reader =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()))
                .read_info()
                .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (32, 16));
        assert_eq!(&pixels[..4], &[frame, 0, 0, 255]);
    }
    // show_ornaments:false reaches the renderer as no grid, and percent 50 as a 32x16 frame.
    {
        let asked = asked.lock().unwrap();
        assert_eq!(asked.grid, [false]);
        assert_eq!(asked.sizes, [(32, 16); 3]);
    }
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.scene_token.to_string(), token);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_playblast_cancels_mid_job_and_refuses_a_second_start() {
    let (mut editor, context, client, server) = rig().await;
    let dir = tempfile::tempdir().unwrap();
    let token = editor.scene_token.to_string();
    let args = arguments(&token, dir.path(), 5000);
    let none = tool(
        &mut editor,
        &context,
        &client,
        "cancel_playblast",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(none["outcome"], "unchanged");
    let started = tool(&mut editor, &context, &client, "playblast", args.clone()).await;
    assert_eq!(started["frames"], 5000);
    let busy = tool(&mut editor, &context, &client, "playblast", args).await;
    assert_eq!(busy["error"]["code"], "playblast_busy");
    let running = progress(&mut editor, &context, &client).await;
    assert_eq!(running["state"], "running");
    let cancelled = tool(
        &mut editor,
        &context,
        &client,
        "cancel_playblast",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(cancelled["outcome"], "changed");
    let after = progress(&mut editor, &context, &client).await;
    assert_eq!(after["state"], "cancelled");
    assert!(after["frames_done"].as_u64().unwrap() < 5000);
    let files = std::fs::read_dir(dir.path()).unwrap().count() as u64;
    assert_eq!(files, after["frames_done"].as_u64().unwrap());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_playblast_validates_parameters() {
    let (mut editor, context, client, server) = rig().await;
    let dir = tempfile::tempdir().unwrap();
    let token = editor.scene_token.to_string();
    let base = || arguments(&token, dir.path(), 3);
    let with = |key: &str, value: Value| {
        let mut args = base();
        args[key] = value;
        args
    };
    let without = |key: &str| {
        let mut args = base();
        args.as_object_mut().unwrap().remove(key);
        args
    };
    for rejected in [
        without("width"),
        without("height"),
        with("output_directory", json!("relative/dir")),
        with("output_directory", json!("")),
        with("percent", json!(0)),
        with("percent", json!(101)),
        with("format", json!("mov")),
        with("width", json!(1e10)),
        with("extra", json!(1)),
    ] {
        let response = call_response(
            &mut editor,
            &context,
            client.peer(),
            "playblast",
            rejected.clone(),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = response else {
            panic!("accepted {rejected}");
        };
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS, "{rejected}");
    }
    let missing = dir.path().join("missing");
    for (arguments, code) in [
        (
            with("output_directory", json!(missing)),
            "playblast_invalid",
        ),
        (with("start_frame", json!(5)), "playblast_invalid"),
        (with("start_frame", json!(-1)), "playblast_invalid"),
        (with("name", json!("a/b")), "playblast_invalid"),
        (with("width", json!(100_000)), "playblast_too_large"),
        (with("end_frame", json!(1_000_000)), "playblast_too_large"),
    ] {
        let reply = tool(
            &mut editor,
            &context,
            &client,
            "playblast",
            arguments.clone(),
        )
        .await;
        assert_eq!(reply["error"]["code"], code, "{arguments}");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    assert_eq!(
        progress(&mut editor, &context, &client).await["state"],
        "idle"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn refused(editor: &mut Editor, context: &egui::Context, client: &Client, token: &str) {
    let dir = tempfile::tempdir().unwrap();
    let busy = tool(
        editor,
        context,
        client,
        "playblast",
        arguments(token, dir.path(), 3),
    )
    .await;
    assert_eq!(busy["error"]["code"], "busy_editing");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    assert_eq!(progress(editor, context, client).await["state"], "idle");
}

#[tokio::test]
async fn sdk_playblast_refuses_a_stale_token_and_a_human_draft() {
    let (mut editor, context, client, server) = rig().await;
    let dir = tempfile::tempdir().unwrap();
    let stale = tool(
        &mut editor,
        &context,
        &client,
        "playblast",
        arguments(&Uuid::new_v4().to_string(), dir.path(), 3),
    )
    .await;
    assert_eq!(stale["error"]["code"], "stale_scene");
    let token = editor.scene_token.to_string();
    editor.name = "differing_human_draft".into();
    refused(&mut editor, &context, &client, &token).await;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_playblast_refuses_during_a_modal_and_a_pending_close() {
    for pending_tool in ["new_scene", "request_close"] {
        let (mut editor, context, client, server) = rig().await;
        let token = editor.scene_token.to_string();
        let pending = tool(
            &mut editor,
            &context,
            &client,
            pending_tool,
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(pending["outcome"], "pending_confirmation", "{pending}");
        refused(&mut editor, &context, &client, &token).await;
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
