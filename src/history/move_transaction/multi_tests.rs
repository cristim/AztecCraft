use super::*;
use crate::native_scene::{NativeTransform, WorldMoveUnsupported, test_fixtures};
use crate::scene::{NodeId, Primitive, SceneError};
use std::time::Instant;

const BUDGET: usize = 256 * 1024 * 1024;
const NATIVE: ManipulationProvenance = ManipulationProvenance::Native;

fn cubes(count: usize) -> (History, Vec<TransformTarget>) {
    let mut scene = NativeScene::default();
    for index in 0..count {
        scene
            .create(Primitive::Cube, &format!("cube{index}"))
            .unwrap();
    }
    let targets = scene.transform_paths().unwrap();
    (History::new(scene), targets)
}

fn target_of(history: &History, node: NodeId) -> TransformTarget {
    history
        .scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.node == node)
        .unwrap()
}

fn path_of(history: &History, names: &[NodeId]) -> TransformTarget {
    let ids: Vec<_> = names
        .iter()
        .map(|node| history.scene.node(*node).unwrap().transform_id())
        .collect();
    history
        .scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.path == ids)
        .unwrap()
}

fn begin(
    history: &mut History,
    targets: &[TransformTarget],
    token: Uuid,
) -> Result<ManipulationId, ManipulationError> {
    history.begin_move_many(targets, WorldAxis::X, token, NATIVE, BUDGET)
}

fn x_of(history: &History, node: NodeId) -> f64 {
    history.scene.node(node).unwrap().transform().translation[0]
}

fn bits(history: &History, node: NodeId) -> [u64; 3] {
    history
        .scene
        .node(node)
        .unwrap()
        .transform()
        .translation
        .map(f64::to_bits)
}

#[test]
fn several_movers_preview_commit_one_labelled_undo_entry_and_undo_redo_bit_identically() {
    let (mut history, targets) = cubes(3);
    let nodes: Vec<_> = targets.iter().map(|target| target.node).collect();
    let token = Uuid::new_v4();
    let before = history.scene.clone();
    let id = begin(&mut history, &targets, token).unwrap();
    let counts = |history: &History| {
        let state = history.active_manipulation().unwrap();
        (state.member_count(), state.covered_count())
    };
    assert_eq!(counts(&history), (3, 0));
    history.update_move(token, id, NATIVE, 2.5).unwrap();
    for node in &nodes {
        assert_eq!(x_of(&history, *node), 2.5);
    }
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    for node in &nodes {
        assert_eq!(x_of(&history, *node), 2.5);
    }
    assert_eq!(history.undo.len(), 1);
    assert_eq!(history.undo_label().as_deref(), Some("Move"));
    let moved = history.scene.clone();
    assert!(history.undo());
    assert_eq!(history.scene, before);
    assert!(history.redo());
    assert_eq!(history.scene, moved);
}

#[test]
fn cancel_restores_every_member() {
    let (mut history, targets) = cubes(3);
    let token = Uuid::new_v4();
    let before = history.scene.clone();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 4.0).unwrap();
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
        .unwrap();
    assert_eq!(history.scene, before);
    assert!(history.undo.is_empty());
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    history.cancel_active_move().unwrap();
    assert_eq!(history.scene, before);
    assert!(history.active_manipulation().is_none());
}

#[test]
fn begin_with_an_unsupported_member_names_it_and_changes_nothing() {
    let (mut scene, pivoted) = test_fixtures::pivoted_shape();
    let plain = scene.create(Primitive::Cube, "plain").unwrap();
    let mut history = History::new(scene);
    let targets = [target_of(&history, plain), target_of(&history, pivoted)];
    let before = history.scene.clone();
    let revision = history.revision();
    let name = history.scene.node(pivoted).unwrap().name().to_string();
    for order in [[0, 1], [1, 0]] {
        let ordered = order.map(|index| targets[index].clone());
        let error = begin(&mut history, &ordered, Uuid::new_v4()).unwrap_err();
        assert_eq!(
            error,
            ManipulationError::Member {
                name: name.clone(),
                source: Box::new(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::SelectedPivot
                )),
            }
        );
        assert!(history.active_manipulation().is_none());
        assert_eq!(history.scene, before);
        assert_eq!(history.revision(), revision);
    }
    assert_eq!(
        begin(&mut history, &[], Uuid::new_v4()),
        Err(ManipulationError::Native(NativeError::InvalidIdentity))
    );
    assert_eq!(history.next_manipulation_id, 0);
}

#[test]
fn a_far_from_origin_member_fails_the_preview_without_writing_any_member() {
    let (mut history, targets) = cubes(3);
    let far = targets[2].node;
    let far_value = NativeTransform {
        translation: [1e15, 0.0, 0.0],
        ..Default::default()
    };
    history.scene.set_transform(far, far_value).unwrap();
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 0.0).unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.update_move(token, id, NATIVE, 1.0),
        Err(ManipulationError::Native(NativeError::UnsupportedMove(
            WorldMoveUnsupported::NumericalDisplacement
        )))
    );
    for node in targets.iter().map(|target| target.node) {
        assert_eq!(history.scene.node(node), preview.node(node));
    }
}

#[test]
fn a_view_budget_failure_after_the_writes_restores_every_member() {
    let (mut history, targets) = cubes(3);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 1.25).unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.update_move(token, id, NATIVE, 1e40),
        Err(ManipulationError::View(ViewError::NonFiniteWorld))
    );
    assert_eq!(history.scene, preview);
    assert_eq!(history.active_manipulation().unwrap().parameter, 1.25);
}

#[test]
fn a_parent_and_child_both_selected_move_the_child_once() {
    let (scene, parent, child) = test_fixtures::group_with_own_shape_and_child();
    let mut history = History::new(scene);
    let child_target = path_of(&history, &[parent, child]);
    let parent_target = path_of(&history, &[parent]);
    let child_before = bits(&history, child);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &[child_target, parent_target], token).unwrap();
    let state = history.active_manipulation().unwrap();
    let counts = (state.member_count(), state.covered_count());
    assert_eq!(counts, (2, 1));
    history.update_move(token, id, NATIVE, 2.0).unwrap();
    assert_eq!(x_of(&history, parent), 2.0);
    assert_eq!(bits(&history, child), child_before);
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    assert_eq!(x_of(&history, parent), 2.0);
    assert_eq!(bits(&history, child), child_before);
    assert_eq!(history.undo.len(), 1);
}

#[test]
fn the_lead_may_be_the_covered_member() {
    let (scene, parent, child) = test_fixtures::group_with_own_shape_and_child();
    let mut history = History::new(scene);
    let targets = [
        path_of(&history, &[parent]),
        path_of(&history, &[parent, child]),
    ];
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    let child_channels = history.scene.node(child).unwrap().transform().translation;
    history.update_move(token, id, NATIVE, 1.5).unwrap();
    assert_eq!(x_of(&history, parent), 1.5);
    // The covered lead reports its own channels, which follow the parent without changing.
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.target.node, child);
    assert_eq!(state.baseline_channels, child_channels);
    assert_eq!(state.current_channels, Some(child_channels));
}

#[test]
fn a_bogus_covered_target_is_refused_whole() {
    let (scene, parent, child) = test_fixtures::group_with_own_shape_and_child();
    let mut history = History::new(scene);
    let parent_target = path_of(&history, &[parent]);
    let mut bogus = path_of(&history, &[parent, child]);
    bogus.node = NodeId::new(999);
    let before = history.scene.clone();
    let error = begin(&mut history, &[bogus, parent_target], Uuid::new_v4()).unwrap_err();
    assert!(matches!(&error, ManipulationError::Member { name, .. } if name == "?"));
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, before);
}

#[test]
fn the_same_target_twice_is_invalid_ids() {
    let (mut history, targets) = cubes(2);
    let twice = [targets[0].clone(), targets[1].clone(), targets[0].clone()];
    let name = history
        .scene
        .node(targets[0].node)
        .unwrap()
        .name()
        .to_string();
    assert_eq!(
        begin(&mut history, &twice, Uuid::new_v4()),
        Err(ManipulationError::Member {
            name,
            source: Box::new(SceneError::InvalidIds.into()),
        })
    );
    assert!(history.active_manipulation().is_none());
}

#[test]
fn move_instanced_ancestor_covers_descendant_by_identity() {
    let (scene, x1, x2, a, b) = test_fixtures::instanced_chain();
    let mut history = History::new(scene);
    // Node a is selected through x1; b is selected through x2/a. a carries b in both instances,
    // so b is covered although "x2/a/b" does not start with "x1/a".
    let targets = [path_of(&history, &[x1, a]), path_of(&history, &[x2, a, b])];
    let b_before = bits(&history, b);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    assert_eq!(history.active_manipulation().unwrap().covered_count(), 1);
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    assert_eq!(x_of(&history, a), 3.0);
    assert_eq!(bits(&history, b), b_before);
}

#[test]
fn move_refuses_two_instances_of_one_node() {
    let (scene, x1, x2, a, _) = test_fixtures::instanced_chain();
    let mut history = History::new(scene);
    let targets = [path_of(&history, &[x1, a]), path_of(&history, &[x2, a])];
    let before = history.scene.clone();
    let name = history.scene.node(a).unwrap().name().to_string();
    let error = begin(&mut history, &targets, Uuid::new_v4()).unwrap_err();
    assert_eq!(
        error,
        ManipulationError::Member {
            name,
            source: Box::new(NativeError::UnsupportedMove(
                WorldMoveUnsupported::SharedInstances
            )),
        }
    );
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, before);
}

#[test]
fn stale_token_id_and_owner_leave_a_multi_transaction_untouched() {
    let (mut history, targets) = cubes(2);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.update_move(Uuid::new_v4(), id, NATIVE, 2.0),
        Err(ManipulationError::StaleScene)
    );
    assert_eq!(
        history.update_move(token, ManipulationId(99), NATIVE, 2.0),
        Err(ManipulationError::StaleManipulation)
    );
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Remote, 2.0),
        Err(ManipulationError::WrongOwner)
    );
    assert_eq!(history.scene, preview);
    assert_eq!(history.active_manipulation().unwrap().parameter, 1.0);
}

#[test]
fn a_failed_restore_is_typed_clears_the_transaction_and_writes_nothing_more() {
    let (mut history, targets) = cubes(3);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    // A scene that lacks the third member: its baseline identity no longer resolves.
    let mut other = NativeScene::default();
    for index in 0..2 {
        other
            .create(Primitive::Cube, &format!("cube{index}"))
            .unwrap();
    }
    history.scene = other.clone();
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Commit),
        Err(ManipulationError::Native(NativeError::InvalidIdentity))
    );
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, other);
    assert!(history.undo.is_empty());
    begin(&mut history, &targets[..2], token).unwrap();
}

#[test]
fn four_thousand_members_begin_within_the_budget_and_not_one_byte_less() {
    let scene = test_fixtures::many_roots(4096);
    let targets = scene.transform_paths().unwrap();
    let mut history = History::new(scene);
    let token = Uuid::new_v4();
    let started = Instant::now();
    let id = begin(&mut history, &targets, token).unwrap();
    let begin_time = started.elapsed();
    let started = Instant::now();
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    eprintln!(
        "4096 movers: begin {begin_time:?}, one update {:?}",
        started.elapsed()
    );
    history.cancel_active_move().unwrap();
    let charged = targets[4095].path.len() * size_of::<NativeId>()
        + size_of::<MoveTransaction>()
        + targets
            .iter()
            .map(|target| target.path.len() * size_of::<NativeId>() + MOVER_BYTES)
            .sum::<usize>();
    assert_eq!(
        history.begin_move_many(&targets, WorldAxis::X, token, NATIVE, charged - 1),
        Err(ManipulationError::View(ViewError::ResourceLimit))
    );
    assert!(history.active_manipulation().is_none());
}

#[test]
fn check_move_axes_gives_the_refusal_a_begin_would_without_starting_one() {
    let (mut scene, pivoted) = test_fixtures::pivoted_shape();
    let plain = scene.create(Primitive::Cube, "plain").unwrap();
    let history = History::new(scene);
    let targets = [target_of(&history, plain), target_of(&history, pivoted)];
    let before = history.scene.clone();
    let both = history.check_move_axes(&targets);
    assert!(
        matches!(&both, Err(ManipulationError::Member { source, .. })
        if **source == NativeError::UnsupportedMove(WorldMoveUnsupported::SelectedPivot))
    );
    let cubes = [target_of(&history, plain), target_of(&history, plain)];
    assert!(
        history
            .check_move_axes(&cubes[..1])
            .unwrap()
            .iter()
            .all(Option::is_none)
    );
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, before);
    assert_eq!(history.next_manipulation_id, 0);
}

#[test]
fn check_move_axes_reports_a_missing_leaf_per_axis_exactly_as_begin_does() {
    let (scene, full, bare) = test_fixtures::binary_cube_and_cube_without_leaf();
    let history = History::new(scene.clone());
    // The leafless node is not the lead, so only its own leaves can refuse; each world axis maps
    // to a different local component, so the three refusals differ.
    let targets = [target_of(&history, bare), target_of(&history, full)];
    let name = history.scene.node(bare).unwrap().name().to_string();
    let axes = history.check_move_axes(&targets).unwrap();
    for (component, (axis, checked)) in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z]
        .into_iter()
        .zip(axes)
        .enumerate()
    {
        assert_eq!(
            checked,
            Some(ManipulationError::Member {
                name: name.clone(),
                source: Box::new(NativeError::AbsentChannelLeaf {
                    channel: crate::native_scene::ChannelLeaf::Translate,
                    component,
                }),
            })
        );
        let mut fresh = History::new(scene.clone());
        let begun = fresh
            .begin_move_many(&targets, axis, Uuid::new_v4(), NATIVE, BUDGET)
            .map(|_| ());
        assert_eq!(begun.err(), checked, "{axis:?}");
    }
    // The complete node alone is available on every axis.
    let alone = history.check_move_axes(&targets[1..]);
    assert!(alone.unwrap().iter().all(Option::is_none));
}
