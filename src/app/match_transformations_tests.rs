//! Issue #313 slice 2: `Operation::MatchTransformations` through the shared dispatch path. The
//! last selected object is the target; native and remote callers reach the same `Command`.
use super::tests::remote;
use super::transform_ops_tests::{ALL, baseline, depth, shape};
use super::*;
use azteccraft::native_scene::{NativeError, TransformChannels, TransformRefusal, WorldAxis};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn matching(channels: TransformChannels, generation: Option<u64>) -> Operation {
    Operation::MatchTransformations {
        channels,
        expected_selection_generation: generation,
    }
}

fn make() -> Operation {
    matching(ALL, None)
}

/// `count` cubes with distinct transforms, all selected in creation order (the last is the lead).
fn cubes(count: usize) -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..count {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    for (i, target) in all.iter().enumerate() {
        let k = i as f64 + 1.0;
        editor
            .apply_operation(Operation::EditNode {
                id: target.node,
                name: None,
                translation: [Some(k), Some(2.0 * k), Some(-k)],
                rotation_degrees: [Some(10.0 * k), Some(5.0 * k), Some(20.0 * k)],
                scale: [Some(k), Some(k + 1.0), Some(2.0 * k)],
            })
            .unwrap();
    }
    editor.set_selection_members_for_test(all.clone());
    editor.sync_fields();
    (editor, all)
}

fn channels_of(editor: &Editor, target: &TransformTarget) -> [[f64; 3]; 3] {
    let t = editor
        .history
        .scene()
        .node(target.node)
        .unwrap()
        .transform();
    [t.translation, t.rotation, t.scale]
}

#[test]
fn it_is_one_named_undo_step_with_an_exact_restore_and_the_last_selected_is_the_target() {
    let (mut editor, ids) = cubes(3);
    let before = editor.history.scene().clone();
    let target = channels_of(&editor, &ids[2]);
    let steps = depth(&mut editor);
    assert!(matches!(
        editor.apply_operation(make()),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(
        editor.history.undo_label().as_deref(),
        Some("Match Transformations")
    );
    assert_eq!(depth(&mut editor), steps + 1);
    for moved in &ids[..2] {
        let now = channels_of(&editor, moved);
        for (got, want) in now.iter().flatten().zip(target.iter().flatten()) {
            assert!((got - want).abs() < 1e-9, "{now:?} vs {target:?}");
        }
    }
    assert_eq!(channels_of(&editor, &ids[2]), target);
    let after = editor.history.scene().clone();
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.redo());
    assert_eq!(editor.history.scene(), &after);
}

#[test]
fn a_partial_match_writes_only_the_chosen_channels() {
    let (mut editor, ids) = cubes(2);
    let before = channels_of(&editor, &ids[0]);
    let target = channels_of(&editor, &ids[1]);
    let rotate = TransformChannels {
        translate: false,
        rotate: true,
        scale: false,
    };
    editor.apply_operation(matching(rotate, None)).unwrap();
    let now = channels_of(&editor, &ids[0]);
    assert_eq!(now[0], before[0]);
    assert_eq!(now[2], before[2]);
    for (got, want) in now[1].iter().zip(target[1]) {
        assert!((got - want).abs() < 1e-9);
    }
}

#[test]
fn native_and_remote_reach_the_same_scene() {
    let (mut native, _) = cubes(2);
    let (mut remote_editor, _) = cubes(2);
    let generation = Some(remote_editor.selection_generation());
    native.apply_operation(make()).unwrap();
    assert!(matches!(
        remote(&mut remote_editor, matching(ALL, generation)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(shape(&native), shape(&remote_editor));
    assert_eq!(
        native.history.undo_label(),
        remote_editor.history.undo_label()
    );
}

#[test]
fn no_selection_one_selection_and_no_channel_are_refused_untouched() {
    let (mut editor, ids) = cubes(2);
    let none = TransformChannels {
        translate: false,
        rotate: false,
        scale: false,
    };
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(matching(none, None)),
        Err(OperationError::Transform(
            NativeError::InvalidTransformEdit(TransformRefusal::EmptySet)
        ))
    ));
    assert_eq!(baseline(&editor), before);
    editor.set_selection_members_for_test(vec![ids[0].clone()]);
    editor.sync_fields();
    let before = baseline(&editor);
    for native in [true, false] {
        let result = if native {
            editor.apply_operation(make())
        } else {
            remote(&mut editor, make())
        };
        assert!(
            matches!(
                result,
                Err(OperationError::Transform(
                    NativeError::InvalidTransformEdit(TransformRefusal::TooFewObjects)
                ))
            ),
            "{result:?}"
        );
        assert_eq!(baseline(&editor), before);
    }
    editor.deselect_all();
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(make()),
        Err(OperationError::NoSelection)
    ));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn a_remote_caller_must_name_a_current_selection_generation() {
    let (mut editor, _) = cubes(2);
    let before = baseline(&editor);
    assert!(matches!(
        remote(&mut editor, make()),
        Err(OperationError::MultipleSelection)
    ));
    let stale = editor.selection_generation() + 1;
    assert!(matches!(
        remote(&mut editor, matching(ALL, Some(stale))),
        Err(OperationError::StaleSelection { .. })
    ));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn component_modes_are_refused_for_both_callers() {
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        for native in [true, false] {
            let (mut editor, _) = cubes(2);
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
}

#[test]
fn a_draft_busies_remote_calls_and_native_ones_commit_it_first() {
    let (mut editor, _) = cubes(2);
    let steps = depth(&mut editor);
    editor.channels[0] = "5".into();
    let scene = editor.history.scene().clone();
    let generation = Some(editor.selection_generation());
    assert!(matches!(
        remote(&mut editor, matching(ALL, generation)),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &scene);
    editor.apply_operation(make()).unwrap();
    assert!(!editor.has_draft());
    assert_eq!(depth(&mut editor), steps + 2, "Edit then the match");
    editor.channels[0] = "invalid".into();
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(make()),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(baseline(&editor), before);
}

#[test]
fn modal_closing_and_an_active_manipulation_refuse_the_operation() {
    let (mut editor, _) = cubes(2);
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(make()),
        Err(OperationError::BusyEditing)
    ));
    let generation = Some(editor.selection_generation());
    assert!(matches!(
        remote(&mut editor, matching(ALL, generation)),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(baseline(&editor), before);

    let mut closing = Editor::default();
    closing.close_document().unwrap();
    assert!(matches!(
        closing.apply_operation(make()),
        Err(OperationError::BusyEditing)
    ));

    let (mut editor, ids) = cubes(2);
    editor.set_selection_members_for_test(vec![ids[0].clone()]);
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
    let scene = editor.history.scene().clone();
    assert!(matches!(
        remote(&mut editor, make()),
        Err(OperationError::BusyManipulation)
    ));
    assert!(editor.apply_operation(make()).is_err());
    assert_eq!(editor.history.scene(), &scene);
    assert!(editor.history.active_manipulation().is_some());
}

#[test]
fn a_read_only_ascii_scene_refuses_the_operation_and_stays_untouched() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.into_iter().take(2).collect());
    editor.sync_fields();
    let before = editor.history.scene().clone();
    assert!(matches!(
        editor.apply_operation(make()),
        Err(OperationError::Transform(_))
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn undoing_a_later_edit_then_the_match_restores_the_original_exactly() {
    let (mut editor, ids) = cubes(2);
    let original = editor.history.scene().clone();
    editor.apply_operation(make()).unwrap();
    let matched = editor.history.scene().clone();
    editor
        .apply_operation(Operation::EditNode {
            id: ids[0].node,
            name: None,
            translation: [Some(7.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &matched);
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &original);
}
