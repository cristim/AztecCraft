//! Issue #39 slice B: `Operation::SetPivots` and `Operation::ResetPivots` through the shared
//! dispatch path. Native and remote callers reach the same `Command`; the guards, the
//! `commit_pending` policy and the selection rules are the other operations'.
use super::tests::remote;
use super::*;
use azteccraft::native_scene::{PivotReset, TransformTarget, WorldAxis};

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
        let operation = || Operation::ResetPivots { mode };
        assert!(matches!(
            native.apply_operation(operation()),
            Ok(OperationReply::Changed)
        ));
        assert!(matches!(
            remote(&mut remote_editor, operation()),
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

/// Like Delete and EditNode, a native edit is not in the modal list (the dialog covers the UI);
/// closing refuses it before the draft commits, and a remote caller is refused in both states.
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
