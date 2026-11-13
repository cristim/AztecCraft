//! Snap to grids for Move: absolute pivot landing, Axis Orientation, guards and exact undo.

use super::*;
use crate::app::tests::remote;
use azteccraft::scene::Primitive;

fn one_cube() -> (Editor, TransformTarget) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let target = editor.selected_move_target().unwrap();
    (editor, target)
}

fn x_of(editor: &Editor, target: &TransformTarget) -> f64 {
    editor
        .history
        .scene()
        .node(target.node)
        .unwrap()
        .transform()
        .translation[0]
}

fn begin(editor: &mut Editor, target: &TransformTarget) -> ManipulationId {
    let reply = remote(
        editor,
        Operation::BeginMove {
            target: target.clone(),
            axis: WorldAxis::X,
            expected_selection_generation: None,
        },
    )
    .unwrap();
    let OperationReply::ManipulationStarted { manipulation_id } = reply else {
        panic!("{reply:?}")
    };
    ManipulationId::from_decimal(&manipulation_id).unwrap()
}

fn update(editor: &mut Editor, id: ManipulationId, distance: f64) -> Result<(), OperationError> {
    remote(
        editor,
        Operation::UpdateMove {
            id,
            distance: MoveDistance::Declared(distance),
        },
    )
    .map(|_| ())
}

fn finish(editor: &mut Editor, id: ManipulationId) {
    remote(
        editor,
        Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        },
    )
    .unwrap();
}

fn move_by(editor: &mut Editor, target: &TransformTarget, distance: f64) {
    let id = begin(editor, target);
    update(editor, id, distance).unwrap();
    finish(editor, id);
}

fn snap(editor: &mut Editor, enabled: bool) -> OperationReply {
    remote(editor, Operation::SetSnapToGrid { enabled }).unwrap()
}

#[test]
fn snap_is_off_by_default_and_the_setter_is_idempotent() {
    let (mut editor, _) = one_cube();
    assert!(!editor.snap_to_grid);
    let undo_depth = editor.history.undo_len();
    assert!(matches!(
        snap(&mut editor, false),
        OperationReply::Unchanged
    ));
    assert!(matches!(snap(&mut editor, true), OperationReply::Changed));
    assert!(matches!(snap(&mut editor, true), OperationReply::Unchanged));
    let inspected = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert_eq!(inspected["snap_to_grid"], true);
    assert_eq!(editor.history.undo_len(), undo_depth);
}

#[test]
fn an_unsnapped_move_keeps_the_requested_distance() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 2.4);
    assert_eq!(x_of(&editor, &target), 2.4);
}

#[test]
fn a_snapped_move_lands_the_pivot_on_a_grid_line_not_a_whole_step() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 0.3);
    snap(&mut editor, true);
    let id = begin(&mut editor, &target);
    update(&mut editor, id, 2.4).unwrap();
    assert_eq!(x_of(&editor, &target), 3.0);
    // The inspection reads the snapped distance (2.7 from 0.3), not the 2.4 asked for.
    let inspected = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let value = inspected["manipulation"]["active"]["value"]
        .as_f64()
        .unwrap();
    assert!((value - 2.7).abs() < 1e-12, "{value}");
    update(&mut editor, id, 2.6).unwrap();
    assert_eq!(x_of(&editor, &target), 3.0);
    update(&mut editor, id, 1.4).unwrap();
    assert_eq!(x_of(&editor, &target), 2.0);
    finish(&mut editor, id);
    assert_eq!(x_of(&editor, &target), 2.0);
}

#[test]
fn undo_and_redo_of_a_snapped_move_are_exact() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 0.3);
    snap(&mut editor, true);
    move_by(&mut editor, &target, 2.4);
    assert_eq!(x_of(&editor, &target), 3.0);
    editor.history_step(false);
    assert_eq!(x_of(&editor, &target).to_bits(), 0.3f64.to_bits());
    editor.history_step(true);
    assert_eq!(x_of(&editor, &target), 3.0);
}

#[test]
fn cancel_restores_the_baseline_exactly() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 0.3);
    snap(&mut editor, true);
    let id = begin(&mut editor, &target);
    update(&mut editor, id, 5.0).unwrap();
    remote(
        &mut editor,
        Operation::FinishMove {
            id,
            finish: ManipulationFinish::Cancel,
        },
    )
    .unwrap();
    assert_eq!(x_of(&editor, &target).to_bits(), 0.3f64.to_bits());
}

#[test]
fn the_snap_state_is_fixed_for_a_manipulation_and_toggling_it_is_busy() {
    let (mut editor, target) = one_cube();
    snap(&mut editor, true);
    let id = begin(&mut editor, &target);
    let before = editor.history.scene().clone();
    assert!(matches!(
        remote(&mut editor, Operation::SetSnapToGrid { enabled: false }),
        Err(OperationError::BusyManipulation)
    ));
    assert!(editor.snap_to_grid);
    assert_eq!(editor.history.scene(), &before);
    update(&mut editor, id, 1.4).unwrap();
    assert_eq!(x_of(&editor, &target), 1.0);
    finish(&mut editor, id);
    // A move begun with snap off afterwards is not snapped.
    snap(&mut editor, false);
    move_by(&mut editor, &target, 0.25);
    assert_eq!(x_of(&editor, &target), 1.25);
}

#[test]
fn setting_snap_is_refused_while_a_file_prompt_is_open() {
    let (mut editor, _) = one_cube();
    editor.apply_operation(Operation::NewScene).unwrap();
    // NewScene on a dirty scene opens the confirmation modal.
    assert!(editor.files.is_modal());
    assert!(matches!(
        remote(&mut editor, Operation::SetSnapToGrid { enabled: true }),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.snap_to_grid);
    // The native path has no earlier remote modal check, so only the dispatch guard stops it.
    let scene = editor.history.scene().clone();
    let undo_depth = editor.history.undo_len();
    assert!(matches!(
        editor.apply_operation(Operation::SetSnapToGrid { enabled: true }),
        Err(OperationError::BusyEditing)
    ));
    assert!(!editor.snap_to_grid);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_len(), undo_depth);
}

fn rotate_y(editor: &mut Editor, target: &TransformTarget, degrees: f64) {
    remote(
        editor,
        Operation::EditNode {
            id: target.node,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None, Some(degrees), None],
            scale: [None; 3],
        },
    )
    .unwrap();
}

#[test]
fn object_axes_follow_the_snap_and_an_unaligned_axis_is_refused_without_a_change() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 0.3);
    remote(
        &mut editor,
        Operation::SetMoveSpace {
            space: MoveSpace::Object,
        },
    )
    .unwrap();
    snap(&mut editor, true);

    // A half turn about Y flips the Object X axis onto world -X: x = 0.3 - 2.4 = -2.1 snaps to -2.
    rotate_y(&mut editor, &target, 180.0);
    let id = begin(&mut editor, &target);
    update(&mut editor, id, 2.4).unwrap();
    assert!((x_of(&editor, &target) + 2.0).abs() < 1e-9);
    finish(&mut editor, id);

    // A 30 degree turn leaves the Object X axis between grid axes: Begin refuses up front.
    rotate_y(&mut editor, &target, 30.0);
    let scene = editor.history.scene().clone();
    let undo_depth = editor.history.undo_len();
    for result in [
        editor.apply_operation(begin_op(&target)),
        remote(&mut editor, begin_op(&target)),
    ] {
        assert!(
            matches!(&result, Err(OperationError::InvalidInput(text)) if text.contains("Snap to grids")),
            "{result:?}"
        );
    }
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_len(), undo_depth);
    // With snap off the same move begins.
    snap(&mut editor, false);
    begin(&mut editor, &target);
}

fn begin_op(target: &TransformTarget) -> Operation {
    Operation::BeginMove {
        target: target.clone(),
        axis: WorldAxis::X,
        expected_selection_generation: None,
    }
}

fn set_x(editor: &mut Editor, target: &TransformTarget, x: f64) {
    remote(
        editor,
        Operation::EditNode {
            id: target.node,
            name: None,
            translation: [Some(x), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        },
    )
    .unwrap();
}

#[test]
fn a_pivot_a_few_ulps_off_a_grid_line_stays_instead_of_failing_the_move() {
    let (mut editor, target) = one_cube();
    set_x(&mut editor, &target, 2.000_000_000_000_000_4);
    snap(&mut editor, true);
    let id = begin(&mut editor, &target);
    update(&mut editor, id, 0.1).unwrap();
    assert_eq!(x_of(&editor, &target), 2.000_000_000_000_000_4);
    update(&mut editor, id, 1.2).unwrap();
    assert_eq!(x_of(&editor, &target), 3.0);
    finish(&mut editor, id);
}

#[test]
fn after_a_half_turn_object_move_the_next_snapped_move_toward_the_line_works() {
    let (mut editor, target) = one_cube();
    move_by(&mut editor, &target, 0.3);
    remote(
        &mut editor,
        Operation::SetMoveSpace {
            space: MoveSpace::Object,
        },
    )
    .unwrap();
    rotate_y(&mut editor, &target, 180.0);
    snap(&mut editor, true);
    move_by(&mut editor, &target, 2.4);
    // The pivot sits on -2 up to rounding; a second move toward the line must not error.
    let id = begin(&mut editor, &target);
    update(&mut editor, id, 0.1).unwrap();
    finish(&mut editor, id);
    assert!((x_of(&editor, &target) + 2.0).abs() < 1e-9);
}

#[test]
fn a_non_finite_distance_is_still_refused_with_snap_on() {
    let (mut editor, target) = one_cube();
    snap(&mut editor, true);
    let id = begin(&mut editor, &target);
    for value in [f64::NAN, f64::INFINITY] {
        assert!(matches!(
            update(&mut editor, id, value),
            Err(OperationError::InvalidInput(_))
        ));
    }
    assert_eq!(x_of(&editor, &target), 0.0);
}
