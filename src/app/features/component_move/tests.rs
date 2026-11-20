//! The native begin of a component Move through the real Operations (no egui): which vertices a
//! drag moves, the refusals before it starts, cancellation, and the inspection block.
use super::*;
use crate::editor_operations::{
    ComponentKind, MarqueeMode, MoveDistance, Operation, OperationError, OperationReply,
    SelectionMode,
};
use azteccraft::{
    history::{ManipulationFinish, ManipulationId},
    scene::Primitive,
    selection::ComponentOwner,
    view_scene::ViewScene,
};
// Only the unix-only source-bound test names it.
#[cfg(unix)]
use azteccraft::native_scene::NativeError;

fn owner_at(editor: &Editor, instance: usize) -> ComponentOwner {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[instance];
    ComponentOwner {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
    }
}

pub(super) fn cube_with(vertices: &[usize]) -> Editor {
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

pub(super) fn pick(editor: &mut Editor, vertices: &[usize]) {
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

pub(super) fn begin_op(editor: &Editor, generation: Option<u64>) -> Operation {
    Operation::BeginMove {
        target: editor.selected_move_lead().unwrap(),
        axis: WorldAxis::X,
        expected_selection_generation: generation,
    }
}

pub(super) fn begin(editor: &mut Editor) -> ManipulationId {
    let op = begin_op(editor, None);
    match editor.apply_operation(op).unwrap() {
        OperationReply::ManipulationStarted { manipulation_id } => {
            ManipulationId::from_decimal(&manipulation_id).unwrap()
        }
        other => panic!("{other:?}"),
    }
}

pub(super) fn update(editor: &mut Editor, id: ManipulationId, cm: f64) {
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

fn refused_with(editor: &mut Editor, text: &str) {
    let before = editor.history.scene().clone();
    let op = begin_op(editor, None);
    match editor.apply_operation(op) {
        Err(OperationError::InvalidInput(message)) => assert!(message.contains(text), "{message}"),
        other => panic!("expected `{text}`, got {other:?}"),
    }
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
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

#[test]
fn begin_refuses_with_the_scene_untouched() {
    // No vertex picked.
    let mut editor = cube_with(&[]);
    refused_with(&mut editor, "at least one selected vertex");
    // Two objects selected.
    let mut editor = cube_with(&[0]);
    editor.create(Primitive::Cube);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths);
    let op = begin_op(&editor, None);
    assert!(matches!(
        editor.apply_operation(op),
        Err(OperationError::MultipleSelection)
    ));
    // A set owned by another object than the selected one.
    let mut editor = cube_with(&[0]);
    editor.create(Primitive::Cube);
    let other = owner_at(&editor, 0);
    editor.modify_component_set(ComponentKind::Vertex, |set| set.replace_with(&other, [0]));
    refused_with(&mut editor, "belong to another object");
    // Parent orientation.
    let mut editor = cube_with(&[0]);
    editor
        .apply_operation(Operation::SetMoveSpace {
            space: azteccraft::native_scene::MoveSpace::Parent,
        })
        .unwrap();
    refused_with(&mut editor, "World and Object axis orientations only");
    // A mirrored ancestor makes the Object frame left-handed. Refusing it is PROVISIONAL: it
    // follows the Rotate and Scale handle rule, and no Maya page documents it for Move.
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.native_group();
    let group = editor.selected_move_lead().unwrap().node;
    editor
        .apply_operation(Operation::EditNode {
            id: group,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None; 3],
            scale: [Some(-1.0), Some(1.0), Some(1.0)],
        })
        .unwrap();
    let child = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .pop()
        .unwrap();
    editor
        .apply_operation(Operation::Select {
            node: Some(child.node),
            instance_path: Some(child.path),
        })
        .unwrap();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    pick(&mut editor, &[0]);
    editor
        .apply_operation(Operation::SetMoveSpace {
            space: azteccraft::native_scene::MoveSpace::Object,
        })
        .unwrap();
    refused_with(&mut editor, "mirrored");
    // Snap to grids.
    let mut editor = cube_with(&[0]);
    editor.snap_to_grid = true;
    refused_with(&mut editor, "Snap to grids");
    // A remote caller must name the generation it inspected.
    let mut editor = cube_with(&[0]);
    let before = editor.history.scene().clone();
    let op = begin_op(&editor, None);
    let remote = editor.apply_operation_guarded(
        azteccraft::history::ManipulationProvenance::Remote,
        op,
        &|| true,
    );
    assert!(matches!(
        remote,
        Err(OperationError::InvalidInput(ref message))
            if message == "A remote component Move needs expected_selection_generation from inspect_scene"
    ));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_set_over_the_vertex_cap_refuses_with_its_count() {
    let mut editor = cube_with(&[]);
    let owner = owner_at(&editor, 0);
    editor.modify_component_set(ComponentKind::Vertex, |set| {
        set.replace_with(&owner, 0..=azteccraft::native_scene::MAX_MOVED_VERTICES)
    });
    refused_with(&mut editor, "limited to 8192 vertices; 8193 are selected");
}

#[test]
fn a_stale_generation_is_refused_before_the_drag_starts() {
    let mut editor = cube_with(&[0]);
    let stale = editor.selection_generation().wrapping_sub(1);
    let op = begin_op(&editor, Some(stale));
    assert!(matches!(
        editor.apply_operation(op),
        Err(OperationError::StaleSelection { .. })
    ));
    assert!(editor.history.active_manipulation().is_none());
}

#[test]
fn inspection_during_a_component_drag_names_the_move() {
    let mut editor = cube_with(&[0]);
    let id = begin(&mut editor);
    update(&mut editor, id, 3.0);
    let inspected = editor.inspect_manipulation().unwrap();
    let active = inspected.active.unwrap();
    assert_eq!(active.kind, "move");
    assert_eq!(active.axis, Some("x"));
    assert_eq!(active.space, "world");
    assert_eq!(active.provenance, "native");
    assert_eq!(active.value, 3.0);
    assert_eq!(active.unit, "cm");
    assert_eq!(active.parameter, "distance");
    assert_eq!(active.channels, "translate");
    assert_eq!(active.baseline_channels, [0.0; 3]);
    assert_eq!(active.current_channels, [0.0; 3]);
    // The transaction counts the owning object, not its vertices (documented in mcp.md).
    assert_eq!((active.member_count, active.covered_count), (1, 0));
    assert!(editor.inspect(false).is_ok());
}

#[test]
fn inspecting_a_free_direction_drag_fails_loud_instead_of_reporting_world() {
    let mut editor = cube_with(&[0]);
    let owner = owner_at(&editor, 0);
    let set = azteccraft::native_scene::VertexSetTarget {
        transform: owner.transform,
        shape: owner.shape,
        path: owner.path,
        vertex_indices: vec![0],
    };
    editor
        .history
        .begin_move_components(
            &set,
            azteccraft::history::MoveDirection::Direction(glam::DVec3::X),
            editor.scene_token,
            azteccraft::history::ManipulationProvenance::Native,
            1 << 20,
        )
        .unwrap();
    assert!(matches!(
        editor.inspect_manipulation(),
        Err(OperationError::InvalidInput(ref message)) if message.contains("World and Object")
    ));
}

#[cfg(unix)]
#[test]
fn an_opened_source_bound_scene_refuses_a_component_move_with_the_scene_reason() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored.mb");
    let mut authored = Editor::default();
    authored.create(Primitive::Cube);
    authored
        .apply_operation(Operation::SaveSceneAs { path: path.clone() })
        .unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Vertex))
        .unwrap();
    let scene = editor.history.scene();
    assert_eq!(
        scene.vertex_move_refusal(scene.shapes()[0].id(), &[0]),
        Some(NativeError::UnsupportedEdit)
    );
    let paths = scene.transform_paths().unwrap();
    let owner = {
        let view = ViewScene::from_native(scene, 1 << 28).unwrap();
        let instance = &view.instances()[0];
        ComponentOwner {
            transform: instance.transform(),
            shape: instance.shape(),
            path: instance.path().to_vec(),
        }
    };
    let node = owner.transform;
    editor
        .apply_operation(Operation::Select {
            node: Some(node),
            instance_path: Some(paths.iter().find(|p| p.node == node).unwrap().path.clone()),
        })
        .unwrap();
    editor
        .apply_operation(Operation::ModifyComponentSelection {
            kind: ComponentKind::Vertex,
            mode: MarqueeMode::Replace,
            owner,
            indices: vec![0],
        })
        .unwrap();
    let before = editor.history.scene().clone();
    let op = begin_op(&editor, None);
    assert!(matches!(
        editor.apply_operation(op),
        Err(OperationError::Scene(NativeError::UnsupportedEdit))
    ));
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
}
