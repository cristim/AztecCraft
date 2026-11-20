//! The native Move tool in Vertex selection through the real egui frame: arrows at the picked
//! vertices, press on an arrow begins the drag, a press elsewhere falls through to selection, and
//! the status and inspection say what the tool does (#41 T4c).
use super::tests::pick;
use super::*;
use crate::app::features::move_tool::tests::{button, frame, key, press_x, raw, setup};
use crate::editor_operations::{Operation, SelectionMode};
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect};

/// The default cube with all eight vertices picked in Vertex selection, so the arrows sit at the
/// cube centre exactly where the object Move arrows are.
fn all_vertices() -> (Editor, egui::Context, Rect) {
    let (mut editor, context, rect) = setup(1.0);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    pick(&mut editor, &[0, 1, 2, 3, 4, 5, 6, 7]);
    (editor, context, rect)
}

fn x_of(editor: &Editor, vertex: usize) -> f64 {
    editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(vertex)
        .unwrap()[0]
}

fn pointer_to(editor: &mut Editor, context: &egui::Context, position: Pos2) {
    frame(
        editor,
        context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(position)],
        ),
    );
}

#[test]
fn an_arrow_drag_moves_the_picked_vertices_and_one_undo_restores_them() {
    let (mut editor, context, rect) = all_vertices();
    let before = editor.history.scene().clone();
    let base = x_of(&editor, 0);
    let begin = press_x(&mut editor, &context, rect, 1.0);
    assert_eq!(editor.status, "World X Move (8 vertices)");
    pointer_to(
        &mut editor,
        &context,
        begin + egui::vec2(rect.height() / 20.0, 0.0),
    );
    assert!(
        (x_of(&editor, 0) - base - 1.0).abs() < 1e-3,
        "{}",
        x_of(&editor, 0)
    );
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(begin + egui::vec2(rect.height() / 20.0, 0.0), false),
        ),
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.status, "Move committed");
    assert_ne!(editor.history.scene(), &before);
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn escape_mid_drag_restores_the_scene_exactly() {
    let (mut editor, context, rect) = all_vertices();
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    pointer_to(
        &mut editor,
        &context,
        begin + egui::vec2(rect.height() / 10.0, 0.0),
    );
    assert_ne!(editor.history.scene(), &before);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![key(Key::Escape)],
        ),
    );
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_shift_press_on_an_arrow_does_not_start_a_drag() {
    let (mut editor, context, rect) = all_vertices();
    let before = editor.history.scene().clone();
    let position = rect.center() + egui::vec2(40.0, 0.0);
    let input = raw(
        egui::vec2(1440.0, 900.0),
        Pos2::ZERO,
        1.0,
        vec![
            Event::ModifiersChanged(Modifiers::SHIFT),
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::SHIFT,
            },
        ],
    );
    frame(&mut editor, &context, input);
    assert!(editor.native_move.is_none() && editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn without_a_picked_vertex_the_arrows_are_unavailable_and_a_press_falls_through() {
    let (mut editor, context, rect) = setup(1.0);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    let before = editor.history.scene().clone();
    let position = rect.center() + egui::vec2(40.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(position, true),
        ),
    );
    assert!(editor.native_move.is_none() && editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    // The press was not consumed by the Move tool and is no refusal: it reached selection.
    assert!(!editor.error, "{}", editor.status);
    assert!(!editor.move_consumed_frame && !editor.move_suppress_primary);
    let target = editor.selected_move_lead().unwrap();
    assert!(editor.move_handle_origin(&target).is_err());
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(!inspected.kinds[0].available);
    assert!(
        inspected.kinds[0]
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains("at least one selected vertex")
    );
}

#[test]
fn inspection_reports_the_move_available_with_a_viewport_block_when_vertices_are_picked() {
    let (mut editor, context, rect) = all_vertices();
    pointer_to(&mut editor, &context, rect.center() + egui::vec2(40.0, 0.0));
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(
        inspected.kinds[0].available,
        "{:?}",
        inspected.kinds[0].unavailable_reason
    );
    let viewport = inspected.viewport.expect("viewport block in Vertex Move");
    assert_eq!(viewport.pointer_hit, Some("x"));
    // Rotate and Scale keep the selection-mode refusal.
    assert!(!inspected.kinds[1].available && !inspected.kinds[2].available);
}

#[test]
fn a_single_vertex_puts_the_arrows_at_that_vertex() {
    let (mut editor, _context, _rect) = setup(1.0);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    pick(&mut editor, &[0]);
    let target = editor.selected_move_lead().unwrap();
    let origin = editor.move_handle_origin(&target).unwrap();
    let point = editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(0)
        .unwrap();
    assert!(
        (origin - glam::DVec3::from_array(point)).length() < 1e-9,
        "{origin:?} {point:?}"
    );
}

/// An own scale too extreme for the world frame to invert accurately: begin refuses, so the plan,
/// the handles and inspection must refuse with the same reason instead of offering arrows.
#[test]
fn an_ill_conditioned_frame_is_refused_by_the_plan_the_handles_inspection_and_begin_alike() {
    let (mut editor, _context, _rect) = setup(1.0);
    let node = editor.selected_move_lead().unwrap().node;
    editor
        .apply_operation(Operation::EditNode {
            id: node,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None; 3],
            scale: [Some(1e8), Some(1e-8), Some(1.0)],
        })
        .unwrap();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    pick(&mut editor, &[0, 1]);
    let target = editor.selected_move_lead().unwrap();
    let scene = editor.history.scene().clone();
    let plan = editor
        .component_move_plan(&target)
        .err()
        .map(|e| e.to_string());
    let reason = plan.expect("the plan refuses an ill-conditioned frame");
    assert!(editor.move_handle_origin(&target).is_err());
    assert!(editor.move_handle_directions(&target).is_err());
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(!inspected.kinds[0].available);
    assert!(
        inspected.kinds[0]
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains(&reason),
        "{reason}"
    );
    let begin = editor.apply_operation(Operation::BeginMove {
        target,
        axis: azteccraft::native_scene::WorldAxis::X,
        expected_selection_generation: None,
    });
    assert!(begin.is_err());
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &scene);
}
