use super::tests::{button, frame, press_x, raw, setup};
use super::*;
use crate::app::tests::remote;
use azteccraft::{history::ManipulationError, scene::Primitive};
use eframe::egui::{self, Event, Pos2};

fn two_selected() -> (Editor, egui::Context, egui::Rect) {
    let (mut editor, context, rect) = setup(1.0);
    editor.create(Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    assert_eq!(paths.len(), 2);
    editor.set_selection_members_for_test(paths);
    (editor, context, rect)
}

fn begin_op(target: TransformTarget, generation: Option<u64>) -> Operation {
    Operation::BeginMove {
        target,
        axis: WorldAxis::X,
        expected_selection_generation: generation,
    }
}

fn xs(editor: &Editor) -> Vec<f64> {
    editor
        .selection
        .members()
        .iter()
        .map(|member| {
            editor
                .history
                .scene()
                .node(member.node)
                .unwrap()
                .transform()
                .translation[0]
        })
        .collect()
}

#[test]
fn native_drag_moves_every_selected_object_and_one_undo_restores_all() {
    let (mut editor, context, rect) = two_selected();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let state = editor.history.active_manipulation().unwrap();
    assert_eq!((state.member_count(), state.covered_count()), (2, 0));
    let end = begin + egui::vec2(rect.height() / 20.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(end)],
        ),
    );
    for x in xs(&editor) {
        assert!((x - 1.0).abs() < 2e-5, "{x}");
    }
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(end, false),
        ),
    );
    assert!(editor.history.active_manipulation().is_none());
    editor.history_step(false);
    assert_eq!(editor.history.scene().nodes().len(), 2);
    assert_eq!(xs(&editor), [0.0, 0.0]);
}

#[test]
fn the_manipulator_is_the_lead_and_the_single_target_still_refuses() {
    let (editor, _, _) = two_selected();
    assert_eq!(
        editor.selected_move_lead().unwrap(),
        editor.selection.lead().unwrap().clone()
    );
    assert!(matches!(
        editor.selected_move_target(),
        Err(OperationError::MultipleSelection)
    ));
}

#[test]
fn an_unresolvable_member_names_it_and_changes_nothing() {
    let (mut editor, _, _) = two_selected();
    let members = editor.selection.members().to_vec();
    let mut bad = members[0].clone();
    bad.path = members[1].path.clone();
    editor.set_selection_members_for_test(vec![bad.clone(), members[1].clone()]);
    let name = editor
        .history
        .scene()
        .node(bad.node)
        .unwrap()
        .name()
        .to_string();
    let scene = editor.history.scene().clone();
    let lead = members[1].clone();
    let error = editor.apply_operation(begin_op(lead, None)).unwrap_err();
    assert!(
        matches!(&error, OperationError::Move(ManipulationError::Member { name: n, .. }) if *n == name),
        "{error:?}"
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn remote_begin_needs_the_current_generation_and_a_lead_target() {
    let (mut editor, _, _) = two_selected();
    let members = editor.selection.members().to_vec();
    let lead = members[1].clone();
    let scene = editor.history.scene().clone();
    assert!(matches!(
        remote(&mut editor, begin_op(lead.clone(), None)),
        Err(OperationError::MultipleSelection)
    ));
    for provenance_remote in [false, true] {
        let op = begin_op(members[0].clone(), Some(editor.selection.generation()));
        let result = if provenance_remote {
            remote(&mut editor, op)
        } else {
            editor.apply_operation(op)
        };
        assert!(matches!(result, Err(OperationError::MultipleSelection)));
    }
    let current_lead = editor.selection.lead().unwrap().clone();
    let generation = editor.selection.generation();
    let reply = remote(&mut editor, begin_op(current_lead, Some(generation))).unwrap();
    assert!(matches!(reply, OperationReply::ManipulationStarted { .. }));
    let inspection = editor.inspect(false).unwrap();
    let active = inspection.manipulation.active.unwrap();
    assert_eq!((active.member_count, active.covered_count), (2, 0));
    assert!(editor.cancel_move().unwrap());
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn rotate_and_scale_still_refuse_several_objects() {
    let (mut editor, _, _) = two_selected();
    let lead = editor.selection.lead().unwrap().clone();
    let space = crate::editor_operations::ManipulationSpace::World;
    let scene = editor.history.scene().clone();
    for operation in [
        Operation::BeginRotate {
            target: lead.clone(),
            axis: azteccraft::native_scene::RotateAxis::X,
            space,
        },
        Operation::BeginScale {
            target: lead.clone(),
            handle: azteccraft::native_scene::ScaleHandle::Uniform,
            space,
        },
    ] {
        assert!(matches!(
            editor.apply_operation(operation),
            Err(OperationError::MultipleSelection)
        ));
    }
    assert_eq!(editor.history.scene(), &scene);
}

fn toggle(editor: &mut Editor, target: &TransformTarget) {
    editor.native_operation(Operation::ModifySelection {
        change: crate::editor_operations::SelectionChange::Toggle,
        node: target.node,
        instance_path: Some(target.path.clone()),
    });
}

#[test]
fn a_native_toggle_of_a_non_lead_member_makes_the_inspected_generation_stale() {
    let (mut editor, _, _) = two_selected();
    editor.create(Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.clone());
    let stale = editor.selection.generation();
    toggle(&mut editor, &paths[1]);
    assert_eq!(
        editor.selection.members(),
        [paths[0].clone(), paths[2].clone()]
    );
    let scene = editor.history.scene().clone();
    for provenance_remote in [false, true] {
        let op = begin_op(paths[2].clone(), Some(stale));
        let result = if provenance_remote {
            remote(&mut editor, op)
        } else {
            editor.apply_operation(op)
        };
        assert!(
            matches!(result, Err(OperationError::InvalidInput(m)) if m.contains(&stale.to_string()))
        );
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.active_manipulation().is_none());
    }
}

#[test]
fn a_generation_naming_a_larger_selection_is_refused_after_the_selection_shrinks() {
    let (mut editor, _, _) = two_selected();
    let members = editor.selection.members().to_vec();
    let stale = editor.selection.generation();
    toggle(&mut editor, &members[0]);
    assert_eq!(editor.selection.len(), 1);
    let scene = editor.history.scene().clone();
    for provenance_remote in [false, true] {
        let op = begin_op(members[1].clone(), Some(stale));
        let result = if provenance_remote {
            remote(&mut editor, op)
        } else {
            editor.apply_operation(op)
        };
        assert!(
            matches!(result, Err(OperationError::InvalidInput(_))),
            "{result:?}"
        );
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.active_manipulation().is_none());
    }
}

#[test]
fn the_lead_node_with_another_path_is_not_the_lead() {
    let (mut editor, _, _) = two_selected();
    let lead = editor.selection.lead().unwrap().clone();
    let wrong = TransformTarget {
        node: lead.node,
        path: Vec::new(),
    };
    for provenance_remote in [false, true] {
        let op = begin_op(wrong.clone(), Some(editor.selection.generation()));
        let result = if provenance_remote {
            remote(&mut editor, op)
        } else {
            editor.apply_operation(op)
        };
        assert!(matches!(result, Err(OperationError::MultipleSelection)));
    }
}

#[test]
fn the_press_frame_is_the_leads_even_when_the_first_member_is_elsewhere() {
    let (mut editor, context, rect) = two_selected();
    let members = editor.selection.members().to_vec();
    editor
        .apply_operation(Operation::EditNode {
            id: members[0].node,
            name: None,
            translation: [Some(5.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let end = begin + egui::vec2(rect.height() / 20.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(end)],
        ),
    );
    let moved = xs(&editor);
    assert!(
        (moved[0] - 6.0).abs() < 2e-5 && (moved[1] - 1.0).abs() < 2e-5,
        "{moved:?}"
    );
}

fn gesture_frame(editor: &mut Editor, context: &egui::Context, events: Vec<Event>) {
    frame(
        editor,
        context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, events),
    );
}

#[test]
fn escape_and_a_tool_change_cancel_every_member() {
    for intent in [egui::Key::Escape, egui::Key::Q] {
        let (mut editor, context, rect) = two_selected();
        let scene = editor.history.scene().clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let end = begin + egui::vec2(rect.height() / 20.0, 0.0);
        gesture_frame(&mut editor, &context, vec![Event::PointerMoved(end)]);
        assert!(xs(&editor).iter().all(|x| *x != 0.0));
        gesture_frame(&mut editor, &context, vec![super::tests::key(intent)]);
        assert!(editor.history.active_manipulation().is_none(), "{intent:?}");
        assert!(editor.native_move.is_none());
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(xs(&editor), [0.0, 0.0]);
    }
}

#[test]
fn hierarchy_mode_moves_the_selection_and_face_mode_still_refuses() {
    let (mut editor, _, _) = two_selected();
    remote(
        &mut editor,
        Operation::SetSelectionMode(crate::editor_operations::SelectionMode::Hierarchy),
    )
    .unwrap();
    let lead = editor.selection.lead().unwrap().clone();
    let generation = editor.selection.generation();
    remote(&mut editor, begin_op(lead, Some(generation))).unwrap();
    assert_eq!(
        editor.history.active_manipulation().unwrap().member_count(),
        2
    );
    editor.cancel_move().unwrap();
    let (mut editor, _, _) = setup(1.0);
    editor
        .apply_operation(Operation::SetSelectionMode(
            crate::editor_operations::SelectionMode::Face,
        ))
        .unwrap();
    let lead = editor.selection.lead().unwrap().clone();
    let scene = editor.history.scene().clone();
    assert!(matches!(
        editor.apply_operation(begin_op(lead, None)),
        Err(OperationError::InvalidInput(m)) if m.contains("Face selection")
    ));
    assert_eq!(editor.history.scene(), &scene);
}
