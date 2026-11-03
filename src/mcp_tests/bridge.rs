use super::*;

#[test]
fn bridge_quota_queue_capacity_and_dropped_receivers_preserve_authoring() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let mut replies = Vec::new();
    for _ in 0..16 {
        replies.push(ingress.queue(create_request(&editor)).unwrap());
    }
    assert!(ingress.queue(create_request(&editor)).is_err());
    frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.scene().nodes().len(), 8);
    assert_eq!(editor.mcp.as_ref().unwrap().test_queue_len(), 8);
    assert!(replies[0].try_recv().unwrap().is_ok());
    assert!(replies[8].try_recv().is_err());
    frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.scene().nodes().len(), 16);
    drop(ingress.queue(create_request(&editor)).unwrap());
    frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.scene().nodes().len(), 16);
}

#[test]
fn ingress_eof_discards_queued_mutation_and_keeps_dirty_native_guard() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor::default();
    editor.create(Primitive::Plane);
    let before = editor.history.scene().clone();
    editor.mcp = Some(bridge);
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    ingress.close();
    frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.files.is_modal());
    assert!(!editor.mcp.as_ref().unwrap().test_alive());
    assert!(reply.try_recv().is_err());
    assert!(editor.history.can_undo());
}

#[test]
fn remote_change_requests_repaint_and_large_inspection_returns_explicit_error() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    frame(&mut editor, &context, vec![]);
    assert!(matches!(
        reply.try_recv().unwrap().unwrap(),
        OperationReply::Created { .. }
    ));
    assert!(context.has_requested_repaint());
    editor.name = "x".repeat(crate::editor_operations::MAX_RESULT_BYTES);
    assert!(editor.commit_pending());
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: None,
                operation: Operation::Inspect { page: None },
            },
            &context
        ),
        Err(OperationError::TooLarge)
    ));
}

#[test]
fn shutdown_child_with_open_stdin() {
    if std::env::var("AZTECCRAFT_MCP_SHUTDOWN_CHILD").as_deref() != Ok("1") {
        return;
    }
    let context = egui::Context::default();
    let mut editor = Editor::default();
    editor.start_mcp(&context).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while !editor.mcp.as_ref().unwrap().test_read_observed() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let mut input = egui::RawInput::default();
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut output = context.run_ui(input, |ui| editor.render(ui));
    output.textures_delta.clear();
    assert!(editor.files.is_closing());
    drop(editor);
}

#[test]
fn approved_native_close_has_bounded_process_cleanup_with_open_silent_stdin() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::mcp_editor_tests::bridge::shutdown_child_with_open_stdin",
            "--nocapture",
        ])
        .env("AZTECCRAFT_MCP_SHUTDOWN_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let open_stdin = child.stdin.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("open stdin blocked native-close cleanup");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(open_stdin);
}
