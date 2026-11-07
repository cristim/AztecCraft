use super::*;
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

async fn version_of(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> u64 {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["preferences"]["maya_compatibility_version"]
        .as_u64()
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
            json!({"scene_token": token, "version": year}),
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
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "set_compatibility_version",
                arguments.clone()
            )
            .await
            .is_err(),
            "{arguments}"
        );
        assert_eq!(version_of(&mut editor, &context, &client).await, 2027);
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_compatibility_version",
        json!({"scene_token": Uuid::new_v4().to_string(), "version": 2015}),
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
        (
            "set_compatibility_version",
            json!({"scene_token": token, "version": 2015}),
        ),
        ("reset_preferences", json!({"scene_token": token})),
    ] {
        let result = call(&mut editor, &context, client.peer(), name, arguments).await;
        assert_eq!(code(&result), "busy_editing");
    }
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
