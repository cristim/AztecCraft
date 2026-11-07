use super::*;
use crate::editor_operations::FileDecision;
use azteccraft::{compatibility::MayaVersion, preferences::Store};
use std::path::PathBuf;

fn v(year: u64) -> MayaVersion {
    MayaVersion::try_from(year).unwrap()
}

fn with_store(editor: &mut Editor, dir: &tempfile::TempDir, contents: Option<&[u8]>) -> PathBuf {
    let path = dir.path().join("preferences.json");
    if let Some(contents) = contents {
        std::fs::write(&path, contents).unwrap();
    }
    let loaded = azteccraft::preferences::load(&path);
    editor.preferences = loaded.preferences;
    editor.preferences_store = loaded.store;
    path
}

fn code(result: &CallToolResult) -> &str {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .unwrap()
}

#[tokio::test]
async fn sdk_set_compatibility_version_changes_then_reports_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let dir = tempfile::tempdir().unwrap();
    let path = with_store(&mut editor, &dir, None);
    let token = editor.scene_token.to_string();
    for (year, outcome) in [(2015, "changed"), (2015, "unchanged"), (2027, "changed")] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_compatibility_version",
            set_args(&token, year),
        )
        .await;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(version_of(&mut editor, &context, &client).await, year);
    }
    assert!(path.exists());
    assert_eq!(editor.scene_token.to_string(), token);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_compatibility_version_refuses_bad_arguments_and_stale_tokens() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token": token, "version": 2010}),
        json!({"scene_token": token, "version": 2028}),
        json!({"scene_token": token, "version": -1}),
        json!({"scene_token": token, "version": 2015.5}),
        json!({"scene_token": token, "version": "2015"}),
        json!({"scene_token": token}),
        json!({"version": 2015}),
        json!({"scene_token": token, "version": 2015, "extra": true}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_compatibility_version",
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
        assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&Uuid::new_v4().to_string(), 2015),
    )
    .await;
    assert_eq!(code(&stale), "stale_scene");
    assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_preference_tools_are_busy_while_a_window_value_is_pending() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.preferences_window.pending = Some(v(2020));
    for (name, arguments) in [
        ("set_compatibility_version", set_args(&token, 2015)),
        ("reset_preferences", json!({"scene_token": token})),
    ] {
        let result = call(&mut editor, &context, client.peer(), name, arguments).await;
        assert_eq!(code(&result), "busy_editing");
    }
    let unrelated = call(
        &mut editor,
        &context,
        client.peer(),
        "set_sidebar_panel_visibility",
        json!({"scene_token": token, "panel": "channel_box", "visible": true}),
    )
    .await;
    assert_ne!(unrelated.is_error, Some(true));
    assert_eq!(editor.preferences_window.pending, Some(v(2020)));
    assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_reset_preferences_acts_only_on_a_blocked_file() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let dir = tempfile::tempdir().unwrap();
    let path = with_store(&mut editor, &dir, Some(b"not json"));
    assert!(matches!(editor.preferences_store, Store::Blocked { .. }));
    let token = editor.scene_token.to_string();
    for outcome in ["changed", "unchanged"] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "reset_preferences",
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
    }
    assert!(!path.exists());
    assert!(path.with_file_name("preferences.json.bad-1").exists());
    let extra = call_response(
        &mut editor,
        &context,
        client.peer(),
        "reset_preferences",
        json!({"scene_token": token, "extra": 1}),
    )
    .await;
    assert!(extra.is_err());
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "reset_preferences",
        json!({"scene_token": Uuid::new_v4().to_string()}),
    )
    .await;
    assert_eq!(code(&stale), "stale_scene");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_a_file_changed_on_disk_is_reported_without_the_path() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let dir = tempfile::tempdir().unwrap();
    let path = with_store(&mut editor, &dir, None);
    std::fs::write(&path, b"edited by hand").unwrap();
    let token = editor.scene_token.to_string();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        json!({"scene_token": token, "version": 2015}),
    )
    .await;
    assert_eq!(code(&result), "preferences_changed_on_disk");
    let rendered = serde_json::to_string(&result).unwrap();
    let root = dir.path().to_string_lossy().into_owned();
    assert!(!rendered.contains(&root) && !rendered.contains("preferences.json"));
    assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn inspection(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

async fn version_of(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> u64 {
    inspection(editor, context, client).await["preferences"]["maya_compatibility_version"]
        .as_u64()
        .unwrap()
}

fn assert_path_free(text: &str, dir: &tempfile::TempDir) {
    let name = dir.path().file_name().unwrap().to_string_lossy();
    for leak in [
        dir.path().to_string_lossy().as_ref(),
        name.as_ref(),
        "preferences.json",
    ] {
        assert!(!text.contains(leak), "{leak} leaked into {text}");
    }
}

fn stored_file(path: &std::path::Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn set_args(token: &str, year: u64) -> Value {
    json!({"scene_token": token, "version": year})
}

#[tokio::test]
async fn sdk_a_saved_version_reaches_the_file_and_units_stay_read_only() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let dir = tempfile::tempdir().unwrap();
    let path = with_store(&mut editor, &dir, None);
    let token = editor.scene_token.to_string();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_path_free(&serde_json::to_string(&result).unwrap(), &dir);
    let inspected = inspection(&mut editor, &context, &client).await;
    assert_eq!(inspected["preferences"]["persistence"]["state"], "saved");
    assert_eq!(inspected["units"]["editable"], false);
    assert_path_free(&inspected.to_string(), &dir);
    assert_eq!(
        stored_file(&path),
        json!({"schema": 1, "maya_compatibility_version": 2015})
    );
    assert_path_free(&editor.status, &dir);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_compatibility_version_is_refused_in_every_busy_editor_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(code(&draft), "busy_editing");
    editor.sync_fields();

    context.memory_mut(|memory| memory.request_focus(name_edit_id()));
    let focus = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(code(&focus), "busy_editing");
    context.memory_mut(|memory| memory.surrender_focus(name_edit_id()));

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(code(&modal), "busy_editing");
    let pending_id = editor.files.pending_inspection().unwrap().id;
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: pending_id.parse().unwrap(),
            decision: FileDecision::Discard,
        })
        .unwrap();
    assert!(editor.files.is_closing());
    let closing = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(code(&closing), "busy_editing");
    assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_preference_tools_are_busy_during_a_manipulation() {
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
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": token, "node_id": created["node_id"], "instance_path": path, "kind": "move", "axis": "x"}),
    )
    .await;
    for (name, arguments) in [
        ("set_compatibility_version", set_args(&token, 2015)),
        ("reset_preferences", json!({"scene_token": token})),
    ] {
        let result = call(&mut editor, &context, client.peer(), name, arguments).await;
        assert_eq!(code(&result), "busy_manipulation", "{name}");
    }
    assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_a_blocked_store_is_reported_then_reset_and_saved_again() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let dir = tempfile::tempdir().unwrap();
    let path = with_store(&mut editor, &dir, Some(b"not json"));
    let token = editor.scene_token.to_string();
    let blocked =
        inspection(&mut editor, &context, &client).await["preferences"]["persistence"].clone();
    assert_eq!(blocked["state"], "not_saved");
    assert_eq!(blocked["reset_available"], true);
    let applied = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(
        applied.structured_content.as_ref().unwrap()["outcome"],
        "changed"
    );
    let after_set = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await;
    let inspected = &after_set.structured_content.as_ref().unwrap()["inspection"]["preferences"];
    assert_eq!(inspected["maya_compatibility_version"], 2015);
    assert_eq!(inspected["persistence"]["state"], "not_saved");
    for text in [
        serde_json::to_string(&applied).unwrap(),
        serde_json::to_string(&after_set).unwrap(),
        editor.status.clone(),
    ] {
        assert_path_free(&text, &dir);
    }
    let reset = call(
        &mut editor,
        &context,
        client.peer(),
        "reset_preferences",
        json!({"scene_token": token}),
    )
    .await;
    assert_path_free(&serde_json::to_string(&reset).unwrap(), &dir);
    assert_path_free(&editor.status, &dir);
    assert!(path.with_file_name("preferences.json.bad-1").exists());
    assert_eq!(
        std::fs::read(path.with_file_name("preferences.json.bad-1")).unwrap(),
        b"not json"
    );
    let set = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(
        set.structured_content.as_ref().unwrap()["outcome"],
        "changed"
    );
    assert_path_free(&serde_json::to_string(&set).unwrap(), &dir);
    assert_path_free(&editor.status, &dir);
    let after = inspection(&mut editor, &context, &client).await["preferences"].clone();
    assert_eq!(after["persistence"]["state"], "saved");
    assert_eq!(after["maya_compatibility_version"], 2015);
    assert_eq!(
        stored_file(&path),
        json!({"schema": 1, "maya_compatibility_version": 2015})
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_a_session_only_store_reports_session_only_and_reset_is_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let set = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        set_args(&token, 2015),
    )
    .await;
    assert_eq!(set.structured_content.unwrap()["outcome"], "changed");
    let persistence =
        inspection(&mut editor, &context, &client).await["preferences"]["persistence"].clone();
    assert_eq!(persistence["state"], "session_only");
    assert_eq!(persistence["reset_available"], false);
    let reset = call(
        &mut editor,
        &context,
        client.peer(),
        "reset_preferences",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(reset.structured_content.unwrap()["outcome"], "unchanged");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
