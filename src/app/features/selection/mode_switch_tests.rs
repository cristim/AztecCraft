//! Maya's Preserve Component Selections (on by default, 2027 Selection preferences page) backs up
//! the component selection on a type switch and restores the type switched to (#29). That covers
//! component-to-component switches only; keeping picks across Object and Hierarchy is PROVISIONAL.
use super::*;
use azteccraft::native_scene::{EdgeTarget, FaceTarget, VertexTarget};

fn target_parts(editor: &Editor) -> (NodeId, NativeId, Vec<NativeId>) {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    (
        instance.transform(),
        instance.shape(),
        instance.path().to_vec(),
    )
}

fn switch(editor: &mut Editor, mode: SelectionMode) {
    editor
        .apply_operation(Operation::SetSelectionMode(mode))
        .unwrap();
}

#[test]
fn every_component_type_pick_survives_switches_between_types_and_through_object_mode() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let (transform, shape, path) = target_parts(&editor);
    let face = FaceTarget {
        transform,
        shape,
        path: path.clone(),
        face_index: 1,
    };
    let vertex = VertexTarget {
        transform,
        shape,
        path: path.clone(),
        vertex_index: 3,
    };
    let edge = EdgeTarget {
        transform,
        shape,
        path,
        edge_index: 5,
    };
    switch(&mut editor, SelectionMode::Vertex);
    editor
        .apply_operation(Operation::SelectVertex {
            target: Some(vertex.clone()),
        })
        .unwrap();
    switch(&mut editor, SelectionMode::Edge);
    editor
        .apply_operation(Operation::SelectEdge {
            target: Some(edge.clone()),
        })
        .unwrap();
    switch(&mut editor, SelectionMode::Face);
    editor
        .apply_operation(Operation::SelectFace {
            target: Some(face.clone()),
        })
        .unwrap();
    let all = |editor: &Editor| {
        (
            editor.remembered_face.clone(),
            editor.remembered_vertex.clone(),
            editor.remembered_edge.clone(),
        )
    };
    let expected = (Some(face), Some(vertex), Some(edge));
    for route in [
        vec![
            SelectionMode::Vertex,
            SelectionMode::Edge,
            SelectionMode::Vertex,
        ],
        vec![
            SelectionMode::Object,
            SelectionMode::Edge,
            SelectionMode::Object,
        ],
        vec![
            SelectionMode::Hierarchy,
            SelectionMode::Face,
            SelectionMode::Vertex,
        ],
    ] {
        for mode in route {
            switch(&mut editor, mode);
            assert_eq!(all(&editor), expected, "after {mode:?}");
        }
    }
    switch(&mut editor, SelectionMode::Vertex);
    assert_eq!(editor.selection_mode, SelectionMode::Vertex);
}
