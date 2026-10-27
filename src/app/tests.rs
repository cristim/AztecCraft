use super::*;

mod material_wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}
mod material_fixture {
    use super::material_wire as wire;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_appearance.rs"
    ));
}

pub(in crate::app) fn material_editor() -> Editor {
    let scene = azteccraft::native_scene::binary::decode(
        material_fixture::appearance_specimen_with_users(2).into(),
    )
    .unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.set_selection_for_test(
        Some(editor.history.scene().nodes()[0].id()),
        vec![editor.history.scene().nodes()[0].transform_id()],
    );
    editor.sync_fields();
    editor
}

fn independent_material_editor() -> Editor {
    let mut bytes = material_fixture::appearance_specimen_with_users(2);
    let link = b"shader.oc\0surface2.ss\0";
    let offset = bytes
        .windows(link.len())
        .position(|word| word == link)
        .unwrap();
    bytes[offset..offset + 6].copy_from_slice(b"shadeB");
    let container = azteccraft::maya_binary::container::inspect(bytes.clone().into()).unwrap();
    let shader = container
        .chunks()
        .iter()
        .find(|chunk| chunk.form() == Some(*b"RPHO"))
        .unwrap();
    let mut second = bytes[shader.header_range().start..shader.padding_range().end].to_vec();
    let offset = second
        .windows(7)
        .position(|word| word == b"shader\0")
        .unwrap();
    second[offset..offset + 6].copy_from_slice(b"shadeB");
    let mut children = Vec::new();
    for (index, chunk) in container
        .chunks()
        .iter()
        .enumerate()
        .filter(|(_, chunk)| chunk.parent() == Some(0))
    {
        if chunk.form() == Some(*b"CONS") {
            let connection = container
                .chunks()
                .iter()
                .find(|child| child.parent() == Some(index) && child.form() == Some(*b"CONN"))
                .unwrap();
            let mut links = bytes
                [connection.payload_range().start + 4..connection.payload_range().end]
                .to_vec();
            links.extend(material_wire::chunk(
                b"CWFL",
                b"\0texture.oc\0shadeB.c\0",
                8,
            ));
            children.extend(material_wire::group(
                b"CONS",
                &material_wire::group(b"CONN", &links, 4),
                4,
            ));
        } else {
            children
                .extend_from_slice(&bytes[chunk.header_range().start..chunk.padding_range().end]);
        }
    }
    children.extend(second);
    let scene = azteccraft::native_scene::binary::decode(
        material_wire::group(b"Maya", &children, 1).into(),
    )
    .unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.set_selection_for_test(Some(editor.history.scene().nodes()[0].id()), Vec::new());
    editor.sync_fields();
    editor
}

#[test]
fn material_selection_commits_only_original_independent_shader() {
    let mut editor = independent_material_editor();
    let first = editor.material_shape.unwrap();
    let second = editor.history.scene().shapes()[1].id();
    let other = editor.history.scene().nodes()[1].id();
    let untouched = editor
        .history
        .scene()
        .material_for_shape(second)
        .unwrap()
        .diffuse
        .to_bits();
    editor.material_draft.as_mut().unwrap().text = "0.125".into();
    editor.select(Some(other));
    assert_eq!(editor.material_shape, Some(second));
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(first)
            .unwrap()
            .diffuse,
        0.125
    );
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(second)
            .unwrap()
            .diffuse
            .to_bits(),
        untouched
    );
    editor.history_step(false);
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(first)
            .unwrap()
            .diffuse
            .to_bits(),
        untouched
    );
    editor.history_step(true);
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(first)
            .unwrap()
            .diffuse,
        0.125
    );
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(second)
            .unwrap()
            .diffuse
            .to_bits(),
        untouched
    );
}

#[test]
fn material_real_text_enter_and_escape_preserve_precision_in_one_history_entry() {
    let mut editor = material_editor();
    let shape = editor.material_shape.unwrap();
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(material_edit_id()));
    frame(&mut editor, &ctx, vec![]);
    let command = egui::Modifiers::COMMAND;
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(command),
            egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            },
            egui::Event::Text("1.0000000000000002".into()),
            key(Key::Enter),
        ],
    );
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(shape)
            .unwrap()
            .diffuse
            .to_bits(),
        1.0000000000000002f64.to_bits()
    );
    assert!(!editor.has_draft());
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(!editor.history.can_undo());
    editor.sync_fields();
    ctx.memory_mut(|memory| memory.request_focus(material_edit_id()));
    frame(&mut editor, &ctx, vec![]);
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(command),
            egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            },
            egui::Event::Text("0.125".into()),
        ],
    );
    assert!(editor.has_draft());
    frame(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert_eq!(editor.history.scene(), &before);
    assert!(!editor.has_draft());
}

#[test]
fn material_failed_and_cancelled_open_preserve_draft_and_success_resets_reused_shape_choice() {
    let mut editor = material_editor();
    editor.material_draft.as_mut().unwrap().text = "0.125".into();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let directory = tempfile::tempdir().unwrap();
    assert!(
        editor
            .open_candidate(&directory.path().join("missing.mb"))
            .is_err()
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.material_draft.as_ref().unwrap().text, "0.125");
    editor.sync_fields();
    let source = directory.path().join("replacement.mb");
    std::fs::write(&source, material_wire::specimen()).unwrap();
    let OperationReply::PendingConfirmation {
        pending_action_id, ..
    } = editor.open_candidate(&source).unwrap()
    else {
        panic!("original nonempty document is dirty");
    };
    let original_shape = editor.material_shape;
    editor
        .resolve_file_action(
            Uuid::parse_str(&pending_action_id).unwrap(),
            crate::editor_operations::FileDecision::Cancel,
        )
        .unwrap();
    assert_eq!(editor.material_shape, original_shape);
    assert_eq!(editor.scene_token, token);
    let OperationReply::PendingConfirmation {
        pending_action_id, ..
    } = editor.open_candidate(&source).unwrap()
    else {
        panic!("confirmation required");
    };
    editor
        .resolve_file_action(
            Uuid::parse_str(&pending_action_id).unwrap(),
            crate::editor_operations::FileDecision::Discard,
        )
        .unwrap();
    assert_ne!(editor.scene_token, token);
    assert!(editor.material_shape.is_none());
    assert!(editor.material_draft.is_none());
    assert!(editor.commit_pending());
}

#[test]
fn material_draft_old_target_shared_history_and_escape_use_authoritative_state() {
    let mut editor = material_editor();
    let shape = editor.material_shape.unwrap();
    let original = editor
        .history
        .scene()
        .material_for_shape(shape)
        .unwrap()
        .diffuse;
    editor.material_draft.as_mut().unwrap().text = "1.2345678901234567".into();
    editor.material_draft.as_mut().unwrap().intent = true;
    let other = editor.history.scene().nodes()[1].id();
    editor.select(Some(other));
    assert_eq!(editor.selection_presentation().0, Some(other));
    let value = editor.history.scene().material_for_shape(shape).unwrap();
    assert_eq!(value.diffuse.to_bits(), 1.2345678901234567f64.to_bits());
    assert_eq!(value.shared_users, 2);
    for candidate in editor.history.scene().shapes() {
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(candidate.id())
                .unwrap()
                .diffuse
                .to_bits(),
            value.diffuse.to_bits()
        );
    }
    editor.history_step(false);
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(shape)
            .unwrap()
            .diffuse
            .to_bits(),
        original.to_bits()
    );
    editor.history_step(true);
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(shape)
            .unwrap()
            .diffuse
            .to_bits(),
        value.diffuse.to_bits()
    );
    let before = editor.history.scene().clone();
    editor.material_draft.as_mut().unwrap().text = "0.25".into();
    let context = egui::Context::default();
    let mut frame = context.run_ui(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        },
        |ui| editor.cancel_on_escape(ui.ctx()),
    );
    frame.textures_delta.clear();
    assert_eq!(editor.history.scene(), &before);
    assert!(!editor.has_draft());
    assert!(
        editor
            .history
            .scene()
            .clone()
            .set_material_diffuse(shape, 0.5)
            .is_ok()
    );
    assert!(editor.history.execute(Command::Remove(other)).is_err());
}

#[test]
fn material_paint_preserves_unrounded_words_and_focus_or_drafts_block_remote_mutation() {
    let context = egui::Context::default();
    let mut editor = material_editor();
    let shape = editor.material_shape.unwrap();
    for value in [1.2345678901234567, -0.0, 4.5] {
        editor
            .history
            .execute(Command::SetMaterialDiffuse { shape, value })
            .unwrap();
        editor.sync_fields();
        let before = editor.history.scene().clone();
        for _ in 0..2 {
            let mut frame =
                context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
            frame.textures_delta.clear();
        }
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(
            editor.material_draft.as_ref().unwrap().text,
            value.to_string()
        );
        assert!(!editor.has_draft());
    }
    context.memory_mut(|memory| memory.request_focus(material_edit_id()));
    let token = editor.scene_token;
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(token),
                operation: Operation::SetMaterialDiffuse { shape, value: 0.25 }
            },
            &context
        ),
        Err(OperationError::BusyEditing)
    ));
    context.memory_mut(|memory| memory.surrender_focus(material_edit_id()));
    editor.material_draft.as_mut().unwrap().text = "0.125".into();
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(token),
                operation: Operation::Undo
            },
            &context
        ),
        Err(OperationError::BusyEditing)
    ));
    let before = editor.history.scene().clone();
    editor.material_draft.as_mut().unwrap().scene_token = Uuid::new_v4();
    assert!(!editor.commit_pending());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn material_invalid_or_simultaneous_channel_draft_never_partially_commits() {
    let mut editor = material_editor();
    let before = editor.history.scene().clone();
    editor.material_draft.as_mut().unwrap().text = "NaN".into();
    assert!(!editor.commit_pending());
    assert_eq!(editor.history.scene(), &before);
    editor.material_draft.as_mut().unwrap().text = "0.2".into();
    editor.channels[0] = "2".into();
    assert!(!editor.commit_pending());
    assert_eq!(editor.history.scene(), &before);
    editor.sync_fields();
    assert!(!editor.has_draft());
}

#[test]
fn material_new_document_commits_the_original_target_then_clears_its_draft() {
    let mut editor = material_editor();
    let token = editor.scene_token;
    let shape = editor.material_shape.unwrap();
    editor.material_draft.as_mut().unwrap().text = "0.125".into();
    let reply = editor.new_document().unwrap();
    let OperationReply::PendingConfirmation {
        pending_action_id, ..
    } = reply
    else {
        panic!("edited material must require confirmation");
    };
    assert_eq!(
        editor
            .history
            .scene()
            .material_for_shape(shape)
            .unwrap()
            .diffuse,
        0.125
    );
    assert!(!editor.has_draft());
    editor
        .resolve_file_action(
            Uuid::parse_str(&pending_action_id).unwrap(),
            crate::editor_operations::FileDecision::Discard,
        )
        .unwrap();
    assert_ne!(editor.scene_token, token);
    assert!(editor.material_draft.is_none());
    assert!(editor.material_shape.is_none());
    assert!(editor.history.scene().shapes().is_empty());
    assert!(editor.commit_pending());
}

#[test]
fn material_negative_source_word_is_not_changed_by_native_paint() {
    let mut bytes = material_fixture::appearance_specimen();
    let marker = [b'd', b'c', 0, 0x24];
    let offset = bytes
        .windows(marker.len())
        .position(|value| value == marker)
        .unwrap()
        + marker.len();
    let value = -1.2345678901234567f64;
    bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
    let scene = azteccraft::native_scene::binary::decode(bytes.into()).unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.set_selection_for_test(Some(editor.history.scene().nodes()[0].id()), Vec::new());
    editor.sync_fields();
    let before = editor.history.scene().clone();
    let context = egui::Context::default();
    context.enable_accesskit();
    let state = |editor: &Editor| {
        (
            click_baseline(editor),
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
            ),
            (
                editor.material_shape,
                editor.material_draft.as_ref().map(|draft| {
                    (
                        draft.text.clone(),
                        draft.original.to_bits(),
                        draft.intent,
                        draft.shape,
                        draft.scene_token,
                        draft.focus,
                    )
                }),
            ),
            context.memory(|memory| memory.focused()),
            (
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.history.undo_label(),
                editor.history.redo_label(),
                editor.history.active_manipulation().is_some(),
            ),
        )
    };
    let owner = editor.history.scene().nodes()[0].id();
    let root = editor.history.scene().nodes()[0].transform_id();
    let stale = NodeId::from_decimal("999").unwrap();
    assert!(editor.history.scene().node(stale).is_err());
    assert!(
        editor
            .history
            .scene()
            .resolve_transform_path(owner, &[root])
            .is_ok()
    );
    assert!(
        editor
            .history
            .scene()
            .resolve_transform_path(owner, &[root, root])
            .is_err()
    );
    for (phase, selected, path, body) in [
        ("material/full", Some(owner), vec![root], true),
        ("material/empty", Some(owner), Vec::new(), true),
        ("material/stale-path", Some(owner), vec![root, root], true),
        ("material/no-owner", None, Vec::new(), false),
        ("material/stale-owner", Some(stale), Vec::new(), false),
    ] {
        editor.set_selection_for_test(selected, path.clone());
        let baseline = state(&editor);
        assert_eq!(baseline.4, (false, false, None, None, false), "{phase}");
        for _ in 0..2 {
            let mut frame =
                context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
            frame.textures_delta.clear();
            let update = frame.platform_output.accesskit_update.as_ref().unwrap();
            assert!(
                update.nodes.iter().any(|(_, node)| {
                    node.role() == egui::accesskit::Role::Label && node.value() == Some("Materials")
                }),
                "{phase}"
            );
            let controls = toolbox_wire::controls(&frame);
            let mut colors = controls
                .iter()
                .filter(|control| control.label == "Color (connected)");
            if body {
                assert!(colors.next().unwrap().disabled, "{phase}");
            }
            assert!(colors.next().is_none(), "{phase}");
        }
        assert_eq!(state(&editor), baseline, "{phase}: paint");
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection");
        };
        assert_eq!(
            inspection.selected_node,
            selected.map(NodeId::to_decimal),
            "{phase}"
        );
        assert_eq!(
            inspection.selected_instance_path,
            path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
            "{phase}"
        );
        assert_eq!(state(&editor), baseline, "{phase}: inspection");
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(
            editor.material_draft.as_ref().unwrap().text,
            value.to_string()
        );
        assert!(!editor.has_draft());
    }
}

#[test]
fn material_multiple_direct_shapes_require_an_explicit_choice_without_guessing() {
    let extra = material_wire::group(
        b"DMSH",
        &[
            material_wire::creation(b"secondShape", Some(b"object")),
            material_wire::chunk(
                b"MESH",
                &material_wire::mesh_payload(&[0, 1, 0x6000_0002]),
                8,
            ),
        ]
        .concat(),
        4,
    );
    let bytes = material_fixture::appearance_specimen_with_fields(material_fixture::Fields {
        groups: &extra,
        ..Default::default()
    });
    let scene = azteccraft::native_scene::binary::decode(bytes.into()).unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.set_selection_for_test(Some(editor.history.scene().nodes()[0].id()), Vec::new());
    editor.sync_fields();
    assert!(editor.material_shape.is_none());
    assert!(editor.material_draft.is_none());
    let context = egui::Context::default();
    let before = editor.history.scene().clone();
    let mut frame = context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
    frame.textures_delta.clear();
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.material_shape.is_none());
    editor.material_shape = Some(editor.history.scene().shapes()[0].id());
    editor.sync_fields();
    assert!(editor.material_draft.is_some());
}

fn selected_face(editor: &Editor, face_index: usize) -> FaceTarget {
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let instance = view
        .instances()
        .iter()
        .find(|instance| Some(instance.transform()) == editor.selection_presentation().0)
        .unwrap();
    FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index,
    }
}

#[test]
fn face_lifecycle_revalidates_numeric_edits_topology_history_and_object_deletion() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let cap = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(cap.clone()),
        },
    )
    .unwrap();
    editor.channels[0] = "2".into();
    editor.native_operation(Operation::FrameSelected);
    assert!(!editor.error);
    assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        2.0
    );
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
    remote(&mut editor, Operation::Redo).unwrap();
    assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
    remote(
        &mut editor,
        Operation::ExtrudeFace {
            target: cap.clone(),
            distance: 0.25,
        },
    )
    .unwrap();
    assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
    let side = FaceTarget {
        face_index: 6,
        ..cap
    };
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(side.clone()),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_face, None);
    assert!(editor.history.can_redo());
    remote(&mut editor, Operation::Redo).unwrap();
    assert_eq!(editor.remembered_face, None);
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(side.clone()),
        },
    )
    .unwrap();
    assert!(matches!(
        remote(&mut editor, Operation::Delete(id)),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(editor.selection_presentation().0, Some(id));
    assert_eq!(editor.remembered_face.as_ref(), Some(&side));
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Object),
    )
    .unwrap();
    remote(&mut editor, Operation::Delete(id)).unwrap();
    assert_eq!(editor.selection_presentation().0, None);
    assert!(editor.selection_presentation().1.is_empty());
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    remote(&mut editor, Operation::Undo).unwrap();
    assert!(editor.history.scene().node(id).is_ok());
    assert_eq!(editor.remembered_face, None);
}

#[test]
fn face_frame_uses_selected_corners_even_when_center_is_not_pickable() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.aspect = 4.0 / 3.0;
    let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
    let instance = &view.instances()[0];
    let (face, target) = instance
        .faces()
        .iter()
        .find_map(|face| {
            let center = face.center(instance.world_vertices()).unwrap();
            let ndc = editor.camera.project(center, editor.aspect).unwrap()?;
            let pointer = egui::vec2((ndc.x + 1.0) * 400.0, (1.0 - ndc.y) * 300.0);
            let target = FaceTarget {
                transform: instance.transform(),
                shape: instance.shape(),
                path: instance.path().to_vec(),
                face_index: face.source_index(),
            };
            let hit = editor
                .camera
                .pick_face_center(
                    &view,
                    Some((instance.transform(), editor.selection_presentation().1)),
                    glam::Vec2::new(pointer.x, pointer.y),
                    [800, 600],
                    1.0,
                )
                .unwrap();
            (hit.as_ref() != Some(&target)).then_some((face, target))
        })
        .expect("cube has an occluded center");
    let corners: Vec<_> = face
        .corners()
        .iter()
        .map(|&index| instance.world_vertices()[index])
        .collect();
    let mut expected = editor.camera.clone();
    expected.frame_points(&corners, editor.aspect).unwrap();
    let scene = editor.history.scene().clone();
    let token = editor.scene_token;
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(target.clone()),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert_eq!(editor.camera, expected);
    assert_eq!(editor.remembered_face.as_ref(), Some(&target));
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.scene_token, token);
    remote(&mut editor, Operation::SelectFace { target: None }).unwrap();
    let camera = editor.camera.clone();
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::NoSelection)
    ));
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.history.scene(), &scene);
    editor.name = "valid_human_rename".into();
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.name, "valid_human_rename");
    editor.native_operation(Operation::FrameSelected);
    assert!(editor.error);
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.remembered_face, None);
    assert_eq!(
        editor
            .history
            .scene()
            .node(target.transform)
            .unwrap()
            .name(),
        "valid_human_rename"
    );
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn face_mode_preserves_the_remembered_target_without_editing_scene_or_redo() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let target = selected_face(&editor, 4);
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "temporary".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    let scene = editor.history.scene().clone();
    let token = editor.scene_token;
    let camera = editor.camera.clone();
    let dirty = editor.inspect(false).unwrap().dirty;
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(target.clone()),
        },
    )
    .unwrap();
    for mode in [
        SelectionMode::Object,
        SelectionMode::Hierarchy,
        SelectionMode::Face,
    ] {
        remote(&mut editor, Operation::SetSelectionMode(mode)).unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&target));
        assert_eq!(editor.selection_presentation().0, Some(id));
        assert_eq!(
            editor.selection_presentation().1,
            &target.path[..target.path.len() - 1]
        );
    }
    remote(
        &mut editor,
        Operation::Select {
            node: Some(id),
            instance_path: None,
        },
    )
    .unwrap();
    assert_eq!(editor.remembered_face.as_ref(), Some(&target));
    remote(&mut editor, Operation::SelectFace { target: None }).unwrap();
    assert_eq!(editor.selection_presentation().0, Some(id));
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.history.scene(), &scene);
    assert!(editor.history.can_redo());
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.inspect(false).unwrap().dirty, dirty);
}

#[test]
fn explicit_face_selection_selects_its_owner_and_object_changes_clear_memory() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = selected_face(&editor, 5);
    editor.create(Primitive::Plane);
    let other = editor.single_target().unwrap().0;
    let second_shape = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::Rename {
            id: other,
            name: "temporary".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(target.clone()),
        },
    )
    .unwrap();
    assert_eq!(editor.selection_presentation().0, Some(target.transform));
    assert_eq!(
        editor.selection_presentation().1,
        &target.path[..target.path.len() - 1]
    );
    assert_eq!(editor.remembered_face.as_ref(), Some(&target));
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(second_shape.clone()),
        },
    )
    .unwrap();
    assert_eq!(editor.selection_presentation().0, Some(other));
    assert_eq!(editor.remembered_face.as_ref(), Some(&second_shape));
    assert_eq!(
        editor.selection_presentation().1,
        &second_shape.path[..second_shape.path.len() - 1]
    );
    let baseline = (
        editor.history.scene().clone(),
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.selection_mode,
        editor.remembered_face.clone(),
        editor.camera.clone(),
        editor.scene_token,
        editor.name.clone(),
        editor.channels.clone(),
        editor.inspect(false).unwrap().dirty,
    );
    assert!(editor.history.can_redo());
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(second_shape)
            }
        )
        .unwrap(),
        OperationReply::Unchanged
    ));
    assert_eq!(
        (
            editor.history.scene().clone(),
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.selection_mode,
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token,
            editor.name.clone(),
            editor.channels.clone(),
            editor.inspect(false).unwrap().dirty,
        ),
        baseline
    );
    assert!(editor.history.can_redo());
    remote(
        &mut editor,
        Operation::Select {
            node: Some(target.transform),
            instance_path: None,
        },
    )
    .unwrap();
    assert_eq!(editor.remembered_face, None);
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(target),
        },
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::Select {
            node: None,
            instance_path: None,
        },
    )
    .unwrap();
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.selection_presentation().0, None);
}

#[test]
fn invalid_face_targets_and_wrong_mode_preserve_the_selection_baseline() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = selected_face(&editor, 1);
    editor.name = "valid_human_draft".into();
    assert!(
        editor
            .apply_operation(Operation::SelectFace {
                target: Some(target.clone())
            })
            .is_err()
    );
    assert!(editor.has_draft());
    assert_eq!(editor.remembered_face, None);
    editor.sync_fields();
    editor.create(Primitive::Plane);
    let other = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(
        &mut editor,
        Operation::SelectFace {
            target: Some(target.clone()),
        },
    )
    .unwrap();
    let scene = editor.history.scene().clone();
    let before = (
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.selection_mode,
        editor.remembered_face.clone(),
        editor.name.clone(),
        editor.channels.clone(),
        editor.scene_token,
        editor.camera.clone(),
    );
    let mut invalid = Vec::new();
    let mut bad = target.clone();
    bad.path = other.path;
    invalid.push(bad);
    let mut bad = target.clone();
    bad.transform = other.transform;
    invalid.push(bad);
    let mut bad = target.clone();
    bad.shape = other.shape;
    invalid.push(bad);
    let mut bad = target.clone();
    bad.face_index = usize::MAX;
    invalid.push(bad);
    let mut bad = target.clone();
    bad.path.clear();
    invalid.push(bad);
    let mut bad = target.clone();
    bad.path = vec![target.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
    invalid.push(bad);
    let mut bad = target.clone();
    bad.transform = NodeId::from_decimal("18446744073709551615").unwrap();
    invalid.push(bad);
    let mut bad = target;
    bad.shape = NativeId::from_decimal("18446744073709551615").unwrap();
    invalid.push(bad);
    for target in invalid {
        assert!(
            remote(
                &mut editor,
                Operation::SelectFace {
                    target: Some(target)
                }
            )
            .is_err()
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(
            (
                editor.selection_presentation().0,
                editor.selection_presentation().1.to_vec(),
                editor.selection_mode,
                editor.remembered_face.clone(),
                editor.name.clone(),
                editor.channels.clone(),
                editor.scene_token,
                editor.camera.clone(),
            ),
            before
        );
    }
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Object),
    )
    .unwrap();
    let before = (
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.selection_mode,
        editor.remembered_face.clone(),
        editor.name.clone(),
        editor.channels.clone(),
        editor.scene_token,
        editor.camera.clone(),
    );
    assert!(remote(&mut editor, Operation::SelectFace { target: None }).is_err());
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(
        (
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.selection_mode,
            editor.remembered_face.clone(),
            editor.name.clone(),
            editor.channels.clone(),
            editor.scene_token,
            editor.camera.clone(),
        ),
        before
    );
}

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[test]
fn ascii_inspection_reports_canonical_curves_without_private_arrays_or_gpu_claims() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    let source = ascii_fixture::specimen();
    std::fs::write(&path, &source).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(
        inspection.document_origin,
        crate::editor_operations::DocumentOrigin::MayaAscii
    );
    assert_eq!(inspection.retained_source_bytes, source.len());
    assert_eq!(inspection.retained_source_records, 10);
    assert!(inspection.read_only_reason.unwrap().contains("read-only"));
    assert!(inspection.shapes.is_empty());
    assert_eq!(inspection.curves.len(), 1);
    assert!(!inspection.save_available && !inspection.save_as_available);
    let curve = &inspection.curves[0];
    assert_eq!(
        curve.id,
        editor.history.scene().curves()[0].id().to_decimal()
    );
    assert_eq!(
        inspection.nodes[0].shape_ids.as_slice(),
        std::slice::from_ref(&curve.id)
    );
    assert_eq!(curve.degree, 3);
    assert_eq!(curve.control_vertices, 4);
    assert_eq!(curve.knots, 6);
    assert_eq!(curve.domain, [0.0, 1.0]);
    assert!(curve.read_only && curve.draw_available);
    assert_eq!(inspection.viewport_backend, "uninitialized");
    assert!(!inspection.depth_complete && !inspection.clipping_complete);
    let value = serde_json::to_value(curve).unwrap();
    assert_eq!(value["form"], "open");
    assert!(value["control_vertices"].is_number() && value["knots"].is_number());
    assert!(!serde_json::to_string(&value).unwrap().contains("script"));

    for source in [
        String::from_utf8(source.clone())
            .unwrap()
            .replace("setAttr \".t\"", "setAttr \".v\" no; setAttr \".t\""),
        String::from_utf8(source)
            .unwrap()
            .replace("12 0 0", "100000000 0 0"),
    ] {
        let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
        let mut editor = Editor {
            history: History::new(scene),
            ..Default::default()
        };
        editor.viewport_error = None;
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(inspection.curves.len(), 1);
        assert!(!inspection.curves[0].draw_available);
        assert_eq!(inspection.viewport_backend, "uninitialized");
        assert!(!inspection.depth_complete && !inspection.clipping_complete);
    }
}

#[test]
fn face_mode_on_a_curves_only_ascii_scene_refuses_face_targets_and_preserves_state() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Default::default()
    };
    editor.viewport_error = None;
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    let curve = &editor.history.scene().curves()[0];
    let target = FaceTarget {
        transform: editor.history.scene().nodes()[0].id(),
        shape: curve.id(),
        path: vec![editor.history.scene().nodes()[0].transform_id(), curve.id()],
        face_index: 0,
    };
    let before = editor.history.scene().clone();
    assert!(
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target)
            }
        )
        .is_err()
    );
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.history.scene(), &before);
    assert!(!editor.history.can_undo());
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(inspection.curves.len(), 1);
}

fn channel_text(output: &egui::FullOutput) -> String {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn help_line_text(editor: &mut Editor) -> String {
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ui| editor.help_line(ui));
    output.textures_delta.clear();
    channel_text(&output)
}

/// Channel Box text while the pointer rests on `field`, so tooltips are included.
fn hover_texts(editor: &mut Editor, context: &egui::Context, field: egui::Id) -> String {
    context.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 900.0));
    let mut text = String::new();
    for step in 0..8 {
        let at = context
            .read_response(field)
            .map_or(egui::pos2(60.0, 60.0), |response| response.rect.center());
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                time: Some(10.0 + f64::from(step) * 0.25),
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            },
            |ui| editor.channels(ui, &mut false),
        );
        output.textures_delta.clear();
        text = channel_text(&output);
    }
    text
}

#[test]
fn ascii_channel_controls_are_disabled_and_show_the_shared_core_reason() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let id = editor.history.scene().nodes()[0].id();
    editor.select(Some(id));
    let before = editor.history.scene().clone();
    let fields = (editor.name.clone(), editor.channels.clone());
    for text_id in [name_edit_id(), channel_edit_id(0)] {
        let context = egui::Context::default();
        context.memory_mut(|memory| memory.request_focus(text_id));
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420.0, 900.0),
                )),
                events: vec![
                    egui::Event::Text("must not become a draft".into()),
                    key(Key::Enter),
                ],
                ..Default::default()
            },
            |ui| editor.channels(ui, &mut false),
        );
        output.textures_delta.clear();
        let reason = editor
            .history
            .scene()
            .authoring_read_only_reason()
            .unwrap()
            .to_string();
        assert!(
            !channel_text(&output).contains(&reason),
            "the panel no longer prints the reason"
        );
        let help = help_line_text(&mut editor);
        assert!(help.contains(&reason), "{help}");
        assert!(
            hover_texts(&mut editor, &context, text_id).contains(&reason),
            "no reason tooltip on the disabled field"
        );
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        assert!(!context.memory(|memory| memory.has_focus(text_id)));
        assert!(!editor.has_draft());
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn read_only_scene_disables_creation_and_history_controls_with_the_core_reason() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.history_step(false);
    assert!(editor.history.can_redo());
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    // Open replaces the history, so nothing from the previous scene can be undone or redone.
    assert!(!editor.history.can_undo() && !editor.history.can_redo());
    let reason = editor
        .history
        .scene()
        .authoring_read_only_reason()
        .unwrap()
        .to_string();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let output = frame(&mut editor, &ctx, vec![]);
    for label in ["Cube", "Plane", "Undo", "Redo"] {
        assert!(button_disabled(&output, label), "{label} enabled");
    }
    for label in ["Cube", "Plane"] {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
        frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        let at = face_text_position(&output, label);
        // The Channel Box already prints the reason; the hover must add a tooltip copy.
        let count = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == reason)
                })
                .count()
        };
        let baseline = count(&output);
        let mut hovered = output;
        for step in 1..=8 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                time: Some(10.0 + f64::from(step) * 0.25),
                events: vec![egui::Event::PointerMoved(at)],
                ..Default::default()
            };
            input.viewports.entry(egui::ViewportId::ROOT).or_default();
            hovered = ctx.run_ui(input, |ui| editor.render(ui));
            hovered.textures_delta.clear();
        }
        assert_eq!(
            count(&hovered),
            baseline + 1,
            "{label} has no reason tooltip"
        );
    }
    let before = editor.history.scene().clone();
    let output = open_polygon_primitives(&mut editor, &ctx, egui::vec2(1440.0, 900.0));
    for (label, primitive) in menus::POLYGON_PRIMITIVES {
        assert!(submenu_item(&output, label).disabled, "{label} enabled");
        if primitive.is_some() {
            let option_box = format!("{label} option box");
            assert!(
                submenu_item(&output, &option_box).disabled,
                "{option_box} enabled"
            );
        }
    }
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn ascii_outliner_selection_and_frame_use_actual_curve_owner_without_authoring() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ui| editor.outliner(ui));
    output.textures_delta.clear();
    let position = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text.contains("◇  curve") => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap();
    for pressed in [true, false] {
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                ..Default::default()
            },
            |ui| editor.outliner(ui),
        );
        output.textures_delta.clear();
    }
    assert_eq!(
        editor.selection_presentation().0,
        Some(editor.history.scene().nodes()[0].id())
    );
    editor
        .apply_operation(Operation::Pan {
            world_units: [20.0, 10.0],
        })
        .unwrap();
    editor.apply_operation(Operation::FrameSelected).unwrap();
    assert_eq!(editor.camera.target(), glam::Vec3::new(13.5, 0.0, 0.0));
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
    assert!(!editor.is_dirty() && !editor.history.can_undo());
}

#[test]
fn channel_box_keeps_human_state_and_leaves_diagnostics_to_the_help_line() {
    for width in [240.0, 420.0] {
        let context = egui::Context::default();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.name = "pending human draft".into();
        context.memory_mut(|memory| memory.request_focus(name_edit_id()));
        let scene = editor.history.scene().clone();
        let selected = editor.selection_presentation().0;
        let path = editor.selection_presentation().1.to_vec();
        let token = editor.scene_token;
        let mut output = None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 720.0),
                )),
                ..Default::default()
            };
            let mut delete_intent = false;
            let mut frame = context.run_ui(input, |ui| editor.channels(ui, &mut delete_intent));
            frame.textures_delta.clear();
            output = Some(frame);
        }
        let text = channel_text(&output.unwrap());
        assert!(text.contains("Translate X"), "{text}");
        assert!(!text.contains("Direct shapes") && !text.contains("Evaluation unavailable"));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.selection_presentation().0, selected);
        assert_eq!(editor.selection_presentation().1, path);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.name, "pending human draft");
        assert!(context.memory(|memory| memory.has_focus(name_edit_id())));
    }
}

#[test]
fn help_line_summary_is_closed_until_the_scene_diagnostics_header_opens() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    let output = frame(&mut editor, &ctx, vec![]);
    let text = channel_text(&output);
    assert!(text.contains("Scene diagnostics"), "{text}");
    assert!(!text.contains("Direct shapes") && !text.contains("Evaluation unavailable"));
    let header = face_text_position(&output, "Scene diagnostics");
    face_pointer_click(&mut editor, &ctx, header, egui::Modifiers::NONE, vec![]);
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    let text = channel_text(&frame(&mut editor, &ctx, vec![]));
    assert!(text.contains("Direct shapes (1)"), "{text}");
    assert!(text.contains("Evaluation unavailable"), "{text}");
}

#[test]
fn channel_box_lists_unsupported_sections_in_maya_order_without_changing_state() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 1200.0),
            )),
            ..Default::default()
        },
        |ui| editor.channels(ui, &mut false),
    );
    output.textures_delta.clear();
    let y = |label: &str| face_text_position(&output, label).y;
    let order = [
        "Translate X",
        "SHAPES",
        "INPUTS",
        "OUTPUTS",
        "Materials",
        "Layers",
    ];
    for pair in order.windows(2) {
        assert!(y(pair[0]) < y(pair[1]), "{} before {}", pair[0], pair[1]);
    }
    let stubs = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.label()
                .is_some_and(|label| label.ends_with(": not implemented yet"))
        })
        .collect::<Vec<_>>();
    let mut names = stubs
        .iter()
        .map(|(_, node)| node.label().unwrap())
        .collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "INPUTS: not implemented yet",
            "Layers: not implemented yet",
            "OUTPUTS: not implemented yet",
            "SHAPES: not implemented yet",
        ]
    );
    assert!(stubs.iter().all(|(_, node)| node.is_disabled()));
    assert_eq!(editor.history.scene(), &scene);
    assert!(!editor.has_draft());
}

/// Rects of every `text` paint whose whole galley lies inside the clip it was painted with.
pub(in crate::app) fn visible_text_rects(output: &egui::FullOutput, text: &str) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(t) if t.galley.job.text == text => {
                let rect = egui::Rect::from_min_size(t.pos, t.galley.size());
                clipped.clip_rect.contains_rect(rect).then_some(rect)
            }
            _ => None,
        })
        .collect()
}

pub(in crate::app) fn wheel_frames(
    editor: &mut Editor,
    ctx: &egui::Context,
    size: egui::Vec2,
    over: egui::Pos2,
    frames: usize,
) -> egui::FullOutput {
    let mut events = vec![egui::Event::PointerMoved(over)];
    let mut output = egui::FullOutput::default();
    for _ in 0..frames {
        events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        output = frame_with(editor, ctx, size, 1.0, std::mem::take(&mut events));
    }
    output
}

#[test]
fn channel_box_reaches_delete_and_layers_at_the_minimum_window_size() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    ctx.enable_accesskit();
    let size = egui::vec2(960.0, 640.0);
    let output = frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    assert!(!visible_text_rects(&output, "Translate X").is_empty());
    assert!(
        visible_text_rects(&output, "Delete selected").is_empty(),
        "scroll not exercised"
    );
    let camera = editor.camera.clone();
    // Pointer over the right-hand Channel Box panel, below its heading.
    let over_panel = egui::pos2(size.x - 100.0, 300.0);
    let output = wheel_frames(&mut editor, &ctx, size, over_panel, 10);
    assert_eq!(editor.camera, camera, "wheel reached the viewport");
    let delete = visible_text_rects(&output, "Delete selected");
    assert!(!delete.is_empty(), "Delete unreachable");
    assert!(
        !visible_text_rects(&output, "Layers").is_empty(),
        "Layers unreachable"
    );
    let stub = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Layers: not implemented yet"))
        .and_then(|(_, node)| node.bounds())
        .map(|b| {
            egui::Rect::from_min_max(
                egui::pos2(b.x0 as f32, b.y0 as f32),
                egui::pos2(b.x1 as f32, b.y1 as f32),
            )
        })
        .expect("Layers stub");
    // The stub's own label reads the same, so the tooltip is the extra copy that appears on hover.
    let label_copies = visible_text_rects(&output, "Not implemented yet").len();
    let mut tooltip = false;
    for step in 0..8 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            time: Some(10.0 + f64::from(step) * 0.25),
            events: vec![egui::Event::PointerMoved(stub.center())],
            ..Default::default()
        };
        if let Some(viewport) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
            viewport.native_pixels_per_point = Some(1.0);
        }
        let hovered = ctx.run_ui(input, |ui| editor.render(ui));
        tooltip |= visible_text_rects(&hovered, "Not implemented yet").len() > label_copies;
    }
    assert!(tooltip, "Layers stub tooltip not painted");
    let at = delete[0].center();
    frame_with(
        &mut editor,
        &ctx,
        size,
        1.0,
        vec![egui::Event::PointerMoved(at)],
    );
    for pressed in [true, false] {
        frame_with(
            &mut editor,
            &ctx,
            size,
            1.0,
            vec![egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
    assert!(editor.history.scene().nodes().is_empty(), "Delete click");
}

#[test]
fn channel_box_is_blank_with_nothing_selected() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.select(None);
    assert!(editor.single_target().is_err());
    let (text, _) = channel_box_text(&mut editor);
    assert_eq!(text, "Channel Box|Layers|Not implemented yet");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    editor.select(None);
    let (text, _) = channel_box_text(&mut editor);
    for absent in ["Select an object", "Direct shapes", "Curves:", "read-only"] {
        assert!(!text.contains(absent), "{text}");
    }
}

#[test]
fn read_only_scene_is_marked_in_the_title_and_file_menu() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    assert_eq!(editor.document_title(), "AztecCraft | curve.ma (read-only)");
    let reason = editor.read_only_reason().unwrap();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let file = face_text_position(&output, "File");
    let output = face_pointer_click(&mut editor, &ctx, file, egui::Modifiers::NONE, vec![]);
    let marker = face_text_position(&output, &format!("Read-only scene: {reason}"));
    assert!(marker.y < face_text_position(&output, "Close").y);

    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert_eq!(editor.document_title(), "AztecCraft | Untitled *");
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let file = face_text_position(&output, "File");
    let output = face_pointer_click(&mut editor, &ctx, file, egui::Modifiers::NONE, vec![]);
    face_text_position(&output, "Close");
    assert!(!output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Text(text) if text.galley.job.text.starts_with("Read-only scene")
    )));
}

#[test]
fn unsaved_prompt_for_an_untitled_scene_uses_mayas_wording_and_buttons() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    for text in [
        "Warning: Scene Not Saved",
        "Save changes to untitled scene?",
    ] {
        face_text_position(&output, text);
    }
    let buttons: Vec<f32> = ["Save", "Don't Save", "Cancel"]
        .iter()
        .map(|label| face_text_position(&output, label).x)
        .collect();
    assert!(
        buttons.windows(2).all(|pair| pair[0] < pair[1]),
        "{buttons:?}"
    );
    // An untitled scene cannot be saved in place yet (Maya would Save As), so Save is inert.
    let before = editor.history.scene().clone();
    let save = face_text_position(&output, "Save");
    face_pointer_click(&mut editor, &ctx, save, egui::Modifiers::NONE, vec![]);
    assert!(editor.files.is_modal());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn help_line_carries_the_read_only_reason_and_curves_note() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let reason = editor.read_only_reason().unwrap();
    let text = help_line_text(&mut editor);
    assert!(text.contains(&reason) && text.contains("Curves:"), "{text}");
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let text = help_line_text(&mut editor);
    assert!(
        !text.contains("Curves:") && text.contains("Scene diagnostics"),
        "{text}"
    );
}

#[test]
fn camera_nonfinite_operations_and_native_draft_failure_preserve_their_baselines() {
    let context = egui::Context::default();
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let camera = editor.camera.clone();
    let before = editor.history.scene().clone();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for operation in [
            Operation::Orbit {
                radians: [value, 0.0],
            },
            Operation::Orbit {
                radians: [0.0, value],
            },
            Operation::Pan {
                world_units: [value, 0.0],
            },
            Operation::Pan {
                world_units: [0.0, value],
            },
            Operation::Dolly { factor: value },
        ] {
            assert!(matches!(
                editor.remote_operation(
                    RemoteRequest {
                        expected_scene_token: Some(editor.scene_token),
                        operation,
                    },
                    &context
                ),
                Err(OperationError::Camera(_))
            ));
            assert_eq!(editor.camera, camera);
            assert_eq!(editor.history.scene(), &before);
        }
    }
    editor.name = "committed_before_navigation".into();
    editor.native_operation(Operation::Dolly { factor: -1.0 });
    assert_eq!(editor.camera, camera);
    assert_eq!(
        editor.history.scene().nodes()[0].name(),
        "committed_before_navigation"
    );
    assert!(!editor.has_draft());
    assert!(editor.error);
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &before);
    editor.native_operation(Operation::Dolly { factor: 0.5 });
    assert_ne!(editor.status, "Scene edited");
    assert!(editor.history.can_redo());
}

#[test]
fn native_hidden_close_checks_pending_input_without_mcp() {
    for pending in [false, true] {
        let context = egui::Context::default();
        let mut editor = Editor::default();
        let mut input = egui::RawInput::default();
        if pending {
            input.events.push(egui::Event::Text("pending".into()));
        }
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.minimized = Some(true);
        viewport.events.push(egui::ViewportEvent::Close);
        eframe::App::raw_input_hook(&mut editor, &context, &mut input);
        let mut native_frame = eframe::Frame::_new_kittest();
        let output = context.run_logic(&input, |ctx| {
            eframe::App::logic(&mut editor, ctx, &mut native_frame)
        });
        let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        assert_eq!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close)),
            !pending
        );
    }
}

pub(in crate::app) fn remote(
    editor: &mut Editor,
    operation: Operation,
) -> Result<OperationReply, OperationError> {
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation,
        },
        &egui::Context::default(),
    )
}

#[test]
fn remote_successes_report_on_the_status_line_like_native_ones() {
    let mut native = Editor::default();
    let mut remote_editor = Editor::default();
    let initial = native.status.clone();
    assert_eq!(remote_editor.status, initial);
    let steps: Vec<fn(&Editor) -> Operation> = vec![
        |_| Operation::Create {
            primitive: Primitive::Cube,
            name: None,
            options: None,
        },
        |editor| Operation::Rename {
            id: editor.selected().unwrap(),
            name: "renamed".into(),
        },
        |editor| Operation::Rename {
            id: editor.selected().unwrap(),
            name: "renamed".into(),
        },
        |editor| Operation::Rename {
            id: editor.selected().unwrap(),
            name: String::new(),
        },
        |_| Operation::Undo,
        |_| Operation::Redo,
        |_| Operation::SetGridVisible { visible: false },
        |_| Operation::SetActiveTool(ActiveTool::Rotate),
    ];
    let mut failures = 0;
    for step in steps {
        let operation = step(&native);
        native.native_operation(operation);
        let operation = step(&remote_editor);
        let before = (remote_editor.status.clone(), remote_editor.error);
        if remote(&mut remote_editor, operation).is_ok() {
            assert!(!native.error);
            assert_eq!(
                (&remote_editor.status, remote_editor.error),
                (&native.status, native.error)
            );
        } else {
            // The caller gets the error; the native status line keeps its baseline.
            assert!(native.error);
            assert_eq!((remote_editor.status.clone(), remote_editor.error), before);
            failures += 1;
        }
    }
    assert_eq!(
        failures, 1,
        "the empty rename must exercise the failure path"
    );
    assert_ne!(native.status, initial);
    // Remote manipulations leave the preview status to native drags and report the result with
    // the native release and Esc wording.
    remote(
        &mut remote_editor,
        Operation::SetActiveTool(ActiveTool::Move),
    )
    .unwrap();
    for (finish, expected) in [
        (
            azteccraft::history::ManipulationFinish::Cancel,
            "Edit cancelled",
        ),
        (
            azteccraft::history::ManipulationFinish::Commit,
            "Move committed",
        ),
    ] {
        let before = remote_editor.status.clone();
        let target = remote_editor.selected_move_target().unwrap();
        remote(
            &mut remote_editor,
            Operation::BeginMove {
                target,
                axis: azteccraft::native_scene::WorldAxis::X,
            },
        )
        .unwrap();
        let id = remote_editor.history.active_manipulation().unwrap().id;
        remote(
            &mut remote_editor,
            Operation::UpdateMove {
                id,
                distance: crate::editor_operations::MoveDistance::Centimeters(2.0),
            },
        )
        .unwrap();
        assert_eq!(remote_editor.status, before);
        remote(&mut remote_editor, Operation::FinishMove { id, finish }).unwrap();
        assert_eq!(
            (remote_editor.status.as_str(), remote_editor.error),
            (expected, false)
        );
    }
    // A request refused by the remote guards never reaches the scene or the status line.
    let before = (remote_editor.status.clone(), remote_editor.error);
    let refused = remote_editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(uuid::Uuid::new_v4()),
            operation: Operation::Undo,
        },
        &egui::Context::default(),
    );
    assert!(matches!(refused, Err(OperationError::StaleScene)));
    assert_eq!((remote_editor.status, remote_editor.error), before);
}

#[test]
fn grid_setter_is_idempotent_and_preserves_scene_history_and_editor_baseline() {
    let mut editor = Editor::default();
    assert!(editor.grid_visible);
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "renamed".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    assert!(editor.history.can_redo());
    let scene = editor.history.scene().clone();
    let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let fields = (editor.name.clone(), editor.channels.clone());
    let status = (editor.status.clone(), editor.error);
    for (visible, changed) in [(true, false), (false, true), (false, false), (true, true)] {
        let reply = remote(&mut editor, Operation::SetGridVisible { visible }).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), changed);
        assert_eq!(matches!(reply, OperationReply::Unchanged), !changed);
        let mut actual = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        assert_eq!(actual["grid_visible"], visible);
        actual["grid_visible"] = serde_json::Value::Bool(true);
        assert_eq!(actual, before);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!((editor.name.clone(), editor.channels.clone()), fields);
        assert_eq!((editor.status.clone(), editor.error), status);
    }
}

#[test]
fn shading_setter_is_idempotent_session_only_and_neither_dirties_nor_changes_the_token() {
    use crate::editor_operations::ShadingMode;
    let mut editor = Editor::default();
    assert_eq!(editor.shading, ShadingMode::SmoothShadeAll);
    assert!(!editor.is_dirty());
    let token = editor.scene_token;
    let scene = editor.history.scene().clone();
    let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert_eq!(before["shading"], "smooth_shade_all");
    let status = (editor.status.clone(), editor.error);
    for (mode, changed) in [
        (ShadingMode::SmoothShadeAll, false),
        (ShadingMode::Wireframe, true),
        (ShadingMode::Wireframe, false),
        (ShadingMode::SmoothShadeAll, true),
    ] {
        let reply = remote(&mut editor, Operation::SetShadingMode(mode)).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), changed);
        assert_eq!(matches!(reply, OperationReply::Unchanged), !changed);
        assert_eq!(editor.shading, mode);
        let mut actual = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        assert_eq!(actual["shading"], serde_json::to_value(mode).unwrap());
        actual["shading"] = before["shading"].clone();
        assert_eq!(actual, before);
        assert!(!editor.is_dirty());
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!((editor.status.clone(), editor.error), status);
    }
}

#[test]
fn shading_refuses_modal_and_closing_and_survives_new_scene() {
    use crate::editor_operations::ShadingMode;
    for closing in [false, true] {
        let mut editor = Editor::default();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        }
        assert!(matches!(
            editor.apply_operation(Operation::SetShadingMode(ShadingMode::Wireframe)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.shading, ShadingMode::SmoothShadeAll);
    }
    let mut editor = Editor::default();
    remote(
        &mut editor,
        Operation::SetShadingMode(ShadingMode::Wireframe),
    )
    .unwrap();
    remote(&mut editor, Operation::NewScene).unwrap();
    assert_eq!(editor.shading, ShadingMode::Wireframe);
}

#[test]
fn num4_and_num5_select_shading_and_num6_num7_only_set_status() {
    use crate::editor_operations::ShadingMode;
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    let token = editor.scene_token;
    frame(&mut editor, &ctx, vec![key(Key::Num4)]);
    assert_eq!(editor.shading, ShadingMode::Wireframe);
    let shifted = egui::Event::Key {
        key: Key::Num5,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    let released = egui::Event::Key {
        key: Key::Num5,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut editor, &ctx, vec![shifted, released]);
    assert_eq!(editor.shading, ShadingMode::Wireframe);
    for (pressed, name) in [
        (Key::Num6, "Hardware Texturing (6) is not implemented yet"),
        (Key::Num7, "Use All Lights (7) is not implemented yet"),
    ] {
        frame(&mut editor, &ctx, vec![key(pressed)]);
        assert_eq!(editor.shading, ShadingMode::Wireframe);
        assert_eq!(editor.status, name);
        assert!(!editor.error && !editor.is_dirty());
        assert_eq!(editor.scene_token, token);
    }
    frame(&mut editor, &ctx, vec![key(Key::Num5)]);
    assert_eq!(editor.shading, ShadingMode::SmoothShadeAll);
    assert!(!editor.is_dirty());
    assert_eq!(editor.scene_token, token);
}

#[test]
fn shading_menu_textured_row_is_disabled_with_the_hotkey_reason_and_inert() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let menu = face_text_position(&output, "Shading");
    let output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let label = "Hardware Texturing (6)";
    assert!(button_disabled(&output, label));
    let row = face_text_position(&output, label);
    let before = (
        editor.shading,
        editor.is_dirty(),
        editor.scene_token,
        editor.history.can_undo(),
    );
    ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let mut hovered = output;
    for step in 1..=8 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 900.0),
            )),
            time: Some(10.0 + f64::from(step) * 0.25),
            events: vec![egui::Event::PointerMoved(row)],
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default();
        hovered = ctx.run_ui(input, |ui| editor.render(ui));
        hovered.textures_delta.clear();
    }
    face_text_position(&hovered, "Hardware Texturing (6) is not implemented yet");
    face_pointer_click(&mut editor, &ctx, row, egui::Modifiers::NONE, vec![]);
    assert_eq!(
        before,
        (
            editor.shading,
            editor.is_dirty(),
            editor.scene_token,
            editor.history.can_undo()
        )
    );
}

#[test]
fn display_menu_shows_grid_options_disabled_with_the_default_grid_reason() {
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let menu = face_text_position(&output, "Display");
    let output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let row = face_text_position(&output, "Grid Options");
    // The Channel Box on the right also carries unsupported stubs; the menu row is the left one.
    let stub = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if text.galley.job.text == "Not implemented yet" && text.pos.x < 1100.0 =>
            {
                Some(text.pos)
            }
            _ => None,
        })
        .expect("menu stub row");
    assert!(stub.y < row.y);
    let before = (editor.grid_visible, editor.history.scene().clone());
    face_pointer_click(&mut editor, &ctx, row, egui::Modifiers::NONE, vec![]);
    assert_eq!(
        (editor.grid_visible, editor.history.scene().clone()),
        before
    );
    assert!(super::menus::GRID_OPTIONS_REASON.contains("+/-12 units (PROVISIONAL)"));
}

/// Maya 2025.1's SelectNone runs `changeSelectMode -component; changeSelectMode -object;
/// select -cl`, so Deselect All also leaves component and hierarchy modes.
#[test]
fn deselect_all_clears_object_and_components_and_returns_to_object_mode() {
    let changed = |reply: Result<OperationReply, OperationError>| {
        matches!(reply.unwrap(), OperationReply::Changed)
    };
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let cap = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    remote(&mut editor, Operation::SelectFace { target: Some(cap) }).unwrap();
    assert!(changed(remote(&mut editor, Operation::DeselectAll)));
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.selection_presentation().0, None);
    assert!(editor.remembered_face.is_none());
    assert!(!changed(remote(&mut editor, Operation::DeselectAll)));
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Hierarchy),
    )
    .unwrap();
    assert!(changed(remote(&mut editor, Operation::DeselectAll)));
    assert_eq!(editor.selection_mode, SelectionMode::Object);
}

#[test]
fn alt_d_deselects_all_and_other_d_chords_do_not() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let mac_cmd = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..Default::default()
    };
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::CTRL,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT | egui::Modifiers::SHIFT,
        // AltGr arrives as Ctrl+Alt on Windows; Alt+Cmd is another chord on macOS.
        egui::Modifiers::ALT | egui::Modifiers::CTRL,
        egui::Modifiers::ALT | mac_cmd,
    ] {
        shortcuts_events(
            &mut editor,
            modifiers,
            vec![key_event(Key::D, modifiers, false)],
        );
        assert!(editor.selection_presentation().0.is_some(), "{modifiers:?}");
    }
    let alt = egui::Modifiers::ALT;
    let ctx = egui::Context::default();
    shortcuts_in(&ctx, &mut editor, alt, vec![key_event(Key::D, alt, false)]);
    assert_eq!(editor.selection_presentation().0, None);
    // Holding the chord repeats nothing: the key is already down in this context.
    editor.select(Some(id));
    shortcuts_in(&ctx, &mut editor, alt, vec![key_event(Key::D, alt, true)]);
    assert_eq!(editor.selection_presentation().0, Some(id));
}

#[test]
fn ctrl_shift_a_reports_that_select_all_is_not_implemented_325() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let selected = editor.selection_presentation().0;
    let chord = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    shortcuts_events(&mut editor, chord, vec![key_event(Key::A, chord, false)]);
    assert_eq!(editor.status, super::menus::SELECT_ALL_STATUS);
    for text in [
        super::menus::SELECT_ALL_STATUS,
        super::menus::MULTI_SELECTION_REASON,
    ] {
        assert!(text.contains("#325") && !text.contains("#166"), "{text}");
    }
    assert!(!editor.error);
    assert_eq!(editor.selection_presentation().0, selected);
}

#[test]
fn select_menu_rows_follow_maya_order_and_only_deselect_all_acts() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let menu = face_text_position(&output, "Select");
    let mut output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let menu_is_open = |output: &egui::FullOutput| {
        output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.job.text == "Deselect All   Alt D")
        })
    };
    let rows = [
        "All   Ctrl/Cmd Shift A",
        "All by Type",
        "Deselect All   Alt D",
        "Hierarchy",
        "Inverse   Ctrl/Cmd Shift I",
        "Object / Component   F8",
    ];
    let ys: Vec<f32> = rows
        .iter()
        .map(|label| face_text_position(&output, label).y)
        .collect();
    assert!(
        ys.windows(2).all(|pair| pair[0] < pair[1]),
        "rows out of order: {ys:?}"
    );
    for label in [
        "All   Ctrl/Cmd Shift A",
        "All by Type",
        "Hierarchy",
        "Inverse   Ctrl/Cmd Shift I",
    ] {
        assert!(button_disabled(&output, label), "{label} must be disabled");
    }
    for label in [
        "All   Ctrl/Cmd Shift A",
        "All by Type",
        "Hierarchy",
        "Inverse   Ctrl/Cmd Shift I",
    ] {
        if !menu_is_open(&output) {
            let menu = face_text_position(&output, "Select");
            output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
        }
        output = face_pointer_click(
            &mut editor,
            &ctx,
            face_text_position(&output, label),
            egui::Modifiers::NONE,
            vec![],
        );
        assert!(editor.selection_presentation().0.is_some(), "{label}");
    }
    if !menu_is_open(&output) {
        let menu = face_text_position(&output, "Select");
        output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    }
    let deselect = face_text_position(&output, "Deselect All   Alt D");
    face_pointer_click(&mut editor, &ctx, deselect, egui::Modifiers::NONE, vec![]);
    assert_eq!(editor.selection_presentation().0, None);
}

#[test]
fn shading_menu_rows_follow_the_maya_2026_panel_menu_order() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let menu = face_text_position(&output, "Shading");
    let output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let rows = [
        "Wireframe (4)",
        "Smooth Shade All (5)",
        "Use default material",
        "Wireframe on Shaded",
        "Hardware Texturing (6)",
    ];
    let ys: Vec<f32> = rows
        .iter()
        .map(|label| face_text_position(&output, label).y)
        .collect();
    assert!(
        ys.windows(2).all(|pair| pair[0] < pair[1]),
        "rows out of order: {ys:?}"
    );
}

#[test]
fn typing_digits_into_focused_fields_does_not_change_shading() {
    use crate::editor_operations::ShadingMode;
    for text_id in [name_edit_id(), channel_edit_id(0)] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor
            .apply_operation(Operation::SetShadingMode(ShadingMode::Wireframe))
            .unwrap();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(text_id));
        frame(&mut editor, &ctx, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(text_id));
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::Text("5".into()),
                key(Key::Num5),
                key(Key::Num4),
            ],
        );
        assert_eq!(editor.shading, ShadingMode::Wireframe);
        assert!(ctx.egui_wants_keyboard_input());
    }
}

#[test]
fn grid_native_draft_policy_and_remote_draft_refusal_share_the_typed_setter() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    editor.channels[0] = "invalid".into();
    assert!(matches!(
        editor.apply_operation(Operation::SetGridVisible { visible: false }),
        Err(OperationError::InvalidInput(_))
    ));
    assert!(editor.grid_visible);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.channels[0], "invalid");
    for draft in ["2", "invalid"] {
        editor.channels[0] = draft.into();
        assert!(matches!(
            remote(&mut editor, Operation::SetGridVisible { visible: false }),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.channels[0], draft);
        assert!(editor.grid_visible);
    }
    editor.channels[0] = "2".into();
    editor.native_operation(Operation::SetGridVisible { visible: false });
    assert!(!editor.grid_visible);
    assert!(!editor.has_draft());
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        2.0
    );
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &scene);
    assert!(!editor.grid_visible);
}

#[test]
fn grid_native_modal_and_close_refuse_before_committing_drafts() {
    for closing in [false, true] {
        let mut editor = Editor::default();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        }
        editor.name = "pending draft".into();
        let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        let scene = editor.history.scene().clone();
        assert!(matches!(
            editor.apply_operation(Operation::SetGridVisible { visible: false }),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(
            serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
            before
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.name, "pending draft");
    }
}

#[test]
fn grid_session_setting_survives_successful_and_canceled_new_and_open() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.aztec");
    azteccraft::document::save_new(&path, &azteccraft::scene::Scene::default()).unwrap();
    let mut editor = Editor::default();
    remote(&mut editor, Operation::SetGridVisible { visible: false }).unwrap();
    for open in [false, true] {
        let operation = if open {
            Operation::OpenScene { path: path.clone() }
        } else {
            Operation::NewScene
        };
        assert!(matches!(
            remote(&mut editor, operation).unwrap(),
            OperationReply::DocumentApplied { .. }
        ));
        assert!(!editor.grid_visible);
        editor.create(Primitive::Cube);
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        let operation = if open {
            Operation::OpenScene { path: path.clone() }
        } else {
            Operation::NewScene
        };
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = remote(&mut editor, operation).unwrap()
        else {
            panic!("expected unsaved confirmation");
        };
        remote(
            &mut editor,
            Operation::ResolveFileAction {
                pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                decision: crate::editor_operations::FileDecision::Cancel,
            },
        )
        .unwrap();
        assert!(!editor.grid_visible);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &scene);
        editor.history_step(false);
    }
}

#[test]
fn sparse_nonfinite_intents_preserve_scene_fields_and_redo() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let before = editor.history.scene().clone();
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "temporary".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    let fields = (editor.name.clone(), editor.channels.clone());
    for group in 0..3 {
        let mut components = [[None; 3]; 3];
        components[group][0] = Some(f64::NAN);
        assert!(
            remote(
                &mut editor,
                Operation::EditNode {
                    id,
                    name: Some("must_not_apply".into()),
                    translation: components[0],
                    rotation_degrees: components[1],
                    scale: components[2]
                }
            )
            .is_err()
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
        assert!(editor.history.can_redo());
    }
}

fn sidebar_op(visible: bool) -> Operation {
    Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ChannelBox,
        visible,
    }
}

#[test]
fn sidebar_setter_is_idempotent_and_preserves_scene_history_and_editor_baseline() {
    let mut editor = Editor::default();
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "renamed".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    assert!(editor.history.can_redo());
    let scene = editor.history.scene().clone();
    let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let fields = (editor.name.clone(), editor.channels.clone());
    let status = (editor.status.clone(), editor.error);
    let token = editor.scene_token;
    for (visible, changed) in [(true, false), (false, true), (false, false), (true, true)] {
        let reply = remote(&mut editor, sidebar_op(visible)).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), changed);
        assert_eq!(matches!(reply, OperationReply::Unchanged), !changed);
        assert_eq!(
            editor.sidebar_dock,
            visible.then_some(SidebarPanel::ChannelBox)
        );
        let mut actual = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        assert_eq!(actual["sidebar_panels"]["channel_box"], visible);
        actual["sidebar_panels"]["channel_box"] = serde_json::Value::Bool(true);
        assert_eq!(actual, before);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!((editor.name.clone(), editor.channels.clone()), fields);
        assert_eq!((editor.status.clone(), editor.error), status);
        assert_eq!(editor.scene_token, token);
    }
}

#[test]
fn sidebar_visible_true_activates_the_panel_and_false_only_hides_the_active_one() {
    // ChannelBox is the only panel, so hiding a non-active panel is reachable only with the
    // dock already hidden, and Unchanged arises only from a repeat.
    let mut editor = Editor {
        sidebar_dock: None,
        ..Editor::default()
    };
    assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert!(matches!(
        editor.apply_operation(sidebar_op(false)),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.sidebar_dock, None);
    assert!(matches!(
        editor.apply_operation(sidebar_op(true)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.sidebar_dock, Some(SidebarPanel::ChannelBox));
    assert!(matches!(
        editor.apply_operation(sidebar_op(false)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.sidebar_dock, None);
}

#[test]
fn sidebar_native_draft_policy_and_remote_draft_refusal_share_the_typed_setter() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    editor.channels[0] = "invalid".into();
    assert!(matches!(
        editor.apply_operation(sidebar_op(false)),
        Err(OperationError::InvalidInput(_))
    ));
    editor.native_operation(sidebar_op(false));
    assert!(editor.error);
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.channels[0], "invalid");
    for draft in ["2", "invalid"] {
        editor.channels[0] = draft.into();
        assert!(matches!(
            remote(&mut editor, sidebar_op(false)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.channels[0], draft);
        assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    }
    editor.channels[0] = "2".into();
    editor.native_operation(sidebar_op(false));
    assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert!(!editor.has_draft());
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        2.0
    );
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &scene);
    assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
}

#[test]
fn sidebar_native_toggle_reports_no_scene_edit_status() {
    let mut editor = Editor {
        status: "sentinel".into(),
        ..Editor::default()
    };
    editor.native_operation(sidebar_op(false));
    assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert_eq!(editor.status, "sentinel");
    editor.native_operation(sidebar_op(false));
    assert_eq!(editor.status, "sentinel");
}

#[test]
fn sidebar_remote_refuses_stale_token_text_focus_modal_and_close_with_baseline_intact() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let stale = editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(Uuid::new_v4()),
            operation: sidebar_op(false),
        },
        &egui::Context::default(),
    );
    assert!(matches!(stale, Err(OperationError::StaleScene)));
    assert_eq!(
        serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
        before
    );
    let context = egui::Context::default();
    context.memory_mut(|memory| memory.request_focus(egui::Id::new("node_name")));
    frame(&mut editor, &context, vec![]);
    assert!(text_edit_focused(&context));
    let focused = editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation: sidebar_op(false),
        },
        &context,
    );
    assert!(matches!(focused, Err(OperationError::BusyEditing)));
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let modal = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert!(matches!(
        remote(&mut editor, sidebar_op(false)),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(
        serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
        modal
    );
    let mut closing = Editor::default();
    closing.close_document().unwrap();
    assert!(matches!(
        remote(&mut closing, sidebar_op(false)),
        Err(OperationError::BusyEditing)
    ));
    assert!(closing.sidebar_panel_visible(SidebarPanel::ChannelBox));
}

#[test]
fn sidebar_native_modal_and_close_refuse_before_committing_drafts() {
    for closing in [false, true] {
        let mut editor = Editor::default();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        }
        editor.name = "pending draft".into();
        let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        let scene = editor.history.scene().clone();
        assert!(matches!(
            editor.apply_operation(sidebar_op(false)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(
            serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
            before
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.name, "pending draft");
    }
}

#[test]
fn sidebar_session_setting_survives_successful_and_canceled_new_and_open() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("empty.aztec");
    azteccraft::document::save_new(&path, &azteccraft::scene::Scene::default()).unwrap();
    let mut editor = Editor::default();
    remote(&mut editor, sidebar_op(false)).unwrap();
    for open in [false, true] {
        let operation = if open {
            Operation::OpenScene { path: path.clone() }
        } else {
            Operation::NewScene
        };
        assert!(matches!(
            remote(&mut editor, operation).unwrap(),
            OperationReply::DocumentApplied { .. }
        ));
        assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        editor.create(Primitive::Cube);
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        let operation = if open {
            Operation::OpenScene { path: path.clone() }
        } else {
            Operation::NewScene
        };
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = remote(&mut editor, operation).unwrap()
        else {
            panic!("expected unsaved confirmation");
        };
        remote(
            &mut editor,
            Operation::ResolveFileAction {
                pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                decision: crate::editor_operations::FileDecision::Cancel,
            },
        )
        .unwrap();
        assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &scene);
        editor.history_step(false);
    }
}

#[test]
fn remote_operations_share_native_scene_selection_history_and_camera() {
    let mut editor = Editor::default();
    let OperationReply::Created { node_id } = remote(
        &mut editor,
        Operation::Create {
            primitive: Primitive::Cube,
            options: None,
            name: Some("remote_cube".into()),
        },
    )
    .unwrap() else {
        panic!("expected created object")
    };
    let id = NodeId::from_decimal(&node_id).unwrap();
    assert_eq!(editor.selection_presentation().0, Some(id));
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "edited".into(),
        },
    )
    .unwrap();
    let transform = NativeTransform {
        translation: [4.0, 2.0, 1.0],
        ..Default::default()
    };
    let input = editor.transform_input(transform).unwrap();
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SetTransform {
                id,
                transform: input
            }
        )
        .unwrap(),
        OperationReply::Changed
    ));
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SetTransform {
                id,
                transform: input
            }
        )
        .unwrap(),
        OperationReply::Unchanged
    ));
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert!(editor.camera.target().distance(Vec3::new(4.0, 2.0, 1.0)) < 0.01);
    remote(
        &mut editor,
        Operation::Select {
            node: None,
            instance_path: None,
        },
    )
    .unwrap();
    assert_eq!(editor.selection_presentation().0, None);
    remote(
        &mut editor,
        Operation::Select {
            node: Some(id),
            instance_path: None,
        },
    )
    .unwrap();
    assert_eq!(editor.name, "edited");
    editor.history_step(false);
    assert_eq!(
        editor.history.scene().node(id).unwrap().transform(),
        NativeTransform::default()
    );
    remote(&mut editor, Operation::Redo).unwrap();
    assert_eq!(
        editor.history.scene().node(id).unwrap().transform(),
        transform
    );
    editor.channels[0] = "6".into();
    editor.create(Primitive::Plane);
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        6.0
    );
    let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
    else {
        panic!("expected inspection")
    };
    assert_eq!(inspection.nodes.len(), 2);
    assert_eq!(inspection.nodes[0].id, node_id);
    assert_eq!(inspection.nodes[0].transform.translation[0], 6.0);
    assert!(inspection.dirty && inspection.can_undo);
    assert!(!inspection.has_draft);
    remote(&mut editor, Operation::Delete(id)).unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.history.scene().node(id).unwrap().name(), "edited");
}

#[test]
fn remote_busy_and_stale_denials_preserve_human_state() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let before = editor.history.scene().clone();
    let camera = editor.camera.eye();
    for draft in ["2", "nan"] {
        editor.channels[0] = draft.into();
        assert!(matches!(
            remote(&mut editor, Operation::Delete(id)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.channels[0], draft);
        assert_eq!(editor.selection_presentation().0, Some(id));
        assert_eq!(editor.camera.eye(), camera);
        assert!(editor.history.can_undo());
        assert!(!editor.history.can_redo());
    }
    let old = Uuid::new_v4();
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(old),
                operation: Operation::Undo,
            },
            &egui::Context::default()
        ),
        Err(OperationError::StaleScene)
    ));
    assert_eq!(editor.channels[0], "nan");
    editor.sync_fields();
    let ctx = egui::Context::default();
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("node_name")));
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation: Operation::Delete(id),
            },
            &ctx
        ),
        Err(OperationError::BusyEditing)
    ));
    editor.request_new();
    assert!(matches!(
        remote(&mut editor, Operation::Undo),
        Err(OperationError::BusyEditing)
    ));
    let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
    else {
        panic!("expected inspection")
    };
    assert!(inspection.modal);
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn remote_delete_refuses_component_selection_modes() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    for (mode, noun) in [
        (SelectionMode::Face, "Face"),
        (SelectionMode::Vertex, "Vertex"),
        (SelectionMode::Edge, "Edge"),
    ] {
        remote(&mut editor, Operation::SetSelectionMode(mode)).unwrap();
        assert_eq!(editor.selection_mode, mode);
        let before = editor.history.scene().clone();
        let can_undo = editor.history.can_undo();
        assert!(matches!(
            remote(&mut editor, Operation::Delete(id)),
            Err(OperationError::InvalidInput(message))
                if message
                    == format!("{noun} deletion is unsupported; switch to Object or Hierarchy selection")
        ));
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.scene().node(id).is_ok());
        assert_eq!(editor.history.can_undo(), can_undo);
    }
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Object),
    )
    .unwrap();
    remote(&mut editor, Operation::Delete(id)).unwrap();
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn remote_errors_and_noop_do_not_change_selection_or_history() {
    let mut editor = Editor::default();
    assert!(matches!(
        remote(&mut editor, Operation::Undo).unwrap(),
        OperationReply::Unchanged
    ));
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::NoSelection)
    ));
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let before = editor.history.scene().clone();
    let missing = NodeId::from_decimal("999").unwrap();
    assert!(matches!(
        remote(
            &mut editor,
            Operation::Select {
                node: Some(missing),
                instance_path: None
            }
        ),
        Err(OperationError::Scene(_))
    ));
    assert!(matches!(
        remote(
            &mut editor,
            Operation::Rename {
                id,
                name: "".into()
            }
        ),
        Err(OperationError::Scene(_))
    ));
    let transform = TransformInput {
        scale: [f64::INFINITY; 3],
        translation: [0.0; 3],
        rotation_degrees: [0.0; 3],
    };
    assert!(matches!(
        remote(&mut editor, Operation::SetTransform { id, transform }),
        Err(OperationError::Scene(_))
    ));
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.selection_presentation().0, Some(id));
    editor.history_step(false);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn remote_guard_distinguishes_rendered_buttons_from_name_and_channel_text_focus() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        editor.render(ui);
        ui.button("Focusable native control").request_focus();
    });
    output.textures_delta.clear();
    assert!(ctx.memory(|memory| memory.focused().is_some()));
    let OperationReply::Scene { inspection } = editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: None,
                operation: Operation::Inspect,
            },
            &ctx,
        )
        .unwrap()
    else {
        panic!("expected inspection")
    };
    assert!(!inspection.text_focused);
    editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation: Operation::Rename {
                    id,
                    name: "permitted".into(),
                },
            },
            &ctx,
        )
        .unwrap();
    assert_eq!(editor.history.scene().node(id).unwrap().name(), "permitted");
    let before = editor.history.scene().clone();
    for text_id in [name_edit_id(), channel_edit_id(0)] {
        ctx.memory_mut(|memory| memory.request_focus(text_id));
        frame(&mut editor, &ctx, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(text_id));
        let OperationReply::Scene { inspection } = editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: None,
                    operation: Operation::Inspect,
                },
                &ctx,
            )
            .unwrap()
        else {
            panic!("expected inspection")
        };
        assert!(inspection.text_focused);
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(editor.scene_token),
                    operation: Operation::Undo,
                },
                &ctx
            ),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn typed_extrusion_uses_native_history_and_preserves_pending_edits_on_error() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let shape = editor.history.scene().shapes()[0].id();
    editor.channels[0] = "2".into();
    let target = FaceTarget {
        transform: id,
        shape,
        path: vec![
            editor.history.scene().node(id).unwrap().transform_id(),
            shape,
        ],
        face_index: 0,
    };
    editor.native_operation(Operation::ExtrudeFace {
        target: target.clone(),
        distance: 0.0,
    });
    assert!(!editor.error);
    assert_eq!(
        editor
            .history
            .scene()
            .shape(shape)
            .unwrap()
            .mesh()
            .point_count(),
        12
    );
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        2.0
    );
    editor.history_step(false);
    assert_eq!(
        editor
            .history
            .scene()
            .shape(shape)
            .unwrap()
            .mesh()
            .point_count(),
        8
    );
    let before = editor.history.scene().clone();
    editor.channels[0] = "nan".into();
    editor.native_operation(Operation::ExtrudeFace {
        target: target.clone(),
        distance: 1.0,
    });
    assert!(editor.error);
    assert_eq!(editor.channels[0], "nan");
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.can_redo());
    editor.sync_fields();
    editor.native_operation(Operation::ExtrudeFace {
        target: target.clone(),
        distance: f64::NAN,
    });
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.can_redo());
    editor.native_operation(Operation::ExtrudeFace {
        target: FaceTarget {
            face_index: 99,
            ..target
        },
        distance: 1.0,
    });
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.can_redo());
    editor.history_step(true);
    assert_eq!(
        editor
            .history
            .scene()
            .shape(shape)
            .unwrap()
            .mesh()
            .point_count(),
        12
    );
}

fn key(key: Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    frame_with(editor, ctx, egui::vec2(1440.0, 900.0), 1.0, events)
}

/// `size` is in points; `ppp` is the native pixels per point reported to egui.
pub(in crate::app) fn frame_with(
    editor: &mut Editor,
    ctx: &egui::Context,
    size: egui::Vec2,
    ppp: f32,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    };
    if let Some(viewport) = input.viewports.get_mut(&egui::ViewportId::ROOT) {
        viewport.native_pixels_per_point = Some(ppp);
    }
    let mut output = ctx.run_ui(input, |ui| editor.render(ui));
    output.textures_delta.clear();
    output
}

/// A control inside the Create > Polygon Primitives popup, of any role (checkboxes included).
pub(in crate::app) struct SubmenuItem {
    pub(in crate::app) label: String,
    pub(in crate::app) rect: egui::Rect,
    pub(in crate::app) disabled: bool,
}

/// Controls to the right of the "Polygon Primitives" button, so the shelf's Cube and Plane never match.
pub(in crate::app) fn polygon_submenu_items(output: &egui::FullOutput) -> Vec<SubmenuItem> {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled");
    let nodes: Vec<SubmenuItem> = update
        .nodes
        .iter()
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(SubmenuItem {
                label: node.label()?.to_owned(),
                rect: egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
            })
        })
        .collect();
    let Some(parent) = nodes
        .iter()
        .find(|item| is_polygon_primitives_button(&item.label))
    else {
        return Vec::new();
    };
    let right = parent.rect.right();
    // The popup's rows share one left edge, which no panel control shares; option boxes sit to their right.
    let Some(anchor) = nodes
        .iter()
        .find(|item| item.label == "Sphere" && item.rect.left() >= right)
    else {
        return Vec::new();
    };
    let left = anchor.rect.left();
    let in_popup = |item: &SubmenuItem| {
        (item.rect.left() - left).abs() < 2.0
            || (item.label.ends_with("option box") && item.rect.left() > left)
    };
    nodes.into_iter().filter(in_popup).collect()
}

/// True once the whole Create popup chain is gone: neither the submenu button nor its rows remain.
pub(in crate::app) fn create_menu_closed(output: &egui::FullOutput) -> bool {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    polygon_submenu_items(output).is_empty()
        && !update
            .nodes
            .iter()
            .any(|(_, node)| node.label().is_some_and(is_polygon_primitives_button))
}

/// egui appends a submenu arrow to the button's accessible label.
fn is_polygon_primitives_button(label: &str) -> bool {
    label.starts_with(menus::POLYGON_PRIMITIVES_MENU)
}

pub(in crate::app) fn submenu_item(output: &egui::FullOutput, label: &str) -> SubmenuItem {
    let mut found: Vec<SubmenuItem> = polygon_submenu_items(output)
        .into_iter()
        .filter(|item| item.label == label)
        .collect();
    assert_eq!(found.len(), 1, "submenu control {label:?}");
    found.remove(0)
}

fn pointer_click_events(at: egui::Pos2) -> [egui::Event; 2] {
    [true, false].map(|pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    })
}

/// Opens Create, then hovers "Polygon Primitives" (it opens on hover) and returns the open frame.
pub(in crate::app) fn open_polygon_primitives(
    editor: &mut Editor,
    ctx: &egui::Context,
    size: egui::Vec2,
) -> egui::FullOutput {
    ctx.enable_accesskit();
    let run = |editor: &mut Editor, events| frame_with(editor, ctx, size, 1.0, events);
    let output = run(editor, vec![]);
    let create = {
        let update = output.platform_output.accesskit_update.as_ref().unwrap();
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.label() == Some("Create"))
            .filter_map(|(_, node)| node.bounds())
            .min_by(|a, b| a.y0.total_cmp(&b.y0))
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .expect("Create menu")
    };
    run(editor, vec![egui::Event::PointerMoved(create)]);
    for event in pointer_click_events(create) {
        run(editor, vec![event]);
    }
    let output = run(editor, vec![]);
    // With Create open but the submenu not hovered, the item-only check would wrongly report closed.
    assert!(polygon_submenu_items(&output).is_empty() && !create_menu_closed(&output));
    let parent = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .filter(|(_, node)| node.label().is_some_and(is_polygon_primitives_button))
        .find_map(|(_, node)| node.bounds())
        .expect("Polygon Primitives submenu button");
    let at = egui::pos2(
        ((parent.x0 + parent.x1) / 2.0) as f32,
        ((parent.y0 + parent.y1) / 2.0) as f32,
    );
    let mut output = run(editor, vec![egui::Event::PointerMoved(at)]);
    for _ in 0..3 {
        output = run(editor, vec![]);
    }
    output
}

/// Moves to a submenu control and clicks it, then returns the frame after the click.
pub(in crate::app) fn click_at(
    editor: &mut Editor,
    ctx: &egui::Context,
    size: egui::Vec2,
    at: egui::Pos2,
) -> egui::FullOutput {
    frame_with(editor, ctx, size, 1.0, vec![egui::Event::PointerMoved(at)]);
    for event in pointer_click_events(at) {
        frame_with(editor, ctx, size, 1.0, vec![event]);
    }
    frame_with(editor, ctx, size, 1.0, vec![])
}

fn face_viewport_rect(output: &egui::FullOutput) -> egui::Rect {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_gray(31) => Some(rect.rect),
            _ => None,
        })
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .unwrap()
}

fn face_text_position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text_shape) if text_shape.galley.job.text == text => {
                Some(text_shape.pos + text_shape.galley.size() * 0.5)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "missing actual text: {text}; rendered labels: {:?}",
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            )
        })
}

fn face_pointer_click(
    editor: &mut Editor,
    ctx: &egui::Context,
    position: egui::Pos2,
    modifiers: egui::Modifiers,
    release_keys: Vec<egui::Event>,
) -> egui::FullOutput {
    frame(
        editor,
        ctx,
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::PointerMoved(position),
        ],
    );
    for pressed in [true, false] {
        let mut events = vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers,
            },
        ];
        if !pressed {
            events.extend(release_keys.iter().cloned());
        }
        frame(editor, ctx, events);
    }
    frame(editor, ctx, vec![])
}

#[test]
fn face_native_keys_coalesce_before_same_frame_click_and_preserve_authoring_state() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let center = face_viewport_rect(&output).center();
    face_pointer_click(
        &mut editor,
        &ctx,
        center,
        egui::Modifiers::NONE,
        vec![key(Key::F8), key(Key::F11), key(Key::F11)],
    );
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 1);
    let target = editor.remembered_face.clone();
    frame(&mut editor, &ctx, vec![key(Key::F8)]);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.remembered_face, target);
    frame(&mut editor, &ctx, vec![key(Key::F11)]);
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert_eq!(editor.remembered_face, target);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
    assert!(editor.is_dirty());
    editor.history_step(false);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn face_native_modified_click_and_delete_never_fall_back_to_object_mutation() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![key(Key::F11)]);
    let rect = face_viewport_rect(&output);
    face_pointer_click(
        &mut editor,
        &ctx,
        rect.center(),
        egui::Modifiers::NONE,
        vec![],
    );
    let baseline = (
        editor.history.scene().clone(),
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.remembered_face.clone(),
        editor.camera.clone(),
        editor.scene_token,
    );
    for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::CTRL] {
        face_pointer_click(
            &mut editor,
            &ctx,
            rect.left_top() + egui::vec2(20.0, 20.0),
            modifiers,
            vec![],
        );
        assert!(editor.error && editor.status.contains("unsupported"));
        assert_eq!(
            (
                editor.history.scene().clone(),
                editor.selection_presentation().0,
                editor.selection_presentation().1.to_vec(),
                editor.remembered_face.clone(),
                editor.camera.clone(),
                editor.scene_token
            ),
            baseline
        );
    }
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            key(Key::Delete),
        ],
    );
    assert!(editor.error && editor.status.contains("Face deletion is unsupported"));
    assert_eq!(editor.history.scene(), &baseline.0);
    assert_eq!(editor.remembered_face, baseline.3);
    face_pointer_click(
        &mut editor,
        &ctx,
        rect.left_top() + egui::vec2(20.0, 20.0),
        egui::Modifiers::NONE,
        vec![],
    );
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.selection_presentation().0, baseline.1);
    assert_eq!(editor.selection_presentation().1, baseline.2);
    assert!(matches!(
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation: Operation::Delete(editor.single_target().unwrap().0),
            },
            &ctx,
        ),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(editor.history.scene(), &baseline.0);
}

#[test]
fn face_native_mode_waits_for_current_text_and_rejects_invalid_drafts() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(&mut editor, &ctx, vec![]);
    frame(
        &mut editor,
        &ctx,
        vec![egui::Event::Text("draft".into()), key(Key::F11)],
    );
    assert!(editor.has_draft());
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.history.scene(), &before);
    frame(&mut editor, &ctx, vec![key(Key::Escape)]);
    editor.channels[0] = "nan".into();
    editor.channel_intents[0] = true;
    frame(&mut editor, &ctx, vec![key(Key::F11)]);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.channels[0], "nan");
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.error);
}

#[test]
fn face_native_closing_editor_refuses_mode_intents_without_selection_changes() {
    let mut editor = Editor::default();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    editor.close_document().unwrap();
    assert!(editor.files.is_closing());
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![key(Key::F11)]);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.selection_presentation().0, None);
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
}

/// Drives `shortcuts` directly with a held modifier, returning the tool intent it produced.
fn shortcuts_with(
    editor: &mut Editor,
    modifiers: egui::Modifiers,
    keys: &[Key],
) -> Option<ActiveTool> {
    let events = keys
        .iter()
        .map(|&key| key_event(key, modifiers, false))
        .collect();
    shortcuts_events(editor, modifiers, events)
}

fn key_event(key: Key, modifiers: egui::Modifiers, repeat: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    }
}

fn shortcuts_events(
    editor: &mut Editor,
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
) -> Option<ActiveTool> {
    shortcuts_in(&egui::Context::default(), editor, modifiers, events)
}

/// egui derives `repeat` from whether the key was already down in this context.
fn shortcuts_in(
    ctx: &egui::Context,
    editor: &mut Editor,
    modifiers: egui::Modifiers,
    events: Vec<egui::Event>,
) -> Option<ActiveTool> {
    let events = std::iter::once(egui::Event::ModifiersChanged(modifiers))
        .chain(events)
        .collect();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let mut intent = None;
    let mut output = ctx.run_ui(input, |ui| intent = editor.shortcuts(ui.ctx()));
    output.textures_delta.clear();
    intent
}

#[test]
fn closing_editor_ignores_file_and_tool_shortcuts() {
    let mut editor = Editor {
        active_tool: ActiveTool::Move,
        ..Editor::default()
    };
    editor.close_document().unwrap();
    assert!(editor.files.is_closing() && !editor.files.is_modal());
    let (status, error, token) = (editor.status.clone(), editor.error, editor.scene_token);
    let scene = editor.history.scene().clone();
    let mode = editor.selection_mode;
    let tool = editor.active_tool;
    // Cmd+S needs Save available to bite; Cmd+Z reaches history_step and BusyEditing without the
    // gate. Cmd+Shift+S is not driven: it would open a native dialog where Save As is available.
    let command = egui::Modifiers::COMMAND;
    for (modifiers, key) in [
        (command, Key::S),
        (command, Key::N),
        (command, Key::Z),
        (command | egui::Modifiers::SHIFT, Key::Z),
        (command, Key::Y),
    ] {
        let intent = shortcuts_with(&mut editor, modifiers, &[key]);
        assert_eq!(intent, None);
    }
    for key in [Key::W, Key::Num5, Key::Plus] {
        let intent = shortcuts_with(&mut editor, egui::Modifiers::NONE, &[key]);
        assert_eq!(intent, None);
    }
    assert_eq!(editor.status, status);
    assert_eq!(editor.error, error);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.selection_mode, mode);
    assert_eq!(editor.active_tool, tool);
    assert_eq!(editor.active_tool, ActiveTool::Move);
    assert!(editor.files.pending_inspection().is_none());
    assert!(editor.files.is_closing() && !editor.files.is_modal());
}

#[test]
fn file_chords_map_to_the_table_row_with_that_label() {
    let actions = Editor::default().file_actions();
    for (key, shift, label) in [
        (Key::N, false, "New Scene"),
        (Key::O, false, "Open Scene"),
        (Key::S, false, "Save Scene"),
        (Key::S, true, "Save Scene As"),
    ] {
        let index = menus::chord_entry(&actions, (key, shift)).expect("a row for the chord");
        assert_eq!(actions[index].label, label);
    }
    assert_eq!(menus::chord_entry(&actions, (Key::N, true)), None);
    let command = egui::Modifiers::COMMAND;
    assert_eq!(
        menus::file_chord(&[key_event(Key::N, command, false)]),
        Some((Key::N, false))
    );
    assert_eq!(menus::file_chord(&[key_event(Key::N, command, true)]), None);
    assert_eq!(
        menus::file_chord(&[key_event(Key::N, egui::Modifiers::NONE, false)]),
        None
    );
    assert_eq!(
        menus::file_chord(&[key_event(Key::Z, command, false)]),
        None
    );
    let alt = command | egui::Modifiers::ALT;
    for key in [Key::N, Key::O, Key::S] {
        assert_eq!(menus::file_chord(&[key_event(key, alt, false)]), None);
    }
}

fn file_outcome(editor: &Editor) -> (String, bool, bool, bool, Uuid, Option<String>) {
    (
        editor.status.clone(),
        editor.error,
        editor.is_dirty(),
        editor.files.is_modal(),
        editor.scene_token,
        editor
            .files
            .pending_inspection()
            .map(|p| format!("{:?}", p.kind)),
    )
}

#[test]
fn cmd_n_runs_the_same_new_scene_path_as_the_menu() {
    for dirty in [false, true] {
        let make = || {
            let mut editor = Editor::default();
            if dirty {
                editor.create(Primitive::Cube);
            }
            editor
        };
        let (mut by_key, mut by_menu) = (make(), make());
        assert_eq!(by_key.is_dirty(), dirty);
        let before = file_outcome(&by_key);
        shortcuts_with(&mut by_key, egui::Modifiers::COMMAND, &[Key::N]);
        by_menu.request_new();
        let after = file_outcome(&by_key);
        assert_ne!(after, before, "dirty={dirty}: the key must act");
        assert_eq!(
            after.4 != before.4,
            !dirty,
            "a clean New replaces the document; a dirty one only asks"
        );
        // The token is a fresh random id per New, so it is compared above, not here.
        let without_token =
            |o: (String, bool, bool, bool, Uuid, Option<String>)| (o.0, o.1, o.2, o.3, o.5);
        assert_eq!(without_token(after), without_token(file_outcome(&by_menu)));
    }
}

#[test]
fn new_chord_ignores_shift_repeat_and_missing_command() {
    let command = egui::Modifiers::COMMAND;
    let shifted = command | egui::Modifiers::SHIFT;
    let fresh = || {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor
    };
    let mut editors = [fresh(), fresh(), fresh()];
    let before = editors.each_ref().map(file_outcome);
    let [shift, bare, repeat] = &mut editors;
    let _ = shortcuts_with(shift, shifted, &[Key::N]);
    let _ = shortcuts_with(bare, egui::Modifiers::NONE, &[Key::N]);
    // The first press runs on a throwaway editor so the context holds N down.
    let ctx = egui::Context::default();
    let press = || vec![key_event(Key::N, command, false)];
    let _ = shortcuts_in(&ctx, &mut Editor::default(), command, press());
    let _ = shortcuts_in(&ctx, repeat, command, press());
    for (i, name) in ["shift", "no command", "repeat"].into_iter().enumerate() {
        assert_eq!(file_outcome(&editors[i]), before[i], "{name}");
    }
}

/// A selected cube and a camera panned off it, so framing visibly moves the camera.
fn frame_key_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert!(editor.selected_move_target().is_ok());
    editor.camera.pan(glam::vec2(4.0, 3.0)).unwrap();
    editor
}

/// egui keeps only the frame-final modifiers, so F must be judged by its own event.
#[test]
fn frame_selected_pressed_with_a_modifier_is_not_rescued_by_a_same_frame_release() {
    for held in [
        egui::Modifiers::CTRL,
        egui::Modifiers::SHIFT,
        egui::Modifiers::ALT,
        egui::Modifiers::COMMAND,
    ] {
        let mut editor = frame_key_editor();
        let before = editor.camera.clone();
        let events = vec![
            key_event(Key::F, held, false),
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ];
        shortcuts_in(&egui::Context::default(), &mut editor, held, events);
        assert!(editor.camera.same_capture(&before), "{held:?}");
    }
}

#[test]
fn frame_selected_pressed_bare_is_not_cancelled_by_a_modifier_arriving_later() {
    let mut editor = frame_key_editor();
    let before = editor.camera.clone();
    let events = vec![
        key_event(Key::F, egui::Modifiers::NONE, false),
        egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
    ];
    let none = egui::Modifiers::NONE;
    shortcuts_in(&egui::Context::default(), &mut editor, none, events);
    assert!(!editor.camera.same_capture(&before));
}

/// Held F repeats (PROVISIONAL, known-issues.md): the second frame on one context, with no
/// release between, carries an OS repeat event and frames again.
#[test]
fn frame_selected_repeats_while_f_is_held() {
    let mut editor = frame_key_editor();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    shortcuts_in(
        &ctx,
        &mut editor,
        none,
        vec![key_event(Key::F, none, false)],
    );
    let first = editor.camera.clone();
    editor.camera.pan(glam::vec2(4.0, 3.0)).unwrap();
    let panned = editor.camera.clone();
    assert!(!panned.same_capture(&first));
    shortcuts_in(&ctx, &mut editor, none, vec![key_event(Key::F, none, true)]);
    let repeated = ctx.input(|i| {
        i.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: Key::F,
                    repeat: true,
                    ..
                }
            )
        })
    });
    assert!(repeated, "the second frame must carry a repeat event");
    assert!(!editor.camera.same_capture(&panned));
}

/// Only the press frames: a release, in the same frame or a later one, must not frame again.
#[test]
fn frame_selected_counts_the_f_press_and_not_the_release() {
    let none = egui::Modifiers::NONE;
    let release = || egui::Event::Key {
        key: Key::F,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: none,
    };
    let mut editor = frame_key_editor();
    let ctx = egui::Context::default();
    let both = vec![key_event(Key::F, none, false), release()];
    shortcuts_in(&ctx, &mut editor, none, both);
    let framed = editor.camera.clone();
    editor.camera.pan(glam::vec2(4.0, 3.0)).unwrap();
    let panned = editor.camera.clone();
    assert!(!panned.same_capture(&framed));
    shortcuts_in(&ctx, &mut editor, none, vec![release()]);
    assert!(editor.camera.same_capture(&panned));
    let mut lone = frame_key_editor();
    let before = lone.camera.clone();
    shortcuts_in(&ctx, &mut lone, none, vec![release()]);
    assert!(lone.camera.same_capture(&before));
}

/// Held Undo and Redo repeat; Maya's hotkeys page does not say (PROVISIONAL, known-issues.md).
#[test]
fn held_undo_and_redo_chords_repeat() {
    let command = egui::Modifiers::COMMAND;
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let full = editor.history.scene().clone();
    editor.history_step(false);
    editor.history_step(false);
    let two_undone = editor.history.scene().clone();
    assert_ne!(two_undone, full);
    // Redo twice by hand, then hold the chords: the second event of each is a real repeat.
    editor.history_step(true);
    editor.history_step(true);
    let hold = |editor: &mut Editor, key: Key| {
        let ctx = egui::Context::default();
        for repeat in [false, true] {
            let events = vec![key_event(key, command, repeat)];
            let _ = shortcuts_in(&ctx, editor, command, events);
        }
    };
    hold(&mut editor, Key::Z);
    assert_eq!(editor.history.scene(), &two_undone);
    hold(&mut editor, Key::Y);
    assert_eq!(editor.history.scene(), &full);
}

#[test]
fn alt_modified_file_chords_do_nothing() {
    let alt = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = file_outcome(&editor);
    for key in [Key::N, Key::S] {
        let _ = shortcuts_with(&mut editor, alt, &[key]);
    }
    assert_eq!(file_outcome(&editor), before);
}

#[test]
fn alt_modified_undo_and_redo_do_nothing() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.history_step(false);
    assert!(editor.history.can_undo() && editor.history.can_redo());
    let outcome = |editor: &Editor| {
        (
            editor.history.scene().clone(),
            editor.status.clone(),
            editor.error,
            editor.scene_token,
        )
    };
    let before = outcome(&editor);
    let alt = egui::Modifiers::COMMAND | egui::Modifiers::ALT;
    for (modifiers, key) in [
        (alt, Key::Z),
        (alt | egui::Modifiers::SHIFT, Key::Z),
        (alt, Key::Y),
    ] {
        shortcuts_with(&mut editor, modifiers, &[key]);
        assert_eq!(outcome(&editor), before, "{modifiers:?} {key:?}");
    }
}

/// An editor with both Undo and Redo available.
fn history_chord_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.history_step(false);
    assert!(editor.history.can_undo() && editor.history.can_redo());
    editor
}

/// What a refused history chord must leave untouched.
fn history_outcome(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.status.clone(),
        editor.error,
        editor.scene_token,
    )
}

/// egui keeps only the frame-final modifiers, so the press must be judged by its own snapshot.
#[test]
fn history_chord_pressed_with_alt_is_not_rescued_by_a_same_frame_alt_release() {
    let (command, shift) = (egui::Modifiers::COMMAND, egui::Modifiers::SHIFT);
    let alt = command | egui::Modifiers::ALT;
    for (pressed, key) in [(alt, Key::Z), (alt | shift, Key::Z), (alt, Key::Y)] {
        let mut editor = history_chord_editor();
        let before = history_outcome(&editor);
        let events = vec![
            key_event(key, pressed, false),
            egui::Event::ModifiersChanged(command),
        ];
        shortcuts_in(&egui::Context::default(), &mut editor, pressed, events);
        assert_eq!(history_outcome(&editor), before, "{pressed:?} {key:?}");
    }
}

#[test]
fn history_chord_pressed_with_alt_is_not_rescued_through_the_frame() {
    let mut editor = history_chord_editor();
    let before = history_outcome(&editor);
    let command = egui::Modifiers::COMMAND;
    let events = vec![
        egui::Event::ModifiersChanged(command | egui::Modifiers::ALT),
        key_event(Key::Z, command | egui::Modifiers::ALT, false),
        egui::Event::ModifiersChanged(command),
    ];
    frame(&mut editor, &egui::Context::default(), events);
    assert_eq!(history_outcome(&editor), before);
}

#[test]
fn history_chord_pressed_before_alt_is_not_cancelled_by_alt_arriving_later() {
    let mut editor = history_chord_editor();
    let undone = editor.history.scene().clone();
    let command = egui::Modifiers::COMMAND;
    let events = vec![
        key_event(Key::Z, command, false),
        egui::Event::ModifiersChanged(command | egui::Modifiers::ALT),
    ];
    shortcuts_in(&egui::Context::default(), &mut editor, command, events);
    assert_ne!(editor.history.scene(), &undone);
    assert!(editor.status.starts_with("Undo"), "{}", editor.status);
}

/// The assertions before each dialog call make a broken setup panic instead of reaching rfd.
#[test]
fn save_as_dialog_refuses_while_closing_and_keeps_the_active_move() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let Ok(OperationReply::PendingConfirmation {
        pending_action_id, ..
    }) = editor.close_document()
    else {
        panic!("a dirty scene must ask before closing");
    };
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
            decision: crate::editor_operations::FileDecision::Discard,
        })
        .unwrap();
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    editor
        .history
        .begin_move(
            &target,
            azteccraft::native_scene::WorldAxis::X,
            editor.scene_token,
            ManipulationProvenance::Remote,
            1 << 20,
        )
        .unwrap();
    // Without the guard the move yields BusyManipulation, a different status, so this fails
    // instead of reaching rfd.
    assert!(
        editor.files.is_closing()
            && !editor.files.save_as_available()
            && editor.history.active_manipulation().is_some()
    );
    editor.save_as_dialog();
    assert_eq!(editor.status, OperationError::BusyEditing.to_string());
    assert!(editor.error);
    assert!(editor.history.active_manipulation().is_some());
}

#[test]
fn open_dialog_refuses_while_closing_and_keeps_the_active_move() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let Ok(OperationReply::PendingConfirmation {
        pending_action_id, ..
    }) = editor.close_document()
    else {
        panic!("a dirty scene must ask before closing");
    };
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
            decision: crate::editor_operations::FileDecision::Discard,
        })
        .unwrap();
    editor.new_document().unwrap();
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    editor
        .history
        .begin_move(
            &target,
            azteccraft::native_scene::WorldAxis::X,
            editor.scene_token,
            ManipulationProvenance::Remote,
            1 << 20,
        )
        .unwrap();
    assert!(
        editor.files.is_closing()
            && editor.files.is_modal()
            && editor.history.active_manipulation().is_some()
    );
    editor.open_dialog();
    assert_eq!(editor.status, OperationError::BusyEditing.to_string());
    assert!(editor.error);
    assert!(editor.history.active_manipulation().is_some());
}

/// egui-winit stamps a key release with the held modifiers, so only the press may step history.
#[test]
fn undo_and_redo_step_once_per_keystroke_not_on_release() {
    let command = egui::Modifiers::COMMAND;
    let release = |key| egui::Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: command,
    };
    for key in [Key::Z, Key::Y] {
        for same_frame in [true, false] {
            let mut editor = Editor::default();
            for _ in 0..3 {
                editor.create(Primitive::Cube);
            }
            let one_object_more = editor.history.scene().clone();
            editor.history_step(false);
            let middle = editor.history.scene().clone();
            editor.history_step(false);
            let two_undone = editor.history.scene().clone();
            // Z starts from the full scene, Y from the doubly undone one; one step reaches `middle`.
            if key == Key::Z {
                editor.history_step(true);
                editor.history_step(true);
            }
            assert_ne!(middle, one_object_more);
            assert_ne!(middle, two_undone);
            let ctx = egui::Context::default();
            if same_frame {
                let events = vec![key_event(key, command, false), release(key)];
                shortcuts_in(&ctx, &mut editor, command, events);
            } else {
                let press = vec![key_event(key, command, false)];
                shortcuts_in(&ctx, &mut editor, command, press);
                shortcuts_in(&ctx, &mut editor, command, vec![release(key)]);
            }
            assert_eq!(
                editor.history.scene(),
                &middle,
                "{key:?} same_frame={same_frame}"
            );
        }
    }
}

/// Undo and Redo take one step per frame, and the last history chord in the frame wins.
#[test]
fn history_chords_step_once_per_frame_and_the_last_one_wins() {
    let command = egui::Modifiers::COMMAND;
    let redo_chord = command | egui::Modifiers::SHIFT;
    let release = || egui::Event::Key {
        key: Key::Z,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: command,
    };
    let steps = |events: Vec<egui::Event>, redo_available: bool| {
        let mut editor = Editor::default();
        for _ in 0..3 {
            editor.create(Primitive::Cube);
        }
        let full = editor.history.scene().clone();
        editor.history_step(false);
        let one_undone = editor.history.scene().clone();
        editor.history_step(false);
        let two_undone = editor.history.scene().clone();
        if !redo_available {
            editor.history_step(true);
            editor.history_step(true);
        }
        let start = editor.history.scene().clone();
        shortcuts_in(&egui::Context::default(), &mut editor, command, events);
        (
            full,
            one_undone,
            two_undone,
            start,
            editor.history.scene().clone(),
        )
    };
    // Two Undo presses with a release between: one step, not one per press.
    let events = vec![
        key_event(Key::Z, command, false),
        release(),
        key_event(Key::Z, command, false),
    ];
    let (full, one_undone, _, start, after) = steps(events, false);
    assert_eq!(start, full);
    assert_eq!(after, one_undone);
    // Redo chord then Undo chord: Undo wins. With the redo stack empty a leaked Redo would re-apply it.
    let events = vec![
        key_event(Key::Z, redo_chord, false),
        egui::Event::ModifiersChanged(command),
        key_event(Key::Z, command, false),
    ];
    let (full, one_undone, _, start, after) = steps(events, false);
    assert_eq!(start, full);
    assert_eq!(after, one_undone);
    // Undo chord then Redo chord: Redo wins and takes exactly one step.
    let events = vec![
        key_event(Key::Z, command, false),
        key_event(Key::Z, redo_chord, false),
    ];
    let (_, one_undone, two_undone, start, after) = steps(events, true);
    assert_eq!(start, two_undone);
    assert_eq!(after, one_undone);
}

#[test]
fn save_chord_does_nothing_while_save_is_unavailable() {
    let mut editor = Editor::default();
    assert!(!editor.files.save_available());
    let before = file_outcome(&editor);
    shortcuts_with(&mut editor, egui::Modifiers::COMMAND, &[Key::S]);
    assert_eq!(file_outcome(&editor), before);
}

#[test]
fn cmd_y_redoes_after_undo_and_shift_y_does_not() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let created = editor.history.scene().clone();
    editor.history_step(false);
    let undone = editor.history.scene().clone();
    assert_ne!(undone, created);
    let command = egui::Modifiers::COMMAND;
    shortcuts_with(&mut editor, command | egui::Modifiers::SHIFT, &[Key::Y]);
    assert_eq!(editor.history.scene(), &undone);
    shortcuts_with(&mut editor, command, &[Key::Y]);
    assert_eq!(editor.history.scene(), &created);
    assert!(editor.status.starts_with("Redo"), "{}", editor.status);
}

/// Two creates so an Undo that also fires in the same frame as the Redo leaves one object, not two.
#[test]
fn cmd_shift_z_still_redoes() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let full = editor.history.scene().clone();
    editor.history_step(false);
    assert_ne!(editor.history.scene(), &full);
    let chord = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    shortcuts_with(&mut editor, chord, &[Key::Z]);
    assert_eq!(editor.history.scene(), &full);
}

#[test]
fn vertex_and_edge_modes_refuse_native_delete_and_frame_without_a_component() {
    for (mode, noun) in [
        (SelectionMode::Vertex, "Vertex"),
        (SelectionMode::Edge, "Edge"),
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.selection_mode = mode;
        let (before, token) = (editor.history.scene().clone(), editor.scene_token);
        editor.native_operation(Operation::Delete(editor.single_target().unwrap().0));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        assert!(editor.error);
        assert_eq!(
            editor.status,
            format!("{noun} deletion is unsupported; switch to Object or Hierarchy selection")
        );
        let camera = editor.camera.clone();
        assert!(matches!(
            editor.apply_operation(Operation::FrameSelected),
            Err(OperationError::NoSelection)
        ));
        assert_eq!(editor.camera, camera);
    }
}

#[test]
fn face_native_pending_mode_prevents_keyboard_and_channel_object_delete() {
    for button in [false, true] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![]);
        if button {
            let position = face_text_position(&output, "Delete selected");
            face_pointer_click(
                &mut editor,
                &ctx,
                position,
                egui::Modifiers::NONE,
                vec![key(Key::F11)],
            );
        } else {
            frame(&mut editor, &ctx, vec![key(Key::F11), key(Key::Delete)]);
        }
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.history.scene(), &before, "button={button}");
        assert_eq!(editor.scene_token, token);
        assert!(editor.error && editor.status.contains("Face deletion is unsupported"));
    }
}

#[test]
fn face_native_edit_menu_delete_keeps_face_mode_unsupported_and_object_mode_working() {
    for face_mode in [false, true] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        let output = frame(
            &mut editor,
            &ctx,
            if face_mode {
                vec![key(Key::F11)]
            } else {
                vec![]
            },
        );
        let edit = face_text_position(&output, "Edit");
        let output = face_pointer_click(&mut editor, &ctx, edit, egui::Modifiers::NONE, vec![]);
        let delete = face_text_position(&output, "Delete");
        let selection = (
            editor.selection_mode,
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token,
        );
        face_pointer_click(&mut editor, &ctx, delete, egui::Modifiers::NONE, vec![]);
        if face_mode {
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(
                (
                    editor.selection_mode,
                    editor.selection_presentation().0,
                    editor.selection_presentation().1.to_vec(),
                    editor.remembered_face.clone(),
                    editor.camera.clone(),
                    editor.scene_token
                ),
                selection
            );
        } else {
            assert!(editor.history.scene().nodes().is_empty());
            editor.history_step(false);
            assert_eq!(editor.history.scene(), &before);
        }
    }
}

#[test]
fn face_native_event_modifiers_survive_later_modifier_changes() {
    for (event_modifiers, final_modifiers, expected) in [
        (
            egui::Modifiers::SHIFT,
            egui::Modifiers::NONE,
            SelectionMode::Object,
        ),
        (
            egui::Modifiers::NONE,
            egui::Modifiers::SHIFT,
            SelectionMode::Face,
        ),
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::Key {
                    key: Key::F11,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: event_modifiers,
                },
                egui::Event::ModifiersChanged(final_modifiers),
            ],
        );
        assert_eq!(editor.selection_mode, expected);
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn face_native_modal_cancel_does_not_replay_blocked_mode_intent() {
    for (button, open) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        let directory = tempfile::tempdir().unwrap();
        if open {
            let path = directory.path().join("replacement.aztec");
            let mut candidate = azteccraft::scene::Scene::default();
            candidate.create(Primitive::Plane, "replacement").unwrap();
            azteccraft::document::save_new(&path, &candidate).unwrap();
            editor
                .apply_operation(Operation::OpenScene { path })
                .unwrap();
        } else {
            editor.new_document().unwrap();
        }
        frame(&mut editor, &ctx, vec![]);
        let output = frame(&mut editor, &ctx, vec![]);
        assert!(editor.files.is_modal());
        let baseline = (
            editor.selection_mode,
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.remembered_face.clone(),
            editor.history.scene().clone(),
            editor.camera.clone(),
            editor.scene_token,
            editor.is_dirty(),
            editor.history.can_undo(),
            editor.history.can_redo(),
        );
        let draft = (
            editor.name.clone(),
            editor.channels.clone(),
            editor.name_intent,
            editor.channel_intents,
        );
        if button {
            let cancel = face_text_position(&output, "Cancel");
            face_pointer_click(
                &mut editor,
                &ctx,
                cancel,
                egui::Modifiers::NONE,
                vec![key(Key::F11)],
            );
        } else {
            frame(&mut editor, &ctx, vec![key(Key::Escape), key(Key::F11)]);
        }
        assert!(!editor.files.is_modal());
        assert_eq!(
            (
                editor.selection_mode,
                editor.selection_presentation().0,
                editor.selection_presentation().1.to_vec(),
                editor.remembered_face.clone(),
                editor.history.scene().clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.is_dirty(),
                editor.history.can_undo(),
                editor.history.can_redo()
            ),
            baseline
        );
        assert_eq!(
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents
            ),
            draft
        );
    }
}

#[test]
fn face_native_ambiguous_primary_frame_preserves_selection_scene_and_draft() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![key(Key::F11)]);
    let point = face_viewport_rect(&output).center();
    face_pointer_click(&mut editor, &ctx, point, egui::Modifiers::NONE, vec![]);
    assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 1);
    editor.native_operation(Operation::Rename {
        id: editor.single_target().unwrap().0,
        name: "redo-payload".into(),
    });
    editor.history_step(false);
    assert!(editor.history.can_redo());
    editor.name = "pending-human-name".into();
    editor.name_intent = true;
    let baseline = (
        editor.selection_mode,
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.remembered_face.clone(),
        editor.history.scene().clone(),
        editor.camera.clone(),
        editor.scene_token,
        editor.history.can_undo(),
        editor.history.can_redo(),
        editor.is_dirty(),
    );
    let draft = (
        editor.name.clone(),
        editor.channels.clone(),
        editor.name_intent,
        editor.channel_intents,
    );
    let end = point + egui::vec2(36.0, 0.0);
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::SHIFT,
            },
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::SHIFT,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(end),
            egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert!(editor.error && editor.status.contains("Ambiguous coalesced primary input"));
    assert_eq!(
        (
            editor.selection_mode,
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.remembered_face.clone(),
            editor.history.scene().clone(),
            editor.camera.clone(),
            editor.scene_token,
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.is_dirty()
        ),
        baseline
    );
    assert_eq!(
        (
            editor.name.clone(),
            editor.channels.clone(),
            editor.name_intent,
            editor.channel_intents
        ),
        draft
    );
}

#[test]
fn face_native_menu_explicit_face_beats_toggle_and_modal_replacement_drops_intent() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let select = face_text_position(&output, "Select");
    let output = face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
    let components = face_text_position(&output, "Components");
    let output = face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
    let face = face_text_position(&output, "Face   F11");
    face_pointer_click(
        &mut editor,
        &ctx,
        face,
        egui::Modifiers::NONE,
        vec![key(Key::F8)],
    );
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert_eq!(editor.history.scene(), &before);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
        .unwrap();
    editor.new_document().unwrap();
    let token = editor.scene_token;
    frame(&mut editor, &ctx, vec![key(Key::F11)]);
    assert!(editor.files.is_modal());
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    let output = frame(&mut editor, &ctx, vec![]);
    let discard = face_text_position(&output, "Don't Save");
    face_pointer_click(
        &mut editor,
        &ctx,
        discard,
        egui::Modifiers::NONE,
        vec![key(Key::F11)],
    );
    assert_ne!(editor.scene_token, token);
    assert!(!editor.files.is_modal());
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn face_native_popup_intercepts_combined_menu_activation_and_viewport_click() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let center = face_viewport_rect(&output).center();
    let select = face_text_position(&output, "Select");
    let output = face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
    let components = face_text_position(&output, "Components");
    let output = face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
    let face = face_text_position(&output, "Face   F11");
    let mut focused = false;
    for _ in 0..32 {
        frame(&mut editor, &ctx, vec![key(Key::Tab)]);
        focused = ctx
            .memory(|memory| memory.focused())
            .and_then(|id| ctx.read_response(id))
            .is_some_and(|response| response.rect.contains(face));
        if focused {
            break;
        }
    }
    assert!(
        focused,
        "actual menu Face item must receive keyboard focus before activation"
    );
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(egui::Popup::is_any_open(&ctx));
    let selection = (
        editor.selection_mode,
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.remembered_face.clone(),
        editor.camera.clone(),
        editor.scene_token,
    );
    let history = (
        editor.history.can_undo(),
        editor.history.can_redo(),
        editor.is_dirty(),
    );
    let draft = (
        editor.name.clone(),
        editor.channels.clone(),
        editor.name_intent,
        editor.channel_intents,
    );
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
            key(Key::Enter),
            key(Key::F8),
            egui::Event::PointerMoved(center),
            egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(
        (
            editor.selection_mode,
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token
        ),
        selection
    );
    assert_eq!(
        (
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.is_dirty()
        ),
        history
    );
    assert_eq!(
        (
            editor.name.clone(),
            editor.channels.clone(),
            editor.name_intent,
            editor.channel_intents
        ),
        draft
    );
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn real_escape_event_cancels_channel_and_name_after_egui_clears_focus() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(("channel", 0_usize))));
    frame(&mut editor, &ctx, vec![]);
    editor.channels[0] = "3".into();
    frame(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert_eq!(editor.channels[0], "0");
    assert_eq!(editor.history.scene(), &before);
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("node_name")));
    frame(&mut editor, &ctx, vec![]);
    editor.name = "discarded".into();
    frame(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert_eq!(editor.history.scene().node(id).unwrap().name(), "pCube1");
    editor.history_step(false);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn real_grid_button_dispatches_current_state_and_disables_for_modal_and_close() {
    for blocked in [0, 1, 2] {
        let context = egui::Context::default();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        if blocked == 1 {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        } else if blocked == 2 {
            editor.close_document().unwrap();
        }
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        let mut position = None;
        let mut run = |editor: &mut Editor, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(720.0, 480.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| editor.viewport(ui, token, None, None, false, true),
            );
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.job.text == "Grid"
                {
                    position = Some(text.pos + text.galley.size() * 0.5);
                }
            }
            output.textures_delta.clear();
            position
        };
        let point = run(&mut editor, vec![]).unwrap();
        for expected in [blocked != 0, true] {
            run(&mut editor, vec![egui::Event::PointerMoved(point)]);
            for pressed in [true, false] {
                run(
                    &mut editor,
                    vec![egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
            assert_eq!(editor.grid_visible, expected);
            assert_eq!(editor.inspect(false).unwrap().grid_visible, expected);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.history.scene(), &scene);
        }
    }
}

fn primary_button(position: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn unfocused_cube_editor() -> (Editor, egui::Context) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
    (editor, ctx)
}

#[test]
fn first_click_with_same_frame_text_reaches_unfocused_channel_field() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let before = editor.history.scene().clone();
    let undo_before = editor.history.undo_label();
    let center = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(center),
            primary_button(center, true),
            egui::Event::Text("7".into()),
        ],
    );
    assert_eq!(editor.channels[0], "07");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(0))
    );
    frame(&mut editor, &ctx, vec![primary_button(center, false)]);
    assert_eq!(editor.channels[0], "07");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.undo_label(), undo_before);
}

#[test]
fn first_click_press_release_and_text_in_one_frame_reaches_channel_field() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let center = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(center),
            primary_button(center, true),
            primary_button(center, false),
            egui::Event::Text("7".into()),
        ],
    );
    assert_eq!(editor.channels[0], "07");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(0))
    );
}

fn click_with_text(editor: &mut Editor, ctx: &egui::Context, field: egui::Id, text: &str) {
    let center = ctx.read_response(field).unwrap().rect.center();
    frame(
        editor,
        ctx,
        vec![
            egui::Event::PointerMoved(center),
            primary_button(center, true),
            egui::Event::Text(text.into()),
        ],
    );
}

fn undo_entries(editor: &mut Editor) -> usize {
    let mut depth = 0;
    while editor.history.undo() {
        depth += 1;
    }
    for _ in 0..depth {
        assert!(editor.history.redo());
    }
    depth
}

fn type_into_channel(editor: &mut Editor, ctx: &egui::Context, index: usize, text: &str) {
    ctx.memory_mut(|memory| memory.request_focus(channel_edit_id(index)));
    frame(editor, ctx, vec![]);
    frame(editor, ctx, vec![egui::Event::Text(text.into())]);
}

#[test]
fn first_click_on_another_field_commits_the_valid_draft_before_typing_there() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let undo_before = undo_entries(&mut editor);
    type_into_channel(&mut editor, &ctx, 0, "5");
    assert_eq!(editor.channels[0], "05");
    click_with_text(&mut editor, &ctx, channel_edit_id(1), "3");
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation,
        [5.0, 0.0, 0.0]
    );
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
    assert_eq!(editor.channels[1], "03");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(1))
    );
    assert!(editor.has_draft());
    let center = ctx.read_response(channel_edit_id(1)).unwrap().rect.center();
    frame(&mut editor, &ctx, vec![primary_button(center, false)]);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[1],
        0.0
    );
    assert_eq!(editor.channels[1], "03");
}

#[test]
fn first_click_on_another_field_keeps_an_invalid_draft_and_still_types() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let before = editor.history.scene().clone();
    type_into_channel(&mut editor, &ctx, 0, "x");
    click_with_text(&mut editor, &ctx, channel_edit_id(1), "3");
    assert!(editor.error);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[0], "0x");
    assert_eq!(editor.channels[1], "03");
    let center = ctx.read_response(channel_edit_id(1)).unwrap().rect.center();
    frame(&mut editor, &ctx, vec![primary_button(center, false)]);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[1], "03");
}

#[test]
fn pressing_the_focused_field_again_does_not_commit_its_draft() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let before = editor.history.scene().clone();
    type_into_channel(&mut editor, &ctx, 0, "5");
    click_with_text(&mut editor, &ctx, channel_edit_id(0), "1");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[0], "051");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(0))
    );
}

#[test]
fn a_held_press_does_not_refocus_the_field_after_escape() {
    // Relies on Escape clearing focus in egui's begin_pass (memory/mod.rs:599).
    let (mut editor, ctx) = unfocused_cube_editor();
    click_with_text(&mut editor, &ctx, channel_edit_id(0), "7");
    let escape = || {
        vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]
    };
    frame(&mut editor, &ctx, escape());
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
}

#[test]
fn first_click_text_is_ignored_during_an_active_manipulation() {
    let (mut editor, ctx) = unfocused_cube_editor();
    // Started after the warm-up, so egui's previous-pass widget flags still say enabled.
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    editor
        .history
        .begin_move(
            &target,
            azteccraft::native_scene::WorldAxis::X,
            editor.scene_token,
            ManipulationProvenance::Remote,
            1 << 20,
        )
        .unwrap();
    let channels = editor.channels.clone();
    click_with_text(&mut editor, &ctx, channel_edit_id(0), "7");
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
    assert_eq!(editor.channels, channels);
}

#[test]
fn first_click_text_is_ignored_while_a_modal_is_open() {
    let (mut editor, ctx) = unfocused_cube_editor();
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let channels = editor.channels.clone();
    click_with_text(&mut editor, &ctx, channel_edit_id(0), "7");
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
    assert_eq!(editor.channels, channels);
}

#[test]
fn first_click_with_same_frame_text_reaches_the_name_field() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let name = editor.name.clone();
    click_with_text(&mut editor, &ctx, name_edit_id(), "x");
    assert_eq!(editor.name, format!("{name}x"));
    assert_eq!(ctx.memory(|memory| memory.focused()), Some(name_edit_id()));
}

#[test]
fn secondary_button_press_does_not_deliver_same_frame_text() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let center = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    let channels = editor.channels.clone();
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(center),
            egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Secondary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Text("7".into()),
        ],
    );
    assert_eq!(editor.channels, channels);
}

#[test]
fn press_then_move_focuses_the_field_egui_hit_at_the_final_position() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let first = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    let second = ctx.read_response(channel_edit_id(1)).unwrap().rect.center();
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(first),
            primary_button(first, true),
            egui::Event::PointerMoved(second),
            egui::Event::Text("7".into()),
        ],
    );
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(1))
    );
    assert_eq!(editor.channels[0], "0");
    assert_eq!(editor.channels[1], "07");
}

#[test]
fn first_click_on_a_field_keeps_same_frame_delete_and_selection_keys_suppressed() {
    // Invariant guard: main already suppresses these, since focus is set before can_select reads it.
    let (mut editor, ctx) = unfocused_cube_editor();
    let center = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    let before = editor.history.scene().clone();
    let mode = editor.selection_mode;
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(center),
            primary_button(center, true),
            key(Key::Delete),
            key(Key::F8),
            key(Key::F11),
        ],
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.selection_mode, mode);
}

#[test]
fn first_click_on_a_field_that_just_disappeared_leaves_no_stale_focus() {
    let (mut editor, ctx) = unfocused_cube_editor();
    editor.set_selection_for_test(None, Vec::new());
    click_with_text(&mut editor, &ctx, channel_edit_id(0), "7");
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    assert!(!text_edit_focused(&ctx));
}

#[test]
fn leaving_the_manipulation_value_field_does_not_commit_a_channel_draft() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let before = editor.history.scene().clone();
    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    ctx.memory_mut(|memory| {
        memory.request_focus(features::manipulation_tool::manipulation_value_id())
    });
    // Unrendered ids lose focus at the end of the next pass, so click in the same frame.
    click_with_text(&mut editor, &ctx, channel_edit_id(1), "3");
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[0], "5");
    assert_eq!(editor.channels[1], "03");
}

#[test]
fn real_channel_text_keeps_f64_precision_and_untouched_slots_in_one_undo() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    editor
        .history
        .execute(Command::SetTransform {
            id,
            transform: NativeTransform {
                translation: [0.0, 1.0000000000000004, 0.0],
                ..Default::default()
            },
        })
        .unwrap();
    editor.sync_fields();
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
    frame(&mut editor, &ctx, vec![]);
    let command = egui::Modifiers::COMMAND;
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::ModifiersChanged(command),
            egui::Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: command,
            },
            egui::Event::Text("1.0000000000000002".into()),
            key(Key::Enter),
        ],
    );
    let translation = editor
        .history
        .scene()
        .node(id)
        .unwrap()
        .transform()
        .translation;
    assert_eq!(translation[0].to_bits(), 1.0000000000000002_f64.to_bits());
    assert_eq!(translation[1].to_bits(), 1.0000000000000004_f64.to_bits());
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn viewport_click_commits_draft_before_picking_its_new_geometry() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    editor.camera = Camera::default();
    let ctx = egui::Context::default();
    let mut viewport_rect = egui::Rect::NOTHING;
    let run = |editor: &mut Editor, events, rect: &mut egui::Rect| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                editor.viewport(ui, editor.scene_token, None, None, false, true);
                *rect = ui.min_rect();
            },
        );
        output.textures_delta.clear();
    };
    run(&mut editor, vec![], &mut viewport_rect);
    let new_center = glam::Vec3::new(2.0, 0.0, 0.0);
    let ndc = editor
        .camera
        .project(new_center, editor.aspect)
        .unwrap()
        .unwrap();
    let width = viewport_rect.width();
    let height = width / editor.aspect;
    let position = egui::pos2(
        viewport_rect.left() + (ndc.x + 1.0) * 0.5 * width,
        viewport_rect.bottom() - (ndc.y + 1.0) * 0.5 * height,
    );
    run(
        &mut editor,
        vec![egui::Event::PointerMoved(position)],
        &mut viewport_rect,
    );
    editor.channels[0] = "2".into();
    editor.channel_intents[0] = true;
    for pressed in [true, false] {
        run(
            &mut editor,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
            &mut viewport_rect,
        );
    }
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        2.0
    );
    assert_eq!(
        editor.selection_presentation().0,
        Some(id),
        "fresh geometry must be hit at the new location"
    );
    assert!(!editor.has_draft());
}

#[test]
fn enter_commits_once_and_selection_commits_draft_to_previous_object() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let old = editor.single_target().unwrap().0;
    editor.create(Primitive::Plane);
    let next = editor.single_target().unwrap().0;
    editor.select(Some(old));
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(("channel", 0_usize))));
    frame(&mut editor, &ctx, vec![]);
    editor.channels[0] = "2".into();
    frame(&mut editor, &ctx, vec![key(Key::Enter)]);
    assert_eq!(
        editor
            .history
            .scene()
            .node(old)
            .unwrap()
            .transform()
            .translation[0],
        2.0
    );
    editor.history_step(false);
    assert_eq!(
        editor
            .history
            .scene()
            .node(old)
            .unwrap()
            .transform()
            .translation[0],
        0.0
    );
    editor.channels[0] = "4".into();
    editor.select(Some(next));
    assert_eq!(
        editor
            .history
            .scene()
            .node(old)
            .unwrap()
            .transform()
            .translation[0],
        4.0
    );
    assert_eq!(
        editor.history.scene().node(next).unwrap().transform(),
        NativeTransform::default()
    );
    assert_eq!(editor.channels[0], "0");
}

#[test]
fn creation_commits_old_draft_and_invalid_draft_blocks_selection_and_actions() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let old = editor.single_target().unwrap().0;
    editor.channels[1] = "5".into();
    editor.create(Primitive::Plane);
    let next = editor.single_target().unwrap().0;
    assert_eq!(
        editor
            .history
            .scene()
            .node(old)
            .unwrap()
            .transform()
            .translation[1],
        5.0
    );
    assert_eq!(
        editor.history.scene().node(next).unwrap().transform(),
        NativeTransform::default()
    );
    editor.channels[0] = "nan".into();
    let before = editor.history.scene().clone();
    editor.select(Some(old));
    assert_eq!(editor.selection_presentation().0, Some(next));
    editor.create(Primitive::Cube);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[0], "nan");
    assert!(editor.error);
}

#[test]
fn create_allocates_name_after_committing_pending_rename() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let old = editor.single_target().unwrap().0;
    editor.name = "pCube2".into();
    editor.create(Primitive::Cube);
    let created = editor.single_target().unwrap().0;
    assert_ne!(old, created);
    assert_eq!(editor.history.scene().node(old).unwrap().name(), "pCube2");
    assert_eq!(
        editor.history.scene().node(created).unwrap().name(),
        "pCube1"
    );
}

fn open_ascii(body: &str) -> Editor {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scene.ma");
    std::fs::write(
        &path,
        format!("//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; {body}"),
    )
    .unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    editor
}

fn channel_box_text(editor: &mut Editor) -> (String, Vec<egui::Shape>) {
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| editor.channels(ui, &mut false),
    );
    output.textures_delta.clear();
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|");
    (text, output.shapes.into_iter().map(|s| s.shape).collect())
}

#[test]
fn channel_box_visibility_row_is_read_only_and_never_defaults_to_on() {
    const CC: &str = "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 -0 0 1 2 3 4 -3 2 6 0 -1;";
    for (v, expected) in [
        ("setAttr \".v\" no;", "off"),
        ("setAttr \".v\" yes;", "on"),
        ("", "on"),
    ] {
        let mut editor = open_ascii(&format!(
            "createNode transform -n \"c\"; {v} createNode nurbsCurve -n \"cShape\" -p \"c\"; {CC}"
        ));
        let id = editor.history.scene().nodes()[0].id();
        assert_eq!(
            editor.inspect(false).unwrap().nodes[0].visible,
            Some(expected == "on")
        );
        editor.select(Some(id));
        let before = editor.history.scene().clone();
        let (text, _) = channel_box_text(&mut editor);
        assert!(text.contains("Visibility"), "{text}");
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains("unknown"));
        assert!(!editor.has_draft());
        assert_eq!(editor.history.scene(), &before);
    }
}

mod menu_bar_wire {
    use super::toolbox_wire::{Control, click, controls, setup};
    use super::*;
    use crate::app::menus::{ALWAYS_ON_MENUS, HELP_MENU, MODELING_SET_MENUS};

    fn expected_bar() -> Vec<&'static str> {
        ALWAYS_ON_MENUS
            .iter()
            .chain(MODELING_SET_MENUS.iter())
            .copied()
            .chain([HELP_MENU])
            .collect()
    }

    /// The menu-bar buttons: the first row of controls, left to right.
    fn bar(output: &egui::FullOutput) -> Vec<Control> {
        let mut found: Vec<Control> = controls(output)
            .into_iter()
            .filter(|c| c.rect.top() < 30.0)
            .collect();
        found.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
        found
    }

    #[test]
    fn menu_bar_lists_the_maya_defaults_then_the_modeling_set_then_help() {
        let (mut editor, context) = setup();
        let output = frame(&mut editor, &context, vec![]);
        let labels: Vec<String> = bar(&output).into_iter().map(|c| c.label).collect();
        assert_eq!(labels, expected_bar());
        assert_eq!(
            expected_bar(),
            [
                "File",
                "Edit",
                "Create",
                "Select",
                "Modify",
                "Display",
                "Windows",
                "Mesh",
                "Edit Mesh",
                "Mesh Tools",
                "Mesh Display",
                "Curves",
                "Surfaces",
                "Deform",
                "UV",
                "Generate",
                "Help",
            ]
        );
        assert!(!labels.iter().any(|label| label == "Render"));
    }

    #[test]
    fn overflowing_menu_bar_scrolls_both_ways_and_keeps_menus_usable_at_minimum_width() {
        let (mut editor, context) = setup();
        let size = egui::vec2(960.0, 640.0);
        context.all_styles_mut(|style| {
            style.override_font_id = Some(egui::FontId::proportional(24.0));
        });
        let mut render = |events| frame_with(&mut editor, &context, size, 1.0, events);
        render(vec![]);
        let output = render(vec![]);
        let found = bar(&output);
        let first = found.iter().find(|c| c.label == "File").unwrap();
        let last = found.iter().find(|c| c.label == HELP_MENU).unwrap();
        assert!(first.rect.left() >= 0.0);
        assert!(last.rect.right() > size.x, "must exercise actual overflow");
        assert!(found.iter().all(|c| c.rect.top() == found[0].rect.top()));
        let row_bottom = found.iter().map(|c| c.rect.bottom()).fold(0.0, f32::max);
        let status = controls(&output)
            .into_iter()
            .find(|c| c.label == "New Scene")
            .expect("Status Line New Scene button");
        assert!(status.rect.top() > row_bottom, "menu overlaps Status Line");
        let wheel_at = egui::pos2(size.x / 2.0, first.rect.center().y);
        let scroll = |delta| {
            vec![
                egui::Event::PointerMoved(wheel_at),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::Vec2::ZERO,
                    phase: egui::TouchPhase::Start,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(delta, 0.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        render(scroll(-2000.0));
        let output = render(vec![]);
        let scrolled = bar(&output);
        let help = scrolled.iter().find(|c| c.label == HELP_MENU).unwrap();
        assert!(help.rect.right() <= size.x);
        assert!(
            scrolled
                .iter()
                .any(|c| c.label == "File" && c.rect.left() < 0.0)
        );
        assert!(scrolled.iter().all(|c| c.rect.top() == first.rect.top()));
        let click_events = |at| {
            [true, false].map(|pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            })
        };
        for (label, contents) in [
            ("Generate", "Not implemented yet"),
            (HELP_MENU, "Local editor prototype"),
        ] {
            let menu = scrolled.iter().find(|c| c.label == label).unwrap();
            assert!(menu.rect.left() >= 0.0 && menu.rect.right() <= size.x);
            let at = menu.rect.center();
            render(vec![egui::Event::PointerMoved(at)]);
            for event in click_events(at) {
                render(vec![event]);
            }
            let opened = render(vec![]);
            assert!(opened.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.job.text == contents
                        && shape.clip_rect.intersects(text.galley.rect.translate(text.pos.to_vec2())))
            }), "{label} popup must show {contents}");
            egui::Popup::close_all(&context);
            render(vec![]);
        }
        render(scroll(2000.0));
        let output = render(vec![]);
        let returned = bar(&output);
        for label in ["File", "Create"] {
            let menu = returned.iter().find(|c| c.label == label).unwrap();
            assert!(menu.rect.left() >= 0.0 && menu.rect.right() <= size.x);
        }
        let at = returned
            .iter()
            .find(|c| c.label == "Create")
            .unwrap()
            .rect
            .center();
        render(vec![egui::Event::PointerMoved(at)]);
        for event in click_events(at) {
            render(vec![event]);
        }
        let opened = render(vec![]);
        let parent = controls(&opened)
            .into_iter()
            .find(|c| is_polygon_primitives_button(&c.label))
            .unwrap();
        render(vec![egui::Event::PointerMoved(parent.rect.center())]);
        let hovered = (0..3).map(|_| render(vec![])).last().unwrap();
        let cube = submenu_item(&hovered, "Cube");
        assert!(!cube.disabled);
        render(vec![egui::Event::PointerMoved(cube.rect.center())]);
        for event in click_events(cube.rect.center()) {
            render(vec![event]);
        }
        assert_eq!(editor.history.scene().nodes().len(), 2);
        let created = editor
            .single_target()
            .expect("Create menu selects the cube")
            .0;
        assert_eq!(
            editor.history.scene().node(created).unwrap().name(),
            "pCube2"
        );
    }

    #[test]
    fn placeholder_menus_open_to_one_visibly_unsupported_row() {
        let placeholders: Vec<&str> = ALWAYS_ON_MENUS[4..]
            .iter()
            .chain(MODELING_SET_MENUS.iter())
            .copied()
            .collect();
        assert_eq!(placeholders.len(), 12);
        for label in placeholders {
            let (mut editor, context) = setup();
            let output = frame(&mut editor, &context, vec![]);
            let at = bar(&output)
                .into_iter()
                .find(|c| c.label == label)
                .unwrap_or_else(|| panic!("menu {label}"))
                .rect
                .center();
            click(&mut editor, &context, at);
            let opened = frame(&mut editor, &context, vec![]);
            let rows: Vec<Control> = controls(&opened)
                .into_iter()
                .filter(|c| {
                    c.rect.top() > 20.0
                        && c.rect.left() < 1100.0
                        && c.label == "Not implemented yet"
                })
                .collect();
            assert_eq!(rows.len(), 1, "{label}");
            assert!(rows[0].disabled, "{label} row must be disabled");
        }
    }
}

mod toolbox_wire {
    use super::*;
    use eframe::egui::accesskit::Role;

    const TOOLBOX: [&str; 7] = [
        "Select",
        "Lasso",
        "Paint Selection",
        "Move",
        "Rotate",
        "Scale",
        "Last Used",
    ];

    pub(super) struct Control {
        pub(super) label: String,
        pub(super) rect: egui::Rect,
        pub(super) disabled: bool,
        pub(super) selected: Option<bool>,
    }

    pub(super) fn controls(output: &egui::FullOutput) -> Vec<Control> {
        let update = output
            .platform_output
            .accesskit_update
            .as_ref()
            .expect("accesskit enabled");
        update
            .nodes
            .iter()
            .filter(|(_, node)| node.role() == Role::Button)
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(Control {
                    label: node.label()?.to_owned(),
                    rect: egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                    disabled: node.is_disabled(),
                    selected: node
                        .toggled()
                        .map(|t| t == eframe::egui::accesskit::Toggled::True),
                })
            })
            .collect()
    }

    pub(super) fn setup() -> (Editor, egui::Context) {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        editor.create(Primitive::Cube);
        (editor, context)
    }

    fn toolbox(output: &egui::FullOutput) -> Vec<Control> {
        let mut found: Vec<Control> = controls(output)
            .into_iter()
            .filter(|c| TOOLBOX.contains(&c.label.as_str()) && c.rect.right() < 100.0)
            .collect();
        found.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
        found
    }

    pub(super) fn click(editor: &mut Editor, context: &egui::Context, at: egui::Pos2) {
        frame(editor, context, vec![egui::Event::PointerMoved(at)]);
        for pressed in [true, false] {
            frame(
                editor,
                context,
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }

    fn center_of(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
        let found = controls(output)
            .into_iter()
            .find(|c| c.label == label && !c.disabled)
            .unwrap_or_else(|| panic!("enabled control {label}"));
        found.rect.center()
    }

    #[test]
    fn toolbox_is_seven_labelled_icon_buttons_in_documented_order() {
        let (mut editor, context) = setup();
        let output = frame(&mut editor, &context, vec![]);
        let found = toolbox(&output);
        let labels: Vec<&str> = found.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, TOOLBOX);
        for control in &found {
            let live = matches!(
                control.label.as_str(),
                "Select" | "Move" | "Rotate" | "Scale"
            );
            assert_eq!(control.disabled, !live, "{}", control.label);
        }
        let all = controls(&output);
        let column = found[0].rect.x_range();
        let mut in_column: Vec<&Control> = all
            .iter()
            .filter(|c| c.rect.top() >= found[0].rect.top() && column.contains(c.rect.center().x))
            .collect();
        in_column.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
        let column_labels: Vec<&str> = in_column.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            column_labels, TOOLBOX,
            "every control in the Tool Box column"
        );
        assert!(
            controls(&output)
                .iter()
                .all(|c| !matches!(c.label.as_str(), "Tools" | "Snap")),
            "Maya 2026 Tool Box has no Tools label or Snap button"
        );
        assert_eq!(found[0].selected, Some(true));
        assert_eq!(found[3].selected, Some(false));
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && shape.clip_rect.right() < 100.0
            {
                let t = text.galley.job.text.as_str();
                assert!(
                    !matches!(t, "Move" | "Rotate" | "Scale" | "Select" | "Tools" | "Snap"),
                    "toolbox painted text {t}"
                );
            }
        }
    }

    #[test]
    fn the_live_tools_toggle_the_active_tool_and_others_do_nothing() {
        let (mut editor, context) = setup();
        let output = frame(&mut editor, &context, vec![]);
        let found = toolbox(&output);
        let move_at = found[3].rect.center();
        let select_at = found[0].rect.center();
        click(&mut editor, &context, move_at);
        assert_eq!(editor.active_tool, ActiveTool::Move);
        click(&mut editor, &context, select_at);
        assert_eq!(editor.active_tool, ActiveTool::Select);
        for (index, tool) in [(4, ActiveTool::Rotate), (5, ActiveTool::Scale)] {
            click(&mut editor, &context, found[index].rect.center());
            assert_eq!(editor.active_tool, tool, "{}", found[index].label);
            assert!(editor.status.contains(" tool ("), "{}", editor.status);
            click(&mut editor, &context, select_at);
            assert_eq!(editor.active_tool, ActiveTool::Select);
        }
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        for index in [1, 2, 6] {
            click(&mut editor, &context, found[index].rect.center());
            assert_eq!(
                editor.active_tool,
                ActiveTool::Select,
                "{}",
                found[index].label
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.scene_token, token);
            assert!(!editor.history.can_redo());
        }
    }

    #[test]
    fn hovering_enabled_and_disabled_icons_shows_their_tooltips() {
        for (index, expected) in [
            (0, "Select tool (Q)"),
            (3, "Move tool (W)"),
            (4, "Rotate tool (E)"),
            (5, "Scale tool (R)"),
            (1, "Lasso: not implemented yet"),
            (
                6,
                "Last Used: not implemented yet; dynamic last-used behaviour unsupported",
            ),
        ] {
            let (mut editor, context) = setup();
            let output = frame(&mut editor, &context, vec![]);
            let at = toolbox(&output)[index].rect.center();
            let mut shown = false;
            for step in 1..=6 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        time: Some(f64::from(step)),
                        events: if step == 1 {
                            vec![egui::Event::PointerMoved(at)]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| editor.render(ui),
                );
                output.textures_delta.clear();
                shown |= output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == expected)
                });
            }
            assert!(shown, "tooltip {expected}");
        }
    }

    fn selected_name(editor: &Editor) -> String {
        editor
            .history
            .scene()
            .node(editor.single_target().expect("selection").0)
            .unwrap()
            .name()
            .to_owned()
    }

    fn disabled(output: &egui::FullOutput, label: &str) -> bool {
        controls(output)
            .into_iter()
            .find(|c| c.label == label)
            .unwrap_or_else(|| panic!("control {label}"))
            .disabled
    }

    #[test]
    fn select_and_move_are_disabled_and_inert_while_modal_or_closing() {
        for closing in [false, true] {
            let (mut editor, context) = setup();
            if closing {
                editor.close_document().unwrap();
            } else {
                editor.new_document().unwrap();
            }
            let output = frame(&mut editor, &context, vec![]);
            let found = toolbox(&output);
            for (index, start) in [(0, ActiveTool::Move), (3, ActiveTool::Select)] {
                editor.active_tool = start;
                assert!(found[index].disabled, "{}", found[index].label);
                click(&mut editor, &context, found[index].rect.center());
                assert_eq!(editor.active_tool, start, "{}", found[index].label);
            }
        }
    }

    #[test]
    fn shelf_controls_are_disabled_without_their_preconditions() {
        let context = egui::Context::default();
        context.enable_accesskit();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        let output = frame(&mut editor, &context, vec![]);
        for label in ["Undo", "Redo", "Frame selected  F"] {
            assert!(disabled(&output, label), "{label}");
        }
        for label in ["Cube", "Plane"] {
            assert!(!disabled(&output, label), "{label}");
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.ma");
        std::fs::write(&path, ascii_fixture::specimen()).unwrap();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let output = frame(&mut editor, &context, vec![]);
        for label in ["Cube", "Plane"] {
            assert!(disabled(&output, label), "{label}");
        }
    }

    #[test]
    fn shelf_buttons_still_create_and_run_history_and_frame() {
        let (mut editor, context) = setup();
        let before = editor.camera.clone();
        let output = frame(&mut editor, &context, vec![]);
        click(
            &mut editor,
            &context,
            center_of(&output, "Frame selected  F"),
        );
        assert_ne!(editor.camera, before);
        let baseline = editor.history.scene().clone();
        let output = frame(&mut editor, &context, vec![]);
        click(&mut editor, &context, center_of(&output, "Plane"));
        assert_ne!(editor.history.scene(), &baseline);
        assert!(selected_name(&editor).contains("Plane"));
        let with_plane = editor.history.scene().clone();
        let output = frame(&mut editor, &context, vec![]);
        click(&mut editor, &context, center_of(&output, "Cube"));
        assert_ne!(editor.history.scene(), &with_plane);
        assert!(selected_name(&editor).contains("Cube"));
        let with_cube = editor.history.scene().clone();
        let output = frame(&mut editor, &context, vec![]);
        click(&mut editor, &context, center_of(&output, "Undo"));
        assert_eq!(editor.history.scene(), &with_plane);
        let output = frame(&mut editor, &context, vec![]);
        click(&mut editor, &context, center_of(&output, "Redo"));
        assert_eq!(editor.history.scene(), &with_cube);
    }
}

#[test]
fn startup_status_names_every_creatable_primitive() {
    assert_eq!(
        Editor::default().status,
        "Ready | Create a cube, plane, sphere, cylinder or cone to start"
    );
}

#[test]
fn create_menu_polygon_sphere_item_creates_a_selectable_framed_movable_sphere() {
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    let output = open_polygon_primitives(&mut editor, &ctx, egui::vec2(1440.0, 900.0));
    let sphere = submenu_item(&output, "Sphere").rect.center();
    click_at(&mut editor, &ctx, egui::vec2(1440.0, 900.0), sphere);
    let id = editor
        .single_target()
        .expect("menu item selects the new sphere")
        .0;
    assert_eq!(editor.history.scene().node(id).unwrap().name(), "pSphere1");
    assert_eq!(editor.history.scene().shapes()[0].mesh().faces().len(), 400);
    assert!(editor.history.can_undo());
    assert!(editor.is_dirty());

    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    let triangle = selected_face(&editor, 0);
    editor
        .apply_operation(Operation::SelectFace {
            target: Some(triangle.clone()),
        })
        .unwrap();
    assert_eq!(editor.remembered_face.as_ref(), Some(&triangle));
    let camera = editor.camera.clone();
    editor.apply_operation(Operation::FrameSelected).unwrap();
    assert_ne!(editor.camera, camera);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
        .unwrap();

    editor
        .apply_operation(Operation::SetActiveTool(ActiveTool::Move))
        .unwrap();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginMove {
            target,
            axis: azteccraft::native_scene::WorldAxis::X,
        })
        .unwrap();
    let move_id = editor.history.active_manipulation().unwrap().id;
    editor
        .apply_operation(Operation::UpdateMove {
            id: move_id,
            distance: crate::editor_operations::MoveDistance::Centimeters(5.0),
        })
        .unwrap();
    editor
        .apply_operation(Operation::FinishMove {
            id: move_id,
            finish: azteccraft::history::ManipulationFinish::Commit,
        })
        .unwrap();
    editor.history.scene().validate().unwrap();
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        5.0
    );
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    assert_eq!(view.instances()[0].faces().len(), 400);
}

#[test]
fn cylinder_and_cone_names_count_up_and_skip_collisions() {
    let mut editor = Editor::default();
    for (primitive, names) in [
        (
            Primitive::Cylinder,
            ["pCylinder1", "pCylinder2", "pCylinder4"],
        ),
        (Primitive::Cone, ["pCone1", "pCone2", "pCone4"]),
    ] {
        let base = names[0].trim_end_matches('1');
        for (expected, taken) in names.iter().zip([None, None, Some(format!("{base}3"))]) {
            if let Some(taken) = taken {
                editor
                    .apply_operation(Operation::Create {
                        primitive: Primitive::Cube,
                        options: None,
                        name: Some(taken),
                    })
                    .unwrap();
            }
            editor.create(primitive);
            let id = editor.single_target().unwrap().0;
            assert_eq!(editor.history.scene().node(id).unwrap().name(), *expected);
        }
    }
}

#[test]
fn borrowed_selection_readers_preserve_exact_paths_and_operation_baselines() {
    let mut editor = deep_group_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    let leaf = paths.last().unwrap().clone();
    assert_eq!(leaf.path.len(), 3);
    assert!(!editor.history.can_undo() && !editor.history.can_redo());
    let state = |editor: &Editor| {
        (
            click_baseline(editor),
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.history.undo_label(),
            editor.history.redo_label(),
            editor.name.clone(),
            editor.channels.clone(),
        )
    };
    for target in [
        None,
        Some(leaf.clone()),
        Some(paths[0].clone()),
        Some(leaf),
        None,
    ] {
        remote(
            &mut editor,
            Operation::Select {
                node: target.as_ref().map(|target| target.node),
                instance_path: target.as_ref().map(|target| target.path.clone()),
            },
        )
        .unwrap();
        let before_inspection = state(&editor);
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection")
        };
        assert_eq!(state(&editor), before_inspection);
        assert_eq!(
            inspection.selected_node,
            target.as_ref().map(|target| target.node.to_decimal())
        );
        assert_eq!(
            inspection.selected_instance_path,
            target
                .as_ref()
                .map(|target| target
                    .path
                    .iter()
                    .map(|id| id.to_decimal())
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
        if let Some(target) = target {
            assert_eq!(
                editor.single_target().unwrap(),
                (target.node, target.path.as_slice())
            );
            let before = state(&editor);
            assert!(matches!(
                remote(
                    &mut editor,
                    Operation::Select {
                        node: Some(target.node),
                        instance_path: Some(target.path[1..].to_vec()),
                    }
                ),
                Err(OperationError::Scene(
                    azteccraft::native_scene::NativeError::InvalidInstancePath
                ))
            ));
            assert_eq!(state(&editor), before);
        } else {
            assert!(matches!(
                editor.single_target(),
                Err(OperationError::NoSelection)
            ));
        }
    }
}

#[test]
fn selection_presentation_has_no_path_without_a_node() {
    let mut editor = deep_group_editor();
    let leaf = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .pop()
        .unwrap();
    editor.set_selection_for_test(None, leaf.path.clone());
    let before = (
        click_baseline(&editor),
        editor.history.can_undo(),
        editor.history.can_redo(),
        editor.history.undo_label(),
        editor.history.redo_label(),
        editor.name.clone(),
        editor.channels.clone(),
    );
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(inspection.selected_node, None);
    assert!(inspection.selected_instance_path.is_empty());
    assert_eq!(editor.selection_presentation(), (None, &[][..]));
    assert!(matches!(
        editor.single_target(),
        Err(OperationError::NoSelection)
    ));
    assert_eq!(
        (
            click_baseline(&editor),
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.history.undo_label(),
            editor.history.redo_label(),
            editor.name.clone(),
            editor.channels.clone()
        ),
        before
    );
}

#[test]
fn selected_move_target_keeps_full_owned_paths_across_selection_changes() {
    let mut editor = deep_group_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    assert_eq!(paths.last().unwrap().path.len(), 3);
    let state = |editor: &Editor| {
        (
            click_baseline(editor),
            (
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.history.undo_label(),
                editor.history.redo_label(),
            ),
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
            ),
        )
    };
    for target in [paths[0].clone(), paths.last().unwrap().clone()] {
        remote(
            &mut editor,
            Operation::Select {
                node: Some(target.node),
                instance_path: Some(target.path.clone()),
            },
        )
        .unwrap();
        let before = state(&editor);
        let owned = editor.selected_move_target().unwrap();
        assert_eq!(owned, target);
        assert_eq!(state(&editor), before);
        remote(
            &mut editor,
            Operation::Select {
                node: None,
                instance_path: None,
            },
        )
        .unwrap();
        let cleared = state(&editor);
        assert!(matches!(
            editor.selected_move_target(),
            Err(OperationError::NoSelection)
        ));
        assert_eq!(state(&editor), cleared);
        assert_eq!(owned, target);
        remote(
            &mut editor,
            Operation::Select {
                node: Some(paths[1].node),
                instance_path: Some(paths[1].path.clone()),
            },
        )
        .unwrap();
        assert_eq!(owned, target);
    }
}

#[test]
fn frame_selected_infers_empty_stored_path_and_preserves_authoring_state() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = editor.selected_move_target().unwrap();
    editor.camera.pan(glam::vec2(4.0, 3.0)).unwrap();
    let unframed = editor.camera.clone();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let mut expected = unframed.clone();
    expected
        .frame_view(&view, Some(&target.path), editor.aspect)
        .unwrap();
    assert_ne!(expected, unframed);
    let state = |editor: &Editor| {
        (
            (
                editor.history.scene().clone(),
                editor.selection_mode,
                editor.selection_presentation().0,
                editor.selection_presentation().1.to_vec(),
                editor.remembered_face.clone(),
                editor.remembered_vertex.clone(),
                editor.remembered_edge.clone(),
                editor.scene_token,
            ),
            (
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.history.undo_label(),
                editor.history.redo_label(),
            ),
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
            ),
        )
    };
    let full_path_baseline = click_baseline(&editor);
    for defensive_empty_path in [false, true] {
        editor.camera = unframed.clone();
        editor.set_selection_for_test(
            editor.selected(),
            if defensive_empty_path {
                Vec::new()
            } else {
                target.path.clone()
            },
        );
        assert_eq!(
            click_baseline(&editor) == full_path_baseline,
            !defensive_empty_path
        );
        let before = state(&editor);
        assert_eq!(
            editor.selected_move_target().unwrap().path,
            if defensive_empty_path {
                &[][..]
            } else {
                target.path.as_slice()
            }
        );
        remote(&mut editor, Operation::FrameSelected).unwrap();
        assert_eq!(editor.camera, expected);
        assert_eq!(state(&editor), before);
    }
    remote(
        &mut editor,
        Operation::Select {
            node: None,
            instance_path: None,
        },
    )
    .unwrap();
    let before = (click_baseline(&editor), state(&editor));
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::NoSelection)
    ));
    assert_eq!((click_baseline(&editor), state(&editor)), before);
}

/// A group (translate x = 2) holding a child transform that owns one triangle mesh.
/// Mirrored by `grouped_scene_bytes()` in src/mcp_tests/official_client/selection.rs; this module builds without the mcp feature, so the builder cannot be shared.
fn group_editor() -> Editor {
    nested_group_editor(&[b"child"])
}

/// Like `group_editor()` but with the transform chain group > mid > child above the shape.
fn deep_group_editor() -> Editor {
    nested_group_editor(&[b"mid", b"child"])
}

/// A group (translate x = 2) holding the chain `names` of transforms; the last owns one triangle mesh.
fn nested_group_editor(names: &[&[u8]]) -> Editor {
    use material_wire as wire;
    let mut parent: &[u8] = b"group";
    let mut chain = wire::group(
        b"XFRM",
        &[
            wire::creation(b"group", None),
            wire::triple(b"t", [2.0, 0.0, 0.0]),
        ]
        .concat(),
        4,
    );
    for name in names {
        chain.extend(wire::group(b"XFRM", &wire::creation(name, Some(parent)), 4));
        parent = name;
    }
    chain.extend(wire::group(
        b"DMSH",
        &[
            wire::creation(b"shape", Some(parent)),
            wire::chunk(b"MESH", &wire::mesh_payload(&[0, 1, 0x6000_0002]), 8),
        ]
        .concat(),
        4,
    ));
    let scene = azteccraft::native_scene::binary::decode(wire::scene(&chain)).unwrap();
    Editor {
        history: History::new(scene),
        ..Editor::default()
    }
}

/// Clicks the viewport at the projected mean of the first instance's world vertices.
fn click_first_instance(editor: &mut Editor) {
    editor.camera = Camera::default();
    let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
    let vertices = view.instances()[0].world_vertices();
    let center = vertices.iter().copied().sum::<glam::Vec3>() / vertices.len() as f32;
    assert!((center.x - 2.0).abs() < 0.5, "world x {}", center.x);
    click_world_point(editor, center);
}

/// Clicks the viewport at the projection of `point` through the editor's current camera.
fn click_world_point(editor: &mut Editor, point: glam::Vec3) {
    let ctx = egui::Context::default();
    let run = |editor: &mut Editor, events: Vec<egui::Event>| {
        let mut rect = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                editor.viewport(ui, editor.scene_token, None, None, false, true);
                rect = ui.min_rect();
            },
        );
        output.textures_delta.clear();
        rect
    };
    let rect = run(editor, vec![]);
    let ndc = editor
        .camera
        .project(point, editor.aspect)
        .unwrap()
        .unwrap();
    let width = rect.width();
    let height = width / editor.aspect;
    let position = egui::pos2(
        rect.left() + (ndc.x + 1.0) * 0.5 * width,
        rect.bottom() - (ndc.y + 1.0) * 0.5 * height,
    );
    run(editor, vec![egui::Event::PointerMoved(position)]);
    for pressed in [true, false] {
        run(
            editor,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}

#[test]
fn hierarchy_click_on_a_descendant_selects_the_root_and_object_click_the_leaf() {
    let mut editor = group_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    let (root, child) = (&paths[0], &paths[1]);
    assert_eq!((root.path.len(), child.path.len()), (1, 2));
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    click_first_instance(&mut editor);
    assert_eq!(editor.selection_presentation().0, Some(root.node));
    assert_eq!(editor.selection_presentation().1, root.path);
    // Path containment only; painting is asserted in viewport_renderer::tests.
    let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
    assert!(
        view.instances()[0]
            .path()
            .starts_with(editor.selection_presentation().1)
    );

    let mut editor = group_editor();
    click_first_instance(&mut editor);
    assert_eq!(editor.selection_presentation().0, Some(child.node));
    assert_eq!(editor.selection_presentation().1, child.path);
}

#[test]
fn hierarchy_click_in_a_three_level_chain_selects_the_root_not_the_parent() {
    let mut editor = deep_group_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    let (root, child) = (&paths[0], &paths[2]);
    assert_eq!(
        (root.path.len(), paths[1].path.len(), child.path.len()),
        (1, 2, 3)
    );
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    click_first_instance(&mut editor);
    assert_eq!(editor.selection_presentation().0, Some(root.node));
    assert_eq!(editor.selection_presentation().1, root.path);

    let mut editor = deep_group_editor();
    click_first_instance(&mut editor);
    assert_eq!(editor.selection_presentation().0, Some(child.node));
    assert_eq!(editor.selection_presentation().1, child.path);
}

#[test]
fn delete_of_a_group_is_a_typed_error_without_mutation_in_hierarchy_mode() {
    let mut editor = group_editor();
    let root = editor.history.scene().transform_paths().unwrap()[0].node;
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    editor.native_operation(Operation::Select {
        node: Some(root),
        instance_path: None,
    });
    let child = editor.history.scene().transform_paths().unwrap()[1].node;
    let scene = editor.history.scene().clone();
    let state = |editor: &Editor| {
        (
            editor.selected(),
            editor.selected_path().to_vec(),
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.history.undo_label(),
            editor.history.redo_label(),
            editor.is_dirty(),
        )
    };
    let baseline = state(&editor);
    for (node, message) in [
        (root, "Deleting a node with children is not supported yet"),
        (
            child,
            "Deleting a node that has a parent is not supported yet",
        ),
    ] {
        let error = remote(&mut editor, Operation::Delete(node)).unwrap_err();
        assert!(matches!(
            error,
            OperationError::Scene(azteccraft::native_scene::NativeError::UnsupportedDelete(_))
        ));
        assert_eq!(error.to_string(), message);
        editor.native_operation(Operation::Delete(node));
        assert_eq!(editor.status, message);
        assert!(editor.error);
        assert_eq!(
            state(&editor),
            baseline,
            "selection, history and dirty flag"
        );
        assert_eq!(editor.history.scene(), &scene);
    }
}

#[test]
fn edit_menu_lists_delete_by_type_rows_as_disabled_and_inert() {
    for (turn, label) in ["Delete by Type", "Delete All by Type"]
        .into_iter()
        .enumerate()
    {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
        let output = frame(&mut editor, &ctx, vec![]);
        let edit = face_text_position(&output, "Edit");
        let output = face_pointer_click(&mut editor, &ctx, edit, egui::Modifiers::NONE, vec![]);
        assert!(button_disabled(&output, label), "{label} enabled");
        let row = face_text_position(&output, label);
        let reason_texts = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                        if text.galley.job.text == super::menus::DELETE_BY_TYPE_REASON)
                })
                .count()
        };
        assert_eq!(
            reason_texts(&output),
            0,
            "{label}: reason shown before hover"
        );
        let mut hovered = output;
        for step in 1..=8 {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                time: Some(10.0 + (turn * 10 + step) as f64 * 0.25),
                events: vec![egui::Event::PointerMoved(row)],
                ..Default::default()
            };
            input.viewports.entry(egui::ViewportId::ROOT).or_default();
            hovered = ctx.run_ui(input, |ui| editor.render(ui));
            hovered.textures_delta.clear();
        }
        assert_eq!(reason_texts(&hovered), 1, "{label}: hover reason");
        face_pointer_click(&mut editor, &ctx, row, egui::Modifiers::NONE, vec![]);
        assert_eq!(editor.history.scene(), &before, "{label}");
    }
}

#[test]
fn hierarchy_delete_works_on_a_flat_scene_and_select_face_is_refused() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    assert!(remote(&mut editor, Operation::SelectFace { target: None }).is_err());
    editor.native_operation(Operation::Delete(id));
    assert!(!editor.error);
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn selecting_in_hierarchy_mode_clears_the_remembered_face() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = selected_face(&editor, 0);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    editor.native_operation(Operation::SelectFace {
        target: Some(target.clone()),
    });
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    assert_eq!(editor.remembered_face.as_ref(), Some(&target));
    editor.native_operation(Operation::Select {
        node: None,
        instance_path: None,
    });
    assert_eq!(editor.remembered_face, None);
}

fn button_disabled(output: &egui::FullOutput, label: &str) -> bool {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled");
    let matches: Vec<bool> = update
        .nodes
        .iter()
        .filter(|(_, node)| {
            node.role() == eframe::egui::accesskit::Role::Button && node.label() == Some(label)
        })
        .map(|(_, node)| node.is_disabled())
        .collect();
    assert_eq!(matches.len(), 1, "button {label:?} found {matches:?}");
    matches[0]
}

#[test]
fn delete_controls_are_enabled_in_hierarchy_mode_and_disabled_in_component_modes() {
    for mode in [
        SelectionMode::Hierarchy,
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.native_operation(Operation::SetSelectionMode(mode));
        assert!(editor.selection_presentation().0.is_some());
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = frame(&mut editor, &ctx, vec![]);
        let expect_disabled = matches!(
            mode,
            SelectionMode::Face | SelectionMode::Vertex | SelectionMode::Edge
        );
        assert_eq!(
            button_disabled(&output, "Delete selected"),
            expect_disabled,
            "channel box, {mode:?}"
        );
        let edit = face_text_position(&output, "Edit");
        let output = face_pointer_click(&mut editor, &ctx, edit, egui::Modifiers::NONE, vec![]);
        assert_eq!(
            button_disabled(&output, "Delete"),
            expect_disabled,
            "edit menu, {mode:?}"
        );
    }
}

#[test]
fn create_menu_cylinder_and_cone_items_create_selectable_framed_movable_solids() {
    for (label, name, faces, cap) in [
        ("Cylinder", "pCylinder1", 22, 20),
        ("Cone", "pCone1", 21, 20),
    ] {
        let mut editor = Editor::default();
        let ctx = egui::Context::default();
        let output = open_polygon_primitives(&mut editor, &ctx, egui::vec2(1440.0, 900.0));
        let item = submenu_item(&output, label).rect.center();
        click_at(&mut editor, &ctx, egui::vec2(1440.0, 900.0), item);
        let id = editor
            .single_target()
            .expect("menu item selects the new solid")
            .0;
        assert_eq!(editor.history.scene().node(id).unwrap().name(), name);
        assert_eq!(
            editor.history.scene().shapes()[0].mesh().faces().len(),
            faces
        );
        assert!(editor.history.can_undo());
        assert!(editor.is_dirty());

        editor
            .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
            .unwrap();
        let cap_face = selected_face(&editor, cap);
        editor
            .apply_operation(Operation::SelectFace {
                target: Some(cap_face.clone()),
            })
            .unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&cap_face));
        let camera = editor.camera.clone();
        editor.apply_operation(Operation::FrameSelected).unwrap();
        assert_ne!(editor.camera, camera);
        editor
            .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
            .unwrap();

        editor
            .apply_operation(Operation::SetActiveTool(ActiveTool::Move))
            .unwrap();
        let target = editor.selected_move_target().unwrap();
        editor
            .apply_operation(Operation::BeginMove {
                target,
                axis: azteccraft::native_scene::WorldAxis::X,
            })
            .unwrap();
        let move_id = editor.history.active_manipulation().unwrap().id;
        editor
            .apply_operation(Operation::UpdateMove {
                id: move_id,
                distance: crate::editor_operations::MoveDistance::Centimeters(5.0),
            })
            .unwrap();
        editor
            .apply_operation(Operation::FinishMove {
                id: move_id,
                finish: azteccraft::history::ManipulationFinish::Commit,
            })
            .unwrap();
        editor.history.scene().validate().unwrap();
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            5.0
        );
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        assert_eq!(view.instances()[0].faces().len(), faces);
    }
}

#[test]
fn frame_selected_in_hierarchy_mode_frames_the_selected_root() {
    let mut editor = group_editor();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    click_first_instance(&mut editor);
    assert!(editor.selection_presentation().0.is_some());
    let before = editor.camera.clone();
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert_ne!(editor.camera, before);
}

#[test]
fn hierarchy_click_on_a_flat_scene_selects_what_an_object_click_selects() {
    let mut picked = Vec::new();
    for mode in [SelectionMode::Object, SelectionMode::Hierarchy] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.single_target().unwrap().0;
        editor.native_operation(Operation::SetSelectionMode(mode));
        editor.native_operation(Operation::Select {
            node: None,
            instance_path: None,
        });
        assert_eq!(editor.selection_presentation().0, None);
        editor.camera = Camera::default();
        click_world_point(&mut editor, glam::Vec3::ZERO);
        assert_eq!(editor.selection_presentation().0, Some(id), "{mode:?}");
        picked.push((
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
        ));
    }
    assert_eq!(picked[0], picked[1]);
}

#[test]
fn hierarchy_click_on_empty_space_clears_the_selection() {
    let mut editor = group_editor();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    click_first_instance(&mut editor);
    assert!(editor.selection_presentation().0.is_some());
    click_world_point(&mut editor, glam::Vec3::new(-3.0, 0.0, 0.0));
    assert_eq!(editor.selection_presentation().0, None);
    assert!(editor.selection_presentation().1.is_empty());
}

/// Two cubes: A at the origin (selected) and B translated to x = 2.
fn two_cubes() -> (Editor, NodeId, NodeId) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let a = editor.single_target().unwrap().0;
    editor.create(Primitive::Cube);
    let b = editor.single_target().unwrap().0;
    editor.channels[0] = "2".into();
    editor.channel_intents[0] = true;
    editor.select(Some(a));
    assert_eq!(editor.selection_presentation().0, Some(a));
    assert!(!editor.has_draft());
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    editor.camera = Camera::default();
    (editor, a, b)
}

#[test]
fn hierarchy_click_commits_a_valid_draft_to_the_old_target_first() {
    let (mut editor, a, b) = two_cubes();
    editor.channels[1] = "5".into();
    editor.channel_intents[1] = true;
    click_world_point(&mut editor, glam::Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(editor.selection_presentation().0, Some(b));
    let translation = |id| {
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation
    };
    assert_eq!(translation(a)[1], 5.0);
    assert_eq!(translation(b), [2.0, 0.0, 0.0]);
    assert!(!editor.has_draft());
}

#[test]
fn hierarchy_click_with_an_invalid_draft_refuses_and_keeps_the_selection() {
    let (mut editor, a, _) = two_cubes();
    editor.channels[0] = "nan".into();
    editor.channel_intents[0] = true;
    let before = editor.history.scene().clone();
    click_world_point(&mut editor, glam::Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(editor.selection_presentation().0, Some(a));
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.channels[0], "nan");
    assert!(editor.error);
}

fn selected_vertex(editor: &Editor, vertex_index: usize) -> VertexTarget {
    let face = selected_face(editor, 0);
    VertexTarget {
        transform: face.transform,
        shape: face.shape,
        path: face.path,
        vertex_index,
    }
}

fn selected_edge(editor: &Editor, edge_index: usize) -> EdgeTarget {
    let face = selected_face(editor, 0);
    EdgeTarget {
        transform: face.transform,
        shape: face.shape,
        path: face.path,
        edge_index,
    }
}

fn pick_vertex(editor: &mut Editor, index: usize) {
    let target = Some(selected_vertex(editor, index));
    remote(editor, Operation::SelectVertex { target }).unwrap();
}

fn pick_edge(editor: &mut Editor, index: usize) {
    let target = Some(selected_edge(editor, index));
    remote(editor, Operation::SelectEdge { target }).unwrap();
}

fn set_mode(editor: &mut Editor, mode: SelectionMode) {
    remote(editor, Operation::SetSelectionMode(mode)).unwrap();
}

fn counts(editor: &Editor) -> (usize, usize) {
    let shape = selected_face(editor, 0).shape;
    let mesh = editor.history.scene().shape(shape).unwrap().mesh();
    (mesh.point_count(), mesh.edges().len())
}

#[test]
fn vertex_and_edge_selection_replace_clear_and_report_unchanged() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let vertex = selected_vertex(&editor, 3);
    let edge = selected_edge(&editor, 5);
    set_mode(&mut editor, SelectionMode::Vertex);
    for (target, expect_changed) in [
        (Some(vertex.clone()), true),
        (Some(vertex.clone()), false),
        (Some(selected_vertex(&editor, 4)), true),
        (None, true),
        (None, false),
    ] {
        let reply = remote(&mut editor, Operation::SelectVertex { target }).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), expect_changed);
    }
    set_mode(&mut editor, SelectionMode::Edge);
    for (target, expect_changed) in [
        (Some(edge.clone()), true),
        (Some(edge.clone()), false),
        (None, true),
        (None, false),
    ] {
        let reply = remote(&mut editor, Operation::SelectEdge { target }).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), expect_changed);
    }
}

#[test]
fn vertex_and_edge_picks_are_kept_per_type_and_cleared_by_selection_changes() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let vertex = selected_vertex(&editor, 3);
    let edge = selected_edge(&editor, 5);
    let face = selected_face(&editor, 1);
    for (mode, op) in [
        (
            SelectionMode::Vertex,
            Operation::SelectVertex {
                target: Some(vertex.clone()),
            },
        ),
        (
            SelectionMode::Edge,
            Operation::SelectEdge {
                target: Some(edge.clone()),
            },
        ),
        (
            SelectionMode::Face,
            Operation::SelectFace {
                target: Some(face.clone()),
            },
        ),
    ] {
        set_mode(&mut editor, mode);
        remote(&mut editor, op).unwrap();
    }
    set_mode(&mut editor, SelectionMode::Object);
    assert_eq!(editor.remembered_vertex.as_ref(), Some(&vertex));
    assert_eq!(editor.remembered_edge.as_ref(), Some(&edge));
    assert_eq!(editor.remembered_face.as_ref(), Some(&face));
    let id = editor.single_target().unwrap().0;
    // Re-selecting the same object is unchanged and keeps the picks; another object clears all.
    remote(
        &mut editor,
        Operation::Select {
            node: Some(id),
            instance_path: None,
        },
    )
    .unwrap();
    assert!(editor.remembered_vertex.is_some() && editor.remembered_edge.is_some());
    editor.create(Primitive::Plane);
    assert_eq!(
        (
            &editor.remembered_vertex,
            &editor.remembered_edge,
            &editor.remembered_face
        ),
        (&None, &None, &None)
    );
    for pick in [0, 1] {
        remote(
            &mut editor,
            Operation::Select {
                node: Some(id),
                instance_path: None,
            },
        )
        .unwrap();
        set_mode(&mut editor, SelectionMode::Vertex);
        pick_vertex(&mut editor, 3);
        set_mode(&mut editor, SelectionMode::Edge);
        pick_edge(&mut editor, 5);
        if pick == 0 {
            remote(
                &mut editor,
                Operation::Select {
                    node: None,
                    instance_path: None,
                },
            )
            .unwrap();
        } else {
            editor.create(Primitive::Cone);
        }
        assert_eq!(
            (&editor.remembered_vertex, &editor.remembered_edge),
            (&None, &None)
        );
    }
}

#[test]
fn invalid_vertex_and_edge_requests_preserve_the_baseline() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let first = editor.single_target().unwrap().0;
    editor.create(Primitive::Plane);
    let (points, edges) = counts(&editor);
    let state = |editor: &Editor| {
        (
            editor.history.scene().clone(),
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec(),
            editor.selection_mode,
            editor.remembered_vertex.clone(),
            editor.remembered_edge.clone(),
            editor.remembered_face.clone(),
            editor.scene_token,
            editor.history.can_undo(),
        )
    };
    let vertex = selected_vertex(&editor, 1);
    let edge = selected_edge(&editor, 1);
    // Object mode: both the target and the null clear are refused.
    for op in [
        Operation::SelectVertex {
            target: Some(vertex.clone()),
        },
        Operation::SelectVertex { target: None },
        Operation::SelectEdge {
            target: Some(edge.clone()),
        },
        Operation::SelectEdge { target: None },
    ] {
        let before = state(&editor);
        assert!(matches!(
            remote(&mut editor, op),
            Err(OperationError::InvalidInput(_))
        ));
        assert_eq!(state(&editor), before);
    }
    set_mode(&mut editor, SelectionMode::Vertex);
    remote(
        &mut editor,
        Operation::SelectVertex {
            target: Some(vertex.clone()),
        },
    )
    .unwrap();
    let mut short = vertex.clone();
    short.path.remove(0);
    let mut wrong_shape = vertex.clone();
    wrong_shape.shape = editor
        .history
        .scene()
        .shapes()
        .iter()
        .map(|shape| shape.id())
        .find(|id| *id != vertex.shape)
        .unwrap();
    let mut wrong_node = vertex.clone();
    wrong_node.transform = first;
    for bad in [
        VertexTarget {
            vertex_index: points,
            ..vertex.clone()
        },
        VertexTarget {
            vertex_index: usize::MAX,
            ..vertex.clone()
        },
        short,
        wrong_shape,
        wrong_node,
    ] {
        let before = state(&editor);
        assert!(remote(&mut editor, Operation::SelectVertex { target: Some(bad) }).is_err());
        assert_eq!(state(&editor), before);
    }
    // Wrong mode for the operation, including null.
    for op in [
        Operation::SelectEdge {
            target: Some(edge.clone()),
        },
        Operation::SelectEdge { target: None },
    ] {
        let before = state(&editor);
        assert!(remote(&mut editor, op).is_err());
        assert_eq!(state(&editor), before);
    }
    set_mode(&mut editor, SelectionMode::Edge);
    for op in [
        Operation::SelectVertex {
            target: Some(vertex),
        },
        Operation::SelectVertex { target: None },
        Operation::SelectEdge {
            target: Some(EdgeTarget {
                edge_index: edges,
                ..edge
            }),
        },
    ] {
        let before = state(&editor);
        assert!(remote(&mut editor, op).is_err());
        assert_eq!(state(&editor), before);
    }
}

#[test]
fn undoing_the_creation_clears_vertex_and_edge_picks() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 2);
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_vertex, None);
    remote(&mut editor, Operation::Redo).unwrap();
    assert_eq!(editor.remembered_vertex, None);
    editor.set_selection_for_test(Some(editor.history.scene().nodes()[0].id()), Vec::new());
    editor.sync_fields();
    set_mode(&mut editor, SelectionMode::Edge);
    pick_edge(&mut editor, 2);
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_edge, None);
}

#[test]
fn undoing_an_extrusion_clears_new_vertex_and_edge_picks_and_redo_does_not_restore_them() {
    for edge_mode in [false, true] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let (old_points, old_edges) = counts(&editor);
        let cap = selected_face(&editor, 0);
        remote(
            &mut editor,
            Operation::ExtrudeFace {
                target: cap,
                distance: 0.25,
            },
        )
        .unwrap();
        let (points, edges) = counts(&editor);
        assert!(points > old_points && edges > old_edges);
        if edge_mode {
            set_mode(&mut editor, SelectionMode::Edge);
            pick_edge(&mut editor, edges - 1);
            assert!(editor.remembered_edge.is_some());
        } else {
            set_mode(&mut editor, SelectionMode::Vertex);
            pick_vertex(&mut editor, points - 1);
            assert!(editor.remembered_vertex.is_some());
        }
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(counts(&editor), (old_points, old_edges));
        assert_eq!(editor.remembered_vertex, None);
        assert_eq!(editor.remembered_edge, None);
        remote(&mut editor, Operation::Redo).unwrap();
        assert_eq!(counts(&editor), (points, edges));
        assert_eq!(editor.remembered_vertex, None);
        assert_eq!(editor.remembered_edge, None);
    }
}

#[test]
fn a_still_valid_vertex_pick_survives_undo_of_an_unrelated_edit() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.channels[0] = "2".into();
    editor.native_operation(Operation::FrameSelected);
    set_mode(&mut editor, SelectionMode::Vertex);
    let vertex = selected_vertex(&editor, 3);
    remote(
        &mut editor,
        Operation::SelectVertex {
            target: Some(vertex.clone()),
        },
    )
    .unwrap();
    editor.name = "renamed".into();
    editor.commit_pending();
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_vertex.as_ref(), Some(&vertex));
}

#[test]
fn new_scene_keeps_the_mode_and_clears_vertex_and_edge_picks() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 1);
    editor.remembered_edge = Some(selected_edge(&editor, 1));
    // The scene is dirty after Create, so New asks first; discard to replace it.
    editor.native_operation(Operation::NewScene);
    if let Some(pending) = editor.files.pending_inspection() {
        remote(
            &mut editor,
            Operation::ResolveFileAction {
                pending_id: Uuid::parse_str(&pending.id).unwrap(),
                decision: crate::editor_operations::FileDecision::Discard,
            },
        )
        .unwrap();
    }
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    assert_eq!(
        (&editor.remembered_vertex, &editor.remembered_edge),
        (&None, &None)
    );
}

#[test]
fn selecting_another_object_clears_vertex_and_edge_picks() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let first = editor.single_target().unwrap().0;
    editor.create(Primitive::Plane);
    let second = editor.single_target().unwrap().0;
    for node in [first, second] {
        remote(
            &mut editor,
            Operation::Select {
                node: Some(node),
                instance_path: None,
            },
        )
        .unwrap();
        set_mode(&mut editor, SelectionMode::Vertex);
        pick_vertex(&mut editor, 1);
        set_mode(&mut editor, SelectionMode::Edge);
        pick_edge(&mut editor, 1);
        let other = if node == first { second } else { first };
        remote(
            &mut editor,
            Operation::Select {
                node: Some(other),
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(
            (&editor.remembered_vertex, &editor.remembered_edge),
            (&None, &None)
        );
    }
}

#[test]
fn native_vertex_and_edge_selection_leave_the_status_untouched() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    for mode in [SelectionMode::Vertex, SelectionMode::Edge] {
        editor.selection_mode = mode;
        editor.status = "marker".into();
        let operation = if mode == SelectionMode::Vertex {
            Operation::SelectVertex {
                target: Some(selected_vertex(&editor, 1)),
            }
        } else {
            Operation::SelectEdge {
                target: Some(selected_edge(&editor, 1)),
            }
        };
        editor.native_operation(operation);
        assert!(!editor.error);
        assert_eq!(editor.status, "marker");
        assert!(editor.remembered_vertex.is_some() || editor.remembered_edge.is_some());
    }
}

#[test]
fn selecting_a_vertex_on_another_node_replaces_it_and_clears_other_picks() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let edge_on_first = selected_edge(&editor, 1);
    let face_on_first = selected_face(&editor, 1);
    let vertex_on_first = selected_vertex(&editor, 1);
    editor.create(Primitive::Plane);
    // Install picks that still resolve on the first node, so only the owner check can clear them.
    editor.remembered_edge = Some(edge_on_first);
    editor.remembered_face = Some(face_on_first);
    editor.remembered_vertex = Some(vertex_on_first);
    set_mode(&mut editor, SelectionMode::Vertex);
    let on_second = selected_vertex(&editor, 2);
    remote(
        &mut editor,
        Operation::SelectVertex {
            target: Some(on_second.clone()),
        },
    )
    .unwrap();
    assert_eq!(editor.remembered_vertex.as_ref(), Some(&on_second));
    assert_eq!(editor.remembered_edge, None);
    assert_eq!(editor.remembered_face, None);
}

#[test]
fn inspection_charges_the_remembered_vertex_and_refuses_an_overlong_path() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let mut vertex = selected_vertex(&editor, 0);
    editor.remembered_vertex = Some(vertex.clone());
    assert!(editor.inspect(false).unwrap().remembered_vertex.is_some());
    vertex.path = vec![vertex.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
    editor.remembered_vertex = Some(vertex);
    assert!(matches!(
        editor.inspect(false),
        Err(OperationError::TooLarge)
    ));
}

#[test]
fn inspection_charges_the_remembered_edge_and_refuses_an_overlong_path() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let mut edge = selected_edge(&editor, 0);
    editor.remembered_edge = Some(edge.clone());
    assert!(editor.inspect(false).unwrap().remembered_edge.is_some());
    edge.path = vec![edge.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
    editor.remembered_edge = Some(edge);
    assert!(matches!(
        editor.inspect(false),
        Err(OperationError::TooLarge)
    ));
}

#[test]
fn overlong_vertex_and_edge_paths_are_rejected_without_mutation() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = (
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
    );
    let mut vertex = selected_vertex(&editor, 0);
    let mut edge = selected_edge(&editor, 0);
    set_mode(&mut editor, SelectionMode::Vertex);
    vertex.path.push(vertex.shape);
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SelectVertex {
                target: Some(vertex.clone())
            }
        ),
        Err(OperationError::Scene(
            azteccraft::native_scene::NativeError::InvalidInstancePath
        ))
    ));
    vertex.path = vec![vertex.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SelectVertex {
                target: Some(vertex)
            }
        ),
        Err(OperationError::InvalidInput(_))
    ));
    set_mode(&mut editor, SelectionMode::Edge);
    edge.path.push(edge.shape);
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SelectEdge {
                target: Some(edge.clone())
            }
        ),
        Err(OperationError::Scene(
            azteccraft::native_scene::NativeError::InvalidInstancePath
        ))
    ));
    edge.path = vec![edge.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
    assert!(matches!(
        remote(&mut editor, Operation::SelectEdge { target: Some(edge) }),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(
        (
            editor.selection_presentation().0,
            editor.selection_presentation().1.to_vec()
        ),
        before
    );
    assert_eq!(
        (&editor.remembered_vertex, &editor.remembered_edge),
        (&None, &None)
    );
}

#[test]
fn undo_clears_only_the_stale_pick_and_keeps_valid_ones() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let cap = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::ExtrudeFace {
            target: cap,
            distance: 0.25,
        },
    )
    .unwrap();
    let (_, edges) = counts(&editor);
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 0);
    set_mode(&mut editor, SelectionMode::Edge);
    pick_edge(&mut editor, edges - 1);
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_edge, None);
    assert_eq!(editor.remembered_vertex.as_ref().unwrap().vertex_index, 0);
}

#[test]
fn delete_hover_text_prefers_read_only_then_names_the_component_in_component_modes() {
    let mut editor = Editor::default();
    for (mode, menu, button) in [
        (
            SelectionMode::Face,
            "Object or Hierarchy selection required; face deletion is unsupported",
            "Face deletion is unsupported; switch to Object or Hierarchy selection",
        ),
        (
            SelectionMode::Vertex,
            "Object or Hierarchy selection required; vertex deletion is unsupported",
            "Vertex deletion is unsupported; switch to Object or Hierarchy selection",
        ),
        (
            SelectionMode::Edge,
            "Object or Hierarchy selection required; edge deletion is unsupported",
            "Edge deletion is unsupported; switch to Object or Hierarchy selection",
        ),
    ] {
        editor.selection_mode = mode;
        assert_eq!(editor.delete_disabled_hover(None, true), menu);
        assert_eq!(editor.delete_disabled_hover(None, false), button);
        // Object mode would not enable Delete on a read-only scene, so that reason wins.
        assert_eq!(
            editor.delete_disabled_hover(Some("read only"), true),
            "read only"
        );
        assert_eq!(
            editor.delete_disabled_hover(Some("read only"), false),
            "read only"
        );
    }
    for mode in [SelectionMode::Object, SelectionMode::Hierarchy] {
        editor.selection_mode = mode;
        assert_eq!(
            editor.delete_disabled_hover(None, true),
            "Select an object to delete"
        );
        assert_eq!(
            editor.delete_disabled_hover(Some("read only"), true),
            "read only"
        );
        assert_eq!(
            editor.delete_disabled_hover(Some("read only"), false),
            "read only"
        );
    }
}

#[test]
fn undo_making_only_the_face_pick_stale_keeps_vertex_and_edge_picks() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let cap = selected_face(&editor, 0);
    remote(
        &mut editor,
        Operation::ExtrudeFace {
            target: cap,
            distance: 0.25,
        },
    )
    .unwrap();
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 0);
    set_mode(&mut editor, SelectionMode::Edge);
    pick_edge(&mut editor, 0);
    set_mode(&mut editor, SelectionMode::Face);
    let side = selected_face(&editor, 6);
    remote(&mut editor, Operation::SelectFace { target: Some(side) }).unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(editor.remembered_face, None);
    assert_eq!(editor.remembered_vertex.as_ref().unwrap().vertex_index, 0);
    assert_eq!(editor.remembered_edge.as_ref().unwrap().edge_index, 0);
}

fn translated_cube_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.channels[0] = "2".into();
    editor.commit_pending();
    editor
}

fn world_points(editor: &Editor, indices: &[usize]) -> Vec<glam::Vec3> {
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let instance = &view.instances()[0];
    indices
        .iter()
        .map(|&index| instance.world_vertices()[index])
        .collect()
}

#[test]
fn frame_selected_frames_the_remembered_vertex_in_world_space() {
    let mut editor = translated_cube_editor();
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 3);
    let points = world_points(&editor, &[3]);
    let mut expected = editor.camera.clone();
    expected.frame_points(&points, editor.aspect).unwrap();
    assert!(expected.target().x > 1.0, "world, not local, coordinates");
    let scene = editor.history.scene().clone();
    let token = editor.scene_token;
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert_eq!(editor.camera, expected);
    assert_eq!(editor.camera.target(), points[0]);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.scene_token, token);
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Ok(OperationReply::Unchanged)
    ));
}

#[test]
fn frame_selected_frames_both_endpoints_of_the_remembered_edge() {
    let mut editor = translated_cube_editor();
    set_mode(&mut editor, SelectionMode::Edge);
    pick_edge(&mut editor, 5);
    let shape = selected_face(&editor, 0).shape;
    let [a, b] = editor.history.scene().shape(shape).unwrap().mesh().edges()[5].endpoints();
    let points = world_points(&editor, &[a, b]);
    assert_ne!(points[0], points[1]);
    let mut expected = editor.camera.clone();
    expected.frame_points(&points, editor.aspect).unwrap();
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert_eq!(editor.camera, expected);
    assert!(
        editor
            .camera
            .target()
            .abs_diff_eq((points[0] + points[1]) * 0.5, 1e-5)
    );
    let mut single = editor.camera.clone();
    single.frame_points(&points[..1], editor.aspect).unwrap();
    assert!(editor.camera.distance() > single.distance());
}

#[test]
fn frame_selected_without_a_pick_or_with_a_stale_pick_keeps_the_camera() {
    for mode in [SelectionMode::Vertex, SelectionMode::Edge] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        set_mode(&mut editor, mode);
        let camera = editor.camera.clone();
        assert!(matches!(
            remote(&mut editor, Operation::FrameSelected),
            Err(OperationError::NoSelection)
        ));
        // A remembered pick that no longer resolves is a typed scene error.
        editor.remembered_vertex = Some(selected_vertex(&editor, 9999));
        editor.remembered_edge = Some(selected_edge(&editor, 9999));
        assert!(matches!(
            remote(&mut editor, Operation::FrameSelected),
            Err(OperationError::Scene(_))
        ));
        assert_eq!(editor.camera, camera);
    }
}

#[test]
fn frame_selected_after_undo_of_the_picked_object_is_no_selection() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 2);
    remote(&mut editor, Operation::Undo).unwrap();
    let camera = editor.camera.clone();
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::NoSelection)
    ));
    assert_eq!(editor.camera, camera);
}

#[test]
fn frame_selected_in_object_mode_ignores_vertex_and_edge_picks() {
    let mut editor = translated_cube_editor();
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 3);
    set_mode(&mut editor, SelectionMode::Object);
    let mut expected = editor.camera.clone();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    expected.frame_view(&view, None, editor.aspect).unwrap();
    remote(&mut editor, Operation::FrameSelected).unwrap();
    assert_eq!(editor.camera, expected);
}

fn key_with(key: Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// A cube in front of a head-on camera, the first frame already run.
fn component_click_editor() -> (Editor, egui::Context, egui::Rect) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let rect = face_viewport_rect(&output);
    (editor, ctx, rect)
}

fn screen_point(editor: &Editor, rect: egui::Rect, world: glam::Vec3) -> egui::Pos2 {
    let ndc = editor
        .camera
        .project(world, editor.aspect)
        .unwrap()
        .unwrap();
    egui::pos2(
        rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
        rect.bottom() - (ndc.y + 1.0) * 0.5 * rect.height(),
    )
}

/// Front-facing vertex indices (nearest to the eye) and the world position of each point.
fn front_vertices(editor: &Editor) -> (Vec<usize>, Vec<glam::Vec3>) {
    let points = world_points(editor, &(0..8).collect::<Vec<_>>());
    let eye = editor.camera.eye();
    let nearest = points
        .iter()
        .map(|point| point.distance(eye))
        .fold(f32::MAX, f32::min);
    let front = (0..points.len())
        .filter(|&index| points[index].distance(eye) < nearest + 0.01)
        .collect();
    (front, points)
}

fn click_baseline(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.selection_mode,
        editor.selection_presentation().0,
        editor.selection_presentation().1.to_vec(),
        editor.remembered_face.clone(),
        editor.remembered_vertex.clone(),
        editor.remembered_edge.clone(),
        editor.camera.clone(),
        editor.scene_token,
    )
}

#[test]
fn f9_and_f10_switch_to_vertex_and_edge_modes_and_keep_each_remembered_pick() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let before = editor.history.scene().clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![key(Key::F9)]);
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    pick_vertex(&mut editor, 2);
    frame(&mut editor, &ctx, vec![key(Key::F10)]);
    assert_eq!(editor.selection_mode, SelectionMode::Edge);
    pick_edge(&mut editor, 3);
    frame(&mut editor, &ctx, vec![key(Key::F9)]);
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    assert_eq!(editor.remembered_vertex.as_ref().unwrap().vertex_index, 2);
    frame(&mut editor, &ctx, vec![key(Key::F11)]);
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn f9_and_f10_ignore_modified_keys() {
    for pressed in [Key::F9, Key::F10] {
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::COMMAND,
        ] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let ctx = egui::Context::default();
            frame(&mut editor, &ctx, vec![]);
            let baseline = click_baseline(&editor);
            frame(&mut editor, &ctx, vec![key_with(pressed, modifiers)]);
            assert_eq!(
                click_baseline(&editor),
                baseline,
                "{pressed:?} {modifiers:?}"
            );
        }
    }
}

#[test]
fn f9_and_f10_are_ignored_while_a_text_field_has_focus() {
    for (start, key_code) in [
        (SelectionMode::Object, Key::F9),
        (SelectionMode::Object, Key::F10),
        (SelectionMode::Vertex, Key::F10),
        (SelectionMode::Edge, Key::F9),
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        set_mode(&mut editor, start);
        ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
        frame(&mut editor, &ctx, vec![]);
        let baseline = click_baseline(&editor);
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("draft".into()), key(key_code)],
        );
        assert_eq!(editor.selection_mode, start, "{start:?} {key_code:?}");
        assert!(editor.has_draft());
        assert_eq!(click_baseline(&editor), baseline, "{start:?} {key_code:?}");
    }
}

#[test]
fn vertex_click_ignores_an_occluded_vertex_and_picks_a_visible_one() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.camera =
        Camera::new(Vec3::ZERO, 0.5, 0.3, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let rect = face_viewport_rect(&output);
    frame(&mut editor, &ctx, vec![key(Key::F9)]);
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
    let points = world_points(&editor, &(0..8).collect::<Vec<_>>());
    let eye = editor.camera.eye();
    let distance = |index: usize| points[index].distance(eye);
    let farthest = (0..8)
        .max_by(|&a, &b| distance(a).total_cmp(&distance(b)))
        .unwrap();
    let nearest = (0..8)
        .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
        .unwrap();
    let hidden = screen_point(&editor, rect, points[farthest]);
    for other in (0..8).filter(|&i| i != farthest) {
        let apart = screen_point(&editor, rect, points[other]) - hidden;
        assert!(
            apart.abs().max_elem() > 20.0,
            "vertex {other} overlaps the hidden one"
        );
    }
    let baseline = click_baseline(&editor);
    face_pointer_click(&mut editor, &ctx, hidden, egui::Modifiers::NONE, vec![]);
    assert_eq!(editor.remembered_vertex, None);
    assert_eq!(click_baseline(&editor), baseline);
    let visible = screen_point(&editor, rect, points[nearest]);
    face_pointer_click(&mut editor, &ctx, visible, egui::Modifiers::NONE, vec![]);
    assert_eq!(
        editor.remembered_vertex.as_ref().unwrap().vertex_index,
        nearest
    );
}

#[test]
fn vertex_click_commits_a_valid_draft_before_picking_the_moved_geometry() {
    let (mut editor, ctx, rect) = component_click_editor();
    frame(&mut editor, &ctx, vec![key(Key::F9)]);
    let id = editor.single_target().unwrap().0;
    let (front, points) = front_vertices(&editor);
    let index = front[0];
    let at = screen_point(&editor, rect, points[index] + Vec3::new(2.0, 0.0, 0.0));
    frame(&mut editor, &ctx, vec![egui::Event::PointerMoved(at)]);
    editor.channels[0] = "2".into();
    editor.channel_intents[0] = true;
    for pressed in [true, false] {
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        2.0
    );
    assert!(!editor.has_draft());
    assert_eq!(
        editor.remembered_vertex.as_ref().unwrap().vertex_index,
        index
    );
}

#[test]
fn f9_and_f10_are_blocked_by_popups_modals_and_closing() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    let output = frame(&mut editor, &ctx, vec![]);
    let select = face_text_position(&output, "Select");
    face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
    assert!(egui::Popup::is_any_open(&ctx));
    let baseline = click_baseline(&editor);
    frame(&mut editor, &ctx, vec![key(Key::F9), key(Key::F10)]);
    assert_eq!(click_baseline(&editor), baseline, "popup");

    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.new_document().unwrap();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    assert!(editor.files.is_modal());
    let baseline = click_baseline(&editor);
    frame(&mut editor, &ctx, vec![key(Key::F9), key(Key::F10)]);
    assert_eq!(click_baseline(&editor), baseline, "modal");

    let mut editor = Editor::default();
    editor.close_document().unwrap();
    assert!(editor.files.is_closing());
    let baseline = click_baseline(&editor);
    frame(&mut editor, &ctx, vec![key(Key::F9), key(Key::F10)]);
    assert_eq!(click_baseline(&editor), baseline, "closing");
}

#[derive(Clone, Copy, Debug)]
enum Block {
    Modal,
    Closing,
    Popup,
}

/// A cube editor whose first frame captured the viewport, in `mode`, then put into `block`.
fn blocked_component_editor(
    block: Option<Block>,
    mode: SelectionMode,
) -> (Editor, egui::Context, egui::Rect) {
    let (mut editor, ctx, rect) = component_click_editor();
    set_mode(&mut editor, mode);
    match block {
        None => {}
        Some(block) => block_editor(&mut editor, &ctx, block),
    }
    (editor, ctx, rect)
}

fn block_editor(editor: &mut Editor, ctx: &egui::Context, block: Block) {
    match block {
        Block::Modal => {
            editor.new_document().unwrap();
            frame(editor, ctx, vec![]);
            frame(editor, ctx, vec![]);
            assert!(editor.files.is_modal());
        }
        Block::Closing => {
            let Ok(OperationReply::PendingConfirmation {
                pending_action_id, ..
            }) = editor.close_document()
            else {
                panic!("a dirty scene must ask before closing");
            };
            editor
                .apply_operation(Operation::ResolveFileAction {
                    pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                    decision: crate::editor_operations::FileDecision::Discard,
                })
                .unwrap();
            frame(editor, ctx, vec![]);
            assert!(editor.files.is_closing());
            assert!(!editor.files.is_modal());
        }
        Block::Popup => {
            let output = frame(editor, ctx, vec![]);
            let select = face_text_position(&output, "Select");
            face_pointer_click(editor, ctx, select, egui::Modifiers::NONE, vec![]);
            assert!(egui::Popup::is_any_open(ctx));
        }
    }
}

#[test]
fn vertex_and_edge_key_intents_are_dropped_while_blocked_with_the_viewport_captured() {
    for block in [Block::Modal, Block::Closing, Block::Popup] {
        for key_code in [Key::F9, Key::F10] {
            let (mut editor, ctx, _) = blocked_component_editor(Some(block), SelectionMode::Object);
            let baseline = click_baseline(&editor);
            frame(&mut editor, &ctx, vec![key(key_code)]);
            assert_eq!(click_baseline(&editor), baseline, "{block:?} {key_code:?}");
        }
    }
}

#[test]
fn vertex_and_edge_clicks_are_dropped_while_blocked_with_the_viewport_captured() {
    // A click outside an open popup first dismisses it, so only modal and closing can block it.
    for block in [Block::Modal, Block::Closing] {
        for mode in [SelectionMode::Vertex, SelectionMode::Edge] {
            let (mut control, ctx, rect) = blocked_component_editor(None, mode);
            let (front, points) = front_vertices(&control);
            let at = screen_point(&control, rect, points[front[0]]);
            face_pointer_click(&mut control, &ctx, at, egui::Modifiers::NONE, vec![]);
            assert!(
                control.remembered_vertex.is_some() || control.remembered_edge.is_some(),
                "{mode:?} control click did not pick"
            );
            let (mut editor, ctx, rect) = blocked_component_editor(Some(block), mode);
            let at = screen_point(&editor, rect, points[front[0]]);
            let baseline = click_baseline(&editor);
            face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
            assert_eq!(click_baseline(&editor), baseline, "{block:?} {mode:?}");
        }
    }
}

#[test]
fn select_menu_vertex_and_edge_entries_do_nothing_while_modal_or_closing() {
    for block in [Block::Modal, Block::Closing] {
        for label in ["Vertex   F9", "Edge   F10"] {
            let (mut editor, ctx, _) = blocked_component_editor(Some(block), SelectionMode::Object);
            let baseline = click_baseline(&editor);
            let output = frame(&mut editor, &ctx, vec![]);
            let select = face_text_position(&output, "Select");
            let output =
                face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
            if matches!(block, Block::Modal) {
                // The file modal covers the menu bar, so the entries cannot be reached at all.
                assert!(!egui::Popup::is_any_open(&ctx), "{label}");
                continue;
            }
            let components = face_text_position(&output, "Components");
            let output =
                face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
            let entry = face_text_position(&output, label);
            face_pointer_click(&mut editor, &ctx, entry, egui::Modifiers::NONE, vec![]);
            assert_eq!(click_baseline(&editor), baseline, "{block:?} {label}");
        }
    }
}

#[test]
fn select_menu_vertex_and_edge_entries_switch_modes() {
    for (label, mode) in [
        ("Vertex   F9", SelectionMode::Vertex),
        ("Edge   F10", SelectionMode::Edge),
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![]);
        let select = face_text_position(&output, "Select");
        let output = face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
        let components = face_text_position(&output, "Components");
        let output =
            face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
        let entry = face_text_position(&output, label);
        face_pointer_click(&mut editor, &ctx, entry, egui::Modifiers::NONE, vec![]);
        assert_eq!(editor.selection_mode, mode, "{label}");
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn viewport_click_in_vertex_mode_replaces_then_clears_the_pick() {
    let (mut editor, ctx, rect) = component_click_editor();
    frame(&mut editor, &ctx, vec![key(Key::F9)]);
    let (front, points) = front_vertices(&editor);
    assert_eq!(front.len(), 4);
    let before = editor.history.scene().clone();
    for &index in &front[..2] {
        let at = screen_point(&editor, rect, points[index]);
        face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
        let target = editor.remembered_vertex.as_ref().unwrap();
        assert_eq!(target.vertex_index, index);
        assert_eq!(Some(target.transform), editor.selection_presentation().0);
    }
    face_pointer_click(
        &mut editor,
        &ctx,
        rect.left_top() + egui::vec2(20.0, 20.0),
        egui::Modifiers::NONE,
        vec![],
    );
    assert_eq!(editor.remembered_vertex, None);
    assert!(editor.selection_presentation().0.is_some());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn viewport_click_in_edge_mode_replaces_then_clears_the_pick() {
    let (mut editor, ctx, rect) = component_click_editor();
    frame(&mut editor, &ctx, vec![key(Key::F10)]);
    let (front, points) = front_vertices(&editor);
    let shape = selected_edge(&editor, 0).shape;
    let edges = editor
        .history
        .scene()
        .shape(shape)
        .unwrap()
        .mesh()
        .edges()
        .to_vec();
    let front_edges: Vec<_> = edges
        .iter()
        .enumerate()
        .filter(|(_, edge)| edge.endpoints().iter().all(|end| front.contains(end)))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(front_edges.len(), 4);
    let before = editor.history.scene().clone();
    for &index in &front_edges[..2] {
        let [a, b] = edges[index].endpoints();
        let at = screen_point(&editor, rect, (points[a] + points[b]) * 0.5);
        face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
        assert_eq!(editor.remembered_edge.as_ref().unwrap().edge_index, index);
    }
    face_pointer_click(
        &mut editor,
        &ctx,
        rect.left_top() + egui::vec2(20.0, 20.0),
        egui::Modifiers::NONE,
        vec![],
    );
    assert_eq!(editor.remembered_edge, None);
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn modified_clicks_in_vertex_and_edge_modes_are_refused_without_changes() {
    for key_code in [Key::F9, Key::F10] {
        let (mut editor, ctx, rect) = component_click_editor();
        frame(&mut editor, &ctx, vec![key(key_code)]);
        let (front, points) = front_vertices(&editor);
        let at = screen_point(&editor, rect, points[front[0]]);
        face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
        let baseline = click_baseline(&editor);
        for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::CTRL] {
            face_pointer_click(&mut editor, &ctx, at, modifiers, vec![]);
            assert!(editor.error && editor.status.contains("unsupported"));
            assert_eq!(
                click_baseline(&editor),
                baseline,
                "{key_code:?} {modifiers:?}"
            );
        }
    }
}

#[test]
fn a_mode_intent_in_the_click_frame_makes_the_click_use_the_new_mode() {
    for (key_code, vertex) in [(Key::F9, true), (Key::F10, false)] {
        let (mut editor, ctx, rect) = component_click_editor();
        assert_eq!(
            face_viewport_rect(&frame(&mut editor, &ctx, vec![])),
            rect,
            "stale viewport rect"
        );
        let (front, points) = front_vertices(&editor);
        let at = screen_point(&editor, rect, points[front[0]]);
        let center = screen_point(&editor, rect, glam::Vec3::ZERO);
        // The vertex sits on the silhouette; a click there can miss the cube and deselect it.
        face_pointer_click(&mut editor, &ctx, center, egui::Modifiers::NONE, vec![]);
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        face_pointer_click(
            &mut editor,
            &ctx,
            at,
            egui::Modifiers::NONE,
            vec![key(key_code)],
        );
        assert_eq!(
            editor.selection_mode,
            if vertex {
                SelectionMode::Vertex
            } else {
                SelectionMode::Edge
            }
        );
        assert_eq!(editor.remembered_vertex.is_some(), vertex);
        assert_eq!(editor.remembered_edge.is_some(), !vertex);
    }
}

#[test]
fn f8_leaves_vertex_and_edge_for_object_and_enters_face_from_object_and_hierarchy() {
    for mode in [SelectionMode::Vertex, SelectionMode::Edge] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        set_mode(&mut editor, mode);
        pick_vertex_or_edge(&mut editor, mode);
        let remembered = (
            editor.remembered_vertex.clone(),
            editor.remembered_edge.clone(),
        );
        frame(&mut editor, &ctx, vec![key(Key::F8)]);
        assert_eq!(editor.selection_mode, SelectionMode::Object, "{mode:?}");
        assert_eq!(
            (
                editor.remembered_vertex.clone(),
                editor.remembered_edge.clone()
            ),
            remembered
        );
    }
    for mode in [SelectionMode::Object, SelectionMode::Hierarchy] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        set_mode(&mut editor, mode);
        frame(&mut editor, &ctx, vec![key(Key::F8)]);
        assert_eq!(editor.selection_mode, SelectionMode::Face, "{mode:?}");
    }
}

fn pick_vertex_or_edge(editor: &mut Editor, mode: SelectionMode) {
    if mode == SelectionMode::Vertex {
        pick_vertex(editor, 1);
    } else {
        pick_edge(editor, 1);
    }
}

#[test]
fn delete_in_the_frame_of_a_vertex_or_edge_intent_sees_the_new_mode() {
    for (key_code, noun) in [(Key::F9, "Vertex"), (Key::F10, "Edge")] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        frame(&mut editor, &ctx, vec![key(key_code), key(Key::Delete)]);
        assert!(
            editor.error
                && editor
                    .status
                    .contains(&format!("{noun} deletion is unsupported"))
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.selection_presentation().0.is_some());
    }
}

#[test]
fn sync_fields_clears_a_pick_whose_owner_path_differs_from_the_selected_instance() {
    // Instancing cannot be built from the bin crate, so the second instance is simulated by
    // the same transform selected under a different path.
    let mut editor = deep_group_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    let leaf = paths.last().unwrap().clone();
    assert_eq!(leaf.path.len(), 3);
    remote(
        &mut editor,
        Operation::Select {
            node: Some(leaf.node),
            instance_path: Some(leaf.path.clone()),
        },
    )
    .unwrap();
    set_mode(&mut editor, SelectionMode::Vertex);
    pick_vertex(&mut editor, 0);
    set_mode(&mut editor, SelectionMode::Edge);
    pick_edge(&mut editor, 0);
    editor.sync_fields();
    assert!(editor.remembered_vertex.is_some() && editor.remembered_edge.is_some());
    let context = egui::Context::default();
    context.enable_accesskit();
    let state = |editor: &Editor| {
        (
            click_baseline(editor),
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
            ),
            (
                editor.material_shape,
                editor.material_draft.as_ref().map(|draft| {
                    (
                        draft.text.clone(),
                        draft.original.to_bits(),
                        draft.intent,
                        draft.shape,
                        draft.scene_token,
                        draft.focus,
                    )
                }),
            ),
            context.memory(|memory| memory.focused()),
            (
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.history.undo_label(),
                editor.history.redo_label(),
                editor.history.active_manipulation().is_some(),
            ),
        )
    };
    for (phase, stale_path, child_selected) in [
        ("outliner/full", false, Some(true)),
        ("outliner/stale-path", true, Some(false)),
    ] {
        if stale_path {
            editor.set_selection_for_test(editor.selected(), leaf.path[1..].to_vec());
        }
        let expected_path = if stale_path {
            &leaf.path[1..]
        } else {
            leaf.path.as_slice()
        };
        let baseline = state(&editor);
        assert_eq!(baseline.4, (false, false, None, None, false), "{phase}");
        for _ in 0..2 {
            let mut frame = context.run_ui(egui::RawInput::default(), |ui| editor.outliner(ui));
            frame.textures_delta.clear();
            let controls = toolbox_wire::controls(&frame);
            assert_eq!(controls.len(), 3, "{phase}");
            for (label, selected) in [
                ("◇  group", Some(false)),
                ("◇  mid", Some(false)),
                ("◇  child", child_selected),
            ] {
                let mut rows = controls.iter().filter(|control| control.label == label);
                assert_eq!(rows.next().unwrap().selected, selected, "{phase}: {label}");
                assert!(rows.next().is_none(), "{phase}: {label}");
            }
        }
        assert_eq!(state(&editor), baseline, "{phase}: paint");
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection");
        };
        assert_eq!(
            inspection.selected_node,
            Some(leaf.node.to_decimal()),
            "{phase}"
        );
        assert_eq!(
            inspection.selected_instance_path,
            expected_path
                .iter()
                .map(|id| id.to_decimal())
                .collect::<Vec<_>>(),
            "{phase}"
        );
        assert_eq!(state(&editor), baseline, "{phase}: inspection");
    }
    editor.sync_fields();
    assert_eq!(editor.remembered_vertex, None);
    assert_eq!(editor.remembered_edge, None);
}

#[test]
fn vertex_edge_and_mode_operations_refuse_modal_and_closing_without_changes() {
    for (mode, pick_target) in [(SelectionMode::Vertex, true), (SelectionMode::Edge, false)] {
        for closing in [false, true] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            set_mode(&mut editor, mode);
            if pick_target {
                pick_vertex(&mut editor, 3);
            } else {
                pick_edge(&mut editor, 5);
            }
            let vertex = selected_vertex(&editor, 4);
            let edge = selected_edge(&editor, 6);
            if closing {
                let Ok(OperationReply::PendingConfirmation {
                    pending_action_id, ..
                }) = editor.close_document()
                else {
                    panic!("a dirty scene must ask before closing");
                };
                editor
                    .apply_operation(Operation::ResolveFileAction {
                        pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                        decision: crate::editor_operations::FileDecision::Discard,
                    })
                    .unwrap();
                assert!(editor.files.is_closing() && !editor.files.is_modal());
            } else {
                editor.new_document().unwrap();
                assert!(editor.files.is_modal() && !editor.files.is_closing());
            }
            let state = |editor: &Editor| {
                (
                    editor.history.scene().clone(),
                    editor.selection_presentation().0,
                    editor.selection_presentation().1.to_vec(),
                    editor.selection_mode,
                    editor.remembered_vertex.clone(),
                    editor.remembered_edge.clone(),
                    editor.remembered_face.clone(),
                    editor.scene_token,
                )
            };
            let before = state(&editor);
            let operations = || {
                [
                    Operation::SelectVertex {
                        target: Some(vertex.clone()),
                    },
                    Operation::SelectVertex { target: None },
                    Operation::SelectEdge {
                        target: Some(edge.clone()),
                    },
                    Operation::SelectEdge { target: None },
                    Operation::SetSelectionMode(SelectionMode::Object),
                    Operation::SetSelectionMode(SelectionMode::Face),
                ]
            };
            for index in 0..operations().len() {
                assert!(matches!(
                    remote(&mut editor, operations().into_iter().nth(index).unwrap()),
                    Err(OperationError::BusyEditing)
                ));
                assert_eq!(state(&editor), before);
                if closing {
                    assert!(matches!(
                        editor.apply_operation_guarded(
                            ManipulationProvenance::Native,
                            operations().into_iter().nth(index).unwrap(),
                            &|| true,
                        ),
                        Err(OperationError::BusyEditing)
                    ));
                    assert_eq!(state(&editor), before);
                }
            }
        }
    }
}

#[test]
fn native_selection_changes_refuse_modal_before_committing_drafts() {
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        set_mode(&mut editor, mode);
        let pick = |editor: &Editor, index: usize| match mode {
            SelectionMode::Face => Operation::SelectFace {
                target: Some(selected_face(editor, index)),
            },
            SelectionMode::Vertex => Operation::SelectVertex {
                target: Some(selected_vertex(editor, index)),
            },
            _ => Operation::SelectEdge {
                target: Some(selected_edge(editor, index)),
            },
        };
        let first = pick(&editor, 1);
        remote(&mut editor, first).unwrap();
        editor.new_document().unwrap();
        assert!(editor.files.is_modal() && !editor.files.is_closing());
        editor.name = "pending draft".into();
        let state = |editor: &Editor| {
            (
                editor.history.scene().clone(),
                editor.selection_presentation().0,
                editor.selection_presentation().1.to_vec(),
                editor.selection_mode,
                editor.remembered_vertex.clone(),
                editor.remembered_edge.clone(),
                editor.remembered_face.clone(),
                editor.scene_token,
                editor.name.clone(),
            )
        };
        let before = state(&editor);
        let clear = match mode {
            SelectionMode::Face => Operation::SelectFace { target: None },
            SelectionMode::Vertex => Operation::SelectVertex { target: None },
            _ => Operation::SelectEdge { target: None },
        };
        for operation in [
            pick(&editor, 2),
            clear,
            Operation::Select {
                node: None,
                instance_path: None,
            },
            Operation::SetSelectionMode(SelectionMode::Object),
            Operation::SetSelectionMode(mode),
        ] {
            let refusal = editor.apply_operation(operation);
            assert!(
                matches!(refusal, Err(OperationError::BusyEditing)),
                "{mode:?}"
            );
            assert_eq!(
                refusal.unwrap_err().to_string(),
                "The editor is busy with a text edit, dialog, close or pending input; try again when it finishes"
            );
            assert_eq!(state(&editor), before);
        }
    }
}

#[test]
fn invalid_input_echo_after_failed_open_never_carries_the_path() {
    let directory = tempfile::Builder::new()
        .prefix("zzprivatedir")
        .tempdir()
        .unwrap();
    let path = directory.path().join("zzsecret.ma");
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.status = format!("open failed for {}", path.display());
    assert!(editor.status.contains("zzprivatedir"), "fixture must leak");
    editor.channels[0] = "invalid".into();
    let Err(OperationError::InvalidInput(message)) = editor.apply_operation(sidebar_op(false))
    else {
        panic!("expected invalid_input");
    };
    assert!(!message.contains("zzprivatedir") && !message.contains("zzsecret"));
}

fn camera_after_frame_all(editor: &Editor) -> Camera {
    let mut expected = editor.camera.clone();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    expected.frame_view(&view, None, editor.aspect).unwrap();
    expected
}

#[test]
fn set_transform_moving_a_rotated_node_reports_undo_move() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    let apply = |editor: &mut Editor, transform: NativeTransform| {
        let input = editor.transform_input(transform).unwrap();
        remote(
            editor,
            Operation::SetTransform {
                id,
                transform: input,
            },
        )
        .unwrap();
    };
    let rotated = NativeTransform {
        rotation: [0.0, 45.0, 0.0],
        ..Default::default()
    };
    apply(&mut editor, rotated);
    assert_eq!(
        editor.inspect(false).unwrap().undo_label.as_deref(),
        Some("Rotate")
    );
    apply(
        &mut editor,
        NativeTransform {
            translation: [3.0, 0.0, 0.0],
            ..rotated
        },
    );
    assert_eq!(
        editor.inspect(false).unwrap().undo_label.as_deref(),
        Some("Move")
    );
    remote(&mut editor, Operation::Undo).unwrap();
    assert_eq!(
        editor.inspect(false).unwrap().redo_label.as_deref(),
        Some("Move")
    );
    assert_eq!(
        editor.inspect(false).unwrap().undo_label.as_deref(),
        Some("Rotate")
    );
}

#[test]
fn frame_all_frames_every_object_regardless_of_selection() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    let transform = NativeTransform {
        translation: [20.0, 0.0, 0.0],
        ..Default::default()
    };
    let input = editor.transform_input(transform).unwrap();
    remote(
        &mut editor,
        Operation::SetTransform {
            id,
            transform: input,
        },
    )
    .unwrap();
    editor.native_operation(Operation::SetGridVisible { visible: true });
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    assert_eq!(view.instances().len(), 2);
    let expected = camera_after_frame_all(&editor);
    for instance in view.instances() {
        let mut single = editor.camera.clone();
        single
            .frame_view(&view, Some(instance.path()), editor.aspect)
            .unwrap();
        assert_ne!(expected, single);
    }
    remote(&mut editor, Operation::FrameAll).unwrap();
    assert_eq!(editor.camera, expected);
    assert_eq!(editor.selected(), Some(id));
    assert!(matches!(
        remote(&mut editor, Operation::FrameAll),
        Ok(OperationReply::Unchanged)
    ));
}

#[test]
fn frame_all_frames_a_curve_only_scene() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    assert!(view.instances().is_empty());
    assert_eq!(view.curve_instances().len(), 1);
    let expected = camera_after_frame_all(&editor);
    editor.camera.dolly(3.0).unwrap();
    assert_ne!(expected, editor.camera);
    assert!(matches!(
        remote(&mut editor, Operation::FrameAll),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.camera, expected);
}

#[test]
fn frame_all_on_an_empty_scene_is_a_typed_refusal_that_keeps_the_camera() {
    let mut editor = Editor::default();
    let camera = editor.camera.clone();
    assert!(matches!(
        remote(&mut editor, Operation::FrameAll),
        Err(OperationError::NothingToFrame)
    ));
    assert_eq!(editor.camera, camera);
}

#[test]
fn a_key_frames_all_and_shift_a_is_ignored() {
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    editor.create(Primitive::Cube);
    let start = editor.camera.clone();
    let expected = camera_after_frame_all(&editor);
    let shifted = egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::SHIFT,
    };
    frame(&mut editor, &ctx, vec![shifted]);
    assert_eq!(editor.camera, start);
    let release = egui::Event::Key {
        key: Key::A,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&mut editor, &ctx, vec![release]);
    frame(&mut editor, &ctx, vec![key(Key::A)]);
    assert_eq!(editor.camera, expected);
}

#[test]
fn a_key_is_ignored_while_a_text_field_has_focus() {
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    editor.create(Primitive::Cube);
    let start = editor.camera.clone();
    ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![key(Key::A)]);
    assert_eq!(editor.camera, start);
}

#[test]
fn view_menu_frame_all_row_dispatches_the_frame_all_operation() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let expected = camera_after_frame_all(&editor);
    let menu = face_text_position(&output, "View");
    let output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let row = face_text_position(&output, "Frame All (A)");
    face_pointer_click(&mut editor, &ctx, row, egui::Modifiers::NONE, vec![]);
    assert_eq!(editor.camera, expected);
}

fn first_click_with_key_into_channel_field(event: egui::Event, caret: usize) -> String {
    let (mut editor, ctx) = unfocused_cube_editor();
    editor.channels[0] = "12".into();
    frame(&mut editor, &ctx, vec![]);
    let mut state = egui::TextEdit::load_state(&ctx, channel_edit_id(0)).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(caret),
        )));
    state.store(&ctx, channel_edit_id(0));
    let center = ctx.read_response(channel_edit_id(0)).unwrap().rect.center();
    frame(
        &mut editor,
        &ctx,
        vec![
            egui::Event::PointerMoved(center),
            primary_button(center, true),
            event,
        ],
    );
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(0))
    );
    editor.channels[0].clone()
}

#[test]
fn first_click_with_same_frame_backspace_edits_the_field_at_the_end_caret() {
    assert_eq!(
        first_click_with_key_into_channel_field(key(Key::Backspace), 2),
        "1"
    );
}

#[test]
fn first_click_with_same_frame_delete_removes_the_character_after_the_caret() {
    assert_eq!(
        first_click_with_key_into_channel_field(key(Key::Delete), 0),
        "2"
    );
}

#[test]
fn first_click_with_same_frame_paste_inserts_at_the_end_caret() {
    assert_eq!(
        first_click_with_key_into_channel_field(egui::Event::Paste("9".into()), 2),
        "129"
    );
}

#[test]
fn first_click_on_the_material_field_receives_same_frame_text() {
    let mut editor = material_editor();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(ctx.memory(|memory| memory.focused()), None);
    let before = editor.material_draft.as_ref().unwrap().text.clone();
    click_with_text(&mut editor, &ctx, material_edit_id(), "5");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(material_edit_id())
    );
    assert_eq!(
        editor.material_draft.as_ref().unwrap().text,
        format!("{before}5")
    );
}

#[test]
fn first_click_on_the_disabled_material_field_keeps_the_channel_draft_and_its_text() {
    // Diffuse is disabled while a channel draft exists, so a channel to Diffuse move cannot occur.
    let mut editor = material_editor();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    let before = editor.history.scene().clone();
    let material = editor.material_draft.as_ref().unwrap().text.clone();
    type_into_channel(&mut editor, &ctx, 0, "5");
    assert_eq!(editor.channels[0], "15");
    click_with_text(&mut editor, &ctx, material_edit_id(), "5");
    assert_eq!(
        ctx.memory(|memory| memory.focused()),
        Some(channel_edit_id(0))
    );
    assert_eq!(editor.channels[0], "155");
    assert_eq!(editor.material_draft.as_ref().unwrap().text, material);
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn release_click_and_tab_after_a_first_click_defer_the_blur_commit_once() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let undo_before = undo_entries(&mut editor);
    type_into_channel(&mut editor, &ctx, 0, "5");
    click_with_text(&mut editor, &ctx, channel_edit_id(1), "3");
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
    let center = ctx.read_response(channel_edit_id(1)).unwrap().rect.center();
    frame(
        &mut editor,
        &ctx,
        vec![primary_button(center, false), key(Key::Tab)],
    );
    // B's blur is reported in the skipped frame; the draft survives it.
    assert_eq!(editor.channels[1], "03");
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation,
        [5.0, 3.0, 0.0]
    );
    assert_eq!(undo_entries(&mut editor), undo_before + 2);
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    assert_eq!(undo_entries(&mut editor), undo_before + 2);
    assert!(!editor.has_draft());
}

fn type_into_name(editor: &mut Editor, ctx: &egui::Context, text: &str) {
    ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(editor, ctx, vec![]);
    frame(editor, ctx, vec![egui::Event::Text(text.into())]);
}

fn node_name(editor: &Editor) -> String {
    editor.history.scene().nodes()[0].name().to_string()
}

#[test]
fn heading_name_enter_commits_one_rename() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let undo_before = undo_entries(&mut editor);
    type_into_name(&mut editor, &ctx, "Boxy");
    assert!(editor.name.ends_with("Boxy") && editor.has_draft());
    frame(&mut editor, &ctx, vec![key(Key::Enter)]);
    assert!(node_name(&editor).ends_with("Boxy"));
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
    assert!(!editor.has_draft());
}

#[test]
fn heading_name_tab_blur_commits_exactly_once() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let undo_before = undo_entries(&mut editor);
    type_into_name(&mut editor, &ctx, "Boxy");
    frame(&mut editor, &ctx, vec![key(Key::Tab)]);
    for _ in 0..4 {
        frame(&mut editor, &ctx, vec![]);
    }
    assert!(node_name(&editor).ends_with("Boxy"));
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
}

#[test]
fn heading_name_blur_honours_the_skip_frames_and_the_kept_draft_commits_after() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let original = node_name(&editor);
    let undo_before = undo_entries(&mut editor);
    type_into_name(&mut editor, &ctx, "Boxy");
    editor.blur_commit_skip_frames = 3;
    frame(&mut editor, &ctx, vec![key(Key::Tab)]);
    assert_eq!(node_name(&editor), original);
    assert!(editor.has_draft());
    assert_eq!(undo_entries(&mut editor), undo_before);
    // The blur was swallowed, so the draft stays until a later commit trigger.
    editor.blur_commit_skip_frames = 0;
    editor.commit_pending();
    assert!(node_name(&editor).ends_with("Boxy"));
    assert_eq!(undo_entries(&mut editor), undo_before + 1);
}

#[test]
fn heading_name_rejected_rename_keeps_the_draft() {
    let (mut editor, ctx) = unfocused_cube_editor();
    let original = node_name(&editor);
    let undo_before = undo_entries(&mut editor);
    ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
    frame(&mut editor, &ctx, vec![]);
    editor.name.clear();
    editor.name_intent = true;
    frame(&mut editor, &ctx, vec![key(Key::Enter)]);
    for _ in 0..3 {
        frame(&mut editor, &ctx, vec![]);
    }
    assert_eq!(node_name(&editor), original);
    assert_eq!(editor.name, "");
    assert!(editor.name_intent && editor.error);
    assert_eq!(undo_entries(&mut editor), undo_before);
}

#[test]
fn read_only_heading_still_exposes_the_object_name_label() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let id = editor.history.scene().nodes()[0].id();
    editor.select(Some(id));
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        editor.channels(ui, &mut false)
    });
    output.textures_delta.clear();
    let node = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Object name"))
        .expect("heading label");
    assert!(node.1.is_disabled());
}

#[test]
fn attribute_editor_keeps_the_labelled_object_name_row() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| editor.attribute_editor(ui));
    output.textures_delta.clear();
    assert!(output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Text(text) if text.galley.job.text == "Object name"
    )));
}

mod primitive_options_create {
    use super::*;
    use azteccraft::primitive_options::{Axis, PrimitiveOptions};

    fn sphere(radius: f64) -> PrimitiveOptions {
        PrimitiveOptions::Sphere {
            radius,
            sub_axis: 8,
            sub_height: 6,
            axis: Axis::Y,
        }
    }

    fn create(
        editor: &mut Editor,
        primitive: Primitive,
        options: Option<PrimitiveOptions>,
    ) -> Result<OperationReply, OperationError> {
        remote(
            editor,
            Operation::Create {
                primitive,
                options,
                name: None,
            },
        )
    }

    #[test]
    fn invalid_options_leave_scene_history_selection_and_token_unchanged() {
        let mut editor = Editor::default();
        create(&mut editor, Primitive::Cube, None).unwrap();
        let scene = editor.history.scene().clone();
        let selection = (editor.selected(), editor.selected_path().to_vec());
        let token = editor.scene_token;
        let dirty = editor.is_dirty();
        let invalid = [
            (Primitive::Sphere, sphere(0.0)),
            (Primitive::Sphere, sphere(f64::INFINITY)),
            (Primitive::Cone, sphere(1.0)),
        ];
        for (primitive, options) in invalid {
            let error = create(&mut editor, primitive, Some(options)).unwrap_err();
            assert!(matches!(error, OperationError::Scene(_)), "{error:?}");
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(
                (editor.selected(), editor.selected_path().to_vec()),
                selection
            );
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.is_dirty(), dirty);
            assert!(!editor.history.can_redo());
        }
    }

    #[test]
    fn optioned_create_selects_undoes_and_redoes() {
        let mut editor = Editor::default();
        let before = editor.history.scene().clone();
        let OperationReply::Created { node_id } =
            create(&mut editor, Primitive::Sphere, Some(sphere(3.0))).unwrap()
        else {
            panic!("expected created object")
        };
        assert_eq!(
            editor.selection_presentation().0,
            NodeId::from_decimal(&node_id).ok()
        );
        let after = editor.history.scene().clone();
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(editor.history.scene(), &before);
        remote(&mut editor, Operation::Redo).unwrap();
        assert_eq!(editor.history.scene(), &after);
    }

    #[test]
    fn save_and_save_as_stay_refused_and_write_nothing_after_an_optioned_create() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("created.mb");
        let mut editor = Editor::default();
        create(&mut editor, Primitive::Sphere, Some(sphere(3.0))).unwrap();
        assert!(!editor.files.save_available());
        assert!(matches!(
            remote(&mut editor, Operation::SaveScene),
            Err(OperationError::NativeSave(_))
        ));
        let save_as = remote(
            &mut editor,
            Operation::SaveSceneAs {
                path: destination.clone(),
            },
        );
        assert!(
            matches!(save_as, Err(OperationError::NativeSave(_))),
            "{save_as:?}"
        );
        assert!(!destination.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn maximum_size_sphere_stays_inside_the_framed_far_clip() {
        let mut editor = Editor::default();
        create(&mut editor, Primitive::Sphere, Some(sphere(1e6))).unwrap();
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        let mut camera = editor.camera.clone();
        camera.frame_view(&view, None, editor.aspect).unwrap();
        let vertices = view.instances()[0].world_vertices();
        assert!(!vertices.is_empty());
        for vertex in vertices {
            let projected = camera.project(*vertex, editor.aspect).unwrap();
            assert!(projected.is_some(), "{vertex:?} is clipped");
        }
    }
}

fn hover_reason_shown(
    editor: &mut Editor,
    ctx: &egui::Context,
    size: egui::Vec2,
    at: egui::Pos2,
    reason: &str,
) -> bool {
    let mut shown = false;
    let start = ctx.input(|input| input.time);
    for step in 1..=8 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            time: Some(start + f64::from(step) * 0.25),
            events: vec![egui::Event::PointerMoved(at)],
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default();
        let mut output = ctx.run_ui(input, |ui| editor.render(ui));
        output.textures_delta.clear();
        shown = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == reason)
        });
    }
    shown
}

#[test]
fn polygon_primitives_submenu_lists_the_maya_labels_in_docs_order() {
    let size = egui::vec2(1440.0, 900.0);
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    let output = open_polygon_primitives(&mut editor, &ctx, size);
    let mut items = polygon_submenu_items(&output);
    items.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
    let rows: Vec<&str> = items
        .iter()
        .map(|item| item.label.as_str())
        .filter(|label| !label.ends_with("option box"))
        .collect();
    // Maya 2027 order for the 15 primitives (the toggles' place is PROVISIONAL):
    // https://help.autodesk.com/cloudhelp/2027/ENU/Maya-Basics/files/GUID-45D2EAD4-5BCF-42DA-A1AB-EC6EE09FE705.htm
    let expected = [
        "Interactive Creation",
        "Exit on Completion",
        "Sphere",
        "Cube",
        "Cylinder",
        "Cone",
        "Torus",
        "Plane",
        "Disc",
        "Platonic Solid",
        "Pyramid",
        "Prism",
        "Pipe",
        "Helix",
        "Gear",
        "Soccer Ball",
        "Super Shapes",
    ];
    assert_eq!(rows, expected);
    assert_eq!(menus::POLYGON_PRIMITIVES.len(), 15);
    for (label, primitive) in menus::POLYGON_PRIMITIVES {
        let option_box = format!("{label} option box");
        let found = items.iter().find(|item| item.label == option_box);
        assert_eq!(found.is_some(), primitive.is_some(), "{option_box}");
        if let Some(found) = found {
            assert!(!found.disabled, "{option_box} disabled");
        }
    }
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    assert!(update.nodes.iter().all(|(_, node)| {
        node.label()
            .is_none_or(|label| !label.starts_with("Polygon c") && label != "Other primitives")
    }));
}

#[test]
fn every_primitive_variant_is_in_the_create_menu() {
    let listed = |primitive: Primitive| {
        menus::POLYGON_PRIMITIVES
            .iter()
            .any(|(_, built)| *built == Some(primitive))
    };
    // A new variant fails to compile here until it is placed in the menu list.
    for primitive in [
        Primitive::Cube,
        Primitive::Plane,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ] {
        match primitive {
            Primitive::Cube
            | Primitive::Plane
            | Primitive::Sphere
            | Primitive::Cylinder
            | Primitive::Cone => assert!(listed(primitive), "{primitive:?}"),
        }
    }
    let built = menus::POLYGON_PRIMITIVES
        .iter()
        .filter(|(_, primitive)| primitive.is_some())
        .count();
    assert_eq!(built, 5);
}

#[test]
fn unbuilt_primitives_and_the_interactive_toggles_are_disabled_with_their_reasons() {
    let size = egui::vec2(1440.0, 900.0);
    let mut editor = Editor::default();
    let ctx = egui::Context::default();
    ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    let output = open_polygon_primitives(&mut editor, &ctx, size);
    let before = editor.history.scene().clone();
    let mut rows: Vec<(String, String)> = vec![
        (
            "Interactive Creation".into(),
            menus::INTERACTIVE_CREATION_REASON.into(),
        ),
        (
            "Exit on Completion".into(),
            menus::EXIT_ON_COMPLETION_REASON.into(),
        ),
    ];
    for (label, primitive) in menus::POLYGON_PRIMITIVES {
        if primitive.is_none() {
            rows.push((
                label.into(),
                format!(
                    "{label} is not implemented yet; the supported primitives are Sphere, Cube, Cylinder, Cone and Plane"
                ),
            ));
        }
    }
    assert_eq!(rows.len(), 12);
    for (label, reason) in rows {
        let item = submenu_item(&output, &label);
        assert!(item.disabled, "{label} enabled");
        assert!(
            hover_reason_shown(&mut editor, &ctx, size, item.rect.center(), &reason),
            "{label} shows no specific reason"
        );
        click_at(&mut editor, &ctx, size, item.rect.center());
        assert_eq!(editor.history.scene(), &before, "{label} changed the scene");
        assert!(!editor.history.can_undo(), "{label} added history");
        open_polygon_primitives(&mut editor, &ctx, size);
    }
}

fn pose_op(yaw: f32, pitch: f32, distance: f32) -> Operation {
    Operation::SetCameraPose {
        target: [1.0, 0.5, -2.0],
        distance,
        yaw,
        pitch,
    }
}

fn remote_pose(
    editor: &mut Editor,
    token: Uuid,
    operation: Operation,
) -> Result<OperationReply, OperationError> {
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation,
        },
        &egui::Context::default(),
    )
}

/// The reply is numeric, so -0.0 over 0.0 is `Unchanged` although the stored sign bits flip.
#[test]
fn set_camera_pose_negative_zero_over_zero_replies_unchanged_and_flips_the_stored_bits() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let token = editor.scene_token;
    let pose = |zero: f32| Operation::SetCameraPose {
        target: [zero; 3],
        distance: 5.0,
        yaw: zero,
        pitch: zero,
    };
    let reply = remote_pose(&mut editor, token, pose(0.0)).unwrap();
    assert!(matches!(reply, OperationReply::Changed));
    let (dirty, scene, undoable) = (
        editor.is_dirty(),
        editor.history.scene().clone(),
        editor.history.can_undo(),
    );
    let reply = remote_pose(&mut editor, token, pose(-0.0)).unwrap();
    assert!(matches!(reply, OperationReply::Unchanged));
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undoable);
    let negative = (-0.0_f32).to_bits();
    assert_eq!(editor.camera.yaw().to_bits(), negative);
    assert_eq!(editor.camera.pitch().to_bits(), negative);
    assert_eq!(editor.camera.target().x.to_bits(), negative);
}

#[test]
fn set_camera_pose_changes_only_the_camera_and_reports_unchanged_on_repeat() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let (token, dirty, scene, undoable) = (
        editor.scene_token,
        editor.is_dirty(),
        editor.history.scene().clone(),
        editor.history.can_undo(),
    );
    let lens = editor.camera.clone();
    let reply = remote_pose(&mut editor, token, pose_op(7.0, 0.2, 5.0)).unwrap();
    assert!(matches!(reply, OperationReply::Changed));
    assert_eq!(editor.camera.target(), Vec3::new(1.0, 0.5, -2.0));
    assert_eq!(editor.camera.distance(), 5.0);
    assert_eq!(editor.camera.pitch(), 0.2);
    assert_eq!(
        editor.camera.yaw(),
        7.0_f32.rem_euclid(std::f32::consts::TAU)
    );
    assert_eq!(
        (
            editor.camera.lens(),
            editor.camera.near(),
            editor.camera.far()
        ),
        (lens.lens(), lens.near(), lens.far())
    );
    let again = remote_pose(&mut editor, token, pose_op(7.0, 0.2, 5.0)).unwrap();
    assert!(matches!(again, OperationReply::Unchanged));
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undoable);
}

#[test]
fn inspect_fov_tracks_the_viewport_aspect() {
    let mut editor = Editor::default();
    for aspect in [1.0_f32, 2.0] {
        editor.aspect = aspect;
        let tan_y = if aspect >= 1.4173 / 0.9449 {
            1.4173_f32 * 25.4 / 70.0 / aspect
        } else {
            0.9449_f32 * 25.4 / 70.0
        };
        let expected = f64::from(2.0 * tan_y.atan()).to_degrees();
        let actual = editor.inspect(false).unwrap().camera.fov_y_degrees;
        assert!(
            (actual - expected).abs() < 1e-4,
            "aspect {aspect}: {actual}"
        );
    }
}

#[test]
fn set_camera_pose_rejects_invalid_fields_with_a_named_error_and_keeps_the_camera() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let token = editor.scene_token;
    let camera = editor.camera.clone();
    let status = editor.status.clone();
    for (operation, text) in [
        (pose_op(0.0, 1.6, 5.0), "pitch"),
        (pose_op(f32::NAN, 0.2, 5.0), "yaw"),
        (pose_op(0.0, 0.2, 0.0), "distance"),
        (pose_op(0.0, 0.2, f32::INFINITY), "distance"),
    ] {
        let error = remote_pose(&mut editor, token, operation).unwrap_err();
        assert!(matches!(error, OperationError::CameraPose(_)), "{error}");
        assert!(error.to_string().contains(text), "{error}");
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.status, status);
    }
}

#[test]
fn set_camera_pose_rejects_a_stale_token_and_native_dispatch_matches_remote() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let camera = editor.camera.clone();
    let error = remote_pose(&mut editor, Uuid::new_v4(), pose_op(0.3, 0.2, 5.0)).unwrap_err();
    assert!(matches!(error, OperationError::StaleScene));
    assert_eq!(editor.camera, camera);
    editor.native_operation(pose_op(0.3, 0.2, 5.0));
    assert_eq!(editor.camera.distance(), 5.0);
    assert!(!editor.error);
    editor.native_operation(pose_op(0.3, 0.2, -1.0));
    assert_eq!(editor.camera.distance(), 5.0);
    assert!(editor.error);
}

fn disturb_camera(editor: &mut Editor) {
    editor.camera.orbit(glam::Vec2::new(1.2, -0.7)).unwrap();
    editor.camera.pan(glam::Vec2::new(2.0, -1.0)).unwrap();
    editor.camera.dolly(2.5).unwrap();
    assert_ne!(editor.camera, Camera::default());
}

fn disturbed_camera_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    disturb_camera(&mut editor);
    editor
}

/// Presses and releases Home so egui does not mark the next press as a repeat.
fn press_alt_home(editor: &mut Editor, ctx: &egui::Context, modifiers: egui::Modifiers) {
    frame(editor, ctx, vec![key_with(Key::Home, modifiers)]);
    frame(
        editor,
        ctx,
        vec![egui::Event::Key {
            key: Key::Home,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
}

#[test]
fn view_menu_default_view_row_restores_the_default_pose() {
    let mut editor = disturbed_camera_editor();
    let (token, scene) = (editor.scene_token, editor.history.scene().clone());
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let output = frame(&mut editor, &ctx, vec![]);
    let menu = face_text_position(&output, "View");
    let output = face_pointer_click(&mut editor, &ctx, menu, egui::Modifiers::NONE, vec![]);
    let row = face_text_position(&output, "Default View (Alt+Home)");
    face_pointer_click(&mut editor, &ctx, row, egui::Modifiers::NONE, vec![]);
    assert_eq!(editor.camera, Camera::default());
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn default_view_after_frame_all_restores_the_pose_but_keeps_the_grown_far_clip() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let transform = NativeTransform {
        translation: [500000.0, 0.0, 0.0],
        ..Default::default()
    };
    let input = editor.transform_input(transform).unwrap();
    let id = editor.selected().unwrap();
    remote(
        &mut editor,
        Operation::SetTransform {
            id,
            transform: input,
        },
    )
    .unwrap();
    let default = Camera::default();
    remote(&mut editor, Operation::FrameAll).unwrap();
    assert!(editor.camera.far() > default.far());
    let grown = editor.camera.far();
    editor.default_view();
    let camera = &editor.camera;
    assert_eq!(camera.target(), default.target());
    assert_eq!(camera.distance(), default.distance());
    assert_eq!(camera.yaw(), default.yaw());
    assert_eq!(camera.pitch(), default.pitch());
    assert_eq!(camera.far(), grown);
}

#[test]
fn alt_home_restores_the_default_pose_and_other_modifiers_do_not() {
    let mut editor = disturbed_camera_editor();
    let disturbed = editor.camera.clone();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::SHIFT | egui::Modifiers::ALT,
        egui::Modifiers::CTRL | egui::Modifiers::ALT,
        egui::Modifiers::COMMAND | egui::Modifiers::ALT,
    ] {
        press_alt_home(&mut editor, &ctx, modifiers);
        assert_eq!(editor.camera, disturbed, "{modifiers:?}");
    }
    press_alt_home(&mut editor, &ctx, egui::Modifiers::ALT);
    assert_eq!(editor.camera, Camera::default());
}

#[test]
fn alt_home_and_default_view_are_refused_while_modal_or_closing() {
    let ctx = egui::Context::default();
    let mut editor = disturbed_camera_editor();
    editor.new_document().unwrap();
    frame(&mut editor, &ctx, vec![]);
    frame(&mut editor, &ctx, vec![]);
    assert!(editor.files.is_modal());
    let disturbed = editor.camera.clone();
    press_alt_home(&mut editor, &ctx, egui::Modifiers::ALT);
    assert_eq!(editor.camera, disturbed, "modal");

    let mut editor = Editor::default();
    editor.close_document().unwrap();
    disturb_camera(&mut editor);
    assert!(editor.files.is_closing());
    let disturbed = editor.camera.clone();
    press_alt_home(&mut editor, &ctx, egui::Modifiers::ALT);
    assert_eq!(editor.camera, disturbed, "closing");
    assert!(!editor.error, "the shortcut gate stops it before dispatch");
    editor.default_view();
    assert_eq!(editor.camera, disturbed, "menu path while closing");
    assert!(editor.error);
}

#[test]
fn selection_mode_camera_tool_and_panel_changes_add_no_undo_entry() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    remote(
        &mut editor,
        Operation::Rename {
            id,
            name: "renamed".into(),
        },
    )
    .unwrap();
    remote(&mut editor, Operation::Undo).unwrap();
    let undo = editor.history.undo_label();
    let redo = editor.history.redo_label();
    assert!(redo.is_some());
    for operation in [
        Operation::Select {
            node: None,
            instance_path: None,
        },
        Operation::Select {
            node: Some(id),
            instance_path: None,
        },
        Operation::SetSelectionMode(SelectionMode::Hierarchy),
        Operation::SetSelectionMode(SelectionMode::Object),
        Operation::Orbit {
            radians: [0.1, 0.1],
        },
        Operation::Pan {
            world_units: [0.5, 0.5],
        },
        Operation::Dolly { factor: 1.5 },
        Operation::FrameSelected,
        Operation::SetActiveTool(ActiveTool::Move),
        Operation::SetGridVisible { visible: false },
    ] {
        remote(&mut editor, operation).unwrap();
        assert_eq!(editor.history.undo_label(), undo);
        assert_eq!(editor.history.redo_label(), redo);
    }
}

fn two_cube_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    assert_eq!(paths.len(), 2);
    editor.set_selection_members_for_test(paths);
    editor
}

#[test]
fn several_selected_objects_refuse_single_target_and_present_the_lead() {
    let editor = two_cube_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    assert!(matches!(
        editor.single_target(),
        Err(OperationError::MultipleSelection)
    ));
    assert_eq!(
        OperationError::MultipleSelection.to_string(),
        "Several objects are selected; select one object"
    );
    let lead = paths.last().unwrap();
    assert_eq!(
        editor.selection_presentation(),
        (Some(lead.node), lead.path.as_slice())
    );
    assert_eq!(
        editor.lead_target().unwrap(),
        (lead.node, lead.path.as_slice())
    );
}

#[test]
fn frame_and_move_refuse_several_selected_objects_and_keep_state() {
    let mut editor = two_cube_editor();
    let camera = editor.camera.clone();
    let scene = editor.history.scene().clone();
    let selection = editor.selection.clone();
    assert!(matches!(
        remote(&mut editor, Operation::FrameSelected),
        Err(OperationError::MultipleSelection)
    ));
    assert!(matches!(
        editor.selected_move_target(),
        Err(OperationError::MultipleSelection)
    ));
    let lead = selection.lead().unwrap().clone();
    assert!(matches!(
        editor.check_manipulation_target("Rotate", &lead),
        Err(OperationError::MultipleSelection)
    ));
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.selection, selection);
}

#[test]
fn undo_of_a_creation_prunes_only_that_member() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let first = editor.history.scene().transform_paths().unwrap().remove(0);
    editor.create(Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(vec![first.clone(), paths[1].clone()]);
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.selection.members(), std::slice::from_ref(&first));
    assert_eq!(editor.lead_target().unwrap().0, first.node);
}

#[test]
fn undo_that_removes_every_member_clears_the_selection() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.native_operation(Operation::Undo);
    editor.native_operation(Operation::Undo);
    assert!(editor.selection.is_empty());
    assert!(matches!(
        editor.single_target(),
        Err(OperationError::NoSelection)
    ));
}

#[test]
fn create_and_select_replace_the_whole_set() {
    let mut editor = two_cube_editor();
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.native_operation(Operation::Select {
        node: Some(paths[1].node),
        instance_path: None,
    });
    assert_eq!(editor.selection.members(), [paths[1].clone()]);
    editor.set_selection_members_for_test(paths);
    editor.create(Primitive::Cube);
    assert_eq!(editor.selection.len(), 1);
}

#[test]
fn deselect_all_and_new_document_clear_several_selected_objects() {
    let mut editor = two_cube_editor();
    editor.deselect_all();
    assert!(editor.selection.is_empty());
    let members = two_cube_editor().selection.members().to_vec();
    let mut editor = Editor::default();
    editor.set_selection_members_for_test(members);
    assert_eq!(editor.selection.len(), 2);
    editor.new_document().unwrap();
    assert!(editor.selection.is_empty());
}

#[test]
fn a_draft_on_the_lead_is_committed_through_the_lead_with_several_selected() {
    let mut editor = two_cube_editor();
    let lead = editor.lead_target().unwrap().0;
    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    assert!(editor.commit_pending());
    let x = editor
        .history
        .scene()
        .node(lead)
        .unwrap()
        .transform()
        .values()[0];
    assert_eq!(x, 5.0);
    assert_eq!(editor.selection.len(), 2);
}

fn multi_baseline(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.selection.clone(),
        editor.selection_mode,
        editor.camera.clone(),
        editor.scene_token,
        editor.history.can_undo(),
        editor.history.undo_label(),
    )
}

#[test]
fn manipulation_begins_refuse_several_selected_objects_natively_and_remotely() {
    let mut editor = two_cube_editor();
    let baseline = multi_baseline(&editor);
    let target = editor.selection.lead().unwrap().clone();
    let space = crate::editor_operations::ManipulationSpace::World;
    let make = |index: usize| match index {
        0 => Operation::BeginMove {
            target: target.clone(),
            axis: azteccraft::native_scene::WorldAxis::X,
        },
        1 => Operation::BeginRotate {
            target: target.clone(),
            axis: azteccraft::native_scene::RotateAxis::X,
            space,
        },
        _ => Operation::BeginScale {
            target: target.clone(),
            handle: azteccraft::native_scene::ScaleHandle::Uniform,
            space,
        },
    };
    for (index, label) in ["move", "rotate", "scale"].into_iter().enumerate() {
        assert!(
            matches!(
                editor.apply_operation(make(index)),
                Err(OperationError::MultipleSelection)
            ),
            "native {label}"
        );
        assert!(
            matches!(
                remote(&mut editor, make(index)),
                Err(OperationError::MultipleSelection)
            ),
            "remote {label}"
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(multi_baseline(&editor), baseline);
    }
}

#[test]
fn component_modes_refuse_several_selected_objects_and_object_modes_do_not() {
    let mut editor = two_cube_editor();
    let baseline = multi_baseline(&editor);
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        assert!(matches!(
            remote(&mut editor, Operation::SetSelectionMode(mode)),
            Err(OperationError::MultipleSelection)
        ));
        assert!(matches!(
            editor.apply_operation(Operation::SetSelectionMode(mode)),
            Err(OperationError::MultipleSelection)
        ));
        assert_eq!(multi_baseline(&editor), baseline);
    }
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Hierarchy),
    )
    .unwrap();
    assert_eq!(editor.selection.len(), 2);
}

#[test]
fn f8_and_the_delete_key_report_the_refusal_with_several_selected() {
    let mut editor = two_cube_editor();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    let baseline = multi_baseline(&editor);
    frame(&mut editor, &ctx, vec![key(Key::F8)]);
    assert!(editor.error, "{}", editor.status);
    assert!(editor.status.contains("Several objects are selected"));
    assert_eq!(multi_baseline(&editor), baseline);
    editor.status.clear();
    editor.error = false;
    frame(&mut editor, &ctx, vec![key(Key::Delete)]);
    assert!(editor.error, "{}", editor.status);
    assert!(editor.status.contains("Several objects are selected"));
    assert_eq!(multi_baseline(&editor), baseline);
}

#[test]
fn channel_box_and_attribute_editor_edits_are_refused_with_several_selected() {
    let mut editor = two_cube_editor();
    assert_eq!(
        editor.authoring_refusal().as_deref(),
        Some("Several objects are selected; select one object")
    );
    assert_eq!(
        editor.delete_disabled_hover(None, true),
        "Several objects are selected; select one object"
    );
    let lead = editor.selection.lead().unwrap().clone();
    editor.set_selection_members_for_test(vec![lead]);
    assert_eq!(editor.authoring_refusal(), None);
    let mut editor = two_cube_editor();
    let ctx = egui::Context::default();
    editor.channels[0] = "9".into();
    editor.channel_intents[0] = true;
    let baseline = multi_baseline(&editor);
    frame(&mut editor, &ctx, vec![]);
    assert_eq!(multi_baseline(&editor), baseline);
    assert_eq!(editor.channels[0], "9");
}

#[test]
fn attribute_editor_tab_follows_the_lead_with_several_selected() {
    let mut editor = two_cube_editor();
    let (first, lead) = {
        let members = editor.selection.members();
        (members[0].node, members[1].node)
    };
    remote(
        &mut editor,
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
    )
    .unwrap();
    assert!(matches!(
        remote(
            &mut editor,
            Operation::SelectAttributeEditorTab {
                tab: AttributeEditorTab::Transform(first),
            },
        ),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(lead))
    );
    remote(
        &mut editor,
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Transform(lead),
        },
    )
    .unwrap();
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(lead))
    );
}

#[test]
fn by_id_edits_of_a_non_lead_member_keep_the_selection_ordered() {
    let mut editor = two_cube_editor();
    let members = editor.selection.members().to_vec();
    let first = members[0].node;
    remote(
        &mut editor,
        Operation::Rename {
            id: first,
            name: "renamed".into(),
        },
    )
    .unwrap();
    assert_eq!(editor.selection.members(), members);
    remote(
        &mut editor,
        Operation::EditNode {
            id: first,
            name: None,
            translation: [Some(3.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        },
    )
    .unwrap();
    assert_eq!(editor.selection.members(), members);
    remote(&mut editor, Operation::Delete(first)).unwrap();
    assert_eq!(editor.selection.members(), &members[1..]);
    assert_eq!(editor.lead_target().unwrap().0, members[1].node);
}

const SEVERAL_REASON: &str = "Several objects are selected; select one object";

fn count_text(output: &egui::FullOutput, text: &str) -> usize {
    output
        .shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape, egui::Shape::Text(shape) if shape.galley.job.text == text)
        })
        .count()
}

/// One full-UI frame a quarter second after the previous one, so tooltips and the Grid sizing
/// pass settle after a few calls.
fn render_at(
    editor: &mut Editor,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let time = ctx.input(|input| input.time) + 0.25;
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        time: Some(time),
        events,
        ..Default::default()
    };
    input.viewports.entry(egui::ViewportId::ROOT).or_default();
    let mut output = ctx.run_ui(input, |ui| editor.render(ui));
    output.textures_delta.clear();
    output
}

fn settle(editor: &mut Editor, ctx: &egui::Context) -> egui::FullOutput {
    let mut output = render_at(editor, ctx, vec![]);
    for _ in 0..3 {
        output = render_at(editor, ctx, vec![]);
    }
    output
}

/// True when resting the pointer on `at` adds a tooltip carrying `SEVERAL_REASON`.
fn hover_adds_the_reason(editor: &mut Editor, ctx: &egui::Context, at: egui::Pos2) -> bool {
    ctx.global_style_mut(|style| style.interaction.tooltip_delay = 0.0);
    // Take the pointer off the window so an earlier tooltip is gone.
    for _ in 0..4 {
        render_at(editor, ctx, vec![egui::Event::PointerGone]);
    }
    let baseline = count_text(&render_at(editor, ctx, vec![]), SEVERAL_REASON);
    let mut hovered = render_at(editor, ctx, vec![]);
    for _ in 0..8 {
        hovered = render_at(editor, ctx, vec![egui::Event::PointerMoved(at)]);
    }
    count_text(&hovered, SEVERAL_REASON) > baseline
}

fn widget_enabled(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.read_response(id)
        .unwrap_or_else(|| panic!("{id:?} was not drawn"))
        .enabled()
}

fn widget_center(ctx: &egui::Context, id: egui::Id) -> egui::Pos2 {
    ctx.read_response(id).expect("drawn").rect.center()
}

fn role_states(output: &egui::FullOutput, role: eframe::egui::accesskit::Role) -> Vec<bool> {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled")
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role)
        .map(|(_, node)| node.is_disabled())
        .collect()
}

fn editing_ctx() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    ctx
}

/// The material fixture's two nodes selected, the node whose shapes carry the editable material
/// last (the lead).
fn multi_material_editor() -> Editor {
    let mut editor = material_editor();
    let lead = editor.selection.lead().unwrap().clone();
    let other = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.node != lead.node)
        .unwrap();
    editor.set_selection_members_for_test(vec![other, lead]);
    editor
}

#[test]
fn channel_box_controls_are_disabled_with_the_reason_while_several_are_selected() {
    let ctx = editing_ctx();
    let mut single = multi_material_editor();
    let lead = single.selection.lead().unwrap().clone();
    single.set_selection_members_for_test(vec![lead]);
    let output = settle(&mut single, &ctx);
    assert!(widget_enabled(&ctx, name_edit_id()), "control: one object");
    assert!(widget_enabled(&ctx, channel_edit_id(0)));
    assert!(widget_enabled(&ctx, material_edit_id()));
    assert!(!button_disabled(&output, "Delete selected"));
    assert!(role_states(&output, eframe::egui::accesskit::Role::Slider).contains(&false));

    let ctx = editing_ctx();
    let mut editor = multi_material_editor();
    let output = settle(&mut editor, &ctx);
    for id in [name_edit_id(), channel_edit_id(0), channel_edit_id(8)] {
        assert!(!widget_enabled(&ctx, id), "{id:?}");
        let at = widget_center(&ctx, id);
        assert!(hover_adds_the_reason(&mut editor, &ctx, at), "{id:?}");
    }
    assert!(!widget_enabled(&ctx, material_edit_id()), "Diffuse text");
    let output2 = settle(&mut editor, &ctx);
    assert!(
        role_states(&output2, eframe::egui::accesskit::Role::Slider)
            .iter()
            .all(|disabled| *disabled),
        "Diffuse slider"
    );
    assert!(count_text(&output2, SEVERAL_REASON) > 0, "Diffuse reason");
    assert!(button_disabled(&output, "Delete selected"));
    assert!(button_disabled(&output, "Frame selected  F"));
    let at = face_text_position(&output2, "Delete selected");
    assert!(hover_adds_the_reason(&mut editor, &ctx, at), "Delete");
    let at = face_text_position(&output2, "Frame selected  F");
    assert!(hover_adds_the_reason(&mut editor, &ctx, at), "Frame");
}

#[test]
fn attribute_editor_controls_are_disabled_with_the_reason_while_several_are_selected() {
    let ctx = editing_ctx();
    let mut editor = multi_material_editor();
    remote(
        &mut editor,
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
    )
    .unwrap();
    settle(&mut editor, &ctx);
    for id in [name_edit_id(), channel_edit_id(0), channel_edit_id(8)] {
        assert!(!widget_enabled(&ctx, id), "{id:?}");
        let at = widget_center(&ctx, id);
        assert!(hover_adds_the_reason(&mut editor, &ctx, at), "{id:?}");
    }
    let shape = editor.material_shape.expect("a direct mesh shape");
    remote(
        &mut editor,
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        },
    )
    .unwrap();
    let output = settle(&mut editor, &ctx);
    assert!(!widget_enabled(&ctx, material_edit_id()), "Diffuse text");
    assert!(
        role_states(&output, eframe::egui::accesskit::Role::Slider)
            .iter()
            .all(|disabled| *disabled),
        "Diffuse slider"
    );
    assert!(count_text(&output, SEVERAL_REASON) > 0);
}

#[test]
fn edit_menu_delete_is_disabled_with_the_reason_while_several_are_selected() {
    let ctx = editing_ctx();
    let mut editor = multi_material_editor();
    let output = frame(&mut editor, &ctx, vec![]);
    let edit = face_text_position(&output, "Edit");
    let output = face_pointer_click(&mut editor, &ctx, edit, egui::Modifiers::NONE, vec![]);
    assert!(button_disabled(&output, "Delete"), "edit menu Delete");
    let at = face_text_position(&output, "Delete");
    assert!(hover_adds_the_reason(&mut editor, &ctx, at));
}

#[test]
fn the_f_key_reports_the_refusal_with_several_selected() {
    let mut editor = two_cube_editor();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    let baseline = multi_baseline(&editor);
    frame(&mut editor, &ctx, vec![key(Key::F)]);
    assert!(editor.error, "{}", editor.status);
    assert!(editor.status.contains(SEVERAL_REASON), "{}", editor.status);
    assert_eq!(multi_baseline(&editor), baseline);
}

fn three_cube_editor() -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let targets = editor.history.scene().transform_paths().unwrap();
    assert_eq!(targets.len(), 3);
    editor.deselect_all();
    (editor, targets)
}

fn modify(
    editor: &mut Editor,
    change: SelectionChange,
    target: &TransformTarget,
) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::ModifySelection {
        change,
        node: target.node,
        instance_path: Some(target.path.clone()),
    })
}

#[test]
fn modify_selection_add_appends_leads_and_moves_an_existing_member_to_the_end() {
    let (mut editor, t) = three_cube_editor();
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[0]),
        Ok(OperationReply::Changed)
    ));
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[1]),
        Ok(OperationReply::Changed)
    ));
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[1]),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.selection.members(), [t[0].clone(), t[1].clone()]);
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[0]),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.selection.members(), [t[1].clone(), t[0].clone()]);
    assert_eq!(editor.lead_target().unwrap().0, t[0].node);
}

#[test]
fn modify_selection_toggle_flips_membership_and_promotes_the_previous_lead() {
    let (mut editor, t) = three_cube_editor();
    for target in &t {
        assert!(matches!(
            modify(&mut editor, SelectionChange::Toggle, target),
            Ok(OperationReply::Changed)
        ));
    }
    assert_eq!(editor.selection.len(), 3);
    modify(&mut editor, SelectionChange::Toggle, &t[2]).unwrap();
    assert_eq!(editor.lead_target().unwrap().0, t[1].node);
    modify(&mut editor, SelectionChange::Toggle, &t[0]).unwrap();
    assert_eq!(editor.selection.members(), [t[1].clone()]);
    modify(&mut editor, SelectionChange::Toggle, &t[1]).unwrap();
    assert!(editor.selection.is_empty());
}

#[test]
fn modify_selection_remove_drops_a_member_and_ignores_an_absent_one() {
    let (mut editor, t) = three_cube_editor();
    modify(&mut editor, SelectionChange::Add, &t[0]).unwrap();
    modify(&mut editor, SelectionChange::Add, &t[1]).unwrap();
    assert!(matches!(
        modify(&mut editor, SelectionChange::Remove, &t[2]),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.selection.len(), 2);
    assert!(matches!(
        modify(&mut editor, SelectionChange::Remove, &t[1]),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(editor.selection.members(), [t[0].clone()]);
    assert_eq!(editor.lead_target().unwrap().0, t[0].node);
}

#[test]
fn modify_selection_rejects_unknown_nodes_and_paths_without_changing_the_set() {
    let (mut editor, t) = three_cube_editor();
    modify(&mut editor, SelectionChange::Add, &t[0]).unwrap();
    let baseline = multi_baseline(&editor);
    let missing = NodeId::from_decimal("999999").unwrap();
    for change in [
        SelectionChange::Add,
        SelectionChange::Toggle,
        SelectionChange::Remove,
    ] {
        assert!(matches!(
            editor.apply_operation(Operation::ModifySelection {
                change,
                node: missing,
                instance_path: None,
            }),
            Err(OperationError::Scene(_))
        ));
        assert!(matches!(
            editor.apply_operation(Operation::ModifySelection {
                change,
                node: t[1].node,
                instance_path: Some(t[2].path.clone()),
            }),
            Err(OperationError::Scene(_))
        ));
        assert_eq!(multi_baseline(&editor), baseline);
    }
}

#[test]
fn modify_selection_without_a_path_resolves_the_single_instance() {
    let (mut editor, t) = three_cube_editor();
    editor
        .apply_operation(Operation::ModifySelection {
            change: SelectionChange::Add,
            node: t[1].node,
            instance_path: None,
        })
        .unwrap();
    assert_eq!(editor.selection.members(), [t[1].clone()]);
}

#[test]
fn modify_selection_is_refused_in_component_modes_modal_and_closing() {
    let (mut editor, t) = three_cube_editor();
    modify(&mut editor, SelectionChange::Add, &t[0]).unwrap();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let baseline = multi_baseline(&editor);
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[1]),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(multi_baseline(&editor), baseline);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Object));
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let baseline = multi_baseline(&editor);
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[1]),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(multi_baseline(&editor), baseline);
    let mut closing = Editor::default();
    closing.create(Primitive::Cube);
    let target = closing.history.scene().transform_paths().unwrap().remove(0);
    closing.close_document().unwrap();
    assert!(matches!(
        modify(&mut closing, SelectionChange::Remove, &target),
        Err(OperationError::BusyEditing)
    ));
}

#[test]
fn modify_selection_commits_a_valid_draft_and_keeps_an_invalid_one() {
    let (mut editor, t) = three_cube_editor();
    modify(&mut editor, SelectionChange::Add, &t[0]).unwrap();
    editor.channels[0] = "abc".into();
    editor.channel_intents[0] = true;
    let before = editor.history.scene().clone();
    assert!(matches!(
        modify(&mut editor, SelectionChange::Add, &t[1]),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(editor.selection.members(), [t[0].clone()]);
    assert_eq!(editor.channels[0], "abc");
    assert_eq!(editor.history.scene(), &before);
    editor.channels[0] = "5".into();
    modify(&mut editor, SelectionChange::Add, &t[1]).unwrap();
    let x = editor
        .history
        .scene()
        .node(t[0].node)
        .unwrap()
        .transform()
        .values()[0];
    assert_eq!(x, 5.0);
    assert_eq!(editor.selection.len(), 2);
}

#[test]
fn modify_selection_reports_nothing_on_the_status_line() {
    let (mut editor, t) = three_cube_editor();
    editor.status = "sentinel".into();
    editor.error = false;
    for (change, target) in [
        (SelectionChange::Add, &t[0]),
        (SelectionChange::Add, &t[0]),
        (SelectionChange::Toggle, &t[1]),
        (SelectionChange::Remove, &t[2]),
    ] {
        editor.native_operation(Operation::ModifySelection {
            change,
            node: target.node,
            instance_path: Some(target.path.clone()),
        });
        assert_eq!((editor.status.as_str(), editor.error), ("sentinel", false));
    }
    assert_eq!(editor.selection.len(), 2);
}

#[test]
fn remote_modify_selection_follows_the_same_guards() {
    let (mut editor, t) = three_cube_editor();
    let operation = |target: &TransformTarget| Operation::ModifySelection {
        change: SelectionChange::Add,
        node: target.node,
        instance_path: Some(target.path.clone()),
    };
    assert!(matches!(
        remote(&mut editor, operation(&t[0])),
        Ok(OperationReply::Changed)
    ));
    editor.channels[0] = "7".into();
    editor.channel_intents[0] = true;
    assert!(matches!(
        remote(&mut editor, operation(&t[1])),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.selection.members(), [t[0].clone()]);
}

/// Three cubes in a row facing the camera, none selected, and a settled frame.
fn gesture_rig() -> (Editor, egui::Context, egui::Rect, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for index in 0..3 {
        editor.create(Primitive::Cube);
        let id = editor.selected().unwrap();
        editor
            .apply_operation(Operation::SetTransform {
                id,
                transform: TransformInput {
                    translation: [3.0 * (index as f64 - 1.0), 0.0, 0.0],
                    rotation_degrees: [0.0; 3],
                    scale: [1.0; 3],
                },
            })
            .unwrap();
    }
    let targets = editor.history.scene().transform_paths().unwrap();
    editor.deselect_all();
    editor.camera =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 14.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    let output = frame(&mut editor, &ctx, vec![]);
    let rect = face_viewport_rect(&output);
    (editor, ctx, rect, targets)
}

fn screen_of(editor: &Editor, rect: egui::Rect, x: f32) -> egui::Pos2 {
    let ndc = editor
        .camera
        .project(Vec3::new(x, 0.0, 0.0), editor.aspect)
        .unwrap()
        .unwrap();
    rect.min
        + egui::vec2(
            (ndc.x + 1.0) * 0.5 * rect.width(),
            (1.0 - ndc.y) * 0.5 * rect.height(),
        )
}

fn click_cube(
    rig: &mut (Editor, egui::Context, egui::Rect, Vec<TransformTarget>),
    index: usize,
    modifiers: egui::Modifiers,
) {
    let at = screen_of(&rig.0, rig.2, 3.0 * (index as f32 - 1.0));
    face_pointer_click(&mut rig.0, &rig.1, at, modifiers, vec![]);
}

fn members(rig: &(Editor, egui::Context, egui::Rect, Vec<TransformTarget>)) -> Vec<NodeId> {
    rig.0.selection.members().iter().map(|t| t.node).collect()
}

const SHIFT_CTRL: egui::Modifiers = egui::Modifiers {
    shift: true,
    ..egui::Modifiers::CTRL
};

#[test]
fn viewport_clicks_replace_toggle_add_and_remove_with_the_lead_last() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    assert_eq!(members(&rig), [ids[0]]);
    click_cube(&mut rig, 1, SHIFT_CTRL);
    assert_eq!(members(&rig), [ids[0], ids[1]]);
    click_cube(&mut rig, 2, egui::Modifiers::SHIFT);
    assert_eq!(members(&rig), [ids[0], ids[1], ids[2]]);
    click_cube(&mut rig, 0, egui::Modifiers::CTRL);
    assert_eq!(members(&rig), [ids[1], ids[2]]);
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    assert_eq!(members(&rig), [ids[0]]);
    assert!(!rig.0.error, "{}", rig.0.status);
}

#[test]
fn viewport_shift_on_existing_member_removes_it() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    click_cube(&mut rig, 1, SHIFT_CTRL);
    click_cube(&mut rig, 0, egui::Modifiers::SHIFT);
    assert_eq!(members(&rig), [ids[1]]);
}

#[test]
fn viewport_add_on_existing_member_makes_it_lead() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    click_cube(&mut rig, 1, SHIFT_CTRL);
    click_cube(&mut rig, 0, SHIFT_CTRL);
    assert_eq!(members(&rig), [ids[1], ids[0]]);
}

#[test]
fn viewport_ctrl_on_unselected_is_a_noop() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    let status = rig.0.status.clone();
    click_cube(&mut rig, 2, egui::Modifiers::CTRL);
    assert_eq!(members(&rig), [ids[0]]);
    assert_eq!((rig.0.status.clone(), rig.0.error), (status, false));
}

#[test]
fn viewport_command_click_acts_as_ctrl() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    click_cube(&mut rig, 1, SHIFT_CTRL);
    let command = egui::Modifiers {
        command: true,
        mac_cmd: true,
        ..egui::Modifiers::NONE
    };
    click_cube(&mut rig, 1, command);
    assert_eq!(members(&rig), [ids[0]]);
    let shift_command = egui::Modifiers {
        shift: true,
        ..command
    };
    click_cube(&mut rig, 2, shift_command);
    assert_eq!(members(&rig), [ids[0], ids[2]]);
}

#[test]
fn viewport_modified_click_on_empty_space_keeps_the_selection_and_a_plain_one_clears_it() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    click_cube(&mut rig, 1, SHIFT_CTRL);
    let empty = rig.2.min + egui::vec2(8.0, 8.0);
    for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::CTRL, SHIFT_CTRL] {
        face_pointer_click(&mut rig.0, &rig.1, empty, modifiers, vec![]);
        assert_eq!(members(&rig), [ids[0], ids[1]], "{modifiers:?}");
        assert!(!rig.0.error);
    }
    face_pointer_click(&mut rig.0, &rig.1, empty, egui::Modifiers::NONE, vec![]);
    assert!(rig.0.selection.is_empty());
}

#[test]
fn viewport_ctrl_click_arriving_as_a_secondary_click_does_not_select() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    let at = screen_of(&rig.0, rig.2, 3.0);
    for pressed in [true, false] {
        frame(
            &mut rig.0,
            &rig.1,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
        );
    }
    frame(&mut rig.0, &rig.1, vec![]);
    assert_eq!(members(&rig), [ids[0]]);
}

#[test]
fn modified_clicks_are_still_refused_in_component_modes() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    rig.0
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::CTRL, SHIFT_CTRL] {
        click_cube(&mut rig, 1, modifiers);
        assert!(
            rig.0.error && rig.0.status.contains("component modes"),
            "{}",
            rig.0.status
        );
        assert_eq!(members(&rig), [ids[0]]);
    }
}

#[test]
fn viewport_modified_click_commits_a_valid_draft_and_refuses_an_invalid_one() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    rig.0.channels[0] = "abc".into();
    rig.0.channel_intents[0] = true;
    click_cube(&mut rig, 1, SHIFT_CTRL);
    assert!(rig.0.error);
    assert_eq!(members(&rig), [ids[0]]);
    assert_eq!(rig.0.channels[0], "abc");
    rig.0.channels[0] = "-3.5".into();
    click_cube(&mut rig, 1, SHIFT_CTRL);
    assert_eq!(members(&rig), [ids[0], ids[1]]);
    let x = rig
        .0
        .history
        .scene()
        .node(ids[0])
        .unwrap()
        .transform()
        .values()[0];
    assert_eq!(x, -3.5);
}

#[test]
fn hierarchy_mode_modified_click_modifies_the_root() {
    let mut rig = gesture_rig();
    let ids: Vec<NodeId> = rig.3.iter().map(|t| t.node).collect();
    rig.0
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy))
        .unwrap();
    click_cube(&mut rig, 0, egui::Modifiers::NONE);
    click_cube(&mut rig, 2, SHIFT_CTRL);
    assert_eq!(members(&rig), [ids[0], ids[2]]);
}

fn outliner_rig() -> (Editor, egui::Context, Vec<NodeId>) {
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let ids: Vec<NodeId> = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .iter()
        .map(|t| t.node)
        .collect();
    editor.deselect_all();
    let ctx = egui::Context::default();
    frame(&mut editor, &ctx, vec![]);
    (editor, ctx, ids)
}

fn click_row(editor: &mut Editor, ctx: &egui::Context, index: usize, modifiers: egui::Modifiers) {
    let output = frame(editor, ctx, vec![]);
    let at = face_text_position(&output, &format!("◇  pCube{}", index + 1));
    face_pointer_click(editor, ctx, at, modifiers, vec![]);
}

fn selected_nodes(editor: &Editor) -> Vec<NodeId> {
    editor.selection.members().iter().map(|t| t.node).collect()
}

#[test]
fn outliner_shift_adds_and_a_plain_click_replaces() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    click_row(&mut editor, &ctx, 2, egui::Modifiers::SHIFT);
    assert_eq!(selected_nodes(&editor), [ids[0], ids[2]]);
    click_row(&mut editor, &ctx, 1, egui::Modifiers::NONE);
    assert_eq!(selected_nodes(&editor), [ids[1]]);
}

#[test]
fn outliner_shift_on_existing_member_makes_it_lead() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    click_row(&mut editor, &ctx, 1, egui::Modifiers::SHIFT);
    click_row(&mut editor, &ctx, 0, egui::Modifiers::SHIFT);
    assert_eq!(selected_nodes(&editor), [ids[1], ids[0]]);
}

#[test]
fn outliner_ctrl_click_toggles_the_clicked_row_not_the_last() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    click_row(&mut editor, &ctx, 1, egui::Modifiers::SHIFT);
    click_row(&mut editor, &ctx, 0, egui::Modifiers::CTRL);
    assert_eq!(selected_nodes(&editor), [ids[1]]);
    let command = egui::Modifiers {
        command: true,
        mac_cmd: true,
        ..egui::Modifiers::NONE
    };
    click_row(&mut editor, &ctx, 0, command);
    assert_eq!(selected_nodes(&editor), [ids[1], ids[0]]);
}

#[test]
fn outliner_ctrl_on_unselected_adds_it() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 2, egui::Modifiers::CTRL);
    assert_eq!(selected_nodes(&editor), [ids[2]]);
}

#[test]
fn outliner_shift_ctrl_adds() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    click_row(&mut editor, &ctx, 1, SHIFT_CTRL);
    click_row(&mut editor, &ctx, 1, SHIFT_CTRL);
    assert_eq!(selected_nodes(&editor), [ids[0], ids[1]]);
}

#[test]
fn outliner_highlights_every_member_and_marks_only_the_lead_row() {
    let (mut editor, ctx, _) = outliner_rig();
    let selected_fill = ctx.global_style().visuals.selection.bg_fill;
    let fills = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == selected_fill)
            })
            .count()
    };
    let none = fills(&frame(&mut editor, &ctx, vec![]));
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    click_row(&mut editor, &ctx, 1, egui::Modifiers::SHIFT);
    let output = frame(&mut editor, &ctx, vec![]);
    let colour = |name: &str| -> Color32 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == name => {
                    Some(text.galley.job.sections[0].format.color)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("row {name}"))
    };
    // The lead row is drawn strong; the other member is not.
    assert_ne!(colour("◇  pCube2"), colour("◇  pCube1"));
    assert_eq!(fills(&output) - none, 2, "one highlighted row per member");
}

#[test]
fn outliner_modified_clicks_are_refused_while_a_file_modal_is_open() {
    let (mut editor, ctx, ids) = outliner_rig();
    click_row(&mut editor, &ctx, 0, egui::Modifiers::NONE);
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    for modifiers in [
        egui::Modifiers::NONE,
        egui::Modifiers::SHIFT,
        egui::Modifiers::CTRL,
    ] {
        click_row(&mut editor, &ctx, 1, modifiers);
        assert_eq!(selected_nodes(&editor), [ids[0]], "{modifiers:?}");
    }
}

#[test]
fn modify_selection_at_the_cap_reports_the_typed_error_and_keeps_the_set() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let real = editor.history.scene().transform_paths().unwrap().remove(0);
    // Stand-in members for 4096 objects: the cap check never looks the members up.
    let full: Vec<TransformTarget> = (0..azteccraft::native_scene::MAX_TRANSFORM_PATHS as u64)
        .map(|index| TransformTarget {
            node: NodeId::from_decimal(&(100_000 + index).to_string()).unwrap(),
            path: real.path.clone(),
        })
        .collect();
    editor.set_selection_members_for_test(full.clone());
    let status = editor.status.clone();
    for change in [SelectionChange::Add, SelectionChange::Toggle] {
        let operation = || Operation::ModifySelection {
            change,
            node: real.node,
            instance_path: Some(real.path.clone()),
        };
        for result in [
            editor.apply_operation(operation()),
            remote(&mut editor, operation()),
        ] {
            let Err(OperationError::InvalidInput(message)) = result else {
                panic!("{change:?} at the cap must fail with InvalidInput");
            };
            assert_eq!(message, "A selection holds at most 4096 objects");
        }
        assert_eq!(editor.selection.members(), full.as_slice(), "{change:?}");
    }
    assert_eq!(editor.status, status);
    // A non-member Remove stays Unchanged at the cap.
    assert!(matches!(
        editor.apply_operation(Operation::ModifySelection {
            change: SelectionChange::Remove,
            node: real.node,
            instance_path: Some(real.path.clone()),
        }),
        Ok(OperationReply::Unchanged)
    ));
}
