//! The native begin of a component Move through the real Operations (no egui): which vertices a
//! drag moves, the refusals before it starts, cancellation, and the inspection block.
use super::*;
use crate::editor_operations::{
    ComponentKind, MarqueeMode, MoveDistance, Operation, OperationReply, SelectionMode,
};
use azteccraft::{
    history::{ManipulationFinish, ManipulationId},
    scene::Primitive,
    selection::ComponentOwner,
    view_scene::ViewScene,
};

fn owner_at(editor: &Editor, instance: usize) -> ComponentOwner {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[instance];
    ComponentOwner {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
    }
}

fn cube_with(vertices: &[usize]) -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    if !vertices.is_empty() {
        pick(&mut editor, vertices);
    }
    editor
}

fn pick(editor: &mut Editor, vertices: &[usize]) {
    let owner = owner_at(editor, 0);
    editor
        .apply_operation(Operation::ModifyComponentSelection {
            kind: ComponentKind::Vertex,
            mode: MarqueeMode::Replace,
            owner,
            indices: vertices.to_vec(),
        })
        .unwrap();
}

fn begin_op(editor: &Editor, generation: Option<u64>) -> Operation {
    Operation::BeginMove {
        target: editor.selected_move_lead().unwrap(),
        axis: WorldAxis::X,
        expected_selection_generation: generation,
    }
}

fn begin(editor: &mut Editor) -> ManipulationId {
    let op = begin_op(editor, None);
    match editor.apply_operation(op).unwrap() {
        OperationReply::ManipulationStarted { manipulation_id } => {
            ManipulationId::from_decimal(&manipulation_id).unwrap()
        }
        other => panic!("{other:?}"),
    }
}

fn update(editor: &mut Editor, id: ManipulationId, cm: f64) {
    editor
        .apply_operation(Operation::UpdateMove {
            id,
            distance: MoveDistance::Centimeters(cm),
        })
        .unwrap();
}

fn x_of(editor: &Editor, vertex: usize) -> f64 {
    editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(vertex)
        .unwrap()[0]
}

#[test]
fn a_native_drag_moves_exactly_the_set_and_one_undo_restores_it() {
    let mut editor = cube_with(&[0, 3]);
    let before = editor.history.scene().clone();
    let untouched: Vec<f64> = (0..8).map(|v| x_of(&editor, v)).collect();
    let id = begin(&mut editor);
    update(&mut editor, id, 5.0);
    update(&mut editor, id, 2.0);
    for (v, base) in untouched.iter().enumerate() {
        let want = base + if v == 0 || v == 3 { 2.0 } else { 0.0 };
        assert!((x_of(&editor, v) - want).abs() < 1e-5, "vertex {v}");
    }
    editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        })
        .unwrap();
    assert!(editor.history.active_manipulation().is_none());
    assert_ne!(editor.history.scene(), &before);
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn cancel_and_a_zero_drag_leave_the_scene_and_the_undo_stack_untouched() {
    let mut editor = cube_with(&[1, 2, 5]);
    let before = editor.history.scene().clone();
    let id = begin(&mut editor);
    update(&mut editor, id, 7.0);
    editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Cancel,
        })
        .unwrap();
    assert_eq!(editor.history.scene(), &before);
    let id = begin(&mut editor);
    update(&mut editor, id, 0.0);
    let reply = editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        })
        .unwrap();
    assert!(matches!(reply, OperationReply::Unchanged));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn selection_mode_tool_and_scene_changes_mid_drag_cancel_with_the_scene_restored() {
    let changes = [
        Operation::SetSelectionMode(SelectionMode::Object),
        Operation::SetActiveTool(crate::editor_operations::ActiveTool::Rotate),
        Operation::DeselectAll,
        Operation::NewScene,
    ];
    for change in changes {
        let mut editor = cube_with(&[0]);
        let before = editor.history.scene().clone();
        let id = begin(&mut editor);
        update(&mut editor, id, 4.0);
        let is_new = matches!(change, Operation::NewScene);
        let _ = editor.apply_operation(change);
        assert!(editor.history.active_manipulation().is_none());
        if !is_new {
            assert_eq!(editor.history.scene(), &before);
        }
    }
}

#[test]
fn a_component_pick_mid_drag_cancels_it() {
    let mut editor = cube_with(&[0]);
    let before = editor.history.scene().clone();
    let id = begin(&mut editor);
    update(&mut editor, id, 4.0);
    pick(&mut editor, &[1]);
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
}
