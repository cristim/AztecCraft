//! Match Transformations: world matrices of the moved nodes against the target, per channel subset.
use super::*;
use crate::native_scene::group::tests::parent_of;
use glam::DMat4;

const EPS: f64 = 1e-9;

fn channels(mask: u8) -> TransformChannels {
    TransformChannels {
        translate: mask & 1 != 0,
        rotate: mask & 2 != 0,
        scale: mask & 4 != 0,
    }
}

fn world(scene: &NativeScene, id: NodeId) -> DMat4 {
    let transform = scene.node(id).unwrap().transform_id();
    scene.path_world(&chain(scene, transform)).unwrap()
}

fn axes(m: DMat4) -> [DVec3; 3] {
    [m.x_axis, m.y_axis, m.z_axis].map(|c| c.truncate())
}

fn assert_matrix(a: DMat4, b: DMat4, what: &str) {
    let worst = a
        .to_cols_array()
        .iter()
        .zip(b.to_cols_array())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    assert!(worst < EPS, "{what}: {worst} {a:?} {b:?}");
}

/// Translation, unit axes (rotation) and axis lengths (scale) of a world matrix, read without
/// the production decomposition.
fn parts(m: DMat4) -> (DVec3, [DVec3; 3], [f64; 3]) {
    let a = axes(m);
    (
        m.w_axis.truncate(),
        a.map(|c| c.normalize()),
        a.map(DVec3::length),
    )
}

fn assert_vec(a: DVec3, b: DVec3, what: &str) {
    assert!((a - b).abs().max_element() < EPS, "{what}: {a:?} {b:?}");
}

/// Chosen components of `after` equal the target's, unchosen ones equal `before`.
fn assert_components(before: DMat4, after: DMat4, target: DMat4, c: TransformChannels) {
    let (b, a, t) = (parts(before), parts(after), parts(target));
    let pick = |on: bool| if on { t } else { b };
    let (want_t, want_r, want_s) = (pick(c.translate), pick(c.rotate), pick(c.scale));
    assert_vec(a.0, want_t.0, "translation");
    for i in 0..3 {
        assert_vec(a.1[i], want_r.1[i], "rotation axis");
        assert!((a.2[i] - want_s.2[i]).abs() < EPS, "scale {c:?}");
    }
}

/// A uniformly scaled, rotated parent holding the moved cube, and a target under another one.
fn rig() -> (NativeScene, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let parent = cube(&mut scene, "moveParent");
    let moved = cube(&mut scene, "moved");
    let holder = cube(&mut scene, "targetParent");
    let target = cube(&mut scene, "target");
    set(
        &mut scene,
        parent,
        [3.0, 1.0, -2.0],
        [20.0, 40.0, 60.0],
        [2.0; 3],
    );
    set(
        &mut scene,
        moved,
        [1.0, 2.0, 3.0],
        [10.0, -25.0, 70.0],
        [1.5, 0.5, 2.0],
    );
    set(
        &mut scene,
        holder,
        [-5.0, 4.0, 1.0],
        [0.0, 35.0, 0.0],
        [0.5; 3],
    );
    set(
        &mut scene,
        target,
        [2.0, -3.0, 6.0],
        [30.0, 15.0, -40.0],
        [3.0, 1.0, 0.25],
    );
    scene
        .reparent(moved, Some(parent), ReparentKeep::Local)
        .unwrap();
    scene
        .reparent(target, Some(holder), ReparentKeep::Local)
        .unwrap();
    (scene, parent, moved, target)
}

#[test]
fn every_channel_subset_matches_the_target_world_and_keeps_unchosen_bits() {
    for mask in 1..=7u8 {
        let c = channels(mask);
        let (mut scene, _, moved, target) = rig();
        let (before, goal, bits_before) = (
            world(&scene, moved),
            world(&scene, target),
            bits(&scene, moved),
        );
        scene.match_transformations(&[moved, target], c).unwrap();
        let after = world(&scene, moved);
        assert_components(before, after, goal, c);
        if mask == 7 {
            assert_matrix(after, goal, "trs");
        }
        let now = bits(&scene, moved);
        for slot in 0..9 {
            if !chosen(c, slot) {
                assert_eq!(now[slot], bits_before[slot], "slot {slot} mask {mask}");
            }
        }
        assert_matrix(world(&scene, target), goal, "target untouched");
    }
}

fn chosen(c: TransformChannels, slot: usize) -> bool {
    [c.translate, c.rotate, c.scale][slot / 3]
}

#[test]
fn an_unrotated_non_uniform_parent_matches_subsets_and_refuses_those_needing_a_scale_swap() {
    for mask in 1..=7u8 {
        let c = channels(mask);
        let (mut scene, parent, moved, target) = rig();
        set(
            &mut scene,
            parent,
            [1.0, 2.0, 3.0],
            [0.0; 3],
            [2.0, 0.5, 3.0],
        );
        set(
            &mut scene,
            moved,
            [1.0, 2.0, 3.0],
            [0.0; 3],
            [1.5, 0.5, 2.0],
        );
        let holder = parent_of(&scene, target).unwrap();
        set(&mut scene, holder, [-5.0, 4.0, 1.0], [0.0; 3], [0.5; 3]);
        set(
            &mut scene,
            target,
            [2.0, -3.0, 6.0],
            [0.0, 0.0, 90.0],
            [1.0; 3],
        );
        let (before, goal) = (world(&scene, moved), world(&scene, target));
        let untouched = scene.clone();
        let outcome = scene.match_transformations(&[moved, target], c);
        if c.rotate && !c.scale {
            // The 90 degree turn swaps the axes the unrotated parent scales unevenly, so the kept
            // local scale would be wrong: refused rather than approximated.
            assert_eq!(
                outcome,
                Err(NativeError::InvalidTransformEdit(
                    TransformRefusal::NotRepresentable
                )),
                "mask {mask}"
            );
            assert_eq!(scene, untouched);
        } else {
            outcome.unwrap_or_else(|e| panic!("mask {mask}: {e:?}"));
            assert_components(before, world(&scene, moved), goal, c);
        }
    }
}

#[test]
fn a_rotated_non_uniform_parent_that_needs_shear_is_refused_untouched() {
    let (mut scene, parent, moved, target) = rig();
    set(
        &mut scene,
        parent,
        [1.0, 2.0, 3.0],
        [10.0, 20.0, 90.0],
        [2.0, 0.5, 3.0],
    );
    let before = scene.clone();
    for mask in [7, 2, 4, 6] {
        assert_eq!(
            scene.match_transformations(&[moved, target], channels(mask)),
            Err(NativeError::InvalidTransformEdit(
                TransformRefusal::NotRepresentable
            )),
            "mask {mask}"
        );
        assert_eq!(scene, before);
    }
}

#[test]
fn the_last_selected_is_the_target_and_several_objects_all_match_it() {
    let (mut scene, _, moved, target) = rig();
    let second = cube(&mut scene, "second");
    set(
        &mut scene,
        second,
        [9.0, 9.0, 9.0],
        [5.0, 6.0, 7.0],
        [4.0, 4.0, 4.0],
    );
    let goal = world(&scene, target);
    scene
        .match_transformations(&[moved, second, target], TransformChannels::ALL)
        .unwrap();
    assert_matrix(world(&scene, moved), goal, "moved");
    assert_matrix(world(&scene, second), goal, "second");
    assert_matrix(world(&scene, target), goal, "target");
}

#[test]
fn a_moved_parent_and_its_moved_child_both_end_on_the_target() {
    let (mut scene, parent, moved, target) = rig();
    let goal = world(&scene, target);
    scene
        .match_transformations(&[moved, parent, target], TransformChannels::ALL)
        .unwrap();
    assert_matrix(world(&scene, parent), goal, "parent");
    assert_matrix(world(&scene, moved), goal, "child");
}

#[test]
fn children_of_a_moved_node_move_rigidly_with_it_and_keep_their_channels() {
    let (mut scene, _, moved, target) = rig();
    let child = cube(&mut scene, "kid");
    set(
        &mut scene,
        child,
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 33.0],
        [1.0, 2.0, 3.0],
    );
    scene
        .reparent(child, Some(moved), ReparentKeep::Local)
        .unwrap();
    let (old_moved, old_child, kept) = (
        world(&scene, moved),
        world(&scene, child),
        bits(&scene, child),
    );
    scene
        .match_transformations(&[moved, target], TransformChannels::ALL)
        .unwrap();
    assert_eq!(bits(&scene, child), kept);
    let delta = world(&scene, moved) * old_moved.inverse();
    assert_matrix(world(&scene, child), delta * old_child, "child follows");
}

#[test]
fn a_refusal_on_the_third_node_changes_nothing() {
    let (mut scene, _, moved, target) = rig();
    let second = cube(&mut scene, "second");
    let third = cube(&mut scene, "third");
    scene.set_keys(&[third], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.match_transformations(&[moved, second, third, target], TransformChannels::ALL),
        Err(NativeError::InvalidTransformEdit(TransformRefusal::Keyed))
    );
    assert_eq!(scene, before);
}

#[test]
fn the_other_refusals_leave_the_scene_untouched() {
    let (mut plain, p_moved, p_target) = {
        let (s, _, m, t) = rig();
        (s, m, t)
    };
    let before = plain.clone();
    let refused = |scene: &mut NativeScene, ids: &[NodeId], c, why: TransformRefusal| {
        let copy = scene.clone();
        assert_eq!(
            scene.match_transformations(ids, c),
            Err(NativeError::InvalidTransformEdit(why))
        );
        assert_eq!(*scene, copy);
    };
    refused(
        &mut plain,
        &[p_target],
        TransformChannels::ALL,
        TransformRefusal::TooFewObjects,
    );
    refused(
        &mut plain,
        &[],
        TransformChannels::ALL,
        TransformRefusal::EmptySet,
    );
    refused(
        &mut plain,
        &[p_moved, p_target],
        channels(0),
        TransformRefusal::EmptySet,
    );
    assert_eq!(
        plain.match_transformations(&[p_moved, p_moved, p_target], TransformChannels::ALL),
        Err(SceneError::InvalidIds.into())
    );
    assert_eq!(plain, before);
    // The target below the moved node would travel with it.
    let (mut nested, _, a, b) = family();
    refused(
        &mut nested,
        &[a, b],
        TransformChannels::ALL,
        TransformRefusal::TargetInsideMoved,
    );
    // A pivot on the target or a moved node cannot be matched exactly.
    let (mut pivoted, _, m, t) = rig();
    pivoted
        .set_pivots(t, Some([1.0, 0.0, 0.0]), None, false)
        .unwrap();
    refused(
        &mut pivoted,
        &[m, t],
        TransformChannels::ALL,
        TransformRefusal::NotPlain,
    );
}

/// The shared refusal texts must read correctly for a Match caller, not only for Reset and Freeze.
#[test]
fn shared_refusal_messages_name_matching() {
    for (refusal, words) in [
        (TransformRefusal::Keyed, "matched"),
        (TransformRefusal::Joint, "matched"),
        (TransformRefusal::NotPlain, "matching"),
        (TransformRefusal::Instanced, "matched"),
    ] {
        let text = refusal.to_string();
        assert!(text.contains(words), "{refusal:?}: {text}");
    }
}
