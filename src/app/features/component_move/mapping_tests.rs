//! The Move tool in Edge and Face selection: the picked components map to their unique vertices
//! (a shared vertex once), with the same refusals as Vertex selection (#41 T5).
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

fn owner(editor: &Editor) -> ComponentOwner {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    ComponentOwner {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
    }
}

fn cube_picking(mode: SelectionMode, kind: ComponentKind, indices: &[usize]) -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor
        .apply_operation(Operation::SetSelectionMode(mode))
        .unwrap();
    let owner = owner(&editor);
    editor
        .apply_operation(Operation::ModifyComponentSelection {
            kind,
            mode: MarqueeMode::Replace,
            owner,
            indices: indices.to_vec(),
        })
        .unwrap();
    editor
}

fn x_of(editor: &Editor, vertex: usize) -> f64 {
    editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(vertex)
        .unwrap()[0]
}

fn begin(editor: &mut Editor) -> Result<ManipulationId, OperationError> {
    let target = editor.selected_move_lead().unwrap();
    let op = Operation::BeginMove {
        target,
        axis: azteccraft::native_scene::WorldAxis::X,
        expected_selection_generation: None,
    };
    match editor.apply_operation(op)? {
        OperationReply::ManipulationStarted { manipulation_id } => {
            Ok(ManipulationId::from_decimal(&manipulation_id).unwrap())
        }
        other => panic!("{other:?}"),
    }
}

/// Drags the current set by `cm` along X and returns which vertices moved and by how much.
fn drag(editor: &mut Editor, cm: f64) -> Vec<(usize, f64)> {
    let before: Vec<f64> = (0..8).map(|v| x_of(editor, v)).collect();
    let id = begin(editor).unwrap();
    editor
        .apply_operation(Operation::UpdateMove {
            id,
            distance: MoveDistance::Centimeters(cm),
        })
        .unwrap();
    editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        })
        .unwrap();
    (0..8)
        .map(|v| (v, x_of(editor, v) - before[v]))
        .filter(|(_, delta)| *delta != 0.0)
        .collect()
}

fn moved_vertices(editor: &Editor, kind: ComponentKind) -> Vec<usize> {
    let scene = editor.history.scene();
    let shape = scene.shapes()[0].id();
    let indices = editor.component_set(kind).indices();
    match kind {
        ComponentKind::Edge => scene.vertices_of_edges(shape, indices).unwrap(),
        ComponentKind::Face => scene.vertices_of_faces(shape, indices).unwrap(),
        ComponentKind::Vertex => indices.to_vec(),
    }
}

#[test]
fn an_edge_drag_moves_both_endpoints_and_shared_vertices_move_once() {
    let mut editor = cube_picking(SelectionMode::Edge, ComponentKind::Edge, &[0]);
    let expected = moved_vertices(&editor, ComponentKind::Edge);
    assert_eq!(expected.len(), 2);
    let moved = drag(&mut editor, 3.0);
    assert_eq!(moved.iter().map(|(v, _)| *v).collect::<Vec<_>>(), expected);
    assert!(moved.iter().all(|(_, delta)| (delta - 3.0).abs() < 1e-5));
    // Two edges that share a vertex: the union, each vertex once.
    let scene = editor.history.scene();
    let shape = scene.shapes()[0].id();
    let pair = (1..12)
        .find(|&other| {
            let union = scene.vertices_of_edges(shape, &[0, other]).unwrap();
            union.len() == 3
        })
        .expect("an edge sharing a vertex with edge 0");
    pick_set(&mut editor, ComponentKind::Edge, &[0, pair]);
    let moved = drag(&mut editor, 2.0);
    assert_eq!(moved.len(), 3);
    assert!(
        moved.iter().all(|(_, delta)| (delta - 2.0).abs() < 1e-5),
        "{moved:?}"
    );
}

fn pick_set(editor: &mut Editor, kind: ComponentKind, indices: &[usize]) {
    let owner = owner(editor);
    editor
        .apply_operation(Operation::ModifyComponentSelection {
            kind,
            mode: MarqueeMode::Replace,
            owner,
            indices: indices.to_vec(),
        })
        .unwrap();
}

#[test]
fn a_face_drag_moves_the_face_vertices_and_two_adjacent_faces_their_union() {
    let mut editor = cube_picking(SelectionMode::Face, ComponentKind::Face, &[0]);
    assert_eq!(moved_vertices(&editor, ComponentKind::Face).len(), 4);
    let moved = drag(&mut editor, 1.5);
    assert_eq!(moved.len(), 4);
    assert!(moved.iter().all(|(_, delta)| (delta - 1.5).abs() < 1e-5));
    let scene = editor.history.scene();
    let shape = scene.shapes()[0].id();
    let adjacent = (1..6)
        .find(|&other| scene.vertices_of_faces(shape, &[0, other]).unwrap().len() == 6)
        .expect("a face sharing an edge with face 0");
    pick_set(&mut editor, ComponentKind::Face, &[0, adjacent]);
    let moved = drag(&mut editor, 1.0);
    assert_eq!(moved.len(), 6);
    assert!(
        moved.iter().all(|(_, delta)| (delta - 1.0).abs() < 1e-5),
        "{moved:?}"
    );
}

#[test]
fn an_edge_or_face_drag_is_one_undo_step_and_cancel_restores_the_scene() {
    for (mode, kind) in [
        (SelectionMode::Edge, ComponentKind::Edge),
        (SelectionMode::Face, ComponentKind::Face),
    ] {
        let mut editor = cube_picking(mode, kind, &[1]);
        let before = editor.history.scene().clone();
        let id = begin(&mut editor).unwrap();
        editor
            .apply_operation(Operation::UpdateMove {
                id,
                distance: MoveDistance::Centimeters(4.0),
            })
            .unwrap();
        editor
            .apply_operation(Operation::FinishMove {
                id,
                finish: ManipulationFinish::Cancel,
            })
            .unwrap();
        assert_eq!(editor.history.scene(), &before);
        drag(&mut editor, 4.0);
        editor.apply_operation(Operation::Undo).unwrap();
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn a_selection_reaching_over_the_vertex_cap_refuses_with_text_and_changes_nothing() {
    let mut editor = Editor::default();
    editor.native_operation(Operation::Create {
        primitive: Primitive::Plane,
        options: Some(azteccraft::primitive_options::PrimitiveOptions::Plane {
            width: 1.0,
            height: 1.0,
            sub_width: 90,
            sub_height: 90,
            axis: azteccraft::primitive_options::Axis::Y,
        }),
        name: None,
        create_uvs: false,
    });
    let faces = editor.history.scene().shapes()[0].mesh().faces().len();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    let all: Vec<usize> = (0..faces).collect();
    pick_set(&mut editor, ComponentKind::Face, &all);
    let before = editor.history.scene().clone();
    match begin(&mut editor) {
        Err(OperationError::InvalidInput(message)) => {
            assert_eq!(
                message,
                "Move is limited to 8192 vertices; the selected faces reach more vertices than that"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(!inspected.kinds[0].available);
}

/// An own scale too extreme for the world frame, and a position whose bounding box overflows: begin
/// refuses, so the plan, the handles and inspection must refuse with it, in Edge and Face too.
#[test]
fn ill_conditioned_and_overflowing_frames_agree_across_plan_handles_inspection_and_begin() {
    for (mode, kind) in [
        (SelectionMode::Edge, ComponentKind::Edge),
        (SelectionMode::Face, ComponentKind::Face),
    ] {
        for (translation, scale) in [
            ([None; 3], [Some(1e8), Some(1e-8), Some(1.0)]),
            ([Some(1.7e308), None, None], [None; 3]),
        ] {
            let mut editor = cube_picking(mode, kind, &[0]);
            let node = editor.selected_move_lead().unwrap().node;
            let untouched = editor.history.scene().clone();
            let edited = editor.apply_operation(Operation::EditNode {
                id: node,
                name: None,
                translation,
                rotation_degrees: [None; 3],
                scale,
            });
            if translation[0].is_some() {
                // The world-range guard keeps a position that would overflow the bounding box out
                // of the scene, so that frame cannot arise; pin that refusal here.
                assert!(edited.is_err());
                assert_eq!(editor.history.scene(), &untouched);
                continue;
            }
            edited.unwrap();
            pick_set(&mut editor, kind, &[0]);
            let target = editor.selected_move_lead().unwrap();
            let scene = editor.history.scene().clone();
            let reason = editor
                .component_move_plan(&target)
                .err()
                .expect("the plan refuses")
                .to_string();
            assert!(editor.move_handle_origin(&target).is_err());
            let inspected = editor.inspect_manipulation().unwrap();
            assert!(!inspected.kinds[0].available);
            assert!(
                inspected.kinds[0]
                    .unavailable_reason
                    .as_deref()
                    .unwrap()
                    .contains(&reason)
            );
            assert!(begin(&mut editor).is_err());
            assert!(editor.history.active_manipulation().is_none());
            assert_eq!(editor.history.scene(), &scene);
        }
    }
}

#[test]
fn another_objects_edges_or_faces_are_not_moved() {
    let mut editor = cube_picking(SelectionMode::Face, ComponentKind::Face, &[0]);
    // The set names the first cube while the second is selected (no prune ran).
    let first = owner(&editor);
    editor.create(Primitive::Cube);
    editor.modify_component_set(ComponentKind::Face, |set| set.replace_with(&first, [0]));
    match begin(&mut editor) {
        Err(OperationError::InvalidInput(message)) => {
            assert_eq!(message, "The selected faces belong to another object");
        }
        other => panic!("{other:?}"),
    }
}
