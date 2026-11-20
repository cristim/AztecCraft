//! A marking menu has no tool of its own: its entries map to existing tools (`MeshAction::
//! mcp_tool`). What MCP sees of the menu is the shared human-input guard.

use super::*;

fn open_menu(editor: &mut Editor) {
    editor.marking_menu.key_open([100.0, 100.0], 0.0);
    assert!(editor.marking_menu.is_active());
}

#[tokio::test]
async fn an_open_marking_menu_refuses_remote_mutation_and_inspection_reports_it() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let args = || json!({"scene_token": token, "mode": "vertex"});
    open_menu(&mut editor);
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        args(),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(
        editor.selection_mode,
        SelectionMode::Object,
        "baseline untouched"
    );
    let inspection = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap()["inspection"]
        .clone();
    assert_eq!(
        inspection["native_input_pending"], true,
        "a blocked client can see why"
    );
    // The human dismisses the menu: the same request now applies.
    editor.marking_menu.escape();
    let done = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        args(),
    )
    .await;
    assert_ne!(done.is_error, Some(true));
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn the_menu_idle_dismissal_lets_a_waiting_remote_mutation_through() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    open_menu(&mut editor);
    // 30 s without human input: the next pass dismisses it before the request is applied.
    let mut input = egui::RawInput {
        time: Some(31.0),
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        ..Default::default()
    };
    input.events.clear();
    let mut output = context.run_ui(input, |ui| {
        let mut native = eframe::Frame::_new_kittest();
        eframe::App::logic(&mut editor, ui.ctx(), &mut native);
        eframe::App::ui(&mut editor, ui, &mut native);
    });
    output.textures_delta.clear();
    assert!(!editor.marking_menu.holds_overlay());
    let done = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token": token, "mode": "face"}),
    )
    .await;
    assert_ne!(done.is_error, Some(true));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
