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
