use super::*;

#[test]
fn ascii_remote_open_obeys_all_existing_editor_guards_before_candidate_adoption() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    for guard in 0..7 {
        let context = egui::Context::default();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected().unwrap();
        editor
            .apply_operation(Operation::Rename {
                id,
                name: "undone".into(),
            })
            .unwrap();
        editor.apply_operation(Operation::Undo).unwrap();
        match guard {
            0 => editor.name = "pending human name".into(),
            1 => editor.channels[0] = "nan".into(),
            2 => context.memory_mut(|memory| memory.request_focus(name_edit_id())),
            3 => editor.native_input_pending = true,
            4 => (),
            5 => {
                editor.apply_operation(Operation::NewScene).unwrap();
            }
            6 => {
                let OperationReply::PendingConfirmation {
                    pending_action_id, ..
                } = editor.apply_operation(Operation::RequestClose).unwrap()
                else {
                    panic!("expected close confirmation")
                };
                editor
                    .apply_operation(Operation::ResolveFileAction {
                        pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                        decision: crate::editor_operations::FileDecision::Discard,
                    })
                    .unwrap();
                assert!(editor.files.is_closing());
            }
            _ => unreachable!(),
        }
        let before = serde_json::to_value(
            editor
                .remote_operation(
                    RemoteRequest {
                        expected_scene_token: None,
                        operation: Operation::Inspect,
                    },
                    &context,
                )
                .unwrap(),
        )
        .unwrap();
        let scene = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let token = editor.scene_token;
        let fields = (editor.name.clone(), editor.channels.clone());
        let result = editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(if guard == 4 { Uuid::new_v4() } else { token }),
                operation: Operation::OpenScene { path: path.clone() },
            },
            &context,
        );
        if guard == 4 {
            assert!(matches!(result, Err(OperationError::StaleScene)));
        } else {
            assert!(matches!(result, Err(OperationError::BusyEditing)));
        }
        let after = serde_json::to_value(
            editor
                .remote_operation(
                    RemoteRequest {
                        expected_scene_token: None,
                        operation: Operation::Inspect,
                    },
                    &context,
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(after, before);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.scene_token, token);
        assert_eq!((editor.name, editor.channels), fields);
    }
}

#[test]
fn pending_file_resolution_retains_input_draft_focus_and_scene_guards() {
    for condition in 0..5 {
        let context = egui::Context::default();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let token = editor.scene_token;
        let reply = editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(token),
                    operation: Operation::NewScene,
                },
                &context,
            )
            .unwrap();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = reply
        else {
            panic!("expected pending confirmation")
        };
        let before = editor.history.scene().clone();
        match condition {
            0 => editor.name = "human_draft".into(),
            1 => editor.channels[0] = "nan".into(),
            2 => context.memory_mut(|memory| memory.request_focus(name_edit_id())),
            3 => editor.native_input_pending = true,
            _ => (),
        }
        let result = editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(if condition == 4 {
                    Uuid::new_v4()
                } else {
                    token
                }),
                operation: Operation::ResolveFileAction {
                    pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                    decision: crate::editor_operations::FileDecision::Discard,
                },
            },
            &context,
        );
        if condition == 4 {
            assert!(matches!(result, Err(OperationError::StaleScene)));
        } else {
            assert!(matches!(result, Err(OperationError::BusyEditing)));
        }
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        let OperationReply::Scene { inspection } = editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: None,
                    operation: Operation::Inspect,
                },
                &context,
            )
            .unwrap()
        else {
            panic!("expected inspection")
        };
        assert_eq!(inspection.pending_action.unwrap().id, pending_action_id);
    }
}

#[test]
fn undelivered_remote_close_withdraws_approval_and_suppresses_disconnect_retry() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.history_step(false);
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    editor.mcp = Some(bridge);
    let mut reply = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::RequestClose,
        })
        .unwrap();
    let first = hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        reply.try_recv().unwrap(),
        Ok(OperationReply::CloseRequested { .. })
    ));
    assert!(
        !first
            .viewport_commands
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|commands| commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)))
    );
    ingress.close();
    let failed = hidden_frame(&mut editor, &context, vec![]);
    assert!(
        !failed
            .viewport_commands
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|commands| commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)))
    );
    assert!(!editor.files.is_closing());
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.can_redo());
    assert_eq!(editor.scene_token, token);
    let OperationReply::Scene { inspection } = editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: None,
                operation: Operation::Inspect,
            },
            &context,
        )
        .unwrap()
    else {
        panic!("expected inspection")
    };
    assert!(!inspection.closing);
    editor.request_close();
    let explicit = hidden_frame(&mut editor, &context, vec![]);
    assert!(
        explicit.viewport_commands[&egui::ViewportId::ROOT]
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Close))
    );
}

#[test]
fn applied_pending_and_cancelled_file_replies_request_a_following_frame() {
    for condition in 0..3 {
        let context = egui::Context::default();
        let (mut bridge, ingress) = mcp::test_bridge(context.clone());
        let mut editor = Editor::default();
        if condition != 0 {
            editor.create(Primitive::Cube);
        }
        let operation = if condition == 2 {
            let OperationReply::PendingConfirmation {
                pending_action_id, ..
            } = editor
                .remote_operation(
                    RemoteRequest {
                        expected_scene_token: Some(editor.scene_token),
                        operation: Operation::NewScene,
                    },
                    &context,
                )
                .unwrap()
            else {
                panic!("expected pending")
            };
            Operation::ResolveFileAction {
                pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                decision: crate::editor_operations::FileDecision::Cancel,
            }
        } else {
            Operation::NewScene
        };
        for _ in 0..2 {
            let _ = context.run_logic(&egui::RawInput::default(), |_| {});
        }
        assert!(!context.has_requested_repaint());
        let mut reply = ingress
            .queue(RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation,
            })
            .unwrap();
        bridge.dispatch(&mut editor, &context);
        match condition {
            0 => assert!(matches!(
                reply.try_recv().unwrap(),
                Ok(OperationReply::DocumentApplied { .. })
            )),
            1 => assert!(matches!(
                reply.try_recv().unwrap(),
                Ok(OperationReply::PendingConfirmation { .. })
            )),
            _ => assert!(matches!(
                reply.try_recv().unwrap(),
                Ok(OperationReply::Cancelled { .. })
            )),
        }
        assert!(context.has_requested_repaint());
    }
}

#[test]
fn approved_close_still_dispatches_readonly_inspection_and_refuses_mutation() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    editor.request_close();
    let mut inspected = ingress
        .queue(RemoteRequest {
            expected_scene_token: None,
            operation: Operation::Inspect,
        })
        .unwrap();
    let mut mutation = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation: Operation::NewScene,
        })
        .unwrap();
    editor.process_mcp(&context);
    let OperationReply::Scene { inspection } = inspected.try_recv().unwrap().unwrap() else {
        panic!("expected inspection")
    };
    assert!(inspection.closing);
    assert!(matches!(
        mutation.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
}
