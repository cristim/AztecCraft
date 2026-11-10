//! Issue #39 slice B: `Operation::SetPivots` and `Operation::ResetPivots` through the shared
//! dispatch path. Native and remote callers reach the same `Command`; the guards, the
//! `commit_pending` policy and the selection rules are the other operations'.
use super::tests::remote;
use super::*;
use azteccraft::native_scene::{ALL_CHANNELS, NativeError, PivotReset, TransformTarget, WorldAxis};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

const RP: [f64; 3] = [1.0, 2.0, 3.0];
const SP: [f64; 3] = [-0.5, 0.25, 4.0];

fn cubes(count: usize) -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    (editor, all)
}

fn set(id: NodeId, preserve: bool) -> Operation {
    Operation::SetPivots {
        id,
        rotate_pivot: Some(RP),
        scale_pivot: Some(SP),
        preserve,
    }
}

fn pivots(editor: &Editor, id: NodeId) -> ([f64; 3], [f64; 3]) {
    let node = editor.history.scene().node(id).unwrap();
    (node.rotate_pivot(), node.scale_pivot())
}

fn baseline(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.selection.clone(),
        editor.selection_mode,
        editor.scene_token,
        editor.history.undo_label(),
        editor.channels.clone(),
        editor.name.clone(),
    )
}

/// The nodes as a caller sees them, without the per-editor scene token.
fn shape(editor: &Editor) -> serde_json::Value {
    let mut value = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    value["scene_token"] = serde_json::Value::Null;
    value
}

#[test]
fn native_and_remote_reach_the_same_command_for_both_operations() {
    for operation in [
        |id| set(id, true),
        |id| set(id, false),
        |id| Operation::SetPivots {
            id,
            rotate_pivot: Some(RP),
            scale_pivot: None,
            preserve: true,
        },
    ] {
        let (mut native, ids) = cubes(1);
        let (mut remote_editor, _) = cubes(1);
        let id = ids[0].node;
        assert!(matches!(
            native.apply_operation(operation(id)),
            Ok(OperationReply::Changed)
        ));
        assert!(matches!(
            remote(&mut remote_editor, operation(id)),
            Ok(OperationReply::Changed)
        ));
        assert_eq!(shape(&native), shape(&remote_editor));
        assert_eq!(native.history.undo_label().as_deref(), Some("Pivot"));
        assert_eq!(
            remote_editor.history.undo_label(),
            native.history.undo_label()
        );
    }
    for mode in [PivotReset::Center, PivotReset::Zero] {
        let (mut native, ids) = cubes(2);
        let (mut remote_editor, _) = cubes(2);
        for editor in [&mut native, &mut remote_editor] {
            editor.apply_operation(set(ids[0].node, false)).unwrap();
            editor.apply_operation(set(ids[1].node, false)).unwrap();
        }
        let operation = |generation| Operation::ResetPivots {
            mode,
            expected_selection_generation: generation,
        };
        assert!(matches!(
            native.apply_operation(operation(None)),
            Ok(OperationReply::Changed)
        ));
        let generation = Some(remote_editor.selection.generation());
        assert!(matches!(
            remote(&mut remote_editor, operation(generation)),
            Ok(OperationReply::Changed)
        ));
        assert_eq!(shape(&native), shape(&remote_editor));
        assert_eq!(native.history.undo_label().as_deref(), Some("Pivot"));
    }
}

#[test]
fn set_pivots_is_one_pivot_undo_step_and_undo_restores_the_scene() {
    let (mut editor, ids) = cubes(1);
    let id = ids[0].node;
    let before = editor.history.scene().clone();
    editor.apply_operation(set(id, true)).unwrap();
    assert_eq!(pivots(&editor, id), (RP, SP));
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.redo());
    assert_eq!(pivots(&editor, id), (RP, SP));
}

#[test]
fn reset_pivots_is_one_pivot_undo_step_for_every_selected_object() {
    let (mut editor, ids) = cubes(3);
    for target in &ids {
        editor.apply_operation(set(target.node, false)).unwrap();
    }
    let before = editor.history.scene().clone();
    editor
        .apply_operation(Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        })
        .unwrap();
    for target in &ids {
        assert_eq!(pivots(&editor, target.node), ([0.0; 3], [0.0; 3]));
    }
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
}

#[test]
fn set_pivots_without_pivots_checks_the_node_and_changes_nothing() {
    let (mut editor, ids) = cubes(1);
    let none = |id| Operation::SetPivots {
        id,
        rotate_pivot: None,
        scale_pivot: None,
        preserve: true,
    };
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(none(ids[0].node)),
        Ok(OperationReply::Unchanged)
    ));
    assert!(matches!(
        editor.apply_operation(none(NodeId::from_decimal("999").unwrap())),
        Err(OperationError::Scene(_))
    ));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn a_stale_scene_token_refuses_both_operations_and_keeps_the_scene() {
    let (mut editor, ids) = cubes(1);
    editor.apply_operation(set(ids[0].node, false)).unwrap();
    let before = baseline(&editor);
    for operation in [
        set(ids[0].node, true),
        Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ] {
        let stale = editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(uuid::Uuid::new_v4()),
                operation,
            },
            &egui::Context::default(),
        );
        assert!(matches!(stale, Err(OperationError::StaleScene)));
        assert_eq!(baseline(&editor), before);
    }
}

#[test]
fn a_draft_busies_remote_pivot_operations_and_native_ones_commit_it_first() {
    for operation in [
        |id| set(id, true),
        |_| Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ] {
        let (mut editor, ids) = cubes(1);
        let id = ids[0].node;
        editor
            .apply_operation(Operation::SetPivots {
                id,
                rotate_pivot: Some([7.0; 3]),
                scale_pivot: Some([7.0; 3]),
                preserve: false,
            })
            .unwrap();
        editor.set_selection_members_for_test(ids.clone());
        editor.sync_fields();
        let depth = |editor: &mut Editor| {
            let mut count = 0;
            while editor.history.undo() {
                count += 1;
            }
            while editor.history.redo() {}
            count
        };
        let before_depth = depth(&mut editor);

        editor.channels[0] = "2".into();
        let scene = editor.history.scene().clone();
        assert!(matches!(
            remote(&mut editor, operation(id)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.channels[0], "2");

        editor.apply_operation(operation(id)).unwrap();
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            2.0,
            "the valid draft was committed first"
        );
        assert!(!editor.has_draft());
        assert_eq!(depth(&mut editor), before_depth + 2, "Edit then Pivot");
    }
}

#[test]
fn an_invalid_draft_refuses_the_native_pivot_operation_and_keeps_everything() {
    for operation in [
        |id| set(id, true),
        |_| Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ] {
        let (mut editor, ids) = cubes(1);
        editor.apply_operation(set(ids[0].node, false)).unwrap();
        editor.channels[0] = "invalid".into();
        let before = baseline(&editor);
        assert!(matches!(
            editor.apply_operation(operation(ids[0].node)),
            Err(OperationError::InvalidInput(_))
        ));
        assert_eq!(baseline(&editor), before);
    }
}

/// Closing refuses a native pivot operation before the draft commits, and a remote caller is
/// refused in both states. The native modal case is pinned apart below.
#[test]
fn modal_and_closing_refuse_a_pivot_operation_before_it_commits_the_draft() {
    for closing in [false, true] {
        let mut editor = Editor::default();
        let mut id = NodeId::from_decimal("1").unwrap();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            id = editor.selected().unwrap();
            editor.new_document().unwrap();
        }
        editor.channels[0] = "2".into();
        let before = baseline(&editor);
        for native in [true, false] {
            if !native || closing {
                for operation in [
                    set(id, true),
                    Operation::ResetPivots {
                        mode: PivotReset::Zero,
                        expected_selection_generation: None,
                    },
                ] {
                    let result = if native {
                        editor.apply_operation(operation)
                    } else {
                        remote(&mut editor, operation)
                    };
                    assert!(
                        matches!(result, Err(OperationError::BusyEditing)),
                        "closing={closing} native={native}: {result:?}"
                    );
                    assert_eq!(baseline(&editor), before);
                }
            }
        }
    }
}

#[test]
fn an_active_manipulation_refuses_both_operations_for_both_callers() {
    let (mut editor, ids) = cubes(1);
    let id = ids[0].node;
    editor.set_selection_members_for_test(ids.clone());
    editor.sync_fields();
    editor
        .apply_operation(Operation::SetActiveTool(ActiveTool::Move))
        .unwrap();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X,
            expected_selection_generation: None,
        })
        .unwrap();
    let before = editor.history.scene().clone();
    let operations: [fn(NodeId) -> Operation; 2] = [
        |id| set(id, true),
        |_| Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ];
    for operation in operations {
        assert!(matches!(
            remote(&mut editor, operation(id)),
            Err(OperationError::BusyManipulation)
        ));
        assert!(editor.apply_operation(operation(id)).is_err());
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.active_manipulation().is_some());
    }
}

#[test]
fn reset_pivots_works_in_object_and_hierarchy_modes() {
    for mode in [SelectionMode::Object, SelectionMode::Hierarchy] {
        let (mut editor, ids) = cubes(2);
        for target in &ids {
            editor.apply_operation(set(target.node, false)).unwrap();
        }
        editor.selection_mode = mode;
        editor
            .apply_operation(Operation::ResetPivots {
                mode: PivotReset::Zero,
                expected_selection_generation: None,
            })
            .unwrap();
        for target in &ids {
            assert_eq!(pivots(&editor, target.node), ([0.0; 3], [0.0; 3]));
        }
    }
}

#[test]
fn reset_pivots_leaves_unselected_objects_alone() {
    let (mut editor, ids) = cubes(2);
    for target in &ids {
        editor.apply_operation(set(target.node, false)).unwrap();
    }
    editor.set_selection_members_for_test(vec![ids[1].clone()]);
    editor.sync_fields();
    editor
        .apply_operation(Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        })
        .unwrap();
    assert_eq!(pivots(&editor, ids[0].node), (RP, SP));
    assert_eq!(pivots(&editor, ids[1].node), ([0.0; 3], [0.0; 3]));
}

#[test]
fn reset_pivots_in_component_modes_is_refused_for_both_callers_and_keeps_state() {
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        for native in [true, false] {
            let (mut editor, ids) = cubes(2);
            editor.apply_operation(set(ids[0].node, false)).unwrap();
            editor.set_selection_members_for_test(vec![ids[0].clone()]);
            editor.sync_fields();
            editor.selection_mode = mode;
            let before = baseline(&editor);
            let operation = Operation::ResetPivots {
                mode: PivotReset::Zero,
                expected_selection_generation: None,
            };
            let result = if native {
                editor.apply_operation(operation)
            } else {
                remote(&mut editor, operation)
            };
            assert!(
                matches!(result, Err(OperationError::InvalidInput(ref text))
                    if text.ends_with("is unsupported; switch to Object or Hierarchy selection")),
                "{mode:?} native={native}: {result:?}"
            );
            assert_eq!(baseline(&editor), before);
        }
    }
}

#[test]
fn reset_pivots_with_nothing_selected_reports_no_selection() {
    let (mut editor, _) = cubes(2);
    editor.deselect_all();
    let before = baseline(&editor);
    for native in [true, false] {
        let operation = Operation::ResetPivots {
            mode: PivotReset::Center,
            expected_selection_generation: None,
        };
        let result = if native {
            editor.apply_operation(operation)
        } else {
            remote(&mut editor, operation)
        };
        assert!(matches!(result, Err(OperationError::NoSelection)));
        assert_eq!(baseline(&editor), before);
    }
}

/// An instanced node can appear twice in the selection; the command refuses duplicate ids, so
/// the operation hands it each node once.
#[test]
fn reset_pivots_applies_once_per_node_when_the_selection_repeats_it() {
    let (mut editor, ids) = cubes(1);
    editor.apply_operation(set(ids[0].node, false)).unwrap();
    let mut other = ids[0].clone();
    other.path.insert(0, ids[0].path[0]);
    editor.set_selection_members_for_test(vec![ids[0].clone(), other]);
    assert_eq!(editor.selection.len(), 2);
    editor
        .apply_operation(Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        })
        .unwrap();
    assert_eq!(pivots(&editor, ids[0].node), ([0.0; 3], [0.0; 3]));
}

/// Q4: every selected object is handled on its own, a selected ancestor and descendant alike.
/// The child sits 10 units off its parent, so the parent's box centre (hierarchy) differs from
/// the child's.
#[cfg(feature = "mcp-test-client")]
fn parent_and_child() -> (Editor, TransformTarget, TransformTarget) {
    let mut scene = azteccraft::native_scene::NativeScene::default();
    let parent = scene.create(Primitive::Cube, "parent").unwrap();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    scene.set_parents_for_test(child, &[parent]).unwrap();
    let mut channels = [None; 9];
    channels[0] = Some(10.0);
    scene.edit_node(child, None, channels).unwrap();
    for (id, rotate, scale) in [(parent, RP, SP), (child, [4.0, 5.0, 6.0], [-1.0, 2.0, 3.0])] {
        scene
            .set_pivots(id, Some(rotate), Some(scale), false)
            .unwrap();
    }
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let paths = editor.history.scene().transform_paths().unwrap();
    let find = |node| paths.iter().find(|p| p.node == node).unwrap().clone();
    let (parent, child) = (find(parent), find(child));
    editor.set_selection_members_for_test(vec![parent.clone(), child.clone()]);
    editor.sync_fields();
    (editor, parent, child)
}

#[cfg(feature = "mcp-test-client")]
#[test]
fn zero_pivot_handles_a_selected_ancestor_and_descendant_independently() {
    let (mut editor, parent, child) = parent_and_child();
    let before = editor.history.scene().clone();
    editor
        .apply_operation(Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        })
        .unwrap();
    for target in [&parent, &child] {
        assert_eq!(pivots(&editor, target.node), ([0.0; 3], [0.0; 3]));
    }
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
}

#[cfg(feature = "mcp-test-client")]
#[test]
fn center_pivot_centres_each_selected_node_on_its_own_hierarchy_box() {
    let (mut editor, parent, child) = parent_and_child();
    editor
        .apply_operation(Operation::ResetPivots {
            mode: PivotReset::Center,
            expected_selection_generation: None,
        })
        .unwrap();
    let (parent_rp, parent_sp) = pivots(&editor, parent.node);
    let (child_rp, child_sp) = pivots(&editor, child.node);
    assert_eq!(parent_rp, parent_sp);
    assert_eq!(child_rp, child_sp);
    assert!(child_rp.iter().all(|c| c.abs() < 1e-9), "{child_rp:?}");
    assert!(
        (parent_rp[0] - 5.0).abs() < 1e-9 && parent_rp[1].abs() < 1e-9 && parent_rp[2].abs() < 1e-9,
        "{parent_rp:?}"
    );
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
}

#[test]
fn a_keyed_node_refuses_with_the_pivot_variant_and_keeps_state() {
    let (mut editor, ids) = cubes(1);
    editor
        .apply_operation(Operation::SetKeyframe {
            nodes: None,
            time: None,
            channels: ALL_CHANNELS,
        })
        .unwrap();
    let before = baseline(&editor);
    for operation in [
        set(ids[0].node, true),
        Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ] {
        assert!(matches!(
            editor.apply_operation(operation),
            Err(OperationError::Pivot(NativeError::PivotOnKeyedNode))
        ));
        assert_eq!(baseline(&editor), before);
    }
}

#[test]
fn a_read_only_ascii_scene_refuses_with_the_pivot_variant() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let id = editor.history.scene().nodes()[0].id();
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let before = editor.history.scene().clone();
    for operation in [
        set(id, true),
        Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None,
        },
    ] {
        assert!(matches!(
            editor.apply_operation(operation),
            Err(OperationError::Pivot(NativeError::ReadOnlyFormat))
        ));
    }
    assert_eq!(editor.history.scene(), &before);
}

/// NaN and an overflowing request are the typed `NonFinite` refusal, whichever way the number
/// goes bad, and leave the scene alone.
#[test]
fn a_non_finite_or_overflowing_pivot_is_the_typed_pivot_refusal() {
    let (mut editor, ids) = cubes(1);
    let id = ids[0].node;
    editor
        .apply_operation(Operation::EditNode {
            id,
            name: None,
            translation: [None; 3],
            rotation_degrees: [Some(30.0), Some(40.0), Some(50.0)],
            scale: [Some(2.0), Some(3.0), Some(4.0)],
        })
        .unwrap();
    let before = baseline(&editor);
    for rotate in [
        [f64::NAN, 0.0, 0.0],
        [f64::INFINITY, 0.0, 0.0],
        [f64::MAX; 3],
        [-f64::MAX, f64::MAX, f64::MAX],
    ] {
        let result = editor.apply_operation(Operation::SetPivots {
            id,
            rotate_pivot: Some(rotate),
            scale_pivot: Some(rotate),
            preserve: true,
        });
        assert!(
            matches!(result, Err(OperationError::Pivot(NativeError::NonFinite))),
            "{rotate:?}: {result:?}"
        );
        assert_eq!(baseline(&editor), before);
    }
}

/// Issue #430: the native modal list in dispatch omits every edit (Delete, EditNode, Create),
/// so a native pivot operation runs under a confirmation dialog; the menus are disabled while
/// one is open. This pins the current outcome, not a wish: fixing #430 changes it for all edits.
#[test]
fn a_native_pivot_operation_still_runs_under_a_modal_dialog() {
    let (mut editor, ids) = cubes(1);
    let id = ids[0].node;
    editor.apply_operation(set(id, false)).unwrap();
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    assert!(matches!(
        editor.apply_operation(Operation::ResetPivots {
            mode: PivotReset::Zero,
            expected_selection_generation: None
        }),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(pivots(&editor, id), ([0.0; 3], [0.0; 3]));
    assert!(matches!(
        editor.apply_operation(set(id, true)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(pivots(&editor, id), (RP, SP));
}

fn reset_with(generation: Option<u64>) -> Operation {
    Operation::ResetPivots {
        mode: PivotReset::Zero,
        expected_selection_generation: generation,
    }
}

#[test]
fn a_remote_reset_over_several_objects_needs_the_current_selection_generation() {
    let (mut editor, ids) = cubes(2);
    for target in &ids {
        editor.apply_operation(set(target.node, false)).unwrap();
    }
    let before = baseline(&editor);
    let current = editor.selection.generation();
    assert!(matches!(
        remote(&mut editor, reset_with(None)),
        Err(OperationError::MultipleSelection)
    ));
    let stale = remote(&mut editor, reset_with(Some(current.wrapping_add(1))));
    assert!(
        matches!(&stale, Err(OperationError::StaleSelection { expected, .. }) if *expected == current.wrapping_add(1)),
        "{stale:?}"
    );
    assert_eq!(baseline(&editor), before);
    assert!(matches!(
        remote(&mut editor, reset_with(Some(current))),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(pivots(&editor, ids[1].node), ([0.0; 3], [0.0; 3]));
}

#[test]
fn a_stale_generation_refuses_a_single_object_reset_and_the_native_caller_needs_none() {
    let (mut editor, ids) = cubes(1);
    editor.apply_operation(set(ids[0].node, false)).unwrap();
    let before = baseline(&editor);
    let wrong = editor.selection.generation().wrapping_add(1);
    assert!(matches!(
        remote(&mut editor, reset_with(Some(wrong))),
        Err(OperationError::StaleSelection { .. })
    ));
    assert_eq!(baseline(&editor), before);
    assert!(matches!(
        remote(&mut editor, reset_with(None)),
        Ok(OperationReply::Changed)
    ));
    let (mut native, _) = cubes(2);
    assert!(matches!(
        native.apply_operation(reset_with(None)),
        Ok(OperationReply::Unchanged)
    ));
}
