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
    assert_inspection_snapshot(name, &inspected, roots);
}

/// Compares an `inspect_scene` reply with its committed snapshot, keyed on the reply's own
/// `document_origin` and never on the file name, so a new Untitled fixture cannot be missed.
fn assert_inspection_snapshot(name: &str, reply: &Value, roots: &[&std::path::Path]) {
    let normalized = crate::app::snapshots::normalize_json(reply, roots);
    let mut text = serde_json::to_string_pretty(&normalized).unwrap();
    text.push('\n');
    let untitled = normalized["inspection"]["document_origin"] == json!("untitled");
    if untitled && !cfg!(unix) {
        // Untitled fixtures are Unix baselines (the authored-scene writer is Unix only). Compare
        // the real reply with the baseline, forcing only `save_as_available`.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/snapshots")
            .join(name);
        let expected = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            normalized,
            untitled_platform_expectation(expected, false),
            "{name} differs"
        );
    } else {
        crate::app::snapshots::assert_snapshot(name, &text);
    }
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
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let mut untitled = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if !(name.starts_with("inspection_") && name.ends_with(".json")) {
            continue;
        }
        let baseline: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if baseline["inspection"]["document_origin"] != json!("untitled") {
            continue;
        }
        untitled += 1;
        assert_eq!(
            baseline["inspection"]["save_as_available"],
            json!(true),
            "{name}: an Untitled fixture is a Unix baseline"
        );
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
    assert!(untitled >= 4, "expected the known Untitled fixtures");
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
    assert_inspection_snapshot("inspection_paged_last_page.json", &last, &[]);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
