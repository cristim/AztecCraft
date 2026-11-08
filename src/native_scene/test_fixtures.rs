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

/// `pivoted_shape` plus rotatePivotTranslate, scalePivotTranslate and shear. Only test
/// constructors can build it: a `.mb` carrying these keys fails to import until #105.
pub(crate) fn pivoted_extras_shape() -> (NativeScene, NodeId) {
    let (mut scene, id) = pivoted_shape();
    give_pivot_extras(&mut scene, id);
    (scene, id)
}

pub(crate) fn give_pivot_extras(scene: &mut NativeScene, node: NodeId) {
    set_extras(
        scene,
        node,
        TransformExtras {
            rotate_pivot_translate: Some([0.5, -1.5, 2.0]),
            scale_pivot_translate: Some([-2.0, 0.75, 1.0]),
            shear: Some([0.1, -0.2, 0.3]),
            ..TransformExtras::default()
        },
    );
    scene.validate().unwrap();
}

/// Makes `node` refuse every edit (`UnsupportedEdit`), whatever its pivots are.
pub(crate) fn restrict_edits(scene: &mut NativeScene, node: NodeId) {
    scene
        .nodes
        .iter_mut()
        .find(|entry| entry.id == node)
        .unwrap()
        .restrictions
        .push("driver".into());
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

/// Roots x1 and x2 both parent transform `a` (instanced), and `b` is a child of `a`. Returns
/// (scene, x1, x2, a, b); `b` is reachable as x1/a/b and x2/a/b and moves with `a`.
pub(crate) fn instanced_chain() -> (NativeScene, NodeId, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let [x1, x2, a, b] =
        ["x1", "x2", "a", "b"].map(|name| scene.create(Primitive::Cube, name).unwrap());
    let native = |scene: &NativeScene, id| scene.node(id).unwrap().transform_id;
    let (x1_native, x2_native) = (native(&scene, x1), native(&scene, x2));
    let (a_native, b_native) = (native(&scene, a), native(&scene, b));
    scene.dag.retain(|edge| {
        !(edge.parent.is_none() && (edge.child == a_native || edge.child == b_native))
    });
    for (parent, child) in [
        (x1_native, a_native),
        (x2_native, a_native),
        (a_native, b_native),
    ] {
        scene.dag.push(DagEdge {
            parent: Some(parent),
            child,
        });
    }
    scene.validate().unwrap();
    (scene, x1, x2, a, b)
}

/// `count` shapeless root transforms named `root0`..; built without per-node validation so a
/// 4096-member selection is cheap to set up.
pub(crate) fn many_roots(count: u64) -> NativeScene {
    let (mut scene, _) = cube();
    let template = scene.nodes[0].clone();
    scene.nodes.clear();
    scene.shapes.clear();
    scene.dag.clear();
    for index in 0..count {
        let mut node = template.clone();
        node.id = NodeId::from_decimal(&(index + 1).to_string()).unwrap();
        node.transform_id = NativeId(index + 1);
        node.name = format!("root{index}");
        scene.dag.push(DagEdge {
            parent: None,
            child: node.transform_id,
        });
        scene.nodes.push(node);
    }
    scene.next_id = count + 1;
    scene.next_native_id = count + 2;
    scene.validate().unwrap();
    scene
}

/// Two cubes in a binary source: the first has its translate leaf, the second has none and cannot
/// gain one (a leaf binds all three components, so a node lacks all of them or none). Returns the
/// scene, the complete node and the leafless node.
pub(crate) fn binary_cube_and_cube_without_leaf() -> (NativeScene, NodeId, NodeId) {
    use source::{SourceLeaf, Storage};
    let mut scene = NativeScene::default();
    let full = scene.create(Primitive::Cube, "full").unwrap();
    let bare = scene.create(Primitive::Cube, "bare").unwrap();
    let owner = scene.node(full).unwrap().transform_id;
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: vec![0u8; 24].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..24).collect(),
        leaves: vec![SourceLeaf {
            payload: 0..24,
            numeric: 0..24,
            owner,
            field: TransformField::Translation,
            storage: Storage::Centimeters,
        }],
        appearance: None,
        has_removals: false,
    });
    let index = scene.nodes.iter().position(|n| n.id == full).unwrap();
    for component in 0..3 {
        scene.nodes[index].channels[component].origin =
            Some(ScalarOrigin::Binary { leaf: 0, component });
        scene.nodes[index].channels[component].replaced = false;
    }
    scene.validate().unwrap();
    (scene, full, bare)
}

/// Marks a transform's effect as unproved, so deleting it or an ancestor is refused.
pub(crate) fn restrict_transform(scene: &mut NativeScene, id: NodeId) {
    let at = scene.nodes.iter().position(|node| node.id == id).unwrap();
    scene.nodes[at]
        .unsupported_transform
        .push("unproved effect".into());
}
