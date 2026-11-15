use super::*;
use crate::native_scene::group::tests::{
    bits, children_of, close, cube, image, local, parent_of, set,
};
use crate::native_scene::{GroupOptions, GroupPivot, JointRequest, test_fixtures};

const WORLD_KEPT: UngroupOptions = UngroupOptions {
    under: GroupUnder::Parent,
    keep: ReparentKeep::World,
};

const GROUP: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Origin,
    preserve_position: true,
};

/// A uniformly scaled, rotated, translated group with two plain children.
fn rotated_group() -> (NativeScene, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    let group = scene.group(&[a, b], GROUP).unwrap();
    set(
        &mut scene,
        group,
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
    (scene, group, a, b)
}

#[test]
fn ungroup_keeps_world_points_by_a_closed_form_oracle_and_removes_the_group() {
    let (mut scene, group, a, b) = rotated_group();
    let nodes = scene.nodes.len();
    let moved = scene.ungroup(&[group], WORLD_KEPT).unwrap();
    assert_eq!(moved, vec![a, b]);
    assert_eq!(scene.nodes.len(), nodes - 1);
    assert!(scene.node(group).is_err());
    assert_eq!(parent_of(&scene, a), None);
    assert_eq!(parent_of(&scene, b), None);
    let p = [0.5, 0.25, 0.75];
    // The group is T(10,0,0) Rz(90) S(2); a was T(1,0,0) Rz(30) S(1,3,1) below it.
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
}

#[test]
fn keep_local_moves_the_children_in_the_world_and_leaves_their_channels() {
    let (mut scene, group, a, b) = rotated_group();
    let before = (bits(&scene, a), bits(&scene, b));
    let options = UngroupOptions {
        keep: ReparentKeep::Local,
        ..WORLD_KEPT
    };
    scene.ungroup(&[group], options).unwrap();
    assert_eq!(before, (bits(&scene, a), bits(&scene, b)));
}

#[test]
fn children_go_to_the_groups_parent_or_to_the_world() {
    let mut scene = NativeScene::default();
    let top = cube(&mut scene, "top");
    let a = cube(&mut scene, "a");
    scene.reparent(a, Some(top), ReparentKeep::Local).unwrap();
    let group = scene.group(&[a], GROUP).unwrap();
    assert_eq!(parent_of(&scene, group), Some(top));
    let mut under_world = scene.clone();
    under_world
        .ungroup(
            &[group],
            UngroupOptions {
                under: GroupUnder::World,
                keep: ReparentKeep::Local,
            },
        )
        .unwrap();
    assert_eq!(parent_of(&under_world, a), None);
    scene.ungroup(&[group], WORLD_KEPT).unwrap();
    assert_eq!(parent_of(&scene, a), Some(top));
    assert_eq!(children_of(&scene, Some(top)).len(), 1);
}

#[test]
fn a_group_inside_a_group_ungroups_one_level_even_with_a_centre_pivot() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    let keyed = cube(&mut scene, "keyed");
    set(&mut scene, a, [2.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    set(&mut scene, keyed, [0.0, 0.0, 6.0], [0.0; 3], [1.0; 3]);
    scene
        .set_keys(&[keyed], 1, crate::native_scene::ALL_CHANNELS, 1)
        .unwrap();
    let centre = GroupOptions {
        pivot: GroupPivot::Center,
        ..GROUP
    };
    let inner = scene.group(&[a, b], centre).unwrap();
    let outer = scene.group(&[inner, keyed], centre).unwrap();
    assert_ne!(scene.node(outer).unwrap().rotate_pivot(), [0.0; 3]);
    let inner_bits = bits(&scene, inner);
    let keyed_bits = bits(&scene, keyed);
    let moved = scene.ungroup(&[outer], WORLD_KEPT).unwrap();
    assert_eq!(moved, vec![inner, keyed]);
    assert_eq!(children_of(&scene, Some(inner)), vec![a, b]);
    assert_eq!(parent_of(&scene, inner), None);
    // The outer group's matrix is exactly the identity, so nothing is rewritten.
    assert_eq!(
        (bits(&scene, inner), bits(&scene, keyed)),
        (inner_bits, keyed_bits)
    );
}

#[test]
fn hierarchy_mode_selection_of_a_group_and_its_descendants_ungroups_once() {
    let (mut scene, group, a, b) = rotated_group();
    let moved = scene.ungroup(&[group, a, b], WORLD_KEPT).unwrap();
    assert_eq!(moved, vec![a, b]);
    assert!(scene.node(group).is_err());
}

#[test]
fn a_mesh_transform_and_a_joint_are_not_groups() {
    let not_a_group = Err(NativeError::InvalidReparent(ReparentRefusal::NotAGroup));
    let (mut scene, node) = test_fixtures::cube();
    let before = scene.clone();
    assert_eq!(scene.ungroup(&[node], WORLD_KEPT), not_a_group);
    assert_eq!(scene, before);

    let mut scene = NativeScene::default();
    let target = scene
        .create_joint(&JointRequest {
            parent: None,
            position: [0.0; 3],
            relative: false,
            radius: 1.0,
            name: None,
        })
        .unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.ungroup(&[target.node], WORLD_KEPT),
        Err(NativeError::InvalidReparent(ReparentRefusal::Joint))
    );
    assert_eq!(scene, before);
}

#[test]
fn refusals_leave_the_scene_equal_to_the_baseline() {
    let (mut scene, group, _, _) = rotated_group();
    let before = scene.clone();
    assert_eq!(
        scene.ungroup(&[], WORLD_KEPT),
        Err(NativeError::InvalidReparent(ReparentRefusal::EmptySet))
    );
    assert_eq!(
        scene.ungroup(&[group, group], WORLD_KEPT),
        Err(SceneError::InvalidIds.into())
    );
    assert_eq!(scene, before);

    // A child that shares its name with a root cannot leave the group.
    let twin = cube(&mut scene, "twin");
    scene.nodes.iter_mut().find(|n| n.id == twin).unwrap().name = "a".into();
    let before = scene.clone();
    assert_eq!(
        scene.ungroup(&[group], WORLD_KEPT),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);

    // An instanced child has two parents, so it cannot leave either.
    let (mut scene, group) = {
        let (mut scene, group, a, _) = rotated_group();
        let other = cube(&mut scene, "other");
        let (child, owner) = (
            scene.node(a).unwrap().transform_id(),
            scene.node(other).unwrap().transform_id(),
        );
        scene.dag.push(crate::native_scene::DagEdge {
            parent: Some(owner),
            child,
        });
        (scene, group)
    };
    let before = scene.clone();
    assert_eq!(
        scene.ungroup(&[group], WORLD_KEPT),
        Err(NativeError::InvalidReparent(ReparentRefusal::Instanced))
    );
    assert_eq!(scene, before);
}

#[test]
fn an_empty_group_is_deleted_and_moves_nothing() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let group = scene.group(&[a], GROUP).unwrap();
    scene.reparent(a, None, ReparentKeep::Local).unwrap();
    assert!(scene.ungroup(&[group], WORLD_KEPT).unwrap().is_empty());
    assert!(scene.node(group).is_err());
}

#[test]
fn a_root_joint_and_a_child_joint_are_refused_by_group_and_ungroup_untouched() {
    let mut scene = NativeScene::default();
    let request = |parent, position| JointRequest {
        parent,
        position,
        relative: false,
        radius: 1.0,
        name: None,
    };
    let root = scene.create_joint(&request(None, [0.0; 3])).unwrap().node;
    let child = scene
        .create_joint(&request(Some(root), [0.0, 1.0, 0.0]))
        .unwrap()
        .node;
    let cube = cube(&mut scene, "cube");
    let before = scene.clone();
    let refused = NativeError::InvalidReparent(ReparentRefusal::Joint);
    for ids in [vec![root], vec![child], vec![cube, root], vec![child, cube]] {
        assert_eq!(scene.group(&ids, GROUP), Err(refused.clone()), "{ids:?}");
        assert_eq!(
            scene.ungroup(&ids, WORLD_KEPT),
            Err(refused.clone()),
            "{ids:?}"
        );
        assert_eq!(scene, before);
    }
}
