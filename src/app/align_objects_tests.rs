//! Issue #313 slice 4: `Operation::AlignObjects` through the shared dispatch path; native and remote
//! callers reach the same `Command`.
use super::tests::remote;
use super::transform_ops_tests::{baseline, depth, shape};
use super::*;
use azteccraft::native_scene::{
    AlignMode, AlignOptions, AlignTo, NativeError, TransformRefusal, WorldAxis,
};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn options(x: AlignMode, y: AlignMode, z: AlignMode) -> AlignOptions {
    AlignOptions {
        modes: [x, y, z],
        to: AlignTo::SelectionAverage,
    }
}

fn aligning(options: AlignOptions, generation: Option<u64>) -> Operation {
    Operation::AlignObjects {
        options,
        expected_selection_generation: generation,
    }
}

fn min_x() -> AlignOptions {
    options(AlignMode::Min, AlignMode::None, AlignMode::None)
}

fn make() -> Operation {
    aligning(min_x(), None)
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
fn it_is_one_named_undo_step_with_an_exact_restore_and_writes_only_translation() {
    let (mut editor, ids) = cubes(3);
    let before = editor.history.scene().clone();
    let kept: Vec<_> = ids.iter().map(|t| channels_of(&editor, t)).collect();
    let steps = depth(&mut editor);
    assert!(matches!(
        editor.apply_operation(make()),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(
        editor.history.undo_label().as_deref(),
        Some("Align Objects")
    );
    assert_eq!(depth(&mut editor), steps + 1);
    for (target, old) in ids.iter().zip(&kept) {
        let now = channels_of(&editor, target);
        assert_eq!(now[1], old[1], "rotation is untouched");
        assert_eq!(now[2], old[2], "scale is untouched");
        assert_eq!(now[0][1], old[0][1], "unchosen Y is untouched");
        assert_eq!(now[0][2], old[0][2], "unchosen Z is untouched");
    }
    assert_ne!(editor.history.scene(), &before);
    let after = editor.history.scene().clone();
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.redo());
    assert_eq!(editor.history.scene(), &after);
}

#[test]
fn native_and_remote_reach_the_same_scene() {
    let (mut native, _) = cubes(2);
    let (mut remote_editor, _) = cubes(2);
    let generation = Some(remote_editor.selection_generation());
    native.apply_operation(make()).unwrap();
    assert!(matches!(
        remote(&mut remote_editor, aligning(min_x(), generation)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(shape(&native), shape(&remote_editor));
    assert_eq!(
        native.history.undo_label(),
        remote_editor.history.undo_label()
    );
}

#[test]
fn no_selection_one_selection_and_no_axis_are_refused_untouched() {
    let (mut editor, ids) = cubes(2);
    let none = options(AlignMode::None, AlignMode::None, AlignMode::None);
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(aligning(none, None)),
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
                    NativeError::InvalidTransformEdit(TransformRefusal::AlignTooFew)
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
        remote(&mut editor, aligning(min_x(), Some(stale))),
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
                    if text.ends_with("is unsupported; switch to Object or Hierarchy selection")),
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
        remote(&mut editor, aligning(min_x(), generation)),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &scene);
    editor.apply_operation(make()).unwrap();
    assert!(!editor.has_draft());
    assert_eq!(depth(&mut editor), steps + 2, "Edit then the align");
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
        remote(&mut editor, aligning(min_x(), generation)),
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
        Err(OperationError::Transform(NativeError::ReadOnlyFormat))
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn last_selected_follows_the_selection_order_not_the_outliner_order() {
    let (mut editor, ids) = cubes(3);
    let to = AlignOptions {
        modes: [AlignMode::Min, AlignMode::None, AlignMode::None],
        to: AlignTo::LastSelected,
    };
    // selected against the outliner order: ids[0] is the lead although it is outliner-first
    let order = vec![ids[2].clone(), ids[1].clone(), ids[0].clone()];
    editor.set_selection_members_for_test(order);
    editor.sync_fields();
    let kept = channels_of(&editor, &ids[0]);
    let moved = channels_of(&editor, &ids[2]);
    editor.apply_operation(aligning(to, None)).unwrap();
    assert_eq!(
        channels_of(&editor, &ids[0]),
        kept,
        "the lead does not move"
    );
    assert_ne!(
        channels_of(&editor, &ids[2]),
        moved,
        "the outliner-last one moves"
    );
}

#[test]
fn undoing_a_later_edit_then_the_align_restores_the_original_exactly() {
    let (mut editor, ids) = cubes(2);
    let original = editor.history.scene().clone();
    editor.apply_operation(make()).unwrap();
    let aligned = editor.history.scene().clone();
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
    assert_eq!(editor.history.scene(), &aligned);
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &original);
}
