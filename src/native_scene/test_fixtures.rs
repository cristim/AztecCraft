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
