use super::*;
use crate::native_scene::{ALL_CHANNELS, MAX_TRANSFORM_DEPTH, test_fixtures};
use crate::scene::Primitive;

const PARENT: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Origin,
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
        pivot: GroupPivot::Origin,
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
        pivot: GroupPivot::Origin,
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

#[test]
fn a_mixed_depth_selection_goes_under_the_lowest_common_parent_and_keeps_world_points() {
    let mut scene = NativeScene::default();
    let top = cube(&mut scene, "top");
    let mid = cube(&mut scene, "mid");
    let deep = cube(&mut scene, "deep");
    let shallow = cube(&mut scene, "shallow");
    set(
        &mut scene,
        top,
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0; 3],
    );
    set(&mut scene, mid, [0.0, 5.0, 0.0], [0.0; 3], [1.0; 3]);
    set(
        &mut scene,
        deep,
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 30.0],
        [1.0, 3.0, 1.0],
    );
    for (child, parent) in [(mid, top), (deep, mid), (shallow, top)] {
        scene
            .reparent(child, Some(parent), ReparentKeep::Local)
            .unwrap();
    }
    let group = scene.group(&[deep, shallow], PARENT).unwrap();
    assert_eq!(parent_of(&scene, group), Some(top));
    let p = [0.5, 0.25, 0.75];
    // deep's old frame is top * mid (a translation apart), so its translation is rewritten.
    let before = image(
        [10.0, 0.0, 0.0],
        90.0,
        [2.0; 3],
        image(
            [0.0, 5.0, 0.0],
            0.0,
            [1.0; 3],
            image([1.0, 0.0, 0.0], 30.0, [1.0, 3.0, 1.0], p),
        ),
    );
    let new = local(&scene, deep);
    let after = image(
        [10.0, 0.0, 0.0],
        90.0,
        [2.0; 3],
        image(new.translation, new.rotation[2], new.scale, p),
    );
    close(before, after);
}

#[test]
fn a_mixed_depth_keyed_member_is_refused_with_the_scene_untouched() {
    let mut scene = NativeScene::default();
    let top = cube(&mut scene, "top");
    let mid = cube(&mut scene, "mid");
    let deep = cube(&mut scene, "deep");
    let shallow = cube(&mut scene, "shallow");
    set(&mut scene, mid, [0.0, 5.0, 0.0], [0.0; 3], [1.0; 3]);
    for (child, parent) in [(mid, top), (deep, mid), (shallow, top)] {
        scene
            .reparent(child, Some(parent), ReparentKeep::Local)
            .unwrap();
    }
    scene.set_keys(&[deep], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.group(&[deep, shallow], PARENT),
        Err(NativeError::InvalidReparent(
            ReparentRefusal::WorldNeedsPlainTransform
        ))
    );
    assert_eq!(scene, before);
}

#[test]
fn only_top_most_objects_are_grouped_and_a_listed_descendant_stays_put() {
    let (mut scene, parent, a, b) = under_a_parent();
    let group = scene.group(&[a, parent, b], PARENT).unwrap();
    assert_eq!(children_of(&scene, Some(group)), vec![parent]);
    assert_eq!(parent_of(&scene, a), Some(parent));
    assert_eq!(parent_of(&scene, b), Some(parent));
}

#[test]
fn refusals_leave_the_scene_untouched() {
    let (mut scene, _, a, b) = under_a_parent();
    let before = scene.clone();
    assert_eq!(
        scene.group(&[], PARENT),
        Err(NativeError::InvalidReparent(ReparentRefusal::EmptySet))
    );
    assert_eq!(
        scene.group(&[a, a], PARENT),
        Err(SceneError::InvalidIds.into())
    );
    assert!(scene.group(&[NodeId::new(9999)], PARENT).is_err());
    assert_eq!(scene, before);

    let transform = scene.node(a).unwrap().transform_id();
    let owner = scene.node(b).unwrap().transform_id();
    scene.dag.push(DagEdge {
        parent: Some(owner),
        child: transform,
    });
    let before = scene.clone();
    assert_eq!(
        scene.group(&[a], PARENT),
        Err(NativeError::InvalidReparent(ReparentRefusal::Instanced))
    );
    assert_eq!(scene, before);

    let (mut restricted, node) = test_fixtures::cube();
    test_fixtures::restrict_edits(&mut restricted, node);
    let before = restricted.clone();
    assert_eq!(
        restricted.group(&[node], PARENT),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(restricted, before);
}

#[test]
fn a_member_name_clash_inside_the_group_is_refused_untouched() {
    let (mut scene, _, a, _) = under_a_parent();
    let twin = cube(&mut scene, "twin");
    scene.rename(twin, "a").unwrap_err();
    // Two same-named objects in different hierarchies cannot both enter the group.
    scene.nodes.iter_mut().find(|n| n.id == twin).unwrap().name = "a".into();
    let before = scene.clone();
    assert_eq!(
        scene.group(&[a, twin], PARENT),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
}

#[test]
fn a_failure_after_the_second_member_leaves_the_baseline_equal() {
    let mut scene = NativeScene::default();
    let mut ids = Vec::new();
    for index in 0..MAX_TRANSFORM_DEPTH {
        let id = cube(&mut scene, &format!("n{index}"));
        if let Some(&last) = ids.last() {
            scene.reparent(id, Some(last), ReparentKeep::Local).unwrap();
        }
        ids.push(id);
    }
    let other = cube(&mut scene, "other");
    let before = scene.clone();
    // Both members move into the group before the extra level breaks the depth limit.
    assert_eq!(
        scene.group(&[other, ids[0]], PARENT),
        Err(NativeError::ResourceLimit)
    );
    assert_eq!(scene, before);
    assert_eq!(scene.next_id, before.next_id);
    assert_eq!(scene.next_native_id, before.next_native_id);
}

const CENTER: GroupOptions = GroupOptions {
    pivot: GroupPivot::Center,
    ..PARENT
};

#[test]
fn center_puts_both_pivots_at_the_hand_derived_bounding_box_centre() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    set(&mut scene, a, [2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    set(&mut scene, b, [0.0, 4.0, 0.0], [0.0; 3], [1.0; 3]);
    let (a_bits, b_bits) = (bits(&scene, a), bits(&scene, b));
    let group = scene.group(&[a, b], CENTER).unwrap();
    // Unit cubes: x spans -0.5..2.5, y -0.5..4.5, z -0.5..0.5.
    let node = scene.node(group).unwrap();
    assert_eq!(node.rotate_pivot(), [1.0, 2.0, 0.0]);
    assert_eq!(node.scale_pivot(), [1.0, 2.0, 0.0]);
    assert_eq!(local(&scene, group), NativeTransform::default());
    assert_eq!((bits(&scene, a), bits(&scene, b)), (a_bits, b_bits));
}

#[test]
fn origin_leaves_the_pivots_at_zero() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    set(&mut scene, a, [2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    let group = scene.group(&[a], PARENT).unwrap();
    let node = scene.node(group).unwrap();
    assert_eq!(node.rotate_pivot(), [0.0; 3]);
    assert_eq!(node.scale_pivot(), [0.0; 3]);
}

#[test]
fn center_of_a_member_with_a_hierarchy_uses_its_descendants_too() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let child = cube(&mut scene, "child");
    set(&mut scene, child, [10.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    scene.reparent(child, Some(a), ReparentKeep::Local).unwrap();
    let group = scene.group(&[a], CENTER).unwrap();
    assert_eq!(scene.node(group).unwrap().rotate_pivot(), [5.0, 0.0, 0.0]);
}

#[test]
fn a_centre_pivoted_group_and_a_keyed_cube_group_with_the_defaults() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    let keyed = cube(&mut scene, "keyed");
    set(&mut scene, a, [2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    set(&mut scene, keyed, [0.0, 0.0, 6.0], [0.0; 3], [1.0; 3]);
    scene.set_keys(&[keyed], 1, ALL_CHANNELS, 1).unwrap();
    let inner = scene.group(&[a, b], CENTER).unwrap();
    let inner_bits = (
        bits(&scene, inner),
        scene.node(inner).unwrap().rotate_pivot(),
    );
    assert_ne!(inner_bits.1, [0.0; 3]);
    let outer = scene.group(&[inner, keyed], CENTER).unwrap();
    assert_eq!(children_of(&scene, Some(outer)), vec![inner, keyed]);
    assert_eq!(
        (
            bits(&scene, inner),
            scene.node(inner).unwrap().rotate_pivot()
        ),
        inner_bits
    );
    // inner spans x -0.5..2.5 and keyed z 5.5..6.5, so the outer centre is (1, 0, 3).
    assert_eq!(scene.node(outer).unwrap().rotate_pivot(), [1.0, 0.0, 3.0]);
}
