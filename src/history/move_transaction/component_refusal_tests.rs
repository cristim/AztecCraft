use super::component_tests::{BUDGET, NATIVE, WORLD_X, begin, id_of, points, set_of};
use super::*;
use crate::native_scene::{NativeTransform, VertexSetTarget, binary, test_fixtures};
use glam::DVec3;

mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[test]
fn an_active_drag_is_busy_to_every_other_edit_and_to_a_second_begin() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    assert!(history.active_manipulation().is_some());
    assert_eq!(
        history.execute(Command::MoveVertices {
            shape: set.shape,
            vertices: vec![1],
            delta_cm: [1.0, 0.0, 0.0],
        }),
        Err(NativeError::BusyManipulation)
    );
    assert!(!history.undo());
    assert!(!history.redo());
    assert_eq!(
        history.begin_move_components(&set, WORLD_X, token, NATIVE, BUDGET),
        Err(ManipulationError::Busy)
    );
}

#[test]
fn update_and_finish_check_token_id_and_owner() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    let remote = ManipulationProvenance::Remote;
    assert_eq!(
        history.update_move(Uuid::new_v4(), id, NATIVE, 1.0),
        Err(ManipulationError::StaleScene)
    );
    assert_eq!(
        history.update_move(token, id, remote, 1.0),
        Err(ManipulationError::WrongOwner)
    );
    assert_eq!(
        history.finish_move(token, id, remote, ManipulationFinish::Commit),
        Err(ManipulationError::WrongOwner)
    );
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
        .unwrap();
    assert_eq!(
        history.update_move(token, id, NATIVE, 1.0),
        Err(ManipulationError::StaleManipulation)
    );
}

#[test]
fn begin_refusals_leave_no_active_manipulation() {
    let token = Uuid::new_v4();
    let mut history = History::new(test_fixtures::cube().0);
    let set = set_of(&history, &[0]);
    for space in [MoveSpace::Parent, MoveSpace::AlongRotationAxis] {
        assert_eq!(
            history.begin_move_components(
                &set,
                MoveDirection::Axis {
                    axis: WorldAxis::X,
                    space
                },
                token,
                NATIVE,
                BUDGET
            ),
            Err(ManipulationError::Native(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ComponentAxisOrientation
            )))
        );
    }
    for vector in [
        DVec3::ZERO,
        DVec3::new(f64::NAN, 0.0, 1.0),
        DVec3::new(f64::INFINITY, 0.0, 0.0),
    ] {
        assert_eq!(
            history.begin_move_components(
                &set,
                MoveDirection::Direction(vector),
                token,
                NATIVE,
                BUDGET
            ),
            Err(ManipulationError::Native(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ComponentDirection
            ))),
            "{vector:?}"
        );
    }
    for vertices in [vec![], vec![8], vec![1, 1]] {
        let bad = VertexSetTarget {
            vertex_indices: vertices,
            ..set.clone()
        };
        assert!(
            history
                .begin_move_components(&bad, WORLD_X, token, NATIVE, BUDGET)
                .is_err()
        );
    }
    // A zero budget cannot hold the drag.
    assert!(matches!(
        history.begin_move_components(&set, WORLD_X, token, NATIVE, 0),
        Err(ManipulationError::View(_))
    ));
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.undo_len(), 0);
}

#[test]
fn a_shape_outside_the_named_owner_is_refused() {
    let (scene, root, _child, shape) = test_fixtures::grouped_shape();
    let mut history = History::new(scene);
    let owner = history
        .scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.node == root)
        .unwrap();
    // The child's shape named under the root's path.
    let mut path = owner.path.clone();
    path.push(shape);
    let set = VertexSetTarget {
        transform: root,
        shape,
        path,
        vertex_indices: vec![0],
    };
    assert!(
        history
            .begin_move_components(&set, WORLD_X, Uuid::new_v4(), NATIVE, BUDGET)
            .is_err()
    );
    assert!(history.active_manipulation().is_none());
}

#[test]
fn a_scene_opened_from_a_file_refuses_at_begin() {
    let scene = binary::decode(wire::specimen().into()).unwrap();
    let mut history = History::new(scene);
    let set = set_of(&history, &[0]);
    assert_eq!(
        history.begin_move_components(&set, WORLD_X, Uuid::new_v4(), NATIVE, BUDGET),
        Err(ManipulationError::Native(NativeError::UnsupportedEdit))
    );
    assert!(history.active_manipulation().is_none());
}

#[test]
fn a_non_finite_distance_is_refused_and_keeps_the_preview() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 2.0).unwrap();
    let held = points(&history);
    for distance in [f64::NAN, f64::INFINITY, 1e300] {
        assert_eq!(
            history.update_move(token, id, NATIVE, distance),
            Err(ManipulationError::Native(NativeError::NonFinite))
        );
        assert_eq!(points(&history), held);
    }
}

#[test]
fn a_sample_the_view_refuses_rolls_back_to_the_previous_preview() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                scale: [1e30; 3],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let mut history = History::new(scene);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 1e30).unwrap();
    let held = points(&history);
    // A local delta of 1e9 is fine, but the world coordinate 1e39 leaves f32.
    assert!(matches!(
        history.update_move(token, id, NATIVE, 1e39),
        Err(ManipulationError::View(_))
    ));
    assert_eq!(points(&history), held);
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
        .unwrap();
}
