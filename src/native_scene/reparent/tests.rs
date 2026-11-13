use super::*;
use crate::native_scene::NativeTransform;
use crate::scene::Primitive;

fn two() -> (NativeScene, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let parent = scene.create(Primitive::Cube, "parentCube").unwrap();
    let child = scene.create(Primitive::Cube, "childCube").unwrap();
    (scene, parent, child)
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

/// Hand-written Rz(degrees) * scale * point, translated: the world image of a point under a
/// translate, Z rotation and scale, with no matrix library.
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

#[test]
fn keep_world_rewrites_the_local_channels_to_closed_form_values() {
    let (mut scene, parent, child) = two();
    set(
        &mut scene,
        parent,
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0; 3],
    );
    set(
        &mut scene,
        child,
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 30.0],
        [1.0, 3.0, 1.0],
    );
    scene
        .reparent(child, Some(parent), ReparentKeep::World)
        .unwrap();
    // Parent linear map is 2 * Rz(90); the child needs 0.5 * Rz(-60) * diag(1, 3, 1), and its
    // origin (world (1, 0, 0)) is (0, 4.5, 0) in the parent's frame.
    let local = scene.node(child).unwrap().transform();
    close(local.translation, [0.0, 4.5, 0.0]);
    close(local.rotation, [0.0, 0.0, -60.0]);
    close(local.scale, [0.5, 1.5, 0.5]);
    assert_eq!(parent_of(&scene, child), Some(parent));
    // Independent oracle: a point of the child's mesh keeps its world position.
    let p = [0.5, 0.25, 0.75];
    let before = image([1.0, 0.0, 0.0], 30.0, [1.0, 3.0, 1.0], p);
    let inner = image(local.translation, local.rotation[2], local.scale, p);
    let after = image([10.0, 0.0, 0.0], 90.0, [2.0; 3], inner);
    close(before, after);
}

#[test]
fn unparent_keeps_the_world_position_and_appends_a_root() {
    let (mut scene, parent, child) = two();
    set(
        &mut scene,
        parent,
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0; 3],
    );
    set(
        &mut scene,
        child,
        [0.0, 4.5, 0.0],
        [0.0, 0.0, -60.0],
        [0.5, 1.5, 0.5],
    );
    scene
        .reparent(child, Some(parent), ReparentKeep::Local)
        .unwrap();
    scene.reparent(child, None, ReparentKeep::World).unwrap();
    let local = scene.node(child).unwrap().transform();
    close(local.translation, [1.0, 0.0, 0.0]);
    close(local.rotation, [0.0, 0.0, 30.0]);
    close(local.scale, [1.0, 3.0, 1.0]);
    assert_eq!(parent_of(&scene, child), None);
    assert_eq!(
        scene
            .dag
            .iter()
            .rfind(|e| e.parent.is_none())
            .unwrap()
            .child,
        scene.node(child).unwrap().transform_id()
    );
}

#[test]
fn keep_local_leaves_every_channel_bit_for_bit() {
    let (mut scene, parent, child) = two();
    set(
        &mut scene,
        parent,
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0; 3],
    );
    set(
        &mut scene,
        child,
        [1.0, 2.0, 3.0],
        [400.0, 0.0, 0.0],
        [1.0, 3.0, 1.0],
    );
    let before = scene.node(child).unwrap().transform();
    scene
        .reparent(child, Some(parent), ReparentKeep::Local)
        .unwrap();
    assert_eq!(scene.node(child).unwrap().transform(), before);
}

#[test]
fn equal_world_frames_keep_the_channels_without_normalising_them() {
    let (mut scene, group, child) = two();
    let twin = scene.create(Primitive::Cube, "twin").unwrap();
    // Two distinct, rotated parents with the same world frame: nothing may be rewritten.
    for id in [group, twin] {
        set(&mut scene, id, [3.0, 0.0, 0.0], [0.0, 0.0, 90.0], [2.0; 3]);
    }
    set(
        &mut scene,
        child,
        [1.0, 2.0, 3.0],
        [400.0, 0.0, 0.0],
        [1.0; 3],
    );
    scene
        .reparent(child, Some(group), ReparentKeep::Local)
        .unwrap();
    let before = scene.node(child).unwrap().transform();
    scene
        .reparent(child, Some(twin), ReparentKeep::World)
        .unwrap();
    assert_eq!(scene.node(child).unwrap().transform(), before);
    assert_eq!(parent_of(&scene, child), Some(twin));
}

#[test]
fn same_parent_is_unchanged() {
    let (mut scene, parent, child) = two();
    scene.reparent(child, None, ReparentKeep::World).unwrap();
    let root_before = scene.clone();
    assert_eq!(scene, root_before);
    scene
        .reparent(child, Some(parent), ReparentKeep::World)
        .unwrap();
    let before = scene.clone();
    scene
        .reparent(child, Some(parent), ReparentKeep::World)
        .unwrap();
    assert_eq!(scene, before);
}

mod refusals;
