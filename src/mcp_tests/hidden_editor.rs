use super::*;

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
                editor.selected(),
                editor.selected_path().to_vec(),
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
                    editor.selected(),
                    editor.selected_path().to_vec(),
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

#[test]
fn remote_numeric_move_survives_successive_actual_hidden_editor_callbacks() {
    use crate::editor_operations::MoveDistance;
    use azteccraft::{
        history::{ManipulationFinish, ManipulationId},
        native_scene::WorldAxis,
    };
    let context = egui::Context::default();
    let (bridge, ingress) = mcp::test_bridge(context.clone());
    let mut editor = Editor {
        mcp: Some(bridge),
        ..Default::default()
    };
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let target = editor.selected_move_target().unwrap();
    let token = editor.scene_token;
    let mut begin = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    let Ok(OperationReply::ManipulationStarted {
        manipulation_id: move_id,
    }) = begin.try_recv().unwrap()
    else {
        panic!()
    };
    let id = ManipulationId::from_decimal(&move_id).unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.active_manipulation().unwrap().id, id);
    let mut update = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::UpdateMove {
                id,
                distance: MoveDistance::Declared(2.0),
            },
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        update.try_recv().unwrap(),
        Ok(OperationReply::Changed)
    ));
    let preview = editor.history.scene().clone();
    assert_eq!(preview.nodes()[0].transform().translation[0], 2.0);
    hidden_frame(&mut editor, &context, vec![]);
    assert_eq!(editor.history.active_manipulation().unwrap().id, id);
    assert_eq!(editor.history.scene(), &preview);
    let mut finish = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::FinishMove {
                id,
                finish: ManipulationFinish::Commit,
            },
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        finish.try_recv().unwrap(),
        Ok(OperationReply::Changed)
    ));
    assert!(editor.history.active_manipulation().is_none());
    let mut undo = ingress
        .queue(RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::Undo,
        })
        .unwrap();
    hidden_frame(&mut editor, &context, vec![]);
    assert!(matches!(
        undo.try_recv().unwrap(),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn camera_sparse_extrusion_and_grid_requests_keep_human_and_closed_bridge_guards() {
    for condition in 0..8 {
        for operation_index in 0..9 {
            let context = egui::Context::default();
            let (bridge, ingress) = mcp::test_bridge(context.clone());
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let id = editor.selected().unwrap();
            let shape = editor.history.scene().shapes()[0].id();
            let operation = match operation_index {
                0..=3 => camera_operations()
                    .into_iter()
                    .nth(operation_index)
                    .unwrap(),
                4 => Operation::EditNode {
                    id,
                    name: None,
                    translation: [None; 3],
                    rotation_degrees: [None; 3],
                    scale: [None; 3],
                },
                5 => Operation::ExtrudeFace {
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
                6 => Operation::SetGridVisible { visible: false },
                7 => Operation::SetShadingMode(crate::editor_operations::ShadingMode::Wireframe),
                _ => Operation::SetSidebarPanelVisible {
                    panel: SidebarPanel::ChannelBox,
                    visible: false,
                },
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
            assert_eq!(
                editor.shading,
                crate::editor_operations::ShadingMode::default()
            );
            assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        }
    }
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
    assert_eq!(editor.selected(), baseline.selected());
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
    assert!(baseline.name.contains("human"));
    assert_eq!(editor.name, baseline.name);
    assert_eq!(editor.channels, baseline.channels);
    assert_eq!(editor.selected(), baseline.selected());
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
    assert_eq!(editor.selected(), baseline.selected());
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
