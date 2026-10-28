use super::*;

#[tokio::test]
async fn sdk_tool_catalog_matches_committed_snapshot() {
    let context = egui::Context::default();
    let (_editor, client, server) = start(&context).await;
    let catalog = client.peer().list_tools(None).await.unwrap();
    assert!(catalog.next_cursor.is_none());
    let mut text = serde_json::to_string_pretty(&catalog.tools).unwrap();
    text.push('\n');
    crate::app::snapshots::assert_snapshot("mcp_tool_catalog.json", &text);
    client.cancel().await.unwrap();
    server.await.unwrap();
}

async fn inspection_snapshot(
    name: &str,
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    roots: &[&std::path::Path],
) {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let normalized = crate::app::snapshots::normalize_json(&inspected, roots);
    let mut text = serde_json::to_string_pretty(&normalized).unwrap();
    text.push('\n');
    crate::app::snapshots::assert_snapshot(name, &text);
}

#[tokio::test]
async fn sdk_inspection_matches_committed_snapshots() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
    let instance = &view.instances()[0];
    let target = FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 0,
    };
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    editor
        .apply_operation(Operation::SelectFace {
            target: Some(target),
        })
        .unwrap();
    editor
        .apply_operation(Operation::SetGridVisible { visible: false })
        .unwrap();
    editor
        .apply_operation(Operation::SetActiveTool(ActiveTool::Move))
        .unwrap();
    inspection_snapshot(
        "inspection_cube_face_grid_off_move_tool.json",
        &mut editor,
        &context,
        &client,
        &[],
    )
    .await;

    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
        .unwrap();
    let node = editor.selected().unwrap();
    let begin = editor
        .apply_operation(Operation::BeginMove {
            target: azteccraft::native_scene::TransformTarget {
                node,
                path: editor.selected_path().to_vec(),
            },
            axis: azteccraft::native_scene::WorldAxis::X,
        })
        .unwrap();
    let OperationReply::ManipulationStarted {
        manipulation_id: move_id,
    } = begin
    else {
        panic!("expected a started Move")
    };
    editor
        .apply_operation(Operation::UpdateMove {
            id: azteccraft::history::ManipulationId::from_decimal(&move_id).unwrap(),
            distance: crate::editor_operations::MoveDistance::Centimeters(2.5),
        })
        .unwrap();
    inspection_snapshot(
        "inspection_cube_move_in_flight.json",
        &mut editor,
        &context,
        &client,
        &[],
    )
    .await;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_ascii_curve_and_pending_action_inspections_match_committed_snapshots() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor
        .apply_operation(Operation::OpenScene { path: path.clone() })
        .unwrap();
    inspection_snapshot(
        "inspection_ascii_curve_specimen.json",
        &mut editor,
        &context,
        &client,
        &[directory.path()],
    )
    .await;

    editor.apply_operation(Operation::NewScene).unwrap();
    editor.create(Primitive::Cube);
    let reply = editor.apply_operation(Operation::NewScene).unwrap();
    assert!(matches!(reply, OperationReply::PendingConfirmation { .. }));
    inspection_snapshot(
        "inspection_pending_file_action.json",
        &mut editor,
        &context,
        &client,
        &[directory.path()],
    )
    .await;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// Pins the serialized names of the `page` block, `next_cursor: null` on the last page included.
#[tokio::test]
async fn sdk_last_page_inspection_matches_committed_snapshot() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let peer = client.peer();
    let first = call(
        &mut editor,
        &context,
        peer,
        "inspect_scene",
        json!({"node_limit": 1}),
    )
    .await;
    let cursor = first.structured_content.unwrap()["inspection"]["page"]["next_cursor"].clone();
    let last = call(
        &mut editor,
        &context,
        peer,
        "inspect_scene",
        json!({"node_limit": 1, "cursor": cursor}),
    )
    .await
    .structured_content
    .unwrap();
    let normalized = crate::app::snapshots::normalize_json(&last, &[]);
    let mut text = serde_json::to_string_pretty(&normalized).unwrap();
    text.push('\n');
    crate::app::snapshots::assert_snapshot("inspection_paged_last_page.json", &text);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
