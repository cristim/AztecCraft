use super::*;

pub(crate) fn share_curve_with_transform(scene: &mut NativeScene, transform: NodeId) {
    scene.dag.push(DagEdge {
        parent: Some(scene.node(transform).unwrap().transform_id),
        child: scene.curves[0].id,
    });
    scene.validate().unwrap();
}

pub(crate) fn unprove_curve_ancestor(scene: &mut NativeScene, transform: NodeId) {
    scene
        .nodes
        .iter_mut()
        .find(|node| node.id == transform)
        .unwrap()
        .view
        .geometry = None;
    scene.validate().unwrap();
}

pub(crate) fn cube() -> (NativeScene, NodeId) {
    let mut scene = NativeScene::default();
    let id = scene.create(Primitive::Cube, "pCube1").unwrap();
    scene.validate().unwrap();
    (scene, id)
}

/// One transform with two identical direct shapes; the second has the larger path.
pub(crate) fn twin_shapes() -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    let mut twin = scene.shapes[0].clone();
    twin.id = NativeId(scene.next_native_id);
    twin.uuid = None;
    scene.next_native_id += 1;
    scene.dag.push(DagEdge {
        parent: Some(scene.node(id).unwrap().transform_id),
        child: twin.id,
    });
    scene.shapes.push(twin);
    scene.validate().unwrap();
    (scene, id)
}

pub(crate) fn shared_shape() -> (NativeScene, NodeId, NodeId, NativeId) {
    let (mut scene, first) = cube();
    let second = scene.create(Primitive::Plane, "instance").unwrap();
    let original = scene.shapes[0].id;
    let discarded = scene.shapes.pop().unwrap().id;
    scene.dag.retain(|edge| edge.child != discarded);
    scene.dag.push(DagEdge {
        parent: Some(scene.node(second).unwrap().transform_id),
        child: original,
    });
    scene.validate().unwrap();
    (scene, first, second, original)
}

pub(crate) fn grouped_shape() -> (NativeScene, NodeId, NodeId, NativeId) {
    let (mut scene, root) = cube();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    let discarded = scene.shapes.remove(0).id;
    let root_native = scene.node(root).unwrap().transform_id;
    let child_native = scene.node(child).unwrap().transform_id;
    scene.dag.retain(|edge| {
        edge.child != discarded && !(edge.child == child_native && edge.parent.is_none())
    });
    scene.dag.push(DagEdge {
        parent: Some(root_native),
        child: child_native,
    });
    scene.validate().unwrap();
    let shape = scene.shapes[0].id;
    (scene, root, child, shape)
}

/// A parent cube with its own shape and a child cube parented under it, 100 units along x.
pub(crate) fn group_with_own_shape_and_child() -> (NativeScene, NodeId, NodeId) {
    let (mut scene, parent) = cube();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    let parent_native = scene.node(parent).unwrap().transform_id;
    let child_native = scene.node(child).unwrap().transform_id;
    scene
        .dag
        .retain(|edge| !(edge.child == child_native && edge.parent.is_none()));
    scene.dag.push(DagEdge {
        parent: Some(parent_native),
        child: child_native,
    });
    scene
        .set_transform(
            child,
            NativeTransform {
                translation: [100.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    scene.validate().unwrap();
    (scene, parent, child)
}

pub(crate) fn hide_first_shape(scene: &mut NativeScene) {
    scene.shapes[0].view.local_visibility = Some(false);
    scene.validate().unwrap();
}

/// root > mid > leaf, with the only shape under leaf.
pub(crate) fn chain_of_three() -> (NativeScene, NodeId, NodeId, NodeId) {
    let (mut scene, root, mid, _) = grouped_shape();
    let leaf = scene.create(Primitive::Cube, "leaf").unwrap();
    let discarded = scene.shapes.remove(0).id;
    let mid_native = scene.node(mid).unwrap().transform_id;
    let leaf_native = scene.node(leaf).unwrap().transform_id;
    scene.dag.retain(|edge| {
        edge.child != discarded && !(edge.child == leaf_native && edge.parent.is_none())
    });
    scene.dag.push(DagEdge {
        parent: Some(mid_native),
        child: leaf_native,
    });
    scene.validate().unwrap();
    (scene, root, mid, leaf)
}

pub(crate) fn pivoted_shape() -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.nodes[0].channels = NativeTransform {
        translation: [3.0, -2.0, 5.0],
        rotation: [20.0, 35.0, -15.0],
        scale: [2.0, 0.5, -1.0],
    }
    .values()
    .map(Scalar::new);
    scene.nodes[0].rotate_pivot = [1.0, 0.25, -2.0].map(Scalar::new);
    scene.nodes[0].scale_pivot = [-0.5, 1.0, 0.0].map(Scalar::new);
    scene.validate().unwrap();
    (scene, id)
}

pub(crate) fn triangle_double_shape() -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.shapes[0].mesh = NativeMesh::triangle_double_fixture();
    scene.nodes[0].channels[0] = Scalar::new(-16777216.0);
    scene.validate().unwrap();
    (scene, id)
}

pub(crate) fn polygon_shape(points: Vec<[f64; 3]>, loops: &[Vec<usize>]) -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.shapes[0].mesh = NativeMesh::polygon_fixture(points, loops);
    scene.validate().unwrap();
    (scene, id)
}

/// Skips `validate` so a test can feed the view builder an edge it must refuse.
pub(crate) fn add_loose_edge_unvalidated(scene: &mut NativeScene, endpoints: [usize; 2]) {
    scene.shapes[0].mesh.add_loose_edge_fixture(endpoints);
}

pub(crate) fn add_hole_to_first_face(scene: &mut NativeScene, hole: &[usize]) {
    scene.shapes[0].mesh.add_hole_fixture(hole);
    scene.validate().unwrap();
}

pub(crate) fn offset_triangle_shape() -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.shapes[0].mesh = NativeMesh::offset_triangle_fixture();
    scene.nodes[0].channels[0] = Scalar::new(-16777216.0);
    scene.validate().unwrap();
    (scene, id)
}

#[test]
fn triangle_native_face_target_does_not_require_a_quad_view() {
    let (scene, transform) = triangle_double_shape();
    let shape = scene.shapes[0].id;
    let mut target = FaceTarget {
        transform,
        shape,
        path: vec![scene.node(transform).unwrap().transform_id(), shape],
        face_index: 0,
    };
    assert_eq!(
        scene.resolve_face_target(&target).unwrap().mesh().faces()[0]
            .outer()
            .len(),
        3
    );
    assert!(
        scene
            .geometry_view_admission(&target.path)
            .unwrap()
            .visible()
    );
    target.face_index = 1;
    assert_eq!(
        scene.resolve_face_target(&target),
        Err(SceneError::MissingFace(1).into())
    );
}

pub(crate) fn hidden_group() -> (NativeScene, NodeId, NodeId, NativeId) {
    let (mut scene, root, child, shape) = grouped_shape();
    scene.nodes[0].view.local_visibility = Some(false);
    scene.validate().unwrap();
    (scene, root, child, shape)
}

pub(crate) fn hidden_shape() -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.shapes[0].view.local_visibility = Some(false);
    scene.validate().unwrap();
    (scene, id)
}

pub(crate) fn unproved_group() -> (NativeScene, NodeId, NodeId, NativeId) {
    let (mut scene, root, child, shape) = grouped_shape();
    scene.nodes[0].view.geometry = None;
    scene.validate().unwrap();
    (scene, root, child, shape)
}

/// root -> middle -> leaf, with the given local transforms; returns the leaf node.
pub(crate) fn three_level_chain(
    root: NativeTransform,
    middle: NativeTransform,
    leaf: NativeTransform,
) -> (NativeScene, NodeId) {
    let (mut scene, root_id, middle_id, _) = grouped_shape();
    let leaf_id = scene.create(Primitive::Cube, "leaf").unwrap();
    let discarded = scene.shapes.pop().unwrap().id;
    let leaf_native = scene.node(leaf_id).unwrap().transform_id;
    let middle_native = scene.node(middle_id).unwrap().transform_id;
    scene.dag.retain(|edge| {
        edge.child != discarded && !(edge.child == leaf_native && edge.parent.is_none())
    });
    scene.dag.push(DagEdge {
        parent: Some(middle_native),
        child: leaf_native,
    });
    scene.validate().unwrap();
    scene.set_transform(root_id, root).unwrap();
    scene.set_transform(middle_id, middle).unwrap();
    scene.set_transform(leaf_id, leaf).unwrap();
    (scene, leaf_id)
}

pub(crate) fn set_extras(scene: &mut NativeScene, node: NodeId, extras: TransformExtras) {
    scene
        .nodes
        .iter_mut()
        .find(|entry| entry.id == node)
        .unwrap()
        .extras = extras;
}

/// Two root transforms: a cube and one with no shape at all.
pub(crate) fn cube_and_empty_transform() -> (NativeScene, NodeId, NodeId) {
    let (mut scene, cube) = cube();
    let empty = scene.create(Primitive::Cube, "empty").unwrap();
    let discarded = scene.shapes.pop().unwrap().id;
    scene.dag.retain(|edge| edge.child != discarded);
    scene.validate().unwrap();
    (scene, cube, empty)
}
