//! Issue #313 slice 1: `Operation::ResetTransformations` and `Operation::FreezeTransformations`
//! through the shared dispatch path. Native and remote callers reach the same `Command`; the
//! guards and the selection rules are the other selection operations'.
use super::tests::remote;
use super::*;
use azteccraft::native_scene::{NativeError, TransformChannels, TransformRefusal, WorldAxis};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

const ALL: TransformChannels = TransformChannels::ALL;
const MAKERS: [fn() -> Operation; 2] = [|| reset(ALL, None), || freeze(ALL, None)];

fn reset(channels: TransformChannels, generation: Option<u64>) -> Operation {
    Operation::ResetTransformations {
        channels,
        expected_selection_generation: generation,
    }
}

fn freeze(channels: TransformChannels, generation: Option<u64>) -> Operation {
    Operation::FreezeTransformations {
        channels,
        expected_selection_generation: generation,
    }
}

/// `count` cubes, all selected, each moved, rotated and scaled.
fn posed(count: usize) -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    for target in &all {
        editor
            .apply_operation(Operation::EditNode {
                id: target.node,
                name: None,
                translation: [Some(1.0), Some(2.0), Some(3.0)],
                rotation_degrees: [Some(0.0), Some(0.0), Some(90.0)],
                scale: [Some(2.0), Some(2.0), Some(2.0)],
            })
            .unwrap();
    }
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    (editor, all)
}

fn baseline(editor: &Editor) -> impl PartialEq + std::fmt::Debug + use<> {
    (
        editor.history.scene().clone(),
        editor.selection.clone(),
        editor.selection_mode,
        editor.scene_token,
        editor.history.undo_label(),
        editor.channels.clone(),
    )
}

/// The nodes as a caller sees them, without the per-editor scene token.
fn shape(editor: &Editor) -> serde_json::Value {
    let mut value = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    value["scene_token"] = serde_json::Value::Null;
    value
}

fn depth(editor: &mut Editor) -> usize {
    let mut count = 0;
    while editor.history.undo() {
        count += 1;
    }
    while editor.history.redo() {}
    count
}

#[test]
fn each_operation_is_one_named_undo_step_with_an_exact_restore() {
    for (make, label) in [
        (MAKERS[0], "Reset Transformations"),
        (MAKERS[1], "Freeze Transformations"),
    ] {
        let (mut editor, ids) = posed(2);
        let before = editor.history.scene().clone();
        let steps = depth(&mut editor);
        assert!(matches!(
            editor.apply_operation(make()),
            Ok(OperationReply::Changed)
        ));
        assert_eq!(editor.history.undo_label().as_deref(), Some(label));
        assert_eq!(depth(&mut editor), steps + 1, "{label}");
        assert_ne!(editor.history.scene(), &before);
        for target in &ids {
            let node = editor.history.scene().node(target.node).unwrap();
            assert_eq!(node.transform().scale, [1.0; 3], "{label}");
            assert_eq!(node.transform().translation, [0.0; 3], "{label}");
        }
        let after = editor.history.scene().clone();
        assert!(editor.history.undo());
        assert_eq!(editor.history.scene(), &before, "{label}");
        assert!(editor.history.redo());
        assert_eq!(editor.history.scene(), &after, "{label}");
    }
}

#[test]
fn native_and_remote_reach_the_same_scene() {
    for (make, remote_make) in [
        (MAKERS[0], reset as fn(_, _) -> _),
        (MAKERS[1], freeze as fn(_, _) -> _),
    ] {
        let (mut native, _) = posed(2);
        let (mut remote_editor, _) = posed(2);
        let generation = Some(remote_editor.selection_generation());
        let remote_operation = remote_make(ALL, generation);
        native.apply_operation(make()).unwrap();
        assert!(matches!(
            remote(&mut remote_editor, remote_operation),
            Ok(OperationReply::Changed)
        ));
        assert_eq!(shape(&native), shape(&remote_editor));
        assert_eq!(
            native.history.undo_label(),
            remote_editor.history.undo_label()
        );
    }
}

#[test]
fn a_partial_reset_keeps_the_other_channels() {
    let (mut editor, ids) = posed(1);
    let channels = TransformChannels {
        translate: true,
        rotate: false,
        scale: false,
    };
    editor.apply_operation(reset(channels, None)).unwrap();
    let transform = editor
        .history
        .scene()
        .node(ids[0].node)
        .unwrap()
        .transform();
    assert_eq!(transform.translation, [0.0; 3]);
    assert_eq!(transform.rotation, [0.0, 0.0, 90.0]);
    assert_eq!(transform.scale, [2.0; 3]);
}

#[test]
fn a_remote_caller_must_name_the_selection_generation_with_several_objects() {
    let (mut editor, _) = posed(2);
    let before = baseline(&editor);
    for make in MAKERS {
        assert!(matches!(
            remote(&mut editor, make()),
            Err(OperationError::MultipleSelection)
        ));
        assert_eq!(baseline(&editor), before);
    }
    let stale = editor.selection_generation() + 1;
    for operation in [reset(ALL, Some(stale)), freeze(ALL, Some(stale))] {
        assert!(matches!(
            remote(&mut editor, operation),
            Err(OperationError::StaleSelection { .. })
        ));
        assert_eq!(baseline(&editor), before);
    }
}

#[test]
fn component_modes_and_an_empty_selection_are_refused_for_both_callers() {
    for make in MAKERS {
        for mode in [
            SelectionMode::Face,
            SelectionMode::Vertex,
            SelectionMode::Edge,
        ] {
            for native in [true, false] {
                let (mut editor, _) = posed(1);
                editor.selection_mode = mode;
                let before = baseline(&editor);
                let result = if native {
                    editor.apply_operation(make())
                } else {
                    remote(&mut editor, make())
                };
                assert!(
                    matches!(result, Err(OperationError::InvalidInput(ref text))
                        if text.ends_with("are unsupported; switch to Object or Hierarchy selection")),
                    "{mode:?} native={native}: {result:?}"
                );
                assert_eq!(baseline(&editor), before);
            }
        }
        let (mut editor, _) = posed(1);
        editor.deselect_all();
        let before = baseline(&editor);
        assert!(matches!(
            editor.apply_operation(make()),
            Err(OperationError::NoSelection)
        ));
        assert_eq!(baseline(&editor), before);
    }
}

#[test]
fn a_draft_busies_remote_calls_and_native_ones_commit_it_first() {
    for make in MAKERS {
        let (mut editor, ids) = posed(1);
        let steps = depth(&mut editor);
        editor.channels[0] = "5".into();
        let scene = editor.history.scene().clone();
        assert!(matches!(
            remote(&mut editor, make()),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.channels[0], "5");

        editor.apply_operation(make()).unwrap();
        assert!(!editor.has_draft());
        assert_eq!(depth(&mut editor), steps + 2, "Edit then the operation");
        let _ = ids;
    }
}

#[test]
fn an_invalid_draft_refuses_the_native_operation_and_keeps_everything() {
    for make in MAKERS {
        let (mut editor, _) = posed(1);
        editor.channels[0] = "invalid".into();
        let before = baseline(&editor);
        assert!(matches!(
            editor.apply_operation(make()),
            Err(OperationError::InvalidInput(_))
        ));
        assert_eq!(baseline(&editor), before);
    }
}

#[test]
fn modal_and_closing_refuse_both_operations_for_a_remote_caller_and_closing_for_native() {
    for closing in [false, true] {
        let mut editor = Editor::default();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        }
        let before = baseline(&editor);
        for make in MAKERS {
            for native in [false, true] {
                if native && !closing {
                    continue;
                }
                let result = if native {
                    editor.apply_operation(make())
                } else {
                    remote(&mut editor, make())
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

#[test]
fn an_active_manipulation_refuses_both_operations() {
    let (mut editor, _) = posed(1);
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
    for make in MAKERS {
        assert!(matches!(
            remote(&mut editor, make()),
            Err(OperationError::BusyManipulation)
        ));
        assert!(editor.apply_operation(make()).is_err());
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.active_manipulation().is_some());
    }
}

#[test]
fn a_mirrored_object_refuses_the_freeze_with_the_typed_transform_error() {
    let (mut editor, ids) = posed(1);
    editor
        .apply_operation(Operation::EditNode {
            id: ids[0].node,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None; 3],
            scale: [Some(-1.0), None, None],
        })
        .unwrap();
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(freeze(ALL, None)),
        Err(OperationError::Transform(
            NativeError::InvalidTransformEdit(TransformRefusal::Reflection)
        ))
    ));
    assert_eq!(baseline(&editor), before);
    assert!(
        editor.apply_operation(reset(ALL, None)).is_ok(),
        "reset has no mirror rule"
    );
}

#[test]
fn a_read_only_ascii_scene_refuses_both_operations_and_stays_untouched() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let before = editor.history.scene().clone();
    for make in MAKERS {
        assert!(matches!(
            editor.apply_operation(make()),
            Err(OperationError::Transform(_))
        ));
    }
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_native_operation_is_refused_under_a_modal_dialog() {
    for make in MAKERS {
        let (mut editor, _) = posed(1);
        editor.new_document().unwrap();
        assert!(editor.files.is_modal());
        let before = baseline(&editor);
        assert!(matches!(
            editor.apply_operation(make()),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(baseline(&editor), before);
    }
}
