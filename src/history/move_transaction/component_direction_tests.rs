use super::component_tests::{NATIVE, WORLD_X, begin, id_of, points, set_of};
use super::*;
use crate::native_scene::{NativeTransform, test_fixtures};
use glam::DVec3;

pub(super) fn skewed(mut scene: NativeScene) -> History {
    let node = scene.transform_paths().unwrap()[0].node;
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [3.0, -2.0, 1.0],
                rotation: [30.0, 45.0, 60.0],
                scale: [2.0, 0.5, 3.0],
            },
        )
        .unwrap();
    History::new(scene)
}

#[test]
fn only_the_listed_vertices_move_along_the_world_axis() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[1, 5]);
    let before: Vec<_> = (0..8)
        .map(|i| {
            history.scene.shapes()[0]
                .mesh()
                .effective_point_cm(i)
                .unwrap()
        })
        .collect();
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 4.0).unwrap();
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    for (i, old) in before.iter().enumerate() {
        let new = history.scene.shapes()[0]
            .mesh()
            .effective_point_cm(i)
            .unwrap();
        if [1, 5].contains(&i) {
            assert_eq!(new, [f64::from((old[0] + 4.0) as f32), old[1], old[2]]);
        } else {
            assert_eq!(&new, old);
        }
    }
}

#[test]
fn the_committed_points_equal_the_last_preview_bit_for_bit() {
    let fixtures: [(&str, NativeScene); 2] = [
        ("f32", test_fixtures::cube().0),
        (
            "f64",
            test_fixtures::polygon_shape(
                vec![[0.1, 0.2, 0.3], [1.7, -0.3, 2.9], [-4.1, 8.3, 0.7]],
                &[vec![0, 1, 2]],
            )
            .0,
        ),
    ];
    for (name, scene) in fixtures {
        let mut history = skewed(scene);
        let token = Uuid::new_v4();
        let set = set_of(&history, &[0, 2]);
        let original = points(&history);
        for direction in [
            WORLD_X,
            MoveDirection::Axis {
                axis: WorldAxis::Y,
                space: MoveSpace::Object,
            },
            MoveDirection::Direction(DVec3::new(0.3, -1.0, 2.0)),
        ] {
            begin(&mut history, &set, direction, token);
            let id = id_of(&history);
            history.update_move(token, id, NATIVE, 9.0).unwrap();
            history.update_move(token, id, NATIVE, 0.1).unwrap();
            history.update_move(token, id, NATIVE, 7.3).unwrap();
            let previewed = points(&history);
            history
                .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
                .unwrap();
            assert_eq!(points(&history), previewed, "{name} {direction:?}");
            assert!(history.undo());
            assert_eq!(points(&history), original, "{name} {direction:?}");
        }
    }
}

#[test]
fn a_direction_moves_along_its_unit_vector_in_the_world() {
    let mut history = skewed(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[2]);
    let target = history.scene.transform_paths().unwrap().remove(0);
    let frame = history.scene.component_move_frame(&target).unwrap();
    let before = history.scene.shapes()[0]
        .mesh()
        .effective_point_cm(2)
        .unwrap();
    let direction = DVec3::new(0.0, 3.0, 4.0);
    begin(
        &mut history,
        &set,
        MoveDirection::Direction(direction),
        token,
    );
    let id = id_of(&history);
    assert_eq!(
        history.active_manipulation().unwrap().kind,
        ManipulationKindState::MoveComponents {
            axis: None,
            space: None
        }
    );
    history.update_move(token, id, NATIVE, 10.0).unwrap();
    let after = history.scene.shapes()[0]
        .mesh()
        .effective_point_cm(2)
        .unwrap();
    let moved = frame.world_point_cm(after) - frame.world_point_cm(before);
    assert!((moved - DVec3::new(0.0, 6.0, 8.0)).abs().max_element() < 1e-3);
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
        .unwrap();
}

#[test]
fn object_axes_follow_the_node_and_world_axes_do_not() {
    let mut history = skewed(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let set = set_of(&history, &[6]);
    let target = history.scene.transform_paths().unwrap().remove(0);
    let frame = history.scene.component_move_frame(&target).unwrap();
    let axes = history.scene.component_move_object_axes(&target).unwrap();
    let before = history.scene.shapes()[0]
        .mesh()
        .effective_point_cm(6)
        .unwrap();
    for (direction, expected) in [
        (
            MoveDirection::Axis {
                axis: WorldAxis::Z,
                space: MoveSpace::Object,
            },
            axes[2] * 5.0,
        ),
        (
            MoveDirection::Axis {
                axis: WorldAxis::Z,
                space: MoveSpace::World,
            },
            DVec3::Z * 5.0,
        ),
    ] {
        begin(&mut history, &set, direction, token);
        let id = id_of(&history);
        history.update_move(token, id, NATIVE, 5.0).unwrap();
        let after = history.scene.shapes()[0]
            .mesh()
            .effective_point_cm(6)
            .unwrap();
        let moved = frame.world_point_cm(after) - frame.world_point_cm(before);
        assert!(
            (moved - expected).abs().max_element() < 1e-3,
            "{direction:?}"
        );
        history
            .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
            .unwrap();
    }
}

#[test]
fn cancel_and_a_zero_move_restore_the_bytes_and_leave_redo_alone() {
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let original = points(&history);
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 1.0).unwrap();
    history
        .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
        .unwrap();
    assert!(history.undo());
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Cancel),
        Ok(EditOutcome::Changed)
    );
    assert_eq!(points(&history), original);
    assert!(history.redo(), "the cancelled drag kept the redo entry");
    assert!(history.undo());
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 3.0).unwrap();
    history.update_move(token, id, NATIVE, -0.0).unwrap();
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Commit),
        Ok(EditOutcome::Unchanged)
    );
    assert_eq!(points(&history), original);
    assert_eq!(history.undo_len(), 0);
}

fn identity_double_history() -> History {
    History::new(
        test_fixtures::polygon_shape(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[vec![0, 1, 2]],
        )
        .0,
    )
}

fn first_point(history: &History) -> [f64; 3] {
    match points(history) {
        crate::native_scene::Points::Double3(points) => points[0],
        crate::native_scene::Points::Float3(_) => unreachable!("a double mesh"),
    }
}

#[test]
fn an_identity_world_drag_commits_the_exact_distance_and_the_last_preview() {
    let mut history = identity_double_history();
    let token = Uuid::new_v4();
    let set = set_of(&history, &[0]);
    for k in 1..=400 {
        let distance = f64::from(k) * 0.0137 - 2.0;
        begin(&mut history, &set, WORLD_X, token);
        let id = id_of(&history);
        history.update_move(token, id, NATIVE, distance).unwrap();
        let previewed = points(&history);
        history
            .finish_move(token, id, NATIVE, ManipulationFinish::Commit)
            .unwrap();
        assert_eq!(points(&history), previewed, "k {k}");
        assert_eq!(
            first_point(&history)[0].to_bits(),
            distance.to_bits(),
            "k {k}"
        );
        assert!(history.undo());
    }
}

#[test]
fn extreme_direction_vectors_still_move_by_the_distance() {
    for vector in [
        DVec3::new(1e308, -1e308, 1e308),
        DVec3::new(5e-324, 0.0, 0.0),
        DVec3::new(0.0, 0.0, -1e-310),
    ] {
        let mut history = identity_double_history();
        let token = Uuid::new_v4();
        let set = set_of(&history, &[0]);
        begin(&mut history, &set, MoveDirection::Direction(vector), token);
        let id = id_of(&history);
        history.update_move(token, id, NATIVE, 10.0).unwrap();
        let moved = DVec3::from_array(first_point(&history));
        assert!((moved.length() - 10.0).abs() < 1e-9, "{vector:?}: {moved}");
        let unit = vector / vector.abs().max_element();
        assert!(
            moved.normalize().dot(unit.normalize()) > 1.0 - 1e-12,
            "{vector:?}"
        );
        history
            .finish_move(token, id, NATIVE, ManipulationFinish::Cancel)
            .unwrap();
    }
}

#[test]
fn a_delta_that_rounds_away_in_the_stored_precision_commits_nothing() {
    // The cube's f32 corner sits near 0.5 where a 1e-8 cm step is under half a ulp.
    let mut history = History::new(test_fixtures::cube().0);
    let token = Uuid::new_v4();
    let original = points(&history);
    let set = set_of(&history, &[0]);
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 1e-8).unwrap();
    assert_eq!(points(&history), original, "the step rounded away");
    // A cancel that restores identical bytes reports no change, and so does a commit.
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Cancel),
        Ok(EditOutcome::Unchanged)
    );
    begin(&mut history, &set, WORLD_X, token);
    let id = id_of(&history);
    history.update_move(token, id, NATIVE, 1e-8).unwrap();
    assert_eq!(
        history.finish_move(token, id, NATIVE, ManipulationFinish::Commit),
        Ok(EditOutcome::Unchanged)
    );
    assert_eq!(history.undo_len(), 0);
    assert_eq!(points(&history), original);
}
