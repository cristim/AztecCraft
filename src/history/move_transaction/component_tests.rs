use super::*;
use crate::native_scene::{Points, VertexSetTarget, test_fixtures};

pub(super) const BUDGET: usize = 256 * 1024 * 1024;
pub(super) const NATIVE: ManipulationProvenance = ManipulationProvenance::Native;
pub(super) const WORLD_X: MoveDirection = MoveDirection::Axis {
    axis: WorldAxis::X,
    space: MoveSpace::World,
};

/// The vertices of the first shape, addressed through its owning transform instance.
pub(super) fn set_of(history: &History, vertices: &[usize]) -> VertexSetTarget {
    let shape = history.scene.shapes()[0].id();
    let owner = history
        .scene
        .dag_edges()
        .iter()
        .find(|edge| edge.child() == shape)
        .and_then(|edge| edge.parent())
        .unwrap();
    let target = history
        .scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.path.last() == Some(&owner))
        .unwrap();
    let mut path = target.path;
    path.push(shape);
    VertexSetTarget {
        transform: target.node,
        shape,
        path,
        vertex_indices: vertices.to_vec(),
    }
}

pub(super) fn points(history: &History) -> Points {
    history.scene.shapes()[0].mesh().points().clone()
}

pub(super) fn begin(
    history: &mut History,
    set: &VertexSetTarget,
    direction: MoveDirection,
    token: Uuid,
) {
    history
        .begin_move_components(set, direction, token, NATIVE, BUDGET)
        .unwrap();
}

pub(super) fn id_of(history: &History) -> ManipulationId {
    history.active_manipulation().unwrap().id
}

#[test]
fn a_drag_previews_without_an_undo_step_and_commits_one_move() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let original = points(&history);
    let set = set_of(&history, &[0, 3]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    assert_eq!(
        history.update_move(token, id, NATIVE, 2.5),
        Ok(EditOutcome::Changed)
    );
    let previewed = points(&history);
    assert_ne!(previewed, original);
    assert_eq!(history.undo_len(), 0);
    // The same distance again changes nothing.
    assert_eq!(
        history.update_move(token, id, NATIVE, 2.5),
        Ok(EditOutcome::Unchanged)
    );
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Commit),
        Ok(EditOutcome::Changed)
    );
    assert_eq!(points(&history), previewed);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo_label().as_deref(), Some("Move"));
    assert!(history.active_manipulation().is_none());
    assert!(history.undo());
    assert_eq!(points(&history), original);
    assert!(history.redo());
    assert_eq!(points(&history), previewed);
}
