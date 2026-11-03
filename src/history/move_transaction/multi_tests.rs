use super::*;
use crate::native_scene::{NativeTransform, WorldMoveUnsupported, test_fixtures};
use crate::scene::{NodeId, Primitive};

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
