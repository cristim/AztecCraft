use super::*;
use crate::native_scene::{ALL_CHANNELS, test_fixtures};

#[test]
fn a_cycle_is_refused_and_the_scene_is_unchanged() {
    let (mut scene, a, b) = two();
    let c = scene.create(Primitive::Cube, "third").unwrap();
    scene.reparent(b, Some(a), ReparentKeep::Local).unwrap();
    scene.reparent(c, Some(b), ReparentKeep::Local).unwrap();
    let before = scene.clone();
    let cycle = Err(NativeError::InvalidReparent(ReparentRefusal::Cycle));
    assert_eq!(scene.reparent(a, Some(a), ReparentKeep::Local), cycle);
    assert_eq!(scene.reparent(a, Some(b), ReparentKeep::World), cycle);
    assert_eq!(scene.reparent(a, Some(c), ReparentKeep::Local), cycle);
    assert_eq!(scene, before);
}

#[test]
fn shear_negative_scale_and_a_zero_scale_parent_refuse_keep_world_only() {
    let refusal = Err(NativeError::InvalidReparent(
        ReparentRefusal::WorldNotRepresentable,
    ));
    for (parent_scale, child_rotation) in [
        ([1.0, 2.0, 1.0], 30.0),
        ([-1.0, 1.0, 1.0], 0.0),
        ([0.0, 1.0, 1.0], 0.0),
    ] {
        let (mut scene, parent, child) = two();
        set(&mut scene, parent, [0.0; 3], [0.0; 3], parent_scale);
        set(
            &mut scene,
            child,
            [1.0, 0.0, 0.0],
            [0.0, 0.0, child_rotation],
            [1.0; 3],
        );
        let before = scene.clone();
        assert_eq!(
            scene.reparent(child, Some(parent), ReparentKeep::World),
            refusal,
            "{parent_scale:?}"
        );
        assert_eq!(scene, before);
        if parent_scale[0] != 0.0 {
            scene
                .reparent(child, Some(parent), ReparentKeep::Local)
                .unwrap();
        }
    }
}

#[test]
fn keep_world_refuses_pivots_extras_and_keys_but_keep_local_does_not() {
    let plain = Err(NativeError::InvalidReparent(
        ReparentRefusal::WorldNeedsPlainTransform,
    ));
    let (mut scene, parent, _) = two();
    let (pivoted, node) = test_fixtures::pivoted_shape();
    let mut pivoted = pivoted;
    assert_eq!(
        pivoted.reparent(node, None, ReparentKeep::World),
        Ok(()),
        "a root stays put"
    );
    let group = pivoted.create(Primitive::Cube, "group").unwrap();
    let before = pivoted.clone();
    assert_eq!(
        pivoted.reparent(node, Some(group), ReparentKeep::World),
        plain
    );
    assert_eq!(pivoted, before);
    pivoted
        .reparent(node, Some(group), ReparentKeep::Local)
        .unwrap();
    let child = scene.create(Primitive::Cube, "keyed").unwrap();
    scene.set_keys(&[child], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.reparent(child, Some(parent), ReparentKeep::World),
        plain
    );
    assert_eq!(scene, before);
}

#[test]
fn restricted_instanced_missing_and_oversize_requests_are_refused_untouched() {
    let (mut scene, parent, child) = two();
    let (mut restricted, node) = test_fixtures::cube();
    test_fixtures::restrict_edits(&mut restricted, node);
    let group = restricted.create(Primitive::Cube, "group").unwrap();
    let before = restricted.clone();
    assert_eq!(
        restricted.reparent(node, Some(group), ReparentKeep::Local),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(restricted, before);

    let transform = scene.node(child).unwrap().transform_id();
    let owner = scene.node(parent).unwrap().transform_id();
    scene.dag.push(DagEdge {
        parent: Some(owner),
        child: transform,
    });
    let before = scene.clone();
    assert_eq!(
        scene.reparent(child, None, ReparentKeep::Local),
        Err(NativeError::InvalidReparent(ReparentRefusal::Instanced))
    );
    assert_eq!(scene, before);

    let (mut scene, parent, _) = two();
    let before = scene.clone();
    let missing = NodeId::new(9999);
    assert!(scene.reparent(missing, None, ReparentKeep::Local).is_err());
    assert!(
        scene
            .reparent(parent, Some(missing), ReparentKeep::Local)
            .is_err()
    );
    assert_eq!(scene, before);
}

#[test]
fn a_chain_deeper_than_the_depth_limit_is_refused_and_restored() {
    let mut scene = NativeScene::default();
    let mut ids = Vec::new();
    for index in 0..crate::native_scene::MAX_TRANSFORM_DEPTH {
        let id = scene.create(Primitive::Cube, &format!("n{index}")).unwrap();
        if let Some(&last) = ids.last() {
            scene.reparent(id, Some(last), ReparentKeep::Local).unwrap();
        }
        ids.push(id);
    }
    let extra = scene.create(Primitive::Cube, "extra").unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.reparent(extra, ids.last().copied(), ReparentKeep::Local),
        Err(NativeError::ResourceLimit)
    );
    assert_eq!(scene, before);
}

#[test]
fn a_name_clash_among_the_new_siblings_is_refused() {
    let (mut scene, parent, child) = two();
    let other = scene.create(Primitive::Cube, "other").unwrap();
    // A transform renamed after the parent's own shape: rename_node checks transforms only.
    scene.rename(child, "parentCubeShape").unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.reparent(child, Some(parent), ReparentKeep::Local),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
    scene
        .reparent(other, Some(parent), ReparentKeep::Local)
        .unwrap();
}

#[test]
fn delete_and_undo_work_on_a_parented_child_and_a_parent_with_children() {
    use crate::history::{Command, History};
    let (scene, parent, child) = two();
    let mut history = History::new(scene);
    history
        .execute(Command::Reparent {
            id: child,
            parent: Some(parent),
            keep: ReparentKeep::World,
        })
        .unwrap();
    assert_eq!(history.undo_label().as_deref(), Some("Parent"));
    history.execute(Command::Remove(child)).unwrap();
    assert!(history.scene().node(child).is_err());
    assert!(history.undo());
    assert!(history.scene().node(child).is_ok());
    history.execute(Command::Remove(parent)).unwrap();
    assert!(history.scene().nodes().is_empty());
    assert!(history.undo() && history.undo());
    assert_eq!(parent_of(history.scene(), child), None);
    assert!(history.redo());
    assert_eq!(parent_of(history.scene(), child), Some(parent));
}

#[test]
fn a_translation_only_parent_difference_keeps_rotation_and_scale_bit_for_bit() {
    let (mut scene, parent, child) = two();
    set(&mut scene, parent, [5.0, -2.0, 1.0], [0.0; 3], [1.0; 3]);
    set(
        &mut scene,
        child,
        [1.0, 2.0, 3.0],
        [400.0, 30.0, -500.0],
        [1.0, 3.0, 0.5],
    );
    let before = scene.node(child).unwrap().transform();
    scene
        .reparent(child, Some(parent), ReparentKeep::World)
        .unwrap();
    let after = scene.node(child).unwrap().transform();
    assert_eq!(
        after.rotation.map(f64::to_bits),
        before.rotation.map(f64::to_bits)
    );
    assert_eq!(
        after.scale.map(f64::to_bits),
        before.scale.map(f64::to_bits)
    );
    close(after.translation, [-4.0, 4.0, 2.0]);
}

#[test]
fn a_small_shear_is_refused_under_a_huge_translation_or_a_huge_parent_scale() {
    let refusal = Err(NativeError::InvalidReparent(
        ReparentRefusal::WorldNotRepresentable,
    ));
    // Parent scale (1, 1 + e, 1) under a 30 degree child rotation shears by about e / 2.
    for (translation, scale, e) in [(1.0e6, 1.0, 1.0e-4), (0.0, 1.0e3, 1.0e-7)] {
        let (mut scene, parent, child) = two();
        set(
            &mut scene,
            parent,
            [translation, 0.0, 0.0],
            [0.0; 3],
            [scale, scale * (1.0 + e), scale],
        );
        set(&mut scene, child, [0.0; 3], [0.0, 0.0, 30.0], [1.0; 3]);
        let before = scene.clone();
        assert_eq!(
            scene.reparent(child, Some(parent), ReparentKeep::World),
            refusal,
            "{translation} {scale}"
        );
        assert_eq!(scene, before);
    }
}
