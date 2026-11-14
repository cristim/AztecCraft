use super::*;
use crate::scene::Primitive;

const PARENT: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    preserve_position: true,
};

fn cube(scene: &mut NativeScene, name: &str) -> NodeId {
    scene.create(Primitive::Cube, name).unwrap()
}

fn set(scene: &mut NativeScene, id: NodeId, t: [f64; 3], r: [f64; 3], s: [f64; 3]) {
    scene
        .set_transform(
            id,
            NativeTransform {
                translation: t,
                rotation: r,
                scale: s,
            },
        )
        .unwrap();
}

fn parent_of(scene: &NativeScene, id: NodeId) -> Option<NodeId> {
    let transform = scene.node(id).unwrap().transform_id();
    let parent = scene
        .dag
        .iter()
        .find(|edge| edge.child == transform)
        .unwrap()
        .parent?;
    scene
        .nodes
        .iter()
        .find(|node| node.transform_id == parent)
        .map(|node| node.id)
}

fn children_of(scene: &NativeScene, id: Option<NodeId>) -> Vec<NodeId> {
    let parent = id.map(|id| scene.node(id).unwrap().transform_id());
    scene
        .dag
        .iter()
        .filter(|edge| edge.parent == parent)
        .filter_map(|edge| scene.nodes.iter().find(|n| n.transform_id == edge.child))
        .map(|node| node.id)
        .collect()
}

fn bits(scene: &NativeScene, id: NodeId) -> [u64; 9] {
    scene
        .node(id)
        .unwrap()
        .channels
        .clone()
        .map(|c| c.value.to_bits())
}

/// Hand-written translate * Rz(degrees) * scale applied to a point, with no matrix library.
fn image(t: [f64; 3], z_degrees: f64, s: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    let (sin, cos) = z_degrees.to_radians().sin_cos();
    let (x, y, z) = (p[0] * s[0], p[1] * s[1], p[2] * s[2]);
    [t[0] + cos * x - sin * y, t[1] + sin * x + cos * y, t[2] + z]
}

fn close(a: [f64; 3], b: [f64; 3]) {
    for axis in 0..3 {
        assert!((a[axis] - b[axis]).abs() < 1e-9, "{a:?} != {b:?}");
    }
}

fn local(scene: &NativeScene, id: NodeId) -> NativeTransform {
    scene.node(id).unwrap().transform()
}

/// A rotated, translated, uniformly scaled parent with two plain children.
fn under_a_parent() -> (NativeScene, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let parent = cube(&mut scene, "parentCube");
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    set(
        &mut scene,
        parent,
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0; 3],
    );
    set(
        &mut scene,
        a,
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 30.0],
        [1.0, 3.0, 1.0],
    );
    set(&mut scene, b, [0.0, 2.0, 0.0], [0.0; 3], [1.0; 3]);
    scene
        .reparent(a, Some(parent), ReparentKeep::Local)
        .unwrap();
    scene
        .reparent(b, Some(parent), ReparentKeep::Local)
        .unwrap();
    (scene, parent, a, b)
}

#[test]
fn group_under_parent_adds_a_group_beside_the_members_and_leaves_their_channels() {
    let (mut scene, parent, a, b) = under_a_parent();
    let (a_bits, b_bits) = (bits(&scene, a), bits(&scene, b));
    let group = scene.group(&[a, b], PARENT).unwrap();
    assert_eq!(scene.node(group).unwrap().name(), "group1");
    assert_eq!(parent_of(&scene, group), Some(parent));
    assert_eq!(children_of(&scene, Some(group)), vec![a, b]);
    assert_eq!(children_of(&scene, Some(parent)), vec![group]);
    assert_eq!(local(&scene, group), NativeTransform::default());
    // The group frame equals the old parent frame, so nothing is rewritten.
    assert_eq!((bits(&scene, a), bits(&scene, b)), (a_bits, b_bits));
}

#[test]
fn group_under_the_world_keeps_each_world_point_by_a_closed_form_oracle() {
    let (mut scene, _, a, b) = under_a_parent();
    let options = GroupOptions {
        under: GroupUnder::World,
        preserve_position: true,
    };
    let group = scene.group(&[a, b], options).unwrap();
    assert_eq!(parent_of(&scene, group), None);
    assert_eq!(children_of(&scene, Some(group)), vec![a, b]);
    let p = [0.5, 0.25, 0.75];
    // a: parent T(10,0,0) Rz(90) S(2) after a's T(1,0,0) Rz(30) S(1,3,1), as the oracle composes.
    let before = image(
        [10.0, 0.0, 0.0],
        90.0,
        [2.0; 3],
        image([1.0, 0.0, 0.0], 30.0, [1.0, 3.0, 1.0], p),
    );
    let new = local(&scene, a);
    close(
        before,
        image(new.translation, new.rotation[2], new.scale, p),
    );
    let before = image(
        [10.0, 0.0, 0.0],
        90.0,
        [2.0; 3],
        image([0.0, 2.0, 0.0], 0.0, [1.0; 3], p),
    );
    let new = local(&scene, b);
    close(
        before,
        image(new.translation, new.rotation[2], new.scale, p),
    );
}

#[test]
fn preserve_position_off_keeps_the_local_channels_and_the_members_shift() {
    let (mut scene, _, a, b) = under_a_parent();
    let before = (bits(&scene, a), bits(&scene, b));
    let options = GroupOptions {
        under: GroupUnder::World,
        preserve_position: false,
    };
    scene.group(&[a, b], options).unwrap();
    assert_eq!(before, (bits(&scene, a), bits(&scene, b)));
}

#[test]
fn one_object_gets_a_group_directly_above_it_and_other_hierarchies_go_to_the_world() {
    let (mut scene, parent, a, _) = under_a_parent();
    let alone = scene.group(&[a], PARENT).unwrap();
    assert_eq!(parent_of(&scene, alone), Some(parent));
    assert_eq!(parent_of(&scene, a), Some(alone));
    let root = cube(&mut scene, "root");
    let root_group = scene.group(&[root], PARENT).unwrap();
    assert_eq!(parent_of(&scene, root_group), None);
    assert_eq!(parent_of(&scene, root), Some(root_group));

    let (mut scene, parent, a, _) = under_a_parent();
    let other = cube(&mut scene, "other");
    let outsider = cube(&mut scene, "outsider");
    scene
        .reparent(outsider, Some(other), ReparentKeep::Local)
        .unwrap();
    let group = scene.group(&[a, outsider], PARENT).unwrap();
    assert_eq!(parent_of(&scene, group), None);
    assert_ne!(parent_of(&scene, group), Some(parent));
}

#[test]
fn names_take_the_lowest_free_number_even_when_a_shape_holds_the_name() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    scene.shapes[0].name = "group1".into();
    let first = scene.group(&[a], PARENT).unwrap();
    assert_eq!(scene.node(first).unwrap().name(), "group2");
    let second = scene.group(&[b], PARENT).unwrap();
    assert_eq!(scene.node(second).unwrap().name(), "group3");
}
