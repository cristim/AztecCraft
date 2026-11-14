//! `Operation::ModifyComponentSelection` and the per-type component sets (#30). Set order and
//! "lead = last inserted" are PROVISIONAL: Maya's `select -add` documents neither; indices are
//! Maya-style `f[n]` positions, so an edit that renumbers faces can make a stored index name
//! another face.
use super::*;
use crate::editor_operations::{ComponentKind, MAX_COMPONENT_INDICES, MarqueeMode};
use azteccraft::selection::ComponentOwner;

fn owner_of(editor: &Editor) -> ComponentOwner {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    ComponentOwner {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
    }
}

fn cube() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor
}

fn mode(editor: &mut Editor, mode: SelectionMode) {
    editor
        .apply_operation(Operation::SetSelectionMode(mode))
        .unwrap();
}

fn modify(
    editor: &mut Editor,
    kind: ComponentKind,
    change: MarqueeMode,
    indices: &[usize],
) -> Result<OperationReply, OperationError> {
    let owner = owner_of(editor);
    editor.apply_operation(Operation::ModifyComponentSelection {
        kind,
        mode: change,
        owner,
        indices: indices.to_vec(),
    })
}

fn changed(reply: Result<OperationReply, OperationError>) -> bool {
    matches!(reply.unwrap(), OperationReply::Changed)
}

fn members(editor: &Editor, kind: ComponentKind) -> Vec<usize> {
    editor.component_set(kind).indices().to_vec()
}

#[test]
fn the_four_modes_order_the_set_and_the_last_member_leads() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Face);
    use ComponentKind::Face;
    assert!(changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Replace,
        &[2, 0, 2, 4]
    )));
    assert_eq!(members(&editor, Face), [2, 0, 4]);
    assert_eq!(editor.remembered_face().unwrap().face_index, 4);
    assert!(changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Add,
        &[0, 5, 1]
    )));
    assert_eq!(members(&editor, Face), [2, 0, 4, 5, 1]);
    assert!(changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Toggle,
        &[0, 3]
    )));
    assert_eq!(members(&editor, Face), [2, 4, 5, 1, 3]);
    assert!(changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Remove,
        &[3, 5]
    )));
    assert_eq!(members(&editor, Face), [2, 4, 1]);
    assert_eq!(editor.remembered_face().unwrap().face_index, 1);
    assert!(!changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Add,
        &[2, 4]
    )));
    assert!(!changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Replace,
        &[2, 4, 1]
    )));
    assert!(changed(modify(
        &mut editor,
        Face,
        MarqueeMode::Replace,
        &[]
    )));
    assert!(editor.remembered_face().is_none());
}

#[test]
fn vertex_and_edge_sets_work_the_same_way() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Vertex);
    modify(
        &mut editor,
        ComponentKind::Vertex,
        MarqueeMode::Add,
        &[7, 1],
    )
    .unwrap();
    assert_eq!(members(&editor, ComponentKind::Vertex), [7, 1]);
    assert_eq!(editor.remembered_vertex().unwrap().vertex_index, 1);
    mode(&mut editor, SelectionMode::Edge);
    modify(
        &mut editor,
        ComponentKind::Edge,
        MarqueeMode::Add,
        &[11, 3, 0],
    )
    .unwrap();
    assert_eq!(members(&editor, ComponentKind::Edge), [11, 3, 0]);
    assert_eq!(editor.remembered_edge().unwrap().edge_index, 0);
}

#[test]
fn a_refused_request_changes_nothing() {
    let mut editor = cube();
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Add, &[0]).unwrap_err();
    mode(&mut editor, SelectionMode::Face);
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Add, &[1, 2]).unwrap();
    let scene = editor.history.scene().clone();
    // Wrong mode, a missing index among valid ones, a cap overflow and a foreign owner.
    let wrong_mode = modify(&mut editor, ComponentKind::Vertex, MarqueeMode::Add, &[0]);
    assert!(matches!(wrong_mode, Err(OperationError::InvalidInput(_))));
    let missing = modify(
        &mut editor,
        ComponentKind::Face,
        MarqueeMode::Replace,
        &[0, 6],
    );
    assert!(matches!(missing, Err(OperationError::Scene(_))));
    let too_many = vec![0; MAX_COMPONENT_INDICES + 1];
    let owner = owner_of(&editor);
    let flood = editor.apply_operation(Operation::ModifyComponentSelection {
        kind: ComponentKind::Face,
        mode: MarqueeMode::Add,
        owner,
        indices: too_many,
    });
    assert!(matches!(flood, Err(OperationError::InvalidInput(_))));
    assert_eq!(members(&editor, ComponentKind::Face), [1, 2]);
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn a_set_of_another_object_is_refused_without_switching_the_selection() {
    let mut editor = cube();
    let first = editor.selection_presentation().0;
    editor.create(Primitive::Cube);
    let second_owner = owner_of(&editor);
    let second = editor.selection_presentation().0;
    editor
        .apply_operation(Operation::Select {
            node: first,
            instance_path: None,
        })
        .unwrap();
    mode(&mut editor, SelectionMode::Face);
    let refused = editor.apply_operation(Operation::ModifyComponentSelection {
        kind: ComponentKind::Face,
        mode: MarqueeMode::Add,
        owner: ComponentOwner {
            transform: second.unwrap(),
            ..second_owner
        },
        indices: vec![0],
    });
    assert!(matches!(refused, Err(OperationError::InvalidInput(_))));
    assert_eq!(editor.selection_presentation().0, first);
    assert!(editor.component_set(ComponentKind::Face).is_empty());
}

#[test]
fn several_selected_objects_refuse_a_component_set() {
    let mut editor = cube();
    editor.create(Primitive::Cube);
    mode(&mut editor, SelectionMode::Face);
    let owner = owner_of(&editor);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
        .unwrap();
    editor
        .apply_operation(Operation::SelectByRule(
            crate::editor_operations::SelectRule::All,
        ))
        .unwrap();
    // Component modes cannot be entered with several objects; the mode check refuses first.
    let refused = editor.apply_operation(Operation::ModifyComponentSelection {
        kind: ComponentKind::Face,
        mode: MarqueeMode::Add,
        owner,
        indices: vec![0],
    });
    assert!(refused.is_err());
}

#[test]
fn each_type_keeps_its_own_set_across_mode_switches_and_object_mode() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Face);
    modify(
        &mut editor,
        ComponentKind::Face,
        MarqueeMode::Add,
        &[3, 1, 2],
    )
    .unwrap();
    mode(&mut editor, SelectionMode::Vertex);
    modify(
        &mut editor,
        ComponentKind::Vertex,
        MarqueeMode::Add,
        &[5, 4],
    )
    .unwrap();
    assert_eq!(members(&editor, ComponentKind::Face), [3, 1, 2]);
    mode(&mut editor, SelectionMode::Face);
    assert_eq!(members(&editor, ComponentKind::Face), [3, 1, 2]);
    assert_eq!(members(&editor, ComponentKind::Vertex), [5, 4]);
    mode(&mut editor, SelectionMode::Object);
    mode(&mut editor, SelectionMode::Face);
    assert_eq!(members(&editor, ComponentKind::Face), [3, 1, 2]);
    // A single pick replaces the whole face set and leaves the other types alone.
    let lead = editor.remembered_face().unwrap();
    let shrunk = editor.apply_operation(Operation::SelectFace { target: Some(lead) });
    assert!(changed(shrunk), "shrinking a set to its lead is a change");
    assert_eq!(members(&editor, ComponentKind::Face), [2]);
    assert_eq!(members(&editor, ComponentKind::Vertex), [5, 4]);
}

#[test]
fn selecting_another_object_or_deselecting_clears_every_set() {
    for clearing in [
        Operation::DeselectAll,
        Operation::Select {
            node: None,
            instance_path: None,
        },
    ] {
        let mut editor = cube();
        mode(&mut editor, SelectionMode::Face);
        modify(&mut editor, ComponentKind::Face, MarqueeMode::Add, &[0, 1]).unwrap();
        mode(&mut editor, SelectionMode::Vertex);
        modify(
            &mut editor,
            ComponentKind::Vertex,
            MarqueeMode::Add,
            &[0, 1],
        )
        .unwrap();
        editor.apply_operation(clearing).unwrap();
        for kind in [
            ComponentKind::Face,
            ComponentKind::Vertex,
            ComponentKind::Edge,
        ] {
            assert!(editor.component_set(kind).is_empty());
        }
    }
}

#[test]
fn component_changes_do_not_move_the_object_selection_generation_or_the_scene() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Face);
    let generation = editor.selection.generation();
    let scene = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    modify(
        &mut editor,
        ComponentKind::Face,
        MarqueeMode::Add,
        &[0, 1, 2],
    )
    .unwrap();
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Toggle, &[1]).unwrap();
    assert_eq!(editor.selection.generation(), generation);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undo);
}

#[test]
fn undo_drops_only_the_members_that_no_longer_exist() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Face);
    let cap = editor.remembered_face().unwrap_or_else(|| {
        let owner = owner_of(&editor);
        azteccraft::native_scene::FaceTarget {
            transform: owner.transform,
            shape: owner.shape,
            path: owner.path,
            face_index: 0,
        }
    });
    editor
        .apply_operation(Operation::ExtrudeFace {
            target: cap,
            distance: 0.25,
        })
        .unwrap();
    modify(
        &mut editor,
        ComponentKind::Face,
        MarqueeMode::Add,
        &[0, 9, 2, 6],
    )
    .unwrap();
    assert_eq!(members(&editor, ComponentKind::Face), [0, 9, 2, 6]);
    editor.apply_operation(Operation::Undo).unwrap();
    assert_eq!(members(&editor, ComponentKind::Face), [0, 2]);
    assert_eq!(editor.remembered_face().unwrap().face_index, 2);
}

#[test]
fn inspection_lists_each_set_with_its_owner_once_and_the_lead() {
    let mut editor = cube();
    assert!(
        editor
            .inspect(false)
            .unwrap()
            .component_selection
            .face
            .is_none()
    );
    mode(&mut editor, SelectionMode::Face);
    modify(
        &mut editor,
        ComponentKind::Face,
        MarqueeMode::Add,
        &[4, 1, 3],
    )
    .unwrap();
    mode(&mut editor, SelectionMode::Vertex);
    modify(&mut editor, ComponentKind::Vertex, MarqueeMode::Add, &[6]).unwrap();
    let inspected = editor.inspect(false).unwrap();
    let face = inspected.component_selection.face.as_ref().unwrap();
    assert_eq!(face.indices, [4, 1, 3]);
    assert_eq!((face.count, face.lead, face.truncated), (3, 3, false));
    let owner = owner_of(&editor);
    assert_eq!(face.node_id, owner.transform.to_decimal());
    assert_eq!(face.shape_id, owner.shape.to_decimal());
    assert_eq!(face.instance_path.len(), owner.path.len());
    assert_eq!(inspected.component_selection.vertex.unwrap().indices, [6]);
    assert!(inspected.component_selection.edge.is_none());
    // The lead stays readable through the single-pick field.
    assert_eq!(inspected.remembered_face.unwrap().face_index, 3);
}

#[test]
fn a_huge_set_inspects_inside_the_result_limit_and_says_it_is_truncated() {
    use crate::editor_operations::{MAX_LISTED_COMPONENTS, MAX_RESULT_BYTES};
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::Create {
            primitive: Primitive::Plane,
            options: Some(azteccraft::primitive_options::PrimitiveOptions::Plane {
                width: 1.0,
                height: 1.0,
                sub_width: 80,
                sub_height: 80,
                axis: azteccraft::primitive_options::Axis::Y,
            }),
            name: None,
        })
        .unwrap();
    mode(&mut editor, SelectionMode::Face);
    let all: Vec<usize> = (0..6400).collect();
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Replace, &all).unwrap();
    mode(&mut editor, SelectionMode::Vertex);
    let vertices: Vec<usize> = (0..6561).collect();
    modify(
        &mut editor,
        ComponentKind::Vertex,
        MarqueeMode::Replace,
        &vertices,
    )
    .unwrap();
    mode(&mut editor, SelectionMode::Edge);
    let edges: Vec<usize> = (0..12_960).collect();
    modify(
        &mut editor,
        ComponentKind::Edge,
        MarqueeMode::Replace,
        &edges,
    )
    .unwrap();
    let inspected = editor.inspect(false).unwrap();
    let face = inspected.component_selection.face.as_ref().unwrap();
    assert_eq!(face.count, 6400);
    assert_eq!(face.lead, 6399);
    assert_eq!(face.indices.len(), MAX_LISTED_COMPONENTS);
    assert!(face.truncated);
    assert!(serde_json::to_string(&inspected).unwrap().len() < MAX_RESULT_BYTES);
}

fn frame_selected(editor: &mut Editor) -> OperationReply {
    editor
        .apply_operation(Operation::FrameSelected {
            with_children: false,
            fit_factor: None,
        })
        .unwrap()
}

#[test]
fn frame_selected_frames_every_vertex_and_face_of_the_set_not_only_the_lead() {
    let mut editor = cube();
    editor.channels[0] = "5".into();
    editor.commit_pending();
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    let world = instance.world_vertices();
    mode(&mut editor, SelectionMode::Vertex);
    modify(
        &mut editor,
        ComponentKind::Vertex,
        MarqueeMode::Add,
        &[0, 6, 3],
    )
    .unwrap();
    let mut expected = editor.camera.clone();
    expected
        .frame_points(&[world[0], world[6], world[3]], editor.aspect)
        .unwrap();
    assert!(matches!(
        frame_selected(&mut editor),
        OperationReply::Changed
    ));
    assert_eq!(editor.camera, expected);
    assert!(matches!(
        frame_selected(&mut editor),
        OperationReply::Unchanged
    ));
    let mut lead_only = Editor::default().camera.clone();
    lead_only.frame_points(&[world[3]], editor.aspect).unwrap();
    assert_ne!(editor.camera, lead_only);

    mode(&mut editor, SelectionMode::Face);
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Add, &[1, 4]).unwrap();
    let corners: Vec<_> = instance
        .faces()
        .iter()
        .filter(|face| [1, 4].contains(&face.source_index()))
        .flat_map(|face| face.corners().iter().map(|&index| world[index]))
        .collect();
    let mut expected = editor.camera.clone();
    expected.frame_points(&corners, editor.aspect).unwrap();
    frame_selected(&mut editor);
    assert_eq!(editor.camera, expected);
}

#[test]
fn native_extrude_refuses_a_set_but_the_explicit_operation_extrudes_its_target_and_keeps_the_set() {
    let mut editor = cube();
    mode(&mut editor, SelectionMode::Face);
    modify(&mut editor, ComponentKind::Face, MarqueeMode::Add, &[2, 5]).unwrap();
    let scene = editor.history.scene().clone();
    assert_eq!(
        editor.extrude_blocker(),
        Some(crate::app::features::extrude_options::ExtrudeBlocker::SeveralFacesSelected)
    );
    let refusal = editor.extrude_refusal().unwrap();
    assert!(refusal.contains("one face"), "{refusal}");
    assert!(editor.extrude_selected_face(0.5).is_err());
    assert_eq!(editor.history.scene(), &scene);

    // MCP `extrude_face` names its own target, so the set does not matter.
    let owner = owner_of(&editor);
    editor
        .apply_operation(Operation::ExtrudeFace {
            target: azteccraft::native_scene::FaceTarget {
                transform: owner.transform,
                shape: owner.shape,
                path: owner.path,
                face_index: 0,
            },
            distance: 0.25,
        })
        .unwrap();
    assert_ne!(editor.history.scene(), &scene);
    // PROVISIONAL, as the single pick: the indices stay, they are not moved to the new faces.
    assert_eq!(members(&editor, ComponentKind::Face), [2, 5]);
}
