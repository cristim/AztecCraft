//! Channel subsets, bit-exact unchosen channels, the remaining refusals and relative degeneracy.
use super::*;
use crate::native_scene::{DEFAULT_JOINT_RADIUS, JointRequest};

fn channels(mask: u8) -> TransformChannels {
    TransformChannels {
        translate: mask & 1 != 0,
        rotate: mask & 2 != 0,
        scale: mask & 4 != 0,
    }
}

fn chosen(channels: TransformChannels, slot: usize) -> bool {
    [channels.translate, channels.rotate, channels.scale][slot / 3]
}

/// Chosen channels are the exact defaults, every other channel keeps its stored bits.
fn assert_channels(
    before: &NativeScene,
    after: &NativeScene,
    ids: &[NodeId],
    c: TransformChannels,
) {
    for id in ids {
        let (old, new) = (bits(before, *id), bits(after, *id));
        for slot in 0..9 {
            let expected = if chosen(c, slot) {
                identity_bits()[slot]
            } else {
                old[slot]
            };
            assert_eq!(new[slot], expected, "node {id:?} slot {slot} {c:?}");
        }
    }
}

fn two_roots(rotation: [f64; 3]) -> (NativeScene, [NodeId; 4]) {
    let (mut scene, parent, a, b) = family();
    let other = cube(&mut scene, "other");
    set(
        &mut scene,
        parent,
        [4.0, -2.0, 1.0],
        rotation,
        [2.0, 0.5, 3.0],
    );
    set(
        &mut scene,
        other,
        [0.0, 1.0, 2.0],
        [30.0, 40.0, 50.0],
        [1.0, 2.0, 4.0],
    );
    (scene, [parent, a, b, other])
}

#[test]
fn every_channel_subset_keeps_unchosen_bits_and_every_world_point() {
    for rotation in [[10.0, 20.0, 90.0], [0.0, 100.0, 0.0]] {
        for roots in [vec![0], vec![0, 3]] {
            for mask in 1..8 {
                let (mut scene, ids) = two_roots(rotation);
                let before = scene.clone();
                let (points, normals) = (world_points(&scene), world_normals(&scene));
                let selected: Vec<NodeId> = roots.iter().map(|i| ids[*i]).collect();
                scene
                    .freeze_transformations(&selected, channels(mask))
                    .unwrap();
                assert_channels(&before, &scene, &ids[..3], channels(mask));
                if roots.len() == 1 {
                    assert_eq!(bits(&scene, ids[3]), bits(&before, ids[3]));
                } else {
                    assert_channels(&before, &scene, &ids[3..], channels(mask));
                }
                same(&points, &world_points(&scene));
                same(&normals, &world_normals(&scene));
            }
        }
    }
}

#[test]
fn a_translate_only_freeze_leaves_a_rotation_of_0_100_0_untouched() {
    let (mut scene, id) = test_fixtures::cube();
    set(&mut scene, id, [1.0, 2.0, 3.0], [0.0, 100.0, 0.0], [1.0; 3]);
    let before = bits(&scene, id);
    scene.freeze_transformations(&[id], channels(1)).unwrap();
    assert_eq!(bits(&scene, id)[3..], before[3..]);
    assert_eq!(bits(&scene, id)[..3], identity_bits()[..3]);
}

fn refuses(mut scene: NativeScene, ids: &[NodeId], refusal: TransformRefusal) {
    let before = scene.clone();
    assert_eq!(
        scene.freeze_transformations(ids, TransformChannels::ALL),
        Err(NativeError::InvalidTransformEdit(refusal))
    );
    assert_eq!(scene, before, "{refusal:?}");
}

fn joint_root(scene: &mut NativeScene) -> NodeId {
    scene
        .create_joint(&JointRequest {
            parent: None,
            position: [1.0, 0.0, 0.0],
            relative: false,
            radius: DEFAULT_JOINT_RADIUS,
            name: None,
        })
        .unwrap()
        .node
}

#[test]
fn joints_instances_and_curves_are_refused_on_the_object_or_below_it() {
    let mut scene = NativeScene::default();
    let joint = joint_root(&mut scene);
    refuses(scene, &[joint], TransformRefusal::Joint);

    let (scene, x1, x2, a, b) = test_fixtures::instanced_chain();
    for ids in [vec![a], vec![b], vec![x1], vec![x2], vec![x1, x2]] {
        refuses(scene.clone(), &ids, TransformRefusal::Instanced);
    }

    // A curve exists only in a sourced scene, which the public freeze refuses first; the guard
    // below it is reached directly.
    let (mut curved, node) = crate::native_scene::curve::tests::candidate();
    assert_eq!(
        curved.freeze_transformations(&[node], TransformChannels::ALL),
        Err(NativeError::ReadOnlyFormat)
    );
    let index = curved.nodes.iter().position(|n| n.id == node).unwrap();
    assert_eq!(
        curved.freeze_node(index, DMat4::IDENTITY, TransformChannels::ALL, 0, &mut 0),
        Err(NativeError::InvalidTransformEdit(TransformRefusal::Curve))
    );
}

#[test]
fn a_refusal_on_the_third_node_leaves_the_whole_selection_untouched() {
    let (mut scene, ids) = two_roots([10.0, 20.0, 90.0]);
    let third = cube(&mut scene, "third");
    scene.set_keys(&[third], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    for mask in [7, 1, 6] {
        assert_eq!(
            scene.freeze_transformations(&[ids[0], ids[3], third], channels(mask)),
            Err(NativeError::InvalidTransformEdit(TransformRefusal::Keyed))
        );
        assert_eq!(scene, before);
    }
}

#[test]
fn uniform_tiny_scales_pass_and_zero_or_collapsed_scales_are_refused() {
    let (mut scene, id) = test_fixtures::cube();
    set(
        &mut scene,
        id,
        [1.0, 2.0, 3.0],
        [20.0, 35.0, 50.0],
        [1e-4; 3],
    );
    let points = world_points(&scene);
    scene
        .freeze_transformations(&[id], TransformChannels::ALL)
        .unwrap();
    same(&points, &world_points(&scene));
    for scale in [[0.0; 3], [1e-12; 3], [1.0, 1.0, 1e-12], [1.0, 0.0, 1.0]] {
        let (mut scene, id) = test_fixtures::cube();
        set(&mut scene, id, [0.0; 3], [0.0; 3], scale);
        refuses(scene, &[id], TransformRefusal::Degenerate);
    }
    let sheared = DMat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1e-12, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    assert!(!keeps_volume(sheared));
    assert!(keeps_volume(DMat4::from_scale(DVec3::splat(1e-4))));
}
