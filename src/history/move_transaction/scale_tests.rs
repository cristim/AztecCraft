use super::*;
use crate::native_scene::{NativeTransform, ScaleUnsupported, test_fixtures};
use crate::scene::{NodeId, Primitive, SceneError};

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
    history.begin_scale_many(
        targets,
        ScaleHandle::Uniform,
        ScaleSpace::Object,
        token,
        NATIVE,
        BUDGET,
    )
}

fn scale_at(history: &History, node: NodeId) -> [f64; 3] {
    history.scene.node(node).unwrap().transform().scale
}

fn set_scale(history: &mut History, node: NodeId, scale: [f64; 3]) {
    history
        .scene
        .set_transform(
            node,
            NativeTransform {
                scale,
                ..Default::default()
            },
        )
        .unwrap();
}

#[test]
fn every_member_scales_about_its_own_value_in_one_labelled_undo_entry() {
    let (mut history, targets) = cubes(3);
    let nodes: Vec<_> = targets.iter().map(|target| target.node).collect();
    set_scale(&mut history, nodes[0], [2.0, 1.0, 1.0]);
    set_scale(&mut history, nodes[1], [1.0, 3.0, 5.0]);
    let token = Uuid::new_v4();
    let before = history.scene.clone();
    let id = begin(&mut history, &targets, token).unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!((state.member_count(), state.covered_count()), (3, 0));
    history.update_move(token, id, NATIVE, 2.0).unwrap();
    assert_eq!(scale_at(&history, nodes[0]), [4.0, 2.0, 2.0]);
    assert_eq!(scale_at(&history, nodes[1]), [2.0, 6.0, 10.0]);
    assert_eq!(scale_at(&history, nodes[2]), [2.0; 3]);
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    assert_eq!(history.undo.len(), 1);
    assert_eq!(history.undo_label().as_deref(), Some("Scale"));
    let scaled = history.scene.clone();
    assert!(history.undo());
    assert_eq!(history.scene, before);
    assert!(history.redo());
    assert_eq!(history.scene, scaled);
}

#[test]
fn an_axis_handle_scales_one_channel_of_each_member() {
    let (mut history, targets) = cubes(2);
    let token = Uuid::new_v4();
    let id = history
        .begin_scale_many(
            &targets,
            ScaleHandle::Y,
            ScaleSpace::Object,
            token,
            NATIVE,
            BUDGET,
        )
        .unwrap();
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    for target in &targets {
        assert_eq!(scale_at(&history, target.node), [1.0, 3.0, 1.0]);
    }
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
    history.update_move(token, id, NATIVE, 1.5).unwrap();
    history.cancel_active_move().unwrap();
    assert_eq!(history.scene, before);
    assert!(history.active_manipulation().is_none());
}

#[test]
fn an_unrepresentable_member_fails_the_preview_without_writing_any_member() {
    let (mut history, targets) = cubes(3);
    set_scale(&mut history, targets[2].node, [1e30; 3]);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.update_move(token, id, NATIVE, 1e290),
        Err(ManipulationError::Native(NativeError::UnsupportedScale(
            ScaleUnsupported::Unrepresentable
        )))
    );
    assert_eq!(history.scene, preview);
}

#[test]
fn begin_with_a_restricted_member_names_it_and_changes_nothing() {
    let (mut scene, restricted) = test_fixtures::cube();
    test_fixtures::restrict_edits(&mut scene, restricted);
    let plain = scene.create(Primitive::Cube, "plain").unwrap();
    let mut history = History::new(scene);
    let paths = history.scene.transform_paths().unwrap();
    let find = |node| paths.iter().find(|t| t.node == node).unwrap().clone();
    let targets = [find(plain), find(restricted)];
    let before = history.scene.clone();
    let name = history.scene.node(restricted).unwrap().name().to_string();
    for order in [[0, 1], [1, 0]] {
        let ordered = order.map(|index| targets[index].clone());
        assert_eq!(
            begin(&mut history, &ordered, Uuid::new_v4()),
            Err(ManipulationError::Member {
                name: name.clone(),
                source: Box::new(NativeError::UnsupportedEdit),
            })
        );
        assert!(history.active_manipulation().is_none());
        assert_eq!(history.scene, before);
    }
    assert_eq!(
        begin(&mut history, &[], Uuid::new_v4()),
        Err(ManipulationError::Native(NativeError::InvalidIdentity))
    );
    assert_eq!(history.next_manipulation_id, 0);
}

#[test]
fn a_selected_child_follows_its_selected_parent_and_gets_no_scale_of_its_own() {
    let (scene, parent, child) = test_fixtures::group_with_own_shape_and_child();
    let mut history = History::new(scene);
    let targets = [
        path_of(&history, &[parent, child]),
        path_of(&history, &[parent]),
    ];
    let child_before = scale_at(&history, child);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!((state.member_count(), state.covered_count()), (2, 1));
    history.update_move(token, id, NATIVE, 2.0).unwrap();
    assert_eq!(scale_at(&history, parent), [2.0; 3]);
    assert_eq!(scale_at(&history, child), child_before);
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    assert_eq!(history.undo.len(), 1);
}

#[test]
fn a_covered_lead_reports_its_own_channels() {
    let (scene, parent, child) = test_fixtures::group_with_own_shape_and_child();
    let mut history = History::new(scene);
    let targets = [
        path_of(&history, &[parent]),
        path_of(&history, &[parent, child]),
    ];
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    let own = scale_at(&history, child);
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.target.node, child);
    assert_eq!(state.baseline_channels, own);
    assert_eq!(state.current_channels, Some(own));
    assert_eq!(scale_at(&history, parent), [3.0; 3]);
}

#[test]
fn the_same_target_twice_is_invalid_ids_and_two_instances_are_refused() {
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
    let (scene, x1, x2, a, _) = test_fixtures::instanced_chain();
    let mut history = History::new(scene);
    let pair = [path_of(&history, &[x1, a]), path_of(&history, &[x2, a])];
    let name = history.scene.node(a).unwrap().name().to_string();
    assert_eq!(
        begin(&mut history, &pair, Uuid::new_v4()),
        Err(ManipulationError::Member {
            name,
            source: Box::new(NativeError::UnsupportedScale(
                ScaleUnsupported::SharedInstances
            )),
        })
    );
    assert!(history.active_manipulation().is_none());
}

#[test]
fn an_uncovered_member_under_a_selected_ancestor_on_another_path_is_refused() {
    let (scene, x1, x2, a, b) = test_fixtures::instanced_chain();
    let mut history = History::new(scene);
    let targets = [path_of(&history, &[x1]), path_of(&history, &[x2, a, b])];
    let name = history.scene.node(b).unwrap().name().to_string();
    let before = history.scene.clone();
    assert_eq!(
        begin(&mut history, &targets, Uuid::new_v4()),
        Err(ManipulationError::Member {
            name,
            source: Box::new(NativeError::UnsupportedScale(
                ScaleUnsupported::AlternatePathAncestor
            )),
        })
    );
    assert_eq!(history.scene, before);
}

#[test]
fn check_scale_handles_agrees_with_begin_without_starting_one() {
    let (mut scene, restricted) = test_fixtures::cube();
    test_fixtures::restrict_edits(&mut scene, restricted);
    let plain = scene.create(Primitive::Cube, "plain").unwrap();
    let history = History::new(scene);
    let targets = [
        path_of(&history, &[plain]),
        path_of(&history, &[restricted]),
    ];
    // The restricted member refuses every handle, exactly as a begin would.
    let name = history.scene.node(restricted).unwrap().name().to_string();
    assert_eq!(
        history
            .check_scale_handles(&targets, ScaleSpace::Object)
            .unwrap_err(),
        ManipulationError::Member {
            name,
            source: Box::new(NativeError::UnsupportedEdit),
        }
    );
    assert!(history.active_manipulation().is_none());
    let (clean, clean_targets) = cubes(2);
    assert_eq!(
        clean
            .check_scale_handles(&clean_targets, ScaleSpace::Object)
            .unwrap()
            .iter()
            .filter(|refusal| refusal.is_some())
            .count(),
        0
    );
}

#[test]
fn stale_token_id_and_owner_leave_a_multi_scale_untouched() {
    let (mut history, targets) = cubes(2);
    let token = Uuid::new_v4();
    let id = begin(&mut history, &targets, token).unwrap();
    history.update_move(token, id, NATIVE, 2.0).unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.update_move(Uuid::new_v4(), id, NATIVE, 3.0),
        Err(ManipulationError::StaleScene)
    );
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Remote, 3.0),
        Err(ManipulationError::WrongOwner)
    );
    assert_eq!(history.scene, preview);
    assert!(history.active_manipulation().is_some());
}

#[test]
fn a_view_budget_that_cannot_hold_every_member_is_refused_at_begin() {
    let (mut history, targets) = cubes(3);
    let one = history.begin_scale_many(
        &targets[..1],
        ScaleHandle::Uniform,
        ScaleSpace::Object,
        Uuid::new_v4(),
        NATIVE,
        0,
    );
    assert_eq!(one, Err(ManipulationError::View(ViewError::ResourceLimit)));
    assert!(history.active_manipulation().is_none());
}

#[test]
fn a_parent_space_scale_of_several_members_follows_each_members_own_parent_axes() {
    let (mut history, targets) = cubes(2);
    // The first cube is turned a quarter about Z, so its local X lies along the parent Y axis; the
    // second keeps its local axes on the parent axes.
    history
        .scene
        .set_transform(
            targets[0].node,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                ..Default::default()
            },
        )
        .unwrap();
    let token = Uuid::new_v4();
    let id = history
        .begin_scale_many(
            &targets,
            ScaleHandle::Y,
            ScaleSpace::Parent,
            token,
            NATIVE,
            BUDGET,
        )
        .unwrap();
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    let rotated = scale_at(&history, targets[0].node);
    let plain = scale_at(&history, targets[1].node);
    for (got, want) in rotated.into_iter().zip([3.0, 1.0, 1.0]) {
        assert!((got - want).abs() < 1e-9, "rotated member: {rotated:?}");
    }
    assert_eq!(plain, [1.0, 3.0, 1.0]);
}
