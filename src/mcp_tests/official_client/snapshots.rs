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
    if untitled_inspection_snapshot(name) {
        // These fixtures use the Unix authored-scene writer. Windows does not
        // offer Untitled Save As yet; compare the real SDK reply to that policy
        // without normalizing away a wrong availability value in the reply.
        if cfg!(unix) && std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|value| value == "1") {
            crate::app::snapshots::assert_snapshot(name, &text);
        } else {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/snapshots")
                .join(name);
            let expected = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!(
                normalized,
                untitled_platform_expectation(expected, cfg!(unix)),
                "{name} differs"
            );
        }
    } else {
        crate::app::snapshots::assert_snapshot(name, &text);
    }
}

fn untitled_inspection_snapshot(name: &str) -> bool {
    matches!(
        name,
        "inspection_cube_face_grid_off_move_tool.json"
            | "inspection_cube_move_in_flight.json"
            | "inspection_pending_file_action.json"
    )
}

fn untitled_platform_expectation(mut expected: Value, unix: bool) -> Value {
    let inspection = expected
        .get_mut("inspection")
        .and_then(Value::as_object_mut)
        .unwrap();
    assert_eq!(inspection.get("document_origin"), Some(&json!("untitled")));
    let available = inspection.get_mut("save_as_available").unwrap();
    assert_eq!(*available, json!(true), "shared fixture must remain Unix");
    *available = json!(unix);
    expected
}

#[test]
fn untitled_snapshots_preserve_platform_availability_and_all_other_fields() {
    for name in [
        "inspection_cube_face_grid_off_move_tool.json",
        "inspection_cube_move_in_flight.json",
        "inspection_pending_file_action.json",
    ] {
        assert!(untitled_inspection_snapshot(name));
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots")
            .join(name);
        let baseline: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            untitled_platform_expectation(baseline.clone(), true),
            baseline
        );
        let windows = untitled_platform_expectation(baseline.clone(), false);
        let mut expected_windows = baseline.clone();
        expected_windows["inspection"]["save_as_available"] = json!(false);
        assert_eq!(windows, expected_windows);
        assert_ne!(windows, baseline, "opposite-platform replies must fail");
        let mut wrong_other_field = windows.clone();
        wrong_other_field["inspection"]["save_available"] = json!(true);
        assert_ne!(windows, wrong_other_field, "other fields must stay checked");
    }
    assert!(!untitled_inspection_snapshot(
        "inspection_ascii_curve_specimen.json"
    ));
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
