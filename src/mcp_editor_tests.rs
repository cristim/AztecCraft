use super::*;
use crate::mcp;

#[test]
fn face_remote_operations_and_hidden_callbacks_preserve_guarded_selection_baselines() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cube.aztec");
    let mut legacy = azteccraft::scene::Scene::default();
    legacy.create(Primitive::Cube, "cube").unwrap();
    azteccraft::document::save_new(&path, &legacy).unwrap();
    for select_face in [false, true] {
        for condition in 0..7 {
            let context = egui::Context::default();
            let (bridge, ingress) = mcp::test_bridge(context.clone());
            let mut editor = Editor::default();
            editor
                .apply_operation(Operation::OpenScene { path: path.clone() })
                .unwrap();
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
                    target: Some(target.clone()),
                })
                .unwrap();
            frame(&mut editor, &context, vec![]);
            match condition {
                1 => editor.name = "differing_human_draft".into(),
                2 => editor.channels[0] = "nan".into(),
                3 => context.memory_mut(|memory| memory.request_focus(name_edit_id())),
                4 => {
                    editor.name = "committed_before_modal".into();
                    editor.request_new();
                    assert!(editor.files.is_modal());
                }
                5 => {
                    editor.request_close();
                    assert!(editor.files.is_closing());
                }
                6 => editor.native_input_pending = true,
                _ => (),
            }
            let baseline = (
                editor.history.scene().clone(),
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.name.clone(),
                editor.channels.clone(),
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.is_dirty(),
            );
            let operation = || {
                if select_face {
                    Operation::SelectFace {
                        target: Some(FaceTarget {
                            face_index: 1,
                            ..target.clone()
                        }),
                    }
                } else {
                    Operation::SetSelectionMode(SelectionMode::Object)
                }
            };
            let request = || RemoteRequest {
                expected_scene_token: Some(if condition == 0 {
                    Uuid::nil()
                } else {
                    baseline.6
                }),
                operation: operation(),
            };
            let result = editor.remote_operation(request(), &context);
            assert!(matches!(
                result,
                Err(OperationError::StaleScene | OperationError::BusyEditing)
            ));
            if condition != 6 {
                editor.mcp = Some(bridge);
                let mut reply = ingress.queue(request()).unwrap();
                hidden_frame(&mut editor, &context, vec![]);
                assert!(matches!(
                    reply.try_recv().unwrap(),
                    Err(OperationError::StaleScene | OperationError::BusyEditing)
                ));
            }
            assert_eq!(
                (
                    editor.history.scene().clone(),
                    editor.selection_mode,
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone(),
                    editor.camera.clone(),
                    editor.scene_token,
                    editor.name.clone(),
                    editor.channels.clone(),
                    editor.history.can_undo(),
                    editor.history.can_redo(),
                    editor.is_dirty(),
                ),
                baseline
            );
        }
    }
}

fn camera_operations() -> [Operation; 3] {
    [
        Operation::Orbit {
            radians: [0.2, -0.1],
        },
        Operation::Pan {
            world_units: [1.0, -2.0],
        },
        Operation::Dolly { factor: 0.5 },
    ]
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

#[test]
fn camera_sparse_extrusion_and_grid_requests_keep_human_and_closed_bridge_guards() {
    for condition in 0..8 {
        for operation_index in 0..6 {
            let context = egui::Context::default();
            let (bridge, ingress) = mcp::test_bridge(context.clone());
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let id = editor.selected.unwrap();
            let shape = editor.history.scene().shapes()[0].id();
            let operation = match operation_index {
                0..=2 => camera_operations()
                    .into_iter()
                    .nth(operation_index)
                    .unwrap(),
                3 => Operation::EditNode {
                    id,
                    name: None,
                    translation: [None; 3],
                    rotation_degrees: [None; 3],
                    scale: [None; 3],
                },
                4 => Operation::ExtrudeFace {
                    target: FaceTarget {
                        transform: id,
                        shape,
                        path: vec![
                            editor.history.scene().node(id).unwrap().transform_id(),
                            shape,
                        ],
                        face_index: 1,
                    },
                    distance: 1.0,
                },
                _ => Operation::SetGridVisible { visible: false },
            };
            match condition {
                0 => editor.name = "human_draft".into(),
                1 => editor.channels[0] = "nan".into(),
                2 => context.memory_mut(|memory| memory.request_focus(name_edit_id())),
                3 => editor.request_close(),
                4 => editor.native_input_pending = true,
                7 => {
                    editor.history_step(false);
                    editor.request_close();
                }
                _ => (),
            }
            let before = editor.history.scene().clone();
            let camera = editor.camera.clone();
            let name = editor.name.clone();
            let channels = editor.channels.clone();
            let token = editor.scene_token;
            let undo = editor.history.can_undo();
            let redo = editor.history.can_redo();
            let mut reply = ingress
                .queue(RemoteRequest {
                    expected_scene_token: Some(if condition == 5 {
                        Uuid::new_v4()
                    } else {
                        token
                    }),
                    operation,
                })
                .unwrap();
            if condition == 6 {
                ingress.close();
            }
            let mut bridge = bridge;
            bridge.dispatch(&mut editor, &context);
            if condition == 6 {
                assert!(reply.try_recv().is_err());
            } else if condition == 5 {
                assert!(matches!(
                    reply.try_recv().unwrap(),
                    Err(OperationError::StaleScene)
                ));
            } else {
                assert!(matches!(
                    reply.try_recv().unwrap(),
                    Err(OperationError::BusyEditing)
                ));
            }
            assert_eq!(editor.camera, camera);
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(editor.name, name);
            assert_eq!(editor.channels, channels);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.history.can_undo(), undo);
            assert_eq!(editor.history.can_redo(), redo);
            assert!(editor.grid_visible);
        }
    }
}

fn frame(editor: &mut Editor, context: &egui::Context, events: Vec<egui::Event>) {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(false);
    viewport.occluded = Some(false);
    eframe::App::raw_input_hook(editor, context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(input, |ui| {
        eframe::App::logic(editor, ui.ctx(), &mut native_frame);
        eframe::App::ui(editor, ui, &mut native_frame);
    });
    output.textures_delta.clear();
}

fn hidden_frame(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::LogicOutput {
    let mut input = egui::RawInput {
        events,
        focused: false,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(true);
    viewport.occluded = Some(false);
    eframe::App::raw_input_hook(editor, context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    context.run_logic(&input, |ctx| {
        eframe::App::logic(editor, ctx, &mut native_frame)
    })
}

#[test]
fn actual_hidden_app_logic_inspects_and_edits_the_same_editor() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let mut created = ingress.queue(create_request(&editor)).unwrap();
    let mut inspected = ingress
        .queue(RemoteRequest {
            expected_scene_token: None,
            operation: Operation::Inspect,
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        created.try_recv().unwrap().unwrap(),
        OperationReply::Created { .. }
    ));
    let OperationReply::Scene { inspection } = inspected.try_recv().unwrap().unwrap() else {
        panic!("expected inspection")
    };
    assert_eq!(inspection.nodes.len(), 1);
    assert_eq!(inspection.nodes[0].name, editor.name);
    assert!(editor.history.can_undo());
    assert!(context.has_requested_repaint());

    let mut undone = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation: Operation::Undo,
        })
        .unwrap();
    let mut inspected = ingress
        .queue(RemoteRequest {
            expected_scene_token: None,
            operation: Operation::Inspect,
        })
        .unwrap();
    let mut input = egui::RawInput::default();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(false);
    viewport.occluded = Some(true);
    eframe::App::raw_input_hook(&mut editor, &context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let _ = context.run_logic(&input, |ctx| {
        eframe::App::logic(&mut editor, ctx, &mut native_frame)
    });
    assert!(undone.try_recv().unwrap().is_ok());
    let OperationReply::Scene { inspection } = inspected.try_recv().unwrap().unwrap() else {
        panic!("expected inspection")
    };
    assert!(inspection.nodes.is_empty());
    assert!(editor.history.can_redo());
}

#[test]
fn hidden_pending_input_preserves_native_baseline_before_visible_consumption() {
    let context = egui::Context::default();
    let baseline_context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor::default();
    let mut baseline = Editor::default();
    editor.create(Primitive::Cube);
    baseline.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let baseline_before = baseline.history.scene().clone();
    editor.mcp = Some(bridge);
    for _ in 0..2 {
        frame(&mut editor, &context, vec![]);
        frame(&mut baseline, &baseline_context, vec![]);
    }
    let position = context.read_response(name_edit_id()).unwrap().rect.center();
    assert_eq!(context.memory(|memory| memory.focused()), None);
    let events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Text("human".into()),
    ];
    let mut mutation = ingress.queue(create_request(&editor)).unwrap();
    hidden_frame(&mut editor, &context, events.clone());
    hidden_frame(&mut baseline, &baseline_context, events.clone());
    assert!(matches!(
        mutation.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
    let mut inspection = ingress
        .queue(RemoteRequest {
            expected_scene_token: None,
            operation: Operation::Inspect,
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    let OperationReply::Scene { inspection } = inspection.try_recv().unwrap().unwrap() else {
        panic!("expected inspection")
    };
    assert!(inspection.native_input_pending);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(baseline.history.scene(), &baseline_before);
    assert_eq!(editor.camera, baseline.camera);
    assert_eq!(editor.name, baseline.name);
    assert_eq!(editor.channels, baseline.channels);
    assert_eq!(
        context.memory(|memory| memory.focused()),
        baseline_context.memory(|memory| memory.focused())
    );
    let mut mutation = ingress.queue(create_request(&editor)).unwrap();
    frame(&mut editor, &context, events.clone());
    frame(&mut baseline, &baseline_context, events);
    assert!(matches!(
        mutation.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.native_input_pending);
    frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("human".into())],
    );
    frame(
        &mut baseline,
        &baseline_context,
        vec![egui::Event::Text("human".into())],
    );
    assert!(editor.name.contains("human"));
    assert_eq!(editor.name, baseline.name);
    assert_eq!(editor.channels, baseline.channels);
    assert_eq!(editor.selected, baseline.selected);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(baseline.history.scene(), &baseline_before);
    assert_eq!(editor.camera, baseline.camera);
    assert_eq!(editor.history.can_undo(), baseline.history.can_undo());
    assert_eq!(editor.history.can_redo(), baseline.history.can_redo());
}

#[test]
fn visible_app_logic_does_not_dispatch_before_ui_input() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    let mut native_frame = eframe::Frame::_new_kittest();
    let mut input = egui::RawInput::default();
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(false);
    viewport.occluded = Some(false);
    eframe::App::raw_input_hook(&mut editor, &context, &mut input);
    let mut output = context.run_ui(input, |ui| {
        eframe::App::logic(&mut editor, ui.ctx(), &mut native_frame);
        assert!(reply.try_recv().is_err());
        assert!(editor.history.scene().nodes().is_empty());
        eframe::App::ui(&mut editor, ui, &mut native_frame);
    });
    output.textures_delta.clear();
    assert!(reply.try_recv().unwrap().is_ok());
}

#[test]
fn hidden_callbacks_keep_draft_focus_modal_and_cancellation_guards() {
    for condition in 0..4 {
        let context = egui::Context::default();
        let (bridge, ingress) = mcp::test_bridge(context.clone());
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        frame(&mut editor, &context, vec![]);
        match condition {
            0 => editor.name = "valid_draft".into(),
            1 => editor.channels[0] = "nan".into(),
            2 => context.memory_mut(|memory| memory.request_focus(name_edit_id())),
            _ => editor.request_close(),
        }
        let before = editor.history.scene().clone();
        let name = editor.name.clone();
        let channels = editor.channels.clone();
        editor.mcp = Some(bridge);
        let mut reply = ingress.queue(create_request(&editor)).unwrap();
        hidden_frame(&mut editor, &context, vec![]);
        assert!(matches!(
            reply.try_recv().unwrap(),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.name, name);
        assert_eq!(editor.channels, channels);
        assert!(editor.history.can_undo());
    }
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    drop(ingress.queue(create_request(&editor)).unwrap());
    hidden_frame(&mut editor, &context, vec![]);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn harmless_hidden_events_allow_requests_but_held_motion_blocks_them() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    hidden_frame(
        &mut editor,
        &context,
        vec![
            egui::Event::WindowFocused(false),
            egui::Event::PointerGone,
            egui::Event::PointerMoved(egui::pos2(650.0, 400.0)),
            egui::Event::MouseMoved(egui::vec2(1.0, 0.0)),
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Copy,
        ],
    );
    assert!(reply.try_recv().unwrap().is_ok());
    frame(
        &mut editor,
        &context,
        vec![
            egui::Event::PointerMoved(egui::pos2(650.0, 400.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(650.0, 400.0),
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert!(context.input(|input| input.pointer.any_down()));
    let before = editor.history.scene().clone();
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    hidden_frame(
        &mut editor,
        &context,
        vec![egui::Event::PointerMoved(egui::pos2(660.0, 400.0))],
    );
    assert!(matches!(
        reply.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn hidden_eof_defers_close_until_pending_input_is_visibly_processed() {
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    ingress.close();
    hidden_frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("pending".into())],
    );
    assert!(editor.native_input_pending);
    assert!(!editor.files.is_closing());
    assert!(editor.mcp.is_some());
    hidden_frame(&mut editor, &context, vec![]);
    assert!(!editor.files.is_closing());
    frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("pending".into())],
    );
    assert!(!editor.native_input_pending);
    assert!(editor.files.is_closing());
    assert!(editor.mcp.is_some());
    let output = hidden_frame(&mut editor, &context, vec![]);
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
    );
    for invalid_draft in [false, true] {
        let context = egui::Context::default();
        let (bridge, ingress) = mcp::test_bridge(context.clone());
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        if invalid_draft {
            editor.channels[0] = "nan".into();
        }
        let before = editor.history.scene().clone();
        editor.mcp = Some(bridge);
        ingress.close();
        hidden_frame(&mut editor, &context, vec![]);
        assert!(!editor.files.is_closing());
        assert_eq!(editor.files.is_modal(), !invalid_draft);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.channels[0], if invalid_draft { "nan" } else { "0" });
    }
}

#[test]
fn hidden_native_close_without_a_bridge_rejects_unprocessed_input() {
    let context = egui::Context::default();
    let mut editor = Editor::default();
    let mut input = egui::RawInput {
        events: vec![egui::Event::Text("pending".into())],
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(true);
    viewport.events.push(egui::ViewportEvent::Close);
    eframe::App::raw_input_hook(&mut editor, &context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let output = context.run_logic(&input, |ctx| {
        eframe::App::logic(&mut editor, ctx, &mut native_frame)
    });
    assert!(!editor.files.is_closing());
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT]
            .contains(&egui::ViewportCommand::CancelClose)
    );
    frame(
        &mut editor,
        &context,
        vec![egui::Event::Text("pending".into())],
    );
    input.events.clear();
    eframe::App::raw_input_hook(&mut editor, &context, &mut input);
    let output = context.run_logic(&input, |ctx| {
        eframe::App::logic(&mut editor, ctx, &mut native_frame)
    });
    assert!(editor.files.is_closing());
    assert!(
        output.viewport_commands[&egui::ViewportId::ROOT].contains(&egui::ViewportCommand::Close)
    );
}

fn create_request(editor: &Editor) -> RemoteRequest {
    RemoteRequest {
        expected_scene_token: Some(editor.scene_token),
        operation: Operation::Create {
            primitive: Primitive::Cube,
            name: None,
        },
    }
}

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
                operation: Operation::Inspect,
            },
            &context
        ),
        Err(OperationError::TooLarge)
    ));
}

#[test]
fn same_frame_click_and_type_blocks_queued_mutation_without_prior_focus() {
    let context = egui::Context::default();
    let baseline_context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor::default();
    let mut baseline = Editor::default();
    editor.create(Primitive::Cube);
    baseline.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    frame(&mut editor, &context, vec![]);
    frame(&mut baseline, &baseline_context, vec![]);
    frame(&mut baseline, &baseline_context, vec![]);
    let before = editor.history.scene().clone();
    let baseline_before = baseline.history.scene().clone();
    let name_rect = context.read_response(name_edit_id()).unwrap().rect;
    assert_eq!(
        name_rect,
        baseline_context.read_response(name_edit_id()).unwrap().rect
    );
    assert_eq!(context.memory(|memory| memory.focused()), None);
    assert_eq!(baseline_context.memory(|memory| memory.focused()), None);
    editor.mcp = Some(bridge);
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    let position = name_rect.center();
    let events = vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::Text("human".into()),
    ];
    frame(&mut baseline, &baseline_context, events.clone());
    frame(&mut editor, &context, events);
    assert!(matches!(
        reply.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
    assert!(!baseline.name.contains("human"));
    assert_eq!(editor.name, baseline.name);
    assert_eq!(editor.channels, baseline.channels);
    assert_eq!(editor.selected, baseline.selected);
    assert_eq!(
        context.memory(|memory| memory.focused()),
        baseline_context.memory(|memory| memory.focused())
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(baseline.history.scene(), &baseline_before);
    assert_eq!(editor.camera, baseline.camera);
    assert_eq!(editor.history.can_undo(), baseline.history.can_undo());
    assert_eq!(editor.history.can_redo(), baseline.history.can_redo());
    assert_eq!(editor.history.scene(), &before);
    let mut reply = ingress.queue(create_request(&editor)).unwrap();
    let events = vec![egui::Event::Text("human".into())];
    frame(&mut baseline, &baseline_context, events.clone());
    frame(&mut editor, &context, events);
    assert!(matches!(
        reply.try_recv().unwrap(),
        Err(OperationError::BusyEditing)
    ));
    assert!(baseline.name.contains("human"));
    assert_eq!(editor.name, baseline.name);
    assert_eq!(editor.channels, baseline.channels);
    assert_eq!(editor.selected, baseline.selected);
    assert_eq!(
        context.memory(|memory| memory.focused()),
        baseline_context.memory(|memory| memory.focused())
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(baseline.history.scene(), &baseline_before);
    assert_eq!(editor.camera, baseline.camera);
    assert!(editor.has_draft());
    assert_eq!(editor.history.undo(), baseline.history.undo());
    assert_eq!(editor.history.scene(), baseline.history.scene());
    assert!(editor.history.scene().nodes().is_empty());
    assert!(!editor.history.can_undo());
    assert!(!baseline.history.can_undo());
    assert_eq!(editor.history.redo(), baseline.history.redo());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(baseline.history.scene(), &baseline_before);
    assert_eq!(editor.camera, baseline.camera);
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
            "app::mcp_editor_tests::shutdown_child_with_open_stdin",
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

#[cfg(feature = "mcp-test-client")]
mod official_client {
    use super::*;
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, CallToolResponse, CallToolResult},
        service::{Peer, RoleClient},
    };
    use serde_json::{Value, json};
    use std::time::Duration;

    mod binary_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_binary.rs"
        ));
    }
    mod appearance_fixture {
        use super::binary_fixture as wire;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_appearance.rs"
        ));
    }

    #[tokio::test]
    async fn material_sdk_edit_inspect_history_and_repeated_save_share_the_native_word() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("material-original.mb");
        let original = appearance_fixture::appearance_specimen_with_users(2);
        std::fs::write(&source, &original).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let before_open = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":before_open,"path":source}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let token = editor.scene_token.to_string();
        let shape = editor.history.scene().shapes()[0].id();
        let initial = editor.history.scene().clone();
        let selected = editor.selected;
        let camera = editor.camera.clone();
        let edited = call(
            &mut editor,
            &context,
            client.peer(),
            "set_material_diffuse",
            json!({"scene_token":token,"shape_id":shape.to_decimal(),"value":1.2345678901234567}),
        )
        .await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let first = editor.history.scene().clone();
        assert_eq!(editor.selected, selected);
        assert_eq!(editor.camera, camera);
        let inspection = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspection["inspection"]["appearance_incomplete"], true);
        assert_eq!(inspection["inspection"]["dirty"], true);
        for material in inspection["inspection"]["shapes"].as_array().unwrap() {
            assert_eq!(material["material"]["diffuse"], 1.2345678901234567);
            assert_eq!(material["material"]["shared_users"], 2);
            assert_eq!(material["material"]["rendering_available"], false);
        }
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &initial);
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &first);
        #[cfg(unix)]
        {
            let destination = directory.path().join("material-working.mb");
            let saved = call(
                &mut editor,
                &context,
                client.peer(),
                "save_scene_as",
                json!({"scene_token":token,"path":destination}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(saved["outcome"], "saved");
            assert!(!editor.is_dirty());
            let opened = call(
                &mut editor,
                &context,
                client.peer(),
                "open_scene",
                json!({"scene_token":token,"path":destination}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(opened["outcome"], "document_applied");
            let previous_token = token;
            let token = editor.scene_token.to_string();
            assert_ne!(token, previous_token);
            let inspected = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(inspected["inspection"]["scene_token"], token);
            assert_eq!(inspected["inspection"]["dirty"], false);
            assert_eq!(inspected["inspection"]["save_available"], true);
            assert_eq!(
                inspected["inspection"]["shapes"].as_array().unwrap().len(),
                2
            );
            for entry in inspected["inspection"]["shapes"].as_array().unwrap() {
                let material = &entry["material"];
                assert_eq!(
                    material["diffuse"].as_f64().unwrap().to_bits(),
                    1.2345678901234567f64.to_bits()
                );
                assert_eq!(material["editing_available"], true);
                assert_eq!(material["rendering_available"], false);
                assert_eq!(material["shared_users"], 2);
            }
            let edited = call(
                &mut editor,
                &context,
                client.peer(),
                "set_material_diffuse",
                json!({"scene_token":token,"shape_id":shape.to_decimal(),"value":-0.0}),
            )
            .await;
            assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
            let saved = call(
                &mut editor,
                &context,
                client.peer(),
                "save_scene",
                json!({"scene_token":token}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(saved["outcome"], "saved");
            let installed = std::fs::read(&destination).unwrap();
            let opened = call(
                &mut editor,
                &context,
                client.peer(),
                "open_scene",
                json!({"scene_token":token,"path":destination}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(opened["outcome"], "document_applied");
            let previous_token = token;
            let token = editor.scene_token.to_string();
            assert_ne!(token, previous_token);
            let inspected = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(inspected["inspection"]["scene_token"], token);
            assert_eq!(
                inspected["inspection"]["shapes"].as_array().unwrap().len(),
                2
            );
            for entry in inspected["inspection"]["shapes"].as_array().unwrap() {
                let material = &entry["material"];
                assert_eq!(
                    material["diffuse"].as_f64().unwrap().to_bits(),
                    (-0.0f64).to_bits()
                );
                assert_eq!(material["editing_available"], true);
                assert_eq!(material["rendering_available"], false);
            }
            let repeated = call(
                &mut editor,
                &context,
                client.peer(),
                "save_scene",
                json!({"scene_token":token}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(repeated["outcome"], "saved");
            assert_eq!(repeated["dirty"], false);
            assert_eq!(std::fs::read(&destination).unwrap(), installed);
        }
        let baseline = editor.history.scene().clone();
        let refused = call(&mut editor, &context, client.peer(), "set_material_diffuse",
            json!({"scene_token":Uuid::new_v4().to_string(),"shape_id":shape.to_decimal(),"value":0.5})).await;
        assert!(refused.is_error.unwrap_or(false));
        assert_eq!(editor.history.scene(), &baseline);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_face_mode_target_inspection_and_clear_share_authoritative_editor_state() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let mut ids = Vec::new();
        for primitive in ["cube", "plane"] {
            let created = call(
                &mut editor,
                &context,
                client.peer(),
                "create_primitive",
                json!({"scene_token":token,"primitive":primitive}),
            )
            .await;
            ids.push(
                created.structured_content.unwrap()["node_id"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        call(
            &mut editor,
            &context,
            client.peer(),
            "select_node",
            json!({"scene_token":token,"node_id":ids[0]}),
        )
        .await;
        call(
            &mut editor,
            &context,
            client.peer(),
            "rename_node",
            json!({"scene_token":token,"node_id":ids[0],"name":"redo_payload"}),
        )
        .await;
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspected["inspection"]["selection_mode"], "object");
        assert!(inspected["inspection"]["remembered_face"].is_null());
        assert_eq!(
            inspected["inspection"]["face_selection_policy"],
            "visible_centers_only"
        );
        let plane = inspected["inspection"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == ids[1])
            .unwrap();
        let mut path = plane["instance_paths"][0].as_array().unwrap().clone();
        path.push(plane["shape_ids"][0].clone());
        let target = json!({"node_id":ids[1],"shape_id":plane["shape_ids"][0],"instance_path":path,"face_index":0});
        let scene = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let dirty = editor.is_dirty();
        assert!(editor.history.can_redo());
        for (tool, args) in [
            (
                "set_selection_mode",
                json!({"scene_token":token,"mode":"face"}),
            ),
            ("select_face", json!({"scene_token":token,"target":target})),
        ] {
            let result = call(&mut editor, &context, client.peer(), tool, args).await;
            assert_ne!(result.is_error, Some(true));
        }
        assert_eq!(editor.selected.unwrap().to_decimal(), ids[1]);
        assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 0);
        let selection = (
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
        );
        for mode in ["face", "object", "face"] {
            call(
                &mut editor,
                &context,
                client.peer(),
                "set_selection_mode",
                json!({"scene_token":token,"mode":mode}),
            )
            .await;
            let inspected = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(inspected["inspection"]["selection_mode"], mode);
            assert_eq!(inspected["inspection"]["remembered_face"], target);
            assert_eq!(inspected["inspection"]["selected_node"], ids[1]);
            assert_eq!(
                inspected["inspection"]["selected_instance_path"],
                plane["instance_paths"][0]
            );
        }
        for field in ["node_id", "shape_id", "instance_path", "face_index"] {
            let mut invalid = target.clone();
            invalid[field] = match field {
                "node_id" => json!(ids[0]),
                "shape_id" => json!("18446744073709551615"),
                "instance_path" => json!(["18446744073709551615", target["shape_id"]]),
                _ => json!(usize::MAX),
            };
            let result = call(
                &mut editor,
                &context,
                client.peer(),
                "select_face",
                json!({"scene_token":token,"target":invalid}),
            )
            .await;
            assert_eq!(result.is_error, Some(true), "{field}");
            assert_eq!(
                (
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone()
                ),
                selection
            );
        }
        for (tool, args) in [
            (
                "set_selection_mode",
                json!({"scene_token":Uuid::nil().to_string(),"mode":"object"}),
            ),
            (
                "select_face",
                json!({"scene_token":Uuid::nil().to_string(),"target":null}),
            ),
        ] {
            let result = call(&mut editor, &context, client.peer(), tool, args).await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "stale_scene"
            );
        }
        let cleared = call(
            &mut editor,
            &context,
            client.peer(),
            "select_face",
            json!({"scene_token":token,"target":null}),
        )
        .await;
        assert_ne!(cleared.is_error, Some(true));
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.selected, selection.0);
        assert_eq!(editor.selected_path, selection.1);
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_selection_mode",
            json!({"scene_token":token,"mode":"object"}),
        )
        .await;
        let wrong_mode = call(
            &mut editor,
            &context,
            client.peer(),
            "select_face",
            json!({"scene_token":token,"target":target}),
        )
        .await;
        assert_eq!(wrong_mode.is_error, Some(true));
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.is_dirty(), dirty);
        assert!(editor.history.can_redo());
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn checked_appearance_open_inspection_and_document_replacement_share_live_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic-appearance.mb");
        std::fs::write(&path, appearance_fixture::appearance_specimen()).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let expected = editor.history.scene().appearance_readiness();
        assert_eq!(expected.checked_source_bindings, 1);
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(
            inspected["inspection"]["source_appearance"],
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(
            inspected["inspection"]["shapes"][0]["source_appearance"],
            serde_json::to_value(expected).unwrap()
        );
        assert_eq!(
            editor.appearance_scope().unwrap(),
            ("Scene totals", expected)
        );
        let token = editor.scene_token.to_string();
        let id = inspected["inspection"]["nodes"][0]["id"].clone();
        let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id,"translation":[4.0,2.0,3.0],"rotation_degrees":[0.0,0.0,0.0],"scale":[1.0,1.0,1.0]})).await;
        assert_ne!(edited.is_error, Some(true));
        let edited_scene = editor.history.scene().clone();
        assert_eq!(edited_scene.appearance_readiness(), expected);
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(pending["outcome"], "pending_confirmation");
        let cancelled = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"cancel"})).await.structured_content.unwrap();
        assert_eq!(cancelled["outcome"], "cancelled");
        assert_eq!(editor.history.scene(), &edited_scene);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene().appearance_readiness(), expected);
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &edited_scene);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        let before = editor.history.scene().clone();
        let token = editor.scene_token.to_string();
        std::fs::write(&path, b"malformed").unwrap();
        let failed = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        assert_eq!(failed.is_error, Some(true));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token.to_string(), token);
        let fresh = call(
            &mut editor,
            &context,
            client.peer(),
            "new_scene",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(
            fresh.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        assert_eq!(editor.history.scene().appearance_readiness().shapes, 0);
        assert_eq!(
            editor.history.scene().appearance_readiness().state,
            azteccraft::native_scene::AppearanceState::Unbound
        );
        client.cancel().await.unwrap();
        drop(editor);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_sdk_save_as_active_save_and_undo_redo_share_live_state() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("original.mb");
        let destination = directory.path().join("new.mb");
        let original = binary_fixture::specimen();
        std::fs::write(&source, &original).unwrap();
        let token = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token": token, "path": source}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let token = editor.scene_token.to_string();
        let node = &editor.history.scene().nodes()[0];
        let id = node.id().to_decimal();
        let mut transform = editor.transform_input(node.transform()).unwrap();
        transform.translation[0] += 1.0;
        let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token": token, "path": destination}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        assert_eq!(saved["dirty"], false);
        assert_eq!(saved["pending_action_status"], "none");
        let first = editor.history.scene().clone();
        transform.translation[0] += 0.5;
        let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let second = editor.history.scene().clone();
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene",
            json!({"scene_token": token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        assert_eq!(saved["dirty"], false);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &first);
        assert!(editor.is_dirty());
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &second);
        assert!(!editor.is_dirty());
        let disk = std::fs::read(&destination).unwrap();
        call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene",
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(std::fs::read(&destination).unwrap(), disk);
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        let inspection = &inspected["inspection"];
        assert_eq!(inspection["document_origin"], "maya_binary");
        assert_eq!(inspection["has_file"], true);
        assert_eq!(inspection["save_available"], true);
        assert_eq!(inspection["save_as_available"], true);
        assert_eq!(inspection["save_reconciliation_required"], false);
        assert_eq!(inspection["scene_token"], token);
        let conflict = call(
            &mut editor,
            &context,
            client.peer(),
            "save_scene_as",
            json!({"scene_token": token, "path": source}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(conflict["error"]["code"], "destination_exists");
        assert_eq!(editor.history.scene(), &second);
        assert!(!editor.is_dirty());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_sdk_saved_source_changed_is_an_acknowledged_write_and_blocks_discard() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        let token = editor.scene_token.to_string();
        call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token": token, "path": source}),
        )
        .await;
        let token = editor.scene_token.to_string();
        let node = &editor.history.scene().nodes()[0];
        let id = node.id().to_decimal();
        let mut transform = editor.transform_input(node.transform()).unwrap();
        transform.translation[0] += 1.0;
        let edited = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token": token, "node_id": id, "translation": transform.translation, "rotation_degrees": transform.rotation_degrees, "scale": transform.scale})).await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let before = editor.history.scene().clone();
        let queued = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token": token, "path": source}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(queued["outcome"], "pending_confirmation");
        let pending = queued["pending_action_id"].as_str().unwrap();
        let saved = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token": token, "pending_action_id": pending, "decision": "save"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(saved["outcome"], "saved");
        assert_eq!(saved["dirty"], false);
        assert_eq!(saved["pending_action_status"], "source_changed");
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(
            azteccraft::document::load_native(&source)
                .unwrap()
                .scene
                .nodes()[0]
                .transform()
                .translation,
            transform.translation
        );
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspected["inspection"]["pending_action"]["id"], pending);
        assert_eq!(
            inspected["inspection"]["pending_action"]["source_changed"],
            true
        );
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token": token, "pending_action_id": pending, "decision": "discard"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(refused["error"]["code"], "source_changed");
        assert_eq!(editor.history.scene(), &before);
        let cancelled = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token": token, "pending_action_id": pending, "decision": "cancel"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(cancelled["outcome"], "cancelled");
        assert_eq!(editor.scene_token.to_string(), token);
        assert!(!editor.is_dirty());
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn native_sdk_uncertain_receipt_survives_close_reply_flush_failure_until_drop() {
        use rmcp::model::{CallToolRequest, ClientRequest};
        use std::{cell::Cell, os::unix::fs::MetadataExt};
        let context = egui::Context::default();
        let (bridge, stream, serving, failure) =
            mcp::test_connection_with_flush_failure(context.clone());
        let server = tokio::spawn(serving);
        let mut editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let client = ()
            .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
                reader, writer,
            ))
            .await
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("document");
        let moved = directory.path().join("moved");
        std::fs::create_dir(&parent).unwrap();
        let source = parent.join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token.to_string();
        let node = &editor.history.scene().nodes()[0];
        let mut transform = editor.transform_input(node.transform()).unwrap();
        let id = node.id();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let before = editor.history.scene().clone();
        let expected = azteccraft::native_scene::binary::encode(&before).unwrap();
        let relocated = Cell::new(false);
        let guard = || {
            if !relocated.get()
                && std::fs::read_dir(&parent).unwrap().any(|entry| {
                    let entry = entry.unwrap();
                    entry.file_name().to_string_lossy().ends_with(".craft_tmp")
                        && std::fs::read(entry.path()).ok().as_deref() == Some(expected.as_slice())
                })
            {
                std::fs::rename(&parent, &moved).unwrap();
                relocated.set(true);
            }
            true
        };
        assert!(matches!(
            editor
                .apply_operation_guarded(Operation::SaveScene, &guard)
                .unwrap(),
            OperationReply::SaveUncertain { .. }
        ));
        assert!(relocated.get());
        std::fs::rename(&moved, &parent).unwrap();
        let inode = std::fs::metadata(&source).unwrap().ino();
        let inspection = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(
            inspection["inspection"]["save_reconciliation_required"],
            true
        );
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "request_close",
            json!({"scene_token": token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(pending["outcome"], "pending_confirmation");
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(CallToolRequestParams::new("resolve_file_action").with_arguments(json!({"scene_token": token, "pending_action_id": pending["pending_action_id"], "decision":"discard"}).as_object().unwrap().clone())));
        failure.arm();
        let handle = client
            .peer()
            .send_cancellable_request(request, Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        let output = hidden_frame(&mut editor, &context, vec![]);
        assert!(
            !output
                .viewport_commands
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|commands| commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close)))
        );
        assert!(editor.files.is_closing());
        assert!(editor.inspect(false).unwrap().save_reconciliation_required);
        tokio::time::timeout(Duration::from_secs(2), async {
            while !failure.observed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let _ = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
            .await
            .unwrap();
        let output = hidden_frame(&mut editor, &context, vec![]);
        assert!(
            !output
                .viewport_commands
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|commands| commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close)))
        );
        assert!(!editor.files.is_closing());
        let inspection = editor.inspect(false).unwrap();
        assert!(inspection.save_reconciliation_required && inspection.dirty);
        assert!(inspection.pending_action.is_none());
        assert!(matches!(
            editor.apply_operation(Operation::ResolveFileAction {
                pending_id: Uuid::parse_str(pending["pending_action_id"].as_str().unwrap())
                    .unwrap(),
                decision: crate::editor_operations::FileDecision::Discard,
            }),
            Err(OperationError::StalePendingAction)
        ));
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(std::fs::read(&source).unwrap(), expected);
        assert_eq!(std::fs::metadata(&source).unwrap().ino(), inode);
        let _ = tokio::time::timeout(Duration::from_secs(2), client.cancel())
            .await
            .unwrap();
        drop(editor);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), expected);
    }

    #[tokio::test]
    async fn remote_close_and_discard_close_flush_their_own_reply_before_hidden_exit() {
        use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
        for dirty in [false, true] {
            let context = egui::Context::default();
            let (mut editor, client, server) = start(&context).await;
            let token = editor.scene_token.to_string();
            let (name, arguments) = if dirty {
                call(
                    &mut editor,
                    &context,
                    client.peer(),
                    "create_primitive",
                    json!({"scene_token":token,"primitive":"cube"}),
                )
                .await;
                let pending = call(
                    &mut editor,
                    &context,
                    client.peer(),
                    "request_close",
                    json!({"scene_token":token}),
                )
                .await
                .structured_content
                .unwrap();
                assert_eq!(pending["outcome"], "pending_confirmation");
                assert_eq!(pending["kind"], "close");
                assert_eq!(editor.mcp.as_mut().unwrap().poll_close_delivery(), None);
                let cancelled = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"cancel"})).await.structured_content.unwrap();
                assert_eq!(cancelled["outcome"], "cancelled");
                assert!(!editor.files.is_closing());
                let pending = call(
                    &mut editor,
                    &context,
                    client.peer(),
                    "request_close",
                    json!({"scene_token":token}),
                )
                .await
                .structured_content
                .unwrap();
                (
                    "resolve_file_action",
                    json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"}),
                )
            } else {
                ("request_close", json!({"scene_token":token}))
            };
            let before = editor.history.scene().clone();
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(
                CallToolRequestParams::new(name)
                    .with_arguments(arguments.as_object().unwrap().clone()),
            ));
            let handle = client
                .peer()
                .send_cancellable_request(request, Default::default())
                .await
                .unwrap();
            wait_for_queue(&editor).await;
            let first = hidden_frame(&mut editor, &context, vec![]);
            assert!(
                !first
                    .viewport_commands
                    .get(&egui::ViewportId::ROOT)
                    .is_some_and(|commands| commands
                        .iter()
                        .any(|command| matches!(command, egui::ViewportCommand::Close)))
            );
            assert_eq!(
                editor.mcp.as_mut().unwrap().poll_close_delivery(),
                Some(mcp::CloseDelivery::Waiting)
            );
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
            assert!(inspection.closing);
            assert!(matches!(
                editor.remote_operation(
                    RemoteRequest {
                        expected_scene_token: Some(editor.scene_token),
                        operation: Operation::NewScene
                    },
                    &context
                ),
                Err(OperationError::BusyEditing)
            ));
            let response = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
                .await
                .unwrap()
                .unwrap();
            let ServerResult::CallToolResult(response) = response else {
                panic!("expected tool response")
            };
            assert_eq!(
                response.structured_content.unwrap()["outcome"],
                "close_requested"
            );
            let output = hidden_frame(&mut editor, &context, vec![]);
            assert!(
                output.viewport_commands[&egui::ViewportId::ROOT]
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close))
            );
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(editor.scene_token.to_string(), token);
            client.cancel().await.unwrap();
            drop(editor);
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn extrusion_moves_the_requested_cap_and_preserves_history_selection_and_other_nodes() {
        for distance in [1.25, 0.0, -0.75] {
            let context = egui::Context::default();
            let (mut editor, client, server) = start(&context).await;
            let token = editor.scene_token.to_string();
            let created = call(
                &mut editor,
                &context,
                client.peer(),
                "create_primitive",
                json!({"scene_token":token,"primitive":"cube","name":"extruded"}),
            )
            .await
            .structured_content
            .unwrap();
            let id = created["node_id"].as_str().unwrap();
            call(
                &mut editor,
                &context,
                client.peer(),
                "create_primitive",
                json!({"scene_token":token,"primitive":"plane","name":"unrelated"}),
            )
            .await;
            let inspected = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(inspected["inspection"]["units"]["linear"], "cm");
            let node = inspected["inspection"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|node| node["id"] == id)
                .unwrap();
            assert_eq!(node["shape_ids"].as_array().unwrap().len(), 1);
            assert_eq!(node["instance_paths"].as_array().unwrap().len(), 1);
            let shape = node["shape_ids"][0].as_str().unwrap();
            let mut path = node["instance_paths"][0].as_array().unwrap().clone();
            path.push(json!(shape));
            let shape_id = NativeId::from_decimal(shape).unwrap();
            let before = editor.history.scene().clone();
            let original_mesh = before.shape(shape_id).unwrap().mesh();
            let selection = (editor.selected, editor.selected_path.clone());
            let camera = editor.camera.clone();
            let title = editor.document_title();
            let args = json!({"scene_token":token,"node_id":id,"shape_id":shape,"instance_path":path,"face_index":1,"distance":distance});
            let result = call(
                &mut editor,
                &context,
                client.peer(),
                "extrude_face",
                args.clone(),
            )
            .await;
            assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
            let extruded = editor.history.scene().clone();
            let mesh = extruded.shape(shape_id).unwrap().mesh();
            assert_eq!(
                (mesh.point_count(), mesh.edges().len(), mesh.faces().len()),
                (12, 20, 10)
            );
            assert_eq!(&mesh.edges()[..12], original_mesh.edges());
            for index in 0..8 {
                assert_eq!(
                    mesh.effective_point_cm(index).unwrap(),
                    original_mesh.effective_point_cm(index).unwrap()
                );
            }
            for index in 0..4 {
                let mut expected = original_mesh.effective_point_cm(index + 4).unwrap();
                expected[2] += distance;
                assert_eq!(mesh.effective_point_cm(index + 8).unwrap(), expected);
            }
            for index in [0, 2, 3, 4, 5] {
                assert_eq!(mesh.faces()[index], original_mesh.faces()[index]);
            }
            let cap: Vec<_> = mesh.faces()[1]
                .outer()
                .iter()
                .map(|edge| mesh.edges()[edge.edge].endpoints()[usize::from(edge.reversed)])
                .collect();
            assert_eq!(cap, [8, 9, 10, 11]);
            assert!(
                mesh.faces()[6..]
                    .iter()
                    .all(|face| face.outer().len() == 4 && face.holes().is_empty())
            );
            let unrelated = selection.0.unwrap();
            assert_eq!(
                extruded.node(unrelated).unwrap(),
                before.node(unrelated).unwrap()
            );
            let unrelated_shape = before
                .shapes()
                .iter()
                .find(|shape| shape.id() != shape_id)
                .unwrap();
            assert_eq!(
                extruded.shape(unrelated_shape.id()).unwrap(),
                unrelated_shape
            );
            assert_eq!(
                (editor.selected, &editor.selected_path),
                (selection.0, &selection.1)
            );
            assert_eq!(editor.camera, camera);
            assert_eq!(editor.scene_token.to_string(), token);
            assert_eq!(editor.document_title(), title);
            call(
                &mut editor,
                &context,
                client.peer(),
                "undo",
                json!({"scene_token":token}),
            )
            .await;
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.can_redo());
            let mut obsolete_side = args;
            obsolete_side["face_index"] = json!(6);
            let refused = call(
                &mut editor,
                &context,
                client.peer(),
                "extrude_face",
                obsolete_side,
            )
            .await;
            assert_eq!(
                refused.structured_content.unwrap()["error"]["code"],
                "scene_error"
            );
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.can_redo());
            call(
                &mut editor,
                &context,
                client.peer(),
                "redo",
                json!({"scene_token":token}),
            )
            .await;
            assert_eq!(editor.history.scene(), &extruded);
            client.cancel().await.unwrap();
            drop(editor);
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn extrusion_refuses_mismatched_current_targets_planes_and_imported_structure() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let shape = editor.history.scene().shapes()[0].id();
        let transform = editor.history.scene().node(id).unwrap().transform_id();
        editor.create(Primitive::Plane);
        let other = editor.selected.unwrap();
        let other_shape = editor.history.scene().shapes()[1].id();
        let other_transform = editor.history.scene().node(other).unwrap().transform_id();
        let before = editor.history.scene().clone();
        let selection = (editor.selected, editor.selected_path.clone());
        let camera = editor.camera.clone();
        let fields = (editor.name.clone(), editor.channels.clone());
        let valid = json!({"scene_token":token,"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[transform.to_decimal(),shape.to_decimal()],"face_index":1,"distance":1});
        for delta in [
            json!({"node_id":other.to_decimal()}),
            json!({"node_id":"999999"}),
            json!({"shape_id":other_shape.to_decimal()}),
            json!({"shape_id":"999999"}),
            json!({"instance_path":[other_transform.to_decimal(),shape.to_decimal()]}),
            json!({"instance_path":[transform.to_decimal(),transform.to_decimal(),shape.to_decimal()]}),
            json!({"instance_path":[transform.to_decimal(),other_shape.to_decimal()]}),
            json!({"face_index":usize::MAX}),
            json!({"distance":f64::MAX}),
            json!({"node_id":other.to_decimal(),"shape_id":other_shape.to_decimal(),"instance_path":[other_transform.to_decimal(),other_shape.to_decimal()],"face_index":0}),
        ] {
            let mut args = valid.clone();
            args.as_object_mut()
                .unwrap()
                .extend(delta.as_object().unwrap().clone());
            let result = call(&mut editor, &context, client.peer(), "extrude_face", args).await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "scene_error"
            );
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(
                (editor.selected, &editor.selected_path),
                (selection.0, &selection.1)
            );
            assert_eq!(editor.camera, camera);
            assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
            assert!(!editor.history.can_redo());
        }
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("imported.mb");
        let source = binary_fixture::specimen();
        std::fs::write(&file, &source).unwrap();
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":file.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        let opened = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":token,"pending_action_id":pending["pending_action_id"],"decision":"discard"})).await.structured_content.unwrap();
        let token = opened["scene_token"].as_str().unwrap();
        let imported = editor.history.scene().clone();
        let id = imported.nodes()[0].id();
        let shape = imported.shapes()[0].id();
        let result = call(&mut editor, &context, client.peer(), "extrude_face", json!({"scene_token":token,"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[imported.node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":0,"distance":1})).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &imported);
        assert!(!editor.history.can_undo());
        assert!(!editor.history.can_redo());
        assert!(!editor.is_dirty());
        assert!(std::fs::read(file).unwrap() == source);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sparse_node_edit_combines_history_preserves_selection_and_rejects_invalid_intent() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube","name":"first"}),
        )
        .await;
        let id = created.structured_content.unwrap()["node_id"]
            .as_str()
            .unwrap()
            .to_owned();
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"plane","name":"selected"}),
        )
        .await;
        let before = editor.history.scene().clone();
        let selection = (editor.selected, editor.selected_path.clone());
        let camera = editor.camera.clone();
        let title = editor.document_title();
        let result = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id,"name":"combined","translation":[16777217.125,null,null],"rotation_degrees":[null,90,null],"scale":[null,null,-1]})).await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
        let edited = editor.history.scene().clone();
        let node = edited.node(NodeId::from_decimal(&id).unwrap()).unwrap();
        assert_eq!(node.name(), "combined");
        assert_eq!(node.transform().translation, [16777217.125, 0.0, 0.0]);
        assert_eq!(node.transform().rotation, [0.0, 90.0, 0.0]);
        assert_eq!(node.transform().scale, [1.0, 1.0, -1.0]);
        assert_eq!(
            (editor.selected, &editor.selected_path),
            (selection.0, &selection.1)
        );
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.document_title(), title);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        let fields = (editor.name.clone(), editor.channels.clone());
        for args in [
            json!({"scene_token":token,"node_id":id}),
            json!({"scene_token":token,"node_id":id,"name":null,"translation":[null,null,null]}),
            json!({"scene_token":token,"node_id":id,"translation":[0,null,null]}),
        ] {
            let result = call(&mut editor, &context, client.peer(), "edit_node", args).await;
            assert_eq!(result.structured_content.unwrap()["outcome"], "unchanged");
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.can_redo());
            assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        }
        for args in [
            json!({"scene_token":token,"node_id":id,"name":"","translation":[3,null,null]}),
            json!({"scene_token":token,"node_id":"999999"}),
        ] {
            let result = call(&mut editor, &context, client.peer(), "edit_node", args).await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "scene_error"
            );
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.can_redo());
        }
        for fields in [
            json!({"translation":null}),
            json!({"scale":[1,2]}),
            json!({"rotation_degrees":[null,"bad",null]}),
            json!({"extra":true}),
            json!({"node_id":1}),
        ] {
            let mut args = json!({"scene_token":token,"node_id":id});
            args.as_object_mut()
                .unwrap()
                .extend(fields.as_object().unwrap().clone());
            assert!(
                call_response(&mut editor, &context, client.peer(), "edit_node", args)
                    .await
                    .is_err()
            );
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.can_redo());
        }
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &edited);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sparse_imported_edit_preserves_raw_radians_and_only_changes_requested_words() {
        use azteccraft::native_scene::binary;
        let source = binary_fixture::scene_with_units(
            &[
                binary_fixture::group(
                    b"XFRM",
                    &[
                        binary_fixture::creation(b"object", None),
                        binary_fixture::triple(b"t", [16777217.125, -2.0, 3.0]),
                        binary_fixture::triple(b"r", [0.1, -0.0, 0.25]),
                        binary_fixture::triple(b"rp", [0.25, 0.0, 0.0]),
                        binary_fixture::triple(b"sp", [0.0, 0.5, 0.0]),
                    ]
                    .concat(),
                    4,
                ),
                binary_fixture::group(
                    b"DMSH",
                    &[
                        binary_fixture::creation(b"shape", Some(b"object")),
                        binary_fixture::chunk(
                            b"MESH",
                            &binary_fixture::mesh_payload(&[0, 1, 0x6000_0002]),
                            8,
                        ),
                    ]
                    .concat(),
                    4,
                ),
            ]
            .concat(),
            b"cm",
            b"rad",
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("radian-scene.mb");
        std::fs::write(&path, &source).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        let token = opened.structured_content.unwrap()["scene_token"]
            .as_str()
            .unwrap()
            .to_owned();
        let original = editor.history.scene().clone();
        let id = original.nodes()[0].id();
        let original_internal = original.internal_transform(id).unwrap();
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        let normalized =
            inspected["inspection"]["nodes"][0]["transform"]["rotation_degrees"].clone();
        let result = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[16777219.375,-2,3],"rotation_degrees":normalized,"scale":[1,1,1]})).await;
        assert_eq!(result.is_error, Some(false));
        assert_ne!(
            editor
                .history
                .scene()
                .internal_transform(id)
                .unwrap()
                .rotation_radians[0]
                .to_bits(),
            original_internal.rotation_radians[0].to_bits(),
            "normalized whole-transform workflow must expose precision loss"
        );
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &original);
        let fields = (editor.name.clone(), editor.channels.clone());
        let empty = call(
            &mut editor,
            &context,
            client.peer(),
            "edit_node",
            json!({"scene_token":token,"node_id":id.to_decimal()}),
        )
        .await;
        assert_eq!(empty.structured_content.unwrap()["outcome"], "unchanged");
        assert!(editor.history.can_redo());
        assert!(!editor.is_dirty());
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        let selection = (editor.selected, editor.selected_path.clone());
        let camera = editor.camera.clone();
        let title = editor.document_title();
        let edited = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[16777219.375,null,null]})).await;
        assert_eq!(edited.structured_content.unwrap()["outcome"], "changed");
        let scene = editor.history.scene();
        let internal = scene.internal_transform(id).unwrap();
        assert_eq!(
            internal.rotation_radians.map(f64::to_bits),
            original_internal.rotation_radians.map(f64::to_bits)
        );
        assert_eq!(
            internal.rotate_pivot_cm.map(f64::to_bits),
            original_internal.rotate_pivot_cm.map(f64::to_bits)
        );
        assert_eq!(
            internal.scale_pivot_cm.map(f64::to_bits),
            original_internal.scale_pivot_cm.map(f64::to_bits)
        );
        assert_eq!(scene.shapes(), original.shapes());
        assert_eq!(scene.dag_edges(), original.dag_edges());
        let encoded = binary::encode(scene).unwrap();
        let offset = source
            .windows(8)
            .position(|word| word == 16777217.125_f64.to_be_bytes())
            .unwrap();
        assert_eq!(encoded.len(), source.len());
        assert!(encoded[..offset] == source[..offset]);
        assert!(encoded[offset..offset + 8] == 16777219.375_f64.to_be_bytes());
        assert!(encoded[offset + 8..] == source[offset + 8..]);
        assert_eq!(
            (editor.selected, &editor.selected_path),
            (selection.0, &selection.1)
        );
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.document_title(), format!("{title} *"));
        assert_eq!(editor.scene_token.to_string(), token);
        let translated = editor.history.scene().clone();
        let refused = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id.to_decimal(),"name":"unsupported","translation":[7,null,null]})).await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &translated);
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &original);
        assert!(binary::encode(editor.history.scene()).unwrap().as_slice() == source.as_ref());
        assert!(!editor.is_dirty());
        call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &translated);
        let rotated = call(&mut editor, &context, client.peer(), "edit_node", json!({"scene_token":token,"node_id":id.to_decimal(),"rotation_degrees":[90,null,null]})).await;
        assert_eq!(rotated.structured_content.unwrap()["outcome"], "changed");
        let rotation = editor
            .history
            .scene()
            .internal_transform(id)
            .unwrap()
            .rotation_radians;
        assert_eq!(rotation[0].to_bits(), std::f64::consts::FRAC_PI_2.to_bits());
        assert_eq!(rotation[1].to_bits(), (-0.0_f64).to_bits());
        assert_eq!(rotation[2].to_bits(), 0.25_f64.to_bits());
        assert!(std::fs::read(path).unwrap().as_slice() == source.as_ref());
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn ncrv_sdk_open_refusal_preserves_the_complete_editor_baseline() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic.mb");
        let baseline = binary_fixture::specimen();
        std::fs::write(&path, &baseline).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let opened = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await;
        assert_eq!(
            opened.structured_content.unwrap()["outcome"],
            "document_applied"
        );
        let token = editor.scene_token.to_string();
        let id = editor.history.scene().nodes()[0].id();
        let selected = call(
            &mut editor,
            &context,
            client.peer(),
            "select_node",
            json!({"scene_token":token,"node_id":id.to_decimal()}),
        )
        .await;
        assert!(selected.structured_content.unwrap().get("error").is_none());
        assert_eq!(editor.selected, Some(id));
        assert!(!editor.selected_path.is_empty());
        let changed = call(
            &mut editor,
            &context,
            client.peer(),
            "edit_node",
            json!({"scene_token":token,"node_id":id.to_decimal(),"translation":[1.0,2.0,3.0]}),
        )
        .await;
        assert!(changed.structured_content.unwrap().get("error").is_none());
        let edited = editor.history.scene().clone();
        let undone = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert!(undone.structured_content.unwrap().get("error").is_none());
        assert!(editor.history.can_redo());
        let before = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        let original = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let selection = (editor.selected, editor.selected_path.clone());
        let title = editor.document_title();
        for children in [
            Vec::new(),
            [
                binary_fixture::creation(b"curve", Some(b"object")),
                binary_fixture::chunk(b"NRBC", &[0xff, 0, 7], 8),
            ]
            .concat(),
        ] {
            let mut source = baseline.clone();
            let offset = source.len();
            source.extend(binary_fixture::group(b"NCRV", &children, 4));
            let size = (source.len() - 16) as u64;
            source[8..16].copy_from_slice(&size.to_be_bytes());
            let refusal =
                azteccraft::native_scene::binary::decode(source.clone().into()).unwrap_err();
            assert_eq!(
                refusal.kind(),
                azteccraft::native_scene::binary::ErrorKind::Unsupported
            );
            assert_eq!(refusal.offset(), Some(offset));
            let rejected = directory.path().join("unsupported.mb");
            std::fs::write(&rejected, &source).unwrap();
            assert!(matches!(
                editor.apply_operation(Operation::OpenScene {
                    path: rejected.clone()
                }),
                Err(OperationError::File(_))
            ));
            let failed = call(
                &mut editor,
                &context,
                client.peer(),
                "open_scene",
                json!({"scene_token":token,"path":rejected.to_str().unwrap()}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(failed["error"]["code"], "file_error");
            assert_eq!(editor.history.scene(), &original);
            assert_eq!(editor.camera, camera);
            assert_eq!(
                (editor.selected, &editor.selected_path),
                (selection.0, &selection.1)
            );
            assert_eq!(editor.scene_token.to_string(), token);
            assert_eq!(editor.document_title(), title);
            assert!(!editor.is_dirty());
            assert!(!editor.files.is_modal());
            assert!(!editor.files.is_closing());
            assert!(std::fs::read(&path).unwrap() == baseline);
            assert!(std::fs::read(&rejected).unwrap() == source);
            let after = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await
            .structured_content
            .unwrap();
            assert_eq!(after, before);
        }
        let redone = call(
            &mut editor,
            &context,
            client.peer(),
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(redone.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(editor.history.scene(), &edited);
        let undone = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(undone.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(editor.history.scene(), &original);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn native_mb_open_inspects_precision_and_discards_the_exact_staged_candidate() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("private-synthetic-scene.mb");
        let source = binary_fixture::specimen();
        std::fs::write(&path, &source).unwrap();
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let original_token = editor.scene_token.to_string();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":original_token,"path":path.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(result["outcome"], "document_applied");
        let token = result["scene_token"].as_str().unwrap().to_owned();
        assert_ne!(token, original_token);
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            16777217.125
        );
        let id = editor.history.scene().nodes()[0].id();
        assert_eq!(
            editor
                .history
                .scene()
                .internal_transform(id)
                .unwrap()
                .rotation_radians[2],
            0.25
        );
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(
            inspected["inspection"]["nodes"][0]["authoring_transform"]["translation"][0].as_f64(),
            Some(16777217.125)
        );
        assert_eq!(inspected["inspection"]["shapes"][0]["points"], 3);
        assert!(
            !serde_json::to_string(&inspected)
                .unwrap()
                .contains(path.to_str().unwrap())
        );
        assert!(
            editor
                .camera
                .target()
                .distance(Vec3::new(16777217.125_f32, -2.0, 3.0))
                < 4.0
        );
        let imported_scene = editor.history.scene().clone();
        let imported_channels = imported_scene.nodes()[0].transform();
        let imported_id = inspected["inspection"]["nodes"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let rotation_degrees =
            inspected["inspection"]["nodes"][0]["transform"]["rotation_degrees"].clone();
        assert_eq!(imported_channels.scale, [1.0; 3]);
        assert!(!editor.is_dirty());
        assert!(!editor.history.can_undo());
        let changed = call(
            &mut editor,
            &context,
            client.peer(),
            "set_transform",
            json!({
                "scene_token":token,
                "node_id":imported_id,
                "translation":[16777221.125,-2.0,3.0],
                "rotation_degrees":rotation_degrees,
                "scale":[1.0,1.0,1.0]
            }),
        )
        .await;
        assert_ne!(changed.is_error, Some(true));
        assert_eq!(changed.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation,
            [16777221.125, -2.0, 3.0]
        );
        assert_eq!(
            editor
                .history
                .scene()
                .internal_transform(id)
                .unwrap()
                .rotation_radians[2]
                .to_bits(),
            0.25_f64.to_bits()
        );
        assert_eq!(
            editor.history.scene().nodes()[0].transform().scale,
            [1.0; 3]
        );
        assert_eq!(editor.history.scene().shapes(), imported_scene.shapes());
        assert_eq!(editor.scene_token.to_string(), token);
        assert!(editor.is_dirty());
        assert!(editor.history.can_undo());
        let edited = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(
            edited["inspection"]["nodes"][0]["authoring_transform"]["translation"][0].as_f64(),
            Some(16777221.125)
        );
        assert_eq!(edited["inspection"]["dirty"], true);
        assert_eq!(edited["inspection"]["can_undo"], true);
        let undone = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_ne!(undone.is_error, Some(true));
        assert_eq!(undone.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(editor.history.scene(), &imported_scene);
        assert_eq!(
            editor.history.scene().nodes()[0].transform(),
            imported_channels
        );
        assert_eq!(editor.scene_token.to_string(), token);
        assert!(!editor.is_dirty());
        assert!(!editor.history.can_undo());
        assert!(editor.history.can_redo());
        let restored = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(
            restored["inspection"]["nodes"],
            inspected["inspection"]["nodes"]
        );
        assert_eq!(restored["inspection"]["dirty"], false);
        assert_eq!(restored["inspection"]["can_undo"], false);
        assert_eq!(restored["inspection"]["can_redo"], true);
        call(
            &mut editor,
            &context,
            client.peer(),
            "new_scene",
            json!({"scene_token":token}),
        )
        .await;
        let token = editor.scene_token.to_string();
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube"}),
        )
        .await;
        let before = editor.history.scene().clone();
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(pending["outcome"], "pending_confirmation");
        assert_eq!(pending["scene_token"], token);
        assert_eq!(pending["kind"], "open");
        let pending_id = pending["pending_action_id"].as_str().unwrap();
        assert!(
            !serde_json::to_string(&pending)
                .unwrap()
                .contains(path.to_str().unwrap())
        );
        std::fs::write(&path, b"changed after admission").unwrap();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":token,"pending_action_id":pending_id,"decision":"discard"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(result["outcome"], "document_applied");
        assert_ne!(editor.history.scene(), &before);
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            16777217.125
        );
        assert_eq!(
            editor
                .history
                .scene()
                .internal_transform(editor.history.scene().nodes()[0].id())
                .unwrap()
                .rotation_radians[2],
            0.25
        );
        assert!(!editor.history.can_undo());
        assert!(!editor.is_dirty());
        let token = editor.scene_token.to_string();
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let failed = call(
            &mut editor,
            &context,
            client.peer(),
            "open_scene",
            json!({"scene_token":token,"path":path.to_str().unwrap()}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(failed["error"]["code"], "file_error");
        assert!(
            !serde_json::to_string(&failed)
                .unwrap()
                .contains(path.to_str().unwrap())
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.camera, camera);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn native_file_confirmation_uses_scene_and_action_identity_without_replay() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let first_token = editor.scene_token.to_string();
        let applied = call(
            &mut editor,
            &context,
            client.peer(),
            "new_scene",
            json!({"scene_token":first_token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(applied["outcome"], "document_applied");
        let token = applied["scene_token"].as_str().unwrap().to_owned();
        assert_ne!(token, first_token);
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube"}),
        )
        .await;
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let selected = editor.selected;
        let selected_path = editor.selected_path.clone();
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "new_scene",
            json!({"scene_token":token}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(pending["outcome"], "pending_confirmation");
        assert_eq!(pending["scene_token"], token);
        assert_eq!(pending["kind"], "new");
        let old_pending = pending["pending_action_id"].as_str().unwrap().to_owned();
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspected["inspection"]["pending_action"]["id"], old_pending);
        assert_eq!(inspected["inspection"]["pending_action"]["kind"], "new");
        assert_eq!(inspected["inspection"]["save_available"], false);
        for (tool, fields) in [
            ("create_primitive", json!({"primitive":"plane"})),
            ("select_node", json!({"node_id":null})),
            (
                "rename_node",
                json!({"node_id":selected.unwrap().to_decimal(),"name":"blocked"}),
            ),
            (
                "set_transform",
                json!({"node_id":selected.unwrap().to_decimal(),"translation":[0,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]}),
            ),
            (
                "edit_node",
                json!({"node_id":selected.unwrap().to_decimal()}),
            ),
            (
                "extrude_face",
                json!({"node_id":selected.unwrap().to_decimal(),"shape_id":editor.history.scene().shapes()[0].id().to_decimal(),"instance_path":[editor.history.scene().node(selected.unwrap()).unwrap().transform_id().to_decimal(),editor.history.scene().shapes()[0].id().to_decimal()],"face_index":1,"distance":1}),
            ),
            (
                "delete_node",
                json!({"node_id":selected.unwrap().to_decimal()}),
            ),
            ("undo", json!({})),
            ("redo", json!({})),
            ("frame_selected", json!({})),
            ("orbit_camera", json!({"radians":[0.2,0]})),
            ("pan_camera", json!({"world_units":[1,0]})),
            ("dolly_camera", json!({"factor":0.5})),
            ("new_scene", json!({})),
            ("request_close", json!({})),
            ("open_scene", json!({"path":"unopened.mb"})),
        ] {
            let mut args = fields;
            args["scene_token"] = json!(token);
            let result = call(&mut editor, &context, client.peer(), tool, args).await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "busy_editing",
                "{tool}"
            );
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(editor.camera, camera);
            assert_eq!(editor.selected, selected);
            assert_eq!(editor.selected_path, selected_path);
        }
        let stale = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":first_token,"pending_action_id":old_pending,"decision":"discard"}),
        )
        .await;
        assert_eq!(
            stale.structured_content.unwrap()["error"]["code"],
            "stale_scene"
        );
        let cancelled = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":token,"pending_action_id":old_pending,"decision":"cancel"}),
        )
        .await;
        assert_eq!(
            cancelled.structured_content.unwrap()["outcome"],
            "cancelled"
        );
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!(editor.history.scene(), &before);
        let pending = call(
            &mut editor,
            &context,
            client.peer(),
            "new_scene",
            json!({"scene_token":token}),
        )
        .await
        .structured_content
        .unwrap();
        let current_pending = pending["pending_action_id"].as_str().unwrap();
        assert_ne!(current_pending, old_pending);
        let stale = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":token,"pending_action_id":old_pending,"decision":"discard"}),
        )
        .await;
        assert_eq!(
            stale.structured_content.unwrap()["error"]["code"],
            "stale_pending_action"
        );
        assert_eq!(editor.history.scene(), &before);
        let applied = call(
            &mut editor,
            &context,
            client.peer(),
            "resolve_file_action",
            json!({"scene_token":token,"pending_action_id":current_pending,"decision":"discard"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(applied["outcome"], "document_applied");
        let new_token = applied["scene_token"].as_str().unwrap();
        assert_ne!(new_token, token);
        assert!(editor.history.scene().nodes().is_empty());
        assert!(!editor.history.can_undo());
        let replay = call(&mut editor, &context, client.peer(), "resolve_file_action", json!({"scene_token":new_token,"pending_action_id":current_pending,"decision":"discard"})).await;
        assert_eq!(
            replay.structured_content.unwrap()["error"]["code"],
            "stale_pending_action"
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn native_sdk_transforms_preserve_double_precision_and_inspect_metadata_only() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube"}),
        )
        .await
        .structured_content
        .unwrap();
        let id = created["node_id"].as_str().unwrap();
        let result = call(&mut editor, &context, client.peer(), "set_transform", json!({"scene_token":token,"node_id":id,"translation":[16777217.125,0,0],"rotation_degrees":[90,0,0],"scale":[1,1,1]})).await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            16777217.125
        );
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        let inspection = &inspected["inspection"];
        let node = &inspection["nodes"][0];
        assert_eq!(
            node["transform"]["translation"][0].as_f64(),
            Some(16777217.125)
        );
        assert_eq!(node["transform"]["rotation_degrees"][0], 90.0);
        assert_eq!(node["authoring_transform"]["rotation"][0], 90.0);
        assert_eq!(inspection["units"]["angular"], "deg");
        assert_eq!(inspection["units"]["linear"], "cm");
        assert!(node.get("mesh").is_none());
        assert!(node.get("source").is_none());
        assert_eq!(inspection["shapes"][0]["points"], 8);
        assert_eq!(inspection["shapes"][0]["faces"], 6);
        assert_eq!(node["shape_ids"][0], inspection["shapes"][0]["id"]);
        assert_eq!(inspection["viewport_backend"], "uninitialized");
        assert_eq!(inspection["viewport_error"], "uninitialized");
        assert_eq!(inspection["depth_complete"], false);
        assert_eq!(inspection["clipping_complete"], false);
        assert!(inspection["shapes"][0].get("vertices").is_none());
        let paths = node["instance_paths"].as_array().unwrap();
        assert_eq!(paths.len(), 1);
        let selected = call(
            &mut editor,
            &context,
            client.peer(),
            "select_node",
            json!({"scene_token":token,"node_id":id,"instance_path":paths[0]}),
        )
        .await;
        assert_eq!(selected.is_error, Some(false));
        let before = editor.history.scene().clone();
        let selected_path = editor.selected_path.clone();
        let wrong = call(
            &mut editor,
            &context,
            client.peer(),
            "select_node",
            json!({"scene_token":token,"node_id":id,"instance_path":["18446744073709551615"]}),
        )
        .await;
        assert_eq!(
            wrong.structured_content.unwrap()["error"]["code"],
            "scene_error"
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.selected_path, selected_path);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_camera_navigation_inspects_real_view_without_authoring_changes() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.create(Primitive::Cube);
        editor.history_step(false);
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let selected = editor.selected;
        let dirty = editor.is_dirty();
        let mut eye = editor.camera.eye();
        let mut target = editor.camera.target();
        let mut distance = editor.camera.distance();
        for (tool, fields) in [
            ("orbit_camera", json!({"radians":[0.2,-0.1]})),
            ("pan_camera", json!({"world_units":[1.0,-2.0]})),
            ("dolly_camera", json!({"factor":0.5})),
            ("orbit_camera", json!({"radians":[0.0,0.0]})),
            ("pan_camera", json!({"world_units":[0.0,0.0]})),
            ("dolly_camera", json!({"factor":1.0})),
        ] {
            let noop = fields["radians"] == json!([0.0, 0.0])
                || fields["world_units"] == json!([0.0, 0.0])
                || fields["factor"] == json!(1.0);
            if !noop {
                match tool {
                    "orbit_camera" => {
                        let yaw = 0.8_f32;
                        let pitch = 0.3_f32;
                        eye = target
                            + distance
                                * Vec3::new(
                                    yaw.sin() * pitch.cos(),
                                    pitch.sin(),
                                    yaw.cos() * pitch.cos(),
                                );
                    }
                    "pan_camera" => {
                        let back = (eye - target).normalize();
                        let right = Vec3::Y.cross(back).normalize();
                        let up = back.cross(right);
                        let translation = right - 2.0 * up;
                        target += translation;
                        eye += translation;
                    }
                    _ => {
                        distance *= 0.5;
                        eye = target + (eye - target) * 0.5;
                    }
                }
            }
            let mut arguments = fields;
            arguments["scene_token"] = json!(token.to_string());
            let result = call(&mut editor, &context, client.peer(), tool, arguments).await;
            assert_eq!(
                result.structured_content.unwrap()["outcome"],
                if noop { "unchanged" } else { "changed" }
            );
            assert!((editor.camera.eye() - eye).length() < 0.00001);
            assert!((editor.camera.target() - target).length() < 0.00001);
            assert_eq!(editor.camera.distance(), distance);
            let inspected = call(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({}),
            )
            .await;
            let camera = &inspected.structured_content.unwrap()["inspection"]["camera"];
            for (field, expected) in [
                ("target", editor.camera.target()),
                ("eye", editor.camera.eye()),
            ] {
                let actual: [f32; 3] = serde_json::from_value(camera[field].clone()).unwrap();
                assert_eq!(actual, expected.to_array());
            }
            assert_eq!(camera["distance"].as_f64().unwrap() as f32, distance);
            assert_eq!(editor.history.scene(), &before);
            assert!(!editor.history.can_undo());
            assert!(editor.history.can_redo());
            assert_eq!(editor.selected, selected);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.is_dirty(), dirty);
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_camera_rejects_bad_parameters_and_invalid_ranges_without_mutation() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let camera = editor.camera.clone();
        let token = editor.scene_token.to_string();
        for (tool, fields) in [
            ("orbit_camera", json!({"radians":[1e100,0]})),
            ("pan_camera", json!({"world_units":[1e100,0]})),
            ("pan_camera", json!({"world_units":[3e38,3e38]})),
            ("dolly_camera", json!({"factor":0})),
            ("dolly_camera", json!({"factor":-1})),
            ("dolly_camera", json!({"factor":1e100})),
            ("dolly_camera", json!({"factor":3e38})),
            ("dolly_camera", json!({"factor":1e-45})),
        ] {
            let mut args = fields;
            args["scene_token"] = json!(token);
            let result = call(&mut editor, &context, client.peer(), tool, args).await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                "camera_error"
            );
            assert_eq!(editor.camera, camera);
            assert!(!editor.history.can_undo());
            assert!(!editor.is_dirty());
        }
        for (tool, fields) in [
            ("orbit_camera", json!({"radians":[0]})),
            ("orbit_camera", json!({"radians":[0,0,0]})),
            ("orbit_camera", json!({"radians":["0",0]})),
            ("pan_camera", json!({"world_units":null})),
            ("pan_camera", json!({"world_units":[0,0],"extra":true})),
            ("dolly_camera", json!({"factor":"1"})),
            ("dolly_camera", json!({})),
        ] {
            let mut args = fields;
            args["scene_token"] = json!(token);
            assert!(
                call_response(&mut editor, &context, client.peer(), tool, args)
                    .await
                    .is_err()
            );
            assert_eq!(editor.camera, camera);
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a real headless GPU adapter"]
    async fn headless_gpu_sdk_camera_selection_and_grid_changes_enter_the_next_fresh_callback() {
        use crate::viewport_renderer::tests::{creation_context, recorded_submission};
        use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
        let context = egui::Context::default();
        let (mut temporary, client, server) = start(&context).await;
        let creation = creation_context(context.clone());
        let mut editor = Editor::new(&creation).unwrap();
        editor.mcp = temporary.mcp.take();
        let token = editor.scene_token.to_string();
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube"}),
        )
        .await;
        frame(&mut editor, &context, vec![]);
        let id = editor.selected.unwrap().to_decimal();
        let path = editor
            .selected_path
            .iter()
            .map(|id| id.to_decimal())
            .collect::<Vec<_>>();
        let scene = editor.history.scene().clone();
        let undo = editor.history.can_undo();
        for (tool, arguments) in [
            ("dolly_camera", json!({"scene_token":token,"factor":0.75})),
            ("select_node", json!({"scene_token":token,"node_id":null})),
            (
                "select_node",
                json!({"scene_token":token,"node_id":id,"instance_path":path}),
            ),
            ("frame_selected", json!({"scene_token":token})),
            (
                "set_grid_visibility",
                json!({"scene_token":token,"visible":false}),
            ),
        ] {
            let before = recorded_submission(editor.renderer.as_ref().unwrap()).vertices;
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(
                CallToolRequestParams::new(tool)
                    .with_arguments(arguments.as_object().unwrap().clone()),
            ));
            let handle = client
                .peer()
                .send_cancellable_request(request, Default::default())
                .await
                .unwrap();
            wait_for_queue(&editor).await;
            frame(&mut editor, &context, vec![]);
            assert_eq!(
                recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
                before,
                "remote changes cannot alter the already prepared callback"
            );
            let response = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
                .await
                .unwrap()
                .unwrap();
            let ServerResult::CallToolResult(response) = response else {
                panic!("tool response required")
            };
            assert_ne!(response.is_error, Some(true));
            frame(&mut editor, &context, vec![]);
            assert_ne!(
                recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
                before,
                "the next actual App frame must use current authoritative state"
            );
            let view = azteccraft::view_scene::ViewScene::from_native(
                editor.history.scene(),
                256 * 1024 * 1024,
            )
            .unwrap();
            assert_eq!(
                editor
                    .camera
                    .pick_view(&view, glam::Vec2::ZERO, editor.aspect)
                    .unwrap()
                    .unwrap()
                    .transform
                    .to_decimal(),
                id
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.history.can_undo(), undo);
        }
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        let inspection = &result["inspection"];
        assert_eq!(inspection["viewport_backend"], "wgpu_geometry_preview");
        assert_eq!(inspection["viewport_error"], Value::Null);
        assert_eq!(inspection["selected_instance_path"], json!(path));
        assert_eq!(inspection["depth_complete"], true);
        assert_eq!(inspection["clipping_complete"], true);
        assert_eq!(inspection["grid_visible"], false);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires a real headless GPU adapter"]
    async fn headless_gpu_sdk_hidden_inspection_reads_live_terminal_health_without_ui_paint() {
        use crate::viewport_renderer::tests::{creation_context, recorded_submission};
        use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
        let context = egui::Context::default();
        let (mut temporary, client, server) = start(&context).await;
        let creation = creation_context(context.clone());
        let mut editor = Editor::new(&creation).unwrap();
        editor.mcp = temporary.mcp.take();
        let token = editor.scene_token.to_string();
        call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube"}),
        )
        .await;
        frame(&mut editor, &context, vec![]);
        let before = recorded_submission(editor.renderer.as_ref().unwrap()).vertices;
        let scene = editor.history.scene().clone();
        let undo = editor.history.can_undo();
        let state = creation.wgpu_render_state.as_ref().unwrap();
        let _invalid = state
            .device
            .create_shader_module(eframe::wgpu::ShaderModuleDescriptor {
                label: Some("private-hidden-driver-marker"),
                source: eframe::wgpu::ShaderSource::Wgsl("invalid shader syntax".into()),
            });
        state
            .device
            .poll(eframe::wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        for (tool, arguments) in [
            ("inspect_scene", json!({})),
            ("dolly_camera", json!({"scene_token":token,"factor":1.0})),
            ("inspect_scene", json!({})),
        ] {
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(
                CallToolRequestParams::new(tool)
                    .with_arguments(arguments.as_object().unwrap().clone()),
            ));
            let handle = client
                .peer()
                .send_cancellable_request(request, Default::default())
                .await
                .unwrap();
            wait_for_queue(&editor).await;
            hidden_frame(&mut editor, &context, vec![]);
            let response = tokio::time::timeout(Duration::from_secs(2), handle.await_response())
                .await
                .unwrap()
                .unwrap();
            let ServerResult::CallToolResult(response) = response else {
                panic!("tool response required")
            };
            assert_ne!(response.is_error, Some(true));
            if tool == "inspect_scene" {
                let content = response.structured_content.unwrap();
                let inspection = &content["inspection"];
                assert_eq!(inspection["viewport_backend"], "wgpu_geometry_preview");
                assert_eq!(inspection["viewport_error"], "backend_failed");
                assert_eq!(inspection["depth_complete"], false);
                assert_eq!(inspection["clipping_complete"], false);
                assert!(!content.to_string().contains("private-hidden-driver-marker"));
            }
            assert_eq!(
                recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
                before
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.history.can_undo(), undo);
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    async fn call_response(
        editor: &mut Editor,
        context: &egui::Context,
        peer: &Peer<RoleClient>,
        name: &'static str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        let calling = peer.call_tool_once(
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone()),
        );
        tokio::pin!(calling);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            tokio::select! {
                result = &mut calling => {
                    let CallToolResponse::Complete(result) = result? else { panic!("expected complete tool result") };
                    return Ok(result);
                }
                _ = tokio::time::sleep(Duration::from_millis(1)) => frame(editor, context, vec![]),
                _ = tokio::time::sleep_until(deadline) => panic!("protocol call did not complete"),
            }
        }
    }

    async fn call(
        editor: &mut Editor,
        context: &egui::Context,
        peer: &Peer<RoleClient>,
        name: &'static str,
        arguments: Value,
    ) -> CallToolResult {
        call_response(editor, context, peer, name, arguments)
            .await
            .unwrap()
    }

    async fn wait_for_queue(editor: &Editor) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while editor.mcp.as_ref().unwrap().test_queue_len() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    async fn start(
        context: &egui::Context,
    ) -> (
        Editor,
        rmcp::service::RunningService<RoleClient, ()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (bridge, stream, serving) = mcp::test_connection(context.clone());
        let server = tokio::spawn(serving);
        let editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let client = ()
            .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
                reader, writer,
            ))
            .await
            .unwrap();
        (editor, client, server)
    }

    #[tokio::test]
    async fn strict_sdk_requests_and_core_errors_preserve_live_editor_state() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.create(Primitive::Cube);
        let token = editor.scene_token.to_string();
        let id = editor.selected.unwrap().to_decimal();
        let before = editor.history.scene().clone();
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "inspect_scene",
                json!({"unknown":true})
            )
            .await
            .is_err()
        );
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "delete_node",
                json!({"scene_token":token,"node_id":1})
            )
            .await
            .is_err()
        );
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "unknown_tool",
                json!({})
            )
            .await
            .is_err()
        );
        for (name, arguments, expected) in [
            (
                "rename_node",
                json!({"scene_token":token,"node_id":id,"name":""}),
                "scene_error",
            ),
            (
                "select_node",
                json!({"scene_token":token,"node_id":"999"}),
                "scene_error",
            ),
            (
                "delete_node",
                json!({"scene_token":Uuid::new_v4().to_string(),"node_id":id}),
                "stale_scene",
            ),
        ] {
            let result = call(&mut editor, &context, client.peer(), name, arguments).await;
            assert_eq!(result.is_error, Some(true));
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                expected
            );
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(editor.selected.unwrap().to_decimal(), id);
        }
        editor.name = "human_draft".into();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "busy_editing"
        );
        assert_eq!(editor.name, "human_draft");
        assert_eq!(editor.history.scene(), &before);
        editor.sync_fields();
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_grid_visibility_is_explicit_idempotent_and_preserves_authoring_state() {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.create(Primitive::Cube);
        editor.name = "temporary".into();
        assert!(editor.commit_pending());
        editor.history_step(false);
        frame(&mut editor, &context, vec![]);
        let token = editor.scene_token.to_string();
        let scene = editor.history.scene().clone();
        let baseline = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        for (visible, outcome) in [
            (false, "changed"),
            (false, "unchanged"),
            (true, "changed"),
            (true, "unchanged"),
        ] {
            let result = call(
                &mut editor,
                &context,
                client.peer(),
                "set_grid_visibility",
                json!({"scene_token": token, "visible": visible}),
            )
            .await;
            assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
            let mut inspected = call(
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
            assert_eq!(inspected["grid_visible"], visible);
            assert_eq!(inspected["viewport_error"], "uninitialized");
            assert_eq!(inspected["depth_complete"], false);
            inspected["grid_visible"] = json!(true);
            assert_eq!(inspected, baseline);
            assert_eq!(editor.history.scene(), &scene);
            assert!(editor.history.can_redo());
        }
        for arguments in [
            json!({"scene_token": token}),
            json!({"visible": false}),
            json!({"scene_token": token, "visible": null}),
            json!({"scene_token": token, "visible": "false"}),
            json!({"scene_token": token, "visible": 0}),
            json!({"scene_token": token, "visible": false, "extra": true}),
        ] {
            assert!(
                call_response(
                    &mut editor,
                    &context,
                    client.peer(),
                    "set_grid_visibility",
                    arguments
                )
                .await
                .is_err()
            );
            assert_eq!(
                serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
                baseline
            );
            assert_eq!(editor.history.scene(), &scene);
        }
        let stale = call(
            &mut editor,
            &context,
            client.peer(),
            "set_grid_visibility",
            json!({"scene_token": Uuid::new_v4().to_string(), "visible": false}),
        )
        .await;
        assert_eq!(
            stale.structured_content.unwrap()["error"]["code"],
            "stale_scene"
        );
        assert_eq!(
            serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
            baseline
        );
        editor.native_operation(Operation::SetGridVisible { visible: false });
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(inspected["inspection"]["grid_visible"], false);
        assert_eq!(editor.history.scene(), &scene);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn legacy_sdk_cancellation_and_eof_skip_hidden_queued_edits() {
        use rmcp::model::{CallToolRequest, ClientRequest};
        for tool in [
            "create_primitive",
            "edit_node",
            "extrude_face",
            "set_grid_visibility",
            "set_selection_mode",
            "select_face",
        ] {
            let context = egui::Context::default();
            let (mut editor, client, server) = start(&context).await;
            editor.create(if tool == "extrude_face" {
                Primitive::Cube
            } else {
                Primitive::Plane
            });
            if tool == "select_face" {
                editor
                    .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
                    .unwrap();
            }
            let before = editor.history.scene().clone();
            let selection = (
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
            );
            let args = if tool == "set_grid_visibility" {
                json!({"scene_token":editor.scene_token.to_string(),"visible":false})
            } else if tool == "edit_node" {
                json!({"scene_token":editor.scene_token.to_string(),"node_id":editor.selected.unwrap().to_decimal(),"translation":[3,null,null]})
            } else if tool == "extrude_face" {
                let id = editor.selected.unwrap();
                let shape = editor.history.scene().shapes()[0].id();
                json!({"scene_token":editor.scene_token.to_string(),"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[editor.history.scene().node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":0,"distance":1})
            } else if tool == "set_selection_mode" {
                json!({"scene_token":editor.scene_token.to_string(),"mode":"face"})
            } else if tool == "select_face" {
                let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
                let instance = &view.instances()[0];
                json!({"scene_token":editor.scene_token.to_string(),"target":{"node_id":instance.transform().to_decimal(),"shape_id":instance.shape().to_decimal(),"instance_path":instance.path().iter().map(|id|id.to_decimal()).collect::<Vec<_>>(),"face_index":0}})
            } else {
                json!({"scene_token":editor.scene_token.to_string(),"primitive":"cube"})
            };
            let request = ClientRequest::CallToolRequest(CallToolRequest::new(
                CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone()),
            ));
            let handle = client
                .peer()
                .send_cancellable_request(request.clone(), Default::default())
                .await
                .unwrap();
            wait_for_queue(&editor).await;
            handle.cancel(None).await.unwrap();
            client
                .peer()
                .send_request(ClientRequest::PingRequest(Default::default()))
                .await
                .unwrap();
            tokio::task::yield_now().await;
            hidden_frame(&mut editor, &context, vec![]);
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.grid_visible);
            assert_eq!(
                (
                    editor.selection_mode,
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone()
                ),
                selection
            );
            let _handle = client
                .peer()
                .send_cancellable_request(request, Default::default())
                .await
                .unwrap();
            wait_for_queue(&editor).await;
            client.cancel().await.unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                while editor.mcp.as_ref().unwrap().test_alive() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            hidden_frame(&mut editor, &context, vec![]);
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.grid_visible);
            assert_eq!(
                (
                    editor.selection_mode,
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone()
                ),
                selection
            );
            assert!(editor.files.is_modal());
            drop(editor);
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn ui_deadline_drops_receiver_before_paused_editor_can_apply_request() {
        for tool in ["create_primitive", "extrude_face", "set_grid_visibility"] {
            let context = egui::Context::default();
            let (mut editor, client, server) = start(&context).await;
            let args = if tool == "set_grid_visibility" {
                json!({"scene_token":editor.scene_token.to_string(),"visible":false})
            } else if tool == "extrude_face" {
                editor.create(Primitive::Cube);
                let id = editor.selected.unwrap();
                let shape = editor.history.scene().shapes()[0].id();
                json!({"scene_token":editor.scene_token.to_string(),"node_id":id.to_decimal(),"shape_id":shape.to_decimal(),"instance_path":[editor.history.scene().node(id).unwrap().transform_id().to_decimal(),shape.to_decimal()],"face_index":1,"distance":1})
            } else {
                json!({"scene_token":editor.scene_token.to_string(),"primitive":"cube"})
            };
            let before = editor.history.scene().clone();
            let CallToolResponse::Complete(result) = client
                .peer()
                .call_tool_once(
                    CallToolRequestParams::new(tool)
                        .with_arguments(args.as_object().unwrap().clone()),
                )
                .await
                .unwrap()
            else {
                panic!("expected complete reply")
            };
            assert_eq!(result.structured_content.unwrap()["error"]["code"], "busy");
            frame(&mut editor, &context, vec![]);
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.grid_visible);
            if tool == "create_primitive" {
                assert!(editor.history.scene().nodes().is_empty());
            }
            client.cancel().await.unwrap();
            drop(editor);
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn official_client_negotiates_legacy_version_and_reads_live_editor() {
        use rmcp::model::{ClientCapabilities, ClientConfig, Implementation, ProtocolVersion};
        let context = egui::Context::default();
        let (bridge, stream, serving) = mcp::test_connection(context.clone());
        let server = tokio::spawn(serving);
        let mut editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let config = ClientConfig::new(
            ClientCapabilities::default(),
            Implementation::new("protocol-test", "1"),
        )
        .with_protocol_version(ProtocolVersion::V_2025_11_25);
        let client = config
            .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
                reader, writer,
            ))
            .await
            .unwrap();
        assert_eq!(
            client.peer_info().unwrap().protocol_version,
            ProtocolVersion::V_2025_11_25
        );
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await;
        assert_eq!(
            inspected.structured_content.unwrap()["inspection"]["scene_token"],
            editor.scene_token.to_string()
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_current_discover_lifecycle_mutates_the_same_editor() {
        use rmcp::{ClientLifecycleMode, ClientServiceExt};
        let context = egui::Context::default();
        let (bridge, stream, serving) = mcp::test_connection(context.clone());
        let server = tokio::spawn(serving);
        let mut editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let client = ()
            .serve_with_lifecycle(
                rmcp::transport::async_rw::AsyncRwTransport::new(reader, writer),
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .unwrap();
        assert_eq!(
            client.peer_info().unwrap().protocol_version,
            rmcp::model::ProtocolVersion::V_2026_07_28
        );
        assert_eq!(
            client.peer().list_tools(None).await.unwrap().tools.len(),
            crate::editor_operations::TOOL_NAMES.len()
        );
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await;
        let token = inspected.structured_content.unwrap()["inspection"]["scene_token"].clone();
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":"plane","name":"discover_plane"}),
        )
        .await;
        assert_eq!(result.is_error, Some(false));
        assert_eq!(editor.history.scene().nodes().len(), 1);
        assert_eq!(editor.name, "discover_plane");
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await;
        assert_eq!(
            inspected.structured_content.unwrap()["inspection"]["nodes"][0]["name"],
            "discover_plane"
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sdk_client_discovers_every_handler_and_edits_same_native_controller() {
        let context = egui::Context::default();
        let (bridge, stream, serving) = mcp::test_connection(context.clone());
        let server_task = tokio::spawn(serving);
        let mut editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let client = ()
            .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
                reader, writer,
            ))
            .await
            .unwrap();
        let peer = client.peer();
        let listed = peer.list_tools(None).await.unwrap();
        assert_eq!(
            listed
                .tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            crate::editor_operations::TOOL_NAMES
        );
        let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({})).await;
        let token = inspected.structured_content.unwrap()["inspection"]["scene_token"]
            .as_str()
            .unwrap()
            .to_string();
        let created = call(
            &mut editor,
            &context,
            peer,
            "create_primitive",
            json!({"scene_token":token,"primitive":"cube","name":"remote_cube"}),
        )
        .await;
        let id = created.structured_content.unwrap()["node_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(editor.name, "remote_cube");
        call(
            &mut editor,
            &context,
            peer,
            "rename_node",
            json!({"scene_token":token,"node_id":id,"name":"renamed"}),
        )
        .await;
        call(&mut editor, &context, peer, "set_transform", json!({"scene_token":token,"node_id":id,"translation":[2,0,0],"rotation_degrees":[0,0,0],"scale":[1,1,1]})).await;
        assert_eq!(editor.channels[0], "2");
        call(
            &mut editor,
            &context,
            peer,
            "frame_selected",
            json!({"scene_token":token}),
        )
        .await;
        call(
            &mut editor,
            &context,
            peer,
            "select_node",
            json!({"scene_token":token,"node_id":null}),
        )
        .await;
        assert_eq!(editor.selected, None);
        call(
            &mut editor,
            &context,
            peer,
            "select_node",
            json!({"scene_token":token,"node_id":id}),
        )
        .await;
        editor.channels[0] = "3".into();
        assert!(editor.commit_pending());
        let inspected = call(&mut editor, &context, peer, "inspect_scene", json!({})).await;
        assert_eq!(
            inspected.structured_content.unwrap()["inspection"]["nodes"][0]["transform"]["translation"]
                [0].as_f64(),
            Some(3.0)
        );
        call(
            &mut editor,
            &context,
            peer,
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.channels[0], "2");
        editor.history_step(false);
        call(
            &mut editor,
            &context,
            peer,
            "redo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.channels[0], "2");
        call(
            &mut editor,
            &context,
            peer,
            "delete_node",
            json!({"scene_token":token,"node_id":id}),
        )
        .await;
        assert!(editor.history.scene().nodes().is_empty());
        client.cancel().await.unwrap();
        server_task.await.unwrap();
    }
}
