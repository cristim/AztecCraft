use azteccraft::{
    history::{Command, EditOutcome, History},
    native_scene::{NativeError, NativeScene},
    scene::{Primitive, Scene},
};

fn cube_history() -> (History, azteccraft::native_scene::NativeId) {
    let mut scene = Scene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let native = NativeScene::from_legacy(scene).unwrap();
    let shape = native.shapes()[0].id();
    (History::new(native), shape)
}

fn points(history: &History) -> Vec<[f64; 3]> {
    let mesh = history.scene().shapes()[0].mesh();
    (0..mesh.point_count())
        .map(|i| mesh.effective_point_cm(i).unwrap())
        .collect()
}

#[test]
fn moves_the_listed_vertices_together_and_undo_redo_restore_the_scene() {
    let (mut history, shape) = cube_history();
    let before = history.scene().clone();
    let start = points(&history);
    let outcome = history
        .execute(Command::MoveVertices {
            shape,
            vertices: vec![0, 3, 7],
            delta: [1.0, 2.0, 3.0],
        })
        .unwrap();
    assert_eq!(outcome, EditOutcome::Changed);
    let moved = points(&history);
    for (i, (a, b)) in start.iter().zip(&moved).enumerate() {
        if [0, 3, 7].contains(&i) {
            assert_eq!(*b, [a[0] + 1.0, a[1] + 2.0, a[2] + 3.0]);
        } else {
            assert_eq!(a, b);
        }
    }
    assert_eq!(history.undo_label().as_deref(), Some("Move"));
    let after = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
    assert!(history.redo());
    assert_eq!(history.scene(), &after);
}

#[test]
fn a_zero_delta_changes_nothing_and_adds_no_undo_step() {
    let (mut history, shape) = cube_history();
    let outcome = history
        .execute(Command::MoveVertices {
            shape,
            vertices: vec![1],
            delta: [0.0; 3],
        })
        .unwrap();
    assert_eq!(outcome, EditOutcome::Unchanged);
    assert_eq!(history.undo_len(), 0);
}

#[test]
fn invalid_requests_are_refused_and_leave_scene_and_history_untouched() {
    let (mut history, shape) = cube_history();
    let before = history.scene().clone();
    let missing = azteccraft::native_scene::NativeId::from_decimal("999999").unwrap();
    for (shape, vertices, delta, expected) in [
        (shape, vec![], [1.0; 3], NativeError::InvalidTopology),
        (shape, vec![8], [1.0; 3], NativeError::InvalidTopology),
        (
            shape,
            vec![usize::MAX],
            [1.0; 3],
            NativeError::InvalidTopology,
        ),
        (shape, vec![2, 2], [1.0; 3], NativeError::InvalidTopology),
        (shape, vec![0], [f64::NAN, 0.0, 0.0], NativeError::NonFinite),
        (
            shape,
            vec![0],
            [f64::INFINITY, 0.0, 0.0],
            NativeError::NonFinite,
        ),
        (shape, vec![0], [1e300, 0.0, 0.0], NativeError::NonFinite),
        (
            missing,
            vec![0],
            [1.0; 3],
            NativeError::MissingShape(missing),
        ),
    ] {
        let error = history
            .execute(Command::MoveVertices {
                shape,
                vertices,
                delta,
            })
            .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(history.scene(), &before);
        assert_eq!(history.undo_len(), 0);
    }
}

#[test]
fn a_move_applies_to_every_vertex_or_none() {
    let (mut history, shape) = cube_history();
    let before = history.scene().clone();
    // The second index is out of range, so the valid first vertex must not move.
    assert!(
        history
            .execute(Command::MoveVertices {
                shape,
                vertices: vec![0, 99],
                delta: [1.0; 3],
            })
            .is_err()
    );
    assert_eq!(history.scene(), &before);
}
