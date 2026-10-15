use super::*;
use crate::native_scene::{NativeTransform, binary, test_fixtures};
use crate::scene::Primitive;

mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

const BUDGET: usize = 256 * 1024 * 1024;

fn target(history: &History) -> TransformTarget {
    history
        .scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
}

fn start(history: &mut History, token: Uuid) -> MoveId {
    history
        .begin_move(
            &target(history),
            WorldAxis::X,
            token,
            MoveProvenance::Native,
            BUDGET,
        )
        .unwrap()
}

fn cube_history() -> History {
    History::new(test_fixtures::cube().0)
}

#[test]
fn many_absolute_previews_make_exactly_one_undo_and_redo() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    for distance in [0.5, 2.0, -4.0, 3.0, 3.0, -1.25] {
        history
            .update_move(token, id, MoveProvenance::Native, distance)
            .unwrap();
        assert_eq!(
            history
                .scene
                .node(target(&history).node)
                .unwrap()
                .transform()
                .translation,
            [distance, 0.0, 0.0]
        );
        assert!(history.undo.is_empty() && history.redo.is_empty());
    }
    let preview = history.scene.clone();
    assert_eq!(
        history
            .finish_move(token, id, MoveProvenance::Native, MoveFinish::Commit)
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.scene, preview);
    assert!(history.active_move().is_none());
    assert_eq!(history.undo.len(), 1);
    assert!(history.undo());
    assert_eq!(history.scene, original);
    assert!(!history.undo());
    assert!(history.redo());
    assert_eq!(history.scene, preview);
    assert!(!history.redo());
}

#[test]
fn cancel_zero_commit_and_repeated_previews_preserve_existing_redo() {
    let mut history = cube_history();
    let node = target(&history).node;
    history
        .execute(Command::Rename {
            id: node,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    let original = history.scene.clone();
    let redo = history.redo.clone();
    let token = Uuid::new_v4();
    let first = start(&mut history, token);
    history
        .update_move(token, first, MoveProvenance::Native, 7.0)
        .unwrap();
    assert_eq!(
        history
            .update_move(token, first, MoveProvenance::Native, 7.0)
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(
        history
            .finish_move(token, first, MoveProvenance::Native, MoveFinish::Cancel)
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.scene, original);
    assert_eq!(history.redo, redo);
    assert!(history.undo.is_empty());
    let second = start(&mut history, token);
    assert_ne!(first, second);
    history
        .update_move(token, second, MoveProvenance::Native, -3.0)
        .unwrap();
    history
        .update_move(token, second, MoveProvenance::Native, -0.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(token, second, MoveProvenance::Native, MoveFinish::Commit)
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(history.scene, original);
    assert_eq!(history.redo, redo);
    assert!(history.undo.is_empty());
    let third = start(&mut history, token);
    history
        .update_move(token, third, MoveProvenance::Native, 1.0)
        .unwrap();
    history
        .finish_move(token, third, MoveProvenance::Native, MoveFinish::Commit)
        .unwrap();
    assert!(history.redo.is_empty());
    assert_eq!(history.undo.len(), 1);
}

#[test]
fn active_move_blocks_every_history_mutation_before_state_or_stack_changes() {
    let mut history = History::default();
    let EditOutcome::Created(node) = history
        .execute(Command::Create {
            primitive: Primitive::Cube,
            name: "cube".into(),
        })
        .unwrap()
    else {
        panic!()
    };
    history
        .execute(Command::Rename {
            id: node,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    let shape = history.scene.shapes()[0].id();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, MoveProvenance::Native, 2.0)
        .unwrap();
    let scene = history.scene.clone();
    let undo = history.undo.clone();
    let redo = history.redo.clone();
    for command in [
        Command::Create {
            primitive: Primitive::Plane,
            name: "blocked".into(),
        },
        Command::Remove(node),
        Command::Rename {
            id: node,
            name: "blocked".into(),
        },
        Command::SetTransform {
            id: node,
            transform: NativeTransform::default(),
        },
        Command::EditNode {
            id: node,
            name: None,
            channels: [None; 9],
        },
        Command::ExtrudeFace {
            shape,
            face_index: 0,
            distance: 1.0,
        },
    ] {
        assert_eq!(history.execute(command), Err(NativeError::BusyManipulation));
        assert_eq!(history.scene, scene);
        assert_eq!(history.undo, undo);
        assert_eq!(history.redo, redo);
    }
    assert!(!history.undo());
    assert!(!history.redo());
    assert_eq!(history.scene, scene);
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
    history.cancel_active_move().unwrap();
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
}

#[test]
fn nested_stale_and_wrong_owner_requests_leave_the_matching_gesture_untouched() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, MoveProvenance::Native, 2.0)
        .unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::Y,
            token,
            MoveProvenance::Remote,
            BUDGET
        ),
        Err(MoveError::Busy)
    );
    for finish in [MoveFinish::Commit, MoveFinish::Cancel] {
        assert_eq!(
            history.finish_move(Uuid::new_v4(), id, MoveProvenance::Native, finish),
            Err(MoveError::StaleScene)
        );
        assert_eq!(
            history.finish_move(token, MoveId(0), MoveProvenance::Native, finish),
            Err(MoveError::StaleMove)
        );
        assert_eq!(
            history.finish_move(token, id, MoveProvenance::Remote, finish),
            Err(MoveError::WrongOwner)
        );
    }
    assert_eq!(
        history.update_move(Uuid::new_v4(), id, MoveProvenance::Native, 9.0),
        Err(MoveError::StaleScene)
    );
    assert_eq!(
        history.update_move(token, MoveId(0), MoveProvenance::Native, 9.0),
        Err(MoveError::StaleMove)
    );
    assert_eq!(
        history.update_move(token, id, MoveProvenance::Remote, 9.0),
        Err(MoveError::WrongOwner)
    );
    assert_eq!(history.scene, preview);
    assert_eq!(history.active_move().unwrap().distance_cm, 2.0);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    history.cancel_active_move().unwrap();
    assert_eq!(
        history.update_move(token, id, MoveProvenance::Native, 9.0),
        Err(MoveError::StaleMove)
    );
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::X,
            Uuid::new_v4(),
            MoveProvenance::Native,
            BUDGET
        ),
        Err(MoveError::StaleScene)
    );
}

#[test]
fn failed_numeric_or_view_preview_preserves_the_exact_previous_preview() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, MoveProvenance::Native, 1.25)
        .unwrap();
    let preview = history.scene.clone();
    for distance in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            history.update_move(token, id, MoveProvenance::Native, distance),
            Err(MoveError::Native(NativeError::NonFinite))
        );
        assert_eq!(history.scene, preview);
    }
    assert_eq!(
        history.update_move(token, id, MoveProvenance::Native, 1e40),
        Err(MoveError::View(ViewError::NonFiniteWorld))
    );
    assert_eq!(history.scene, preview);
    assert_eq!(history.active_move().unwrap().distance_cm, 1.25);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    history
        .finish_move(token, id, MoveProvenance::Native, MoveFinish::Commit)
        .unwrap();
    assert_eq!(history.scene, preview);
    assert_eq!(history.undo.len(), 1);
}

#[test]
fn failed_begin_and_id_exhaustion_do_not_consume_state_or_ids() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::X,
            token,
            MoveProvenance::Native,
            0
        ),
        Err(MoveError::View(ViewError::ResourceLimit))
    );
    assert_eq!(history.scene, original);
    assert!(history.active_move().is_none());
    assert_eq!(history.next_move_id, 0);
    assert!(history.move_scene_token.is_none());
    let id = start(&mut history, token);
    assert_eq!(id, MoveId(1));
    history.cancel_active_move().unwrap();
    history.next_move_id = u64::MAX;
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::X,
            token,
            MoveProvenance::Native,
            BUDGET
        ),
        Err(MoveError::IdsExhausted)
    );
    assert_eq!(history.scene, original);
    assert!(history.active_move().is_none());
    assert!(history.undo.is_empty() && history.redo.is_empty());
}

#[test]
fn source_bound_many_previews_zero_cancel_and_commit_retain_exact_scalar_intent() {
    let scene = binary::decode(wire::specimen().into()).unwrap();
    let mut history = History::new(scene.clone());
    let token = Uuid::new_v4();
    let first = start(&mut history, token);
    for distance in [1.0, -0.5, 2.0, -0.0] {
        history
            .update_move(token, first, MoveProvenance::Native, distance)
            .unwrap();
    }
    assert_eq!(history.scene, scene);
    history
        .finish_move(token, first, MoveProvenance::Native, MoveFinish::Commit)
        .unwrap();
    assert_eq!(history.scene, scene);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    let second = start(&mut history, token);
    history
        .update_move(token, second, MoveProvenance::Native, -1.0)
        .unwrap();
    history
        .finish_move(token, second, MoveProvenance::Native, MoveFinish::Cancel)
        .unwrap();
    assert_eq!(history.scene, scene);
    let third = start(&mut history, token);
    history
        .update_move(token, third, MoveProvenance::Native, 1.0)
        .unwrap();
    let final_scene = history.scene.clone();
    history
        .finish_move(token, third, MoveProvenance::Native, MoveFinish::Commit)
        .unwrap();
    assert_eq!(history.scene, final_scene);
    assert_eq!(history.undo.len(), 1);
    assert!(history.undo());
    assert_eq!(history.scene, scene);
    assert!(history.redo());
    assert_eq!(history.scene, final_scene);
}

#[test]
fn source_mismatch_refuses_preview_and_finish_without_restoring_into_another_source() {
    let scene = binary::decode(wire::specimen().into()).unwrap();
    let mut history = History::new(scene.clone());
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, MoveProvenance::Native, 1.0)
        .unwrap();
    let previous = history.scene.clone();
    history.scene = binary::decode(wire::specimen().into()).unwrap();
    let changed_identity = history.scene.clone();
    assert_eq!(
        history.update_move(token, id, MoveProvenance::Native, 2.0),
        Err(MoveError::Native(NativeError::InvalidRetention))
    );
    assert_eq!(
        history.finish_move(token, id, MoveProvenance::Native, MoveFinish::Commit),
        Err(MoveError::Native(NativeError::InvalidRetention))
    );
    assert_eq!(
        history.cancel_active_move(),
        Err(MoveError::Native(NativeError::InvalidRetention))
    );
    assert_eq!(history.scene, changed_identity);
    assert_eq!(history.active_move().unwrap().id, id);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    history.scene = previous;
    history.cancel_active_move().unwrap();
    assert_eq!(history.scene, scene);
}

#[test]
fn source_bound_prior_authored_baseline_retains_signed_zero_and_both_history_stacks() {
    let bytes = wire::scene(
        &[
            wire::group(
                b"XFRM",
                &[
                    wire::creation(b"object", None),
                    wire::triple(b"t", [10.0, -0.0, 3.0]),
                ]
                .concat(),
                4,
            ),
            wire::group(
                b"DMSH",
                &[
                    wire::creation(b"shape", Some(b"object")),
                    wire::chunk(b"MESH", &wire::mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    );
    let mut history = History::new(binary::decode(bytes).unwrap());
    let node = target(&history).node;
    let edit_x = [Some(12.5), None, None, None, None, None, None, None, None];
    history
        .execute(Command::EditNode {
            id: node,
            name: None,
            channels: edit_x,
        })
        .unwrap();
    history
        .execute(Command::EditNode {
            id: node,
            name: None,
            channels: [None, None, Some(4.0), None, None, None, None, None, None],
        })
        .unwrap();
    assert!(history.undo());
    let authored = history.scene.clone();
    let undo = history.undo.clone();
    let redo = history.redo.clone();
    assert_eq!(
        authored
            .node(node)
            .unwrap()
            .transform()
            .translation
            .map(f64::to_bits),
        [12.5, -0.0, 3.0].map(f64::to_bits)
    );
    assert_eq!(undo.len(), 1);
    assert_eq!(redo.len(), 1);
    let token = Uuid::new_v4();
    let first = start(&mut history, token);
    history
        .update_move(token, first, MoveProvenance::Native, 2.0)
        .unwrap();
    history
        .update_move(token, first, MoveProvenance::Native, -0.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(token, first, MoveProvenance::Native, MoveFinish::Commit)
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(history.scene, authored);
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
    let second = start(&mut history, token);
    history
        .update_move(token, second, MoveProvenance::Native, -4.0)
        .unwrap();
    history
        .finish_move(token, second, MoveProvenance::Native, MoveFinish::Cancel)
        .unwrap();
    assert_eq!(history.scene, authored);
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
    let third = start(&mut history, token);
    history
        .update_move(token, third, MoveProvenance::Native, 1.25)
        .unwrap();
    history
        .finish_move(token, third, MoveProvenance::Native, MoveFinish::Commit)
        .unwrap();
    assert_eq!(history.undo.len(), undo.len() + 1);
    assert!(history.redo.is_empty());
    assert!(history.undo());
    assert_eq!(history.scene, authored);
    assert_eq!(history.undo, undo);
}

#[test]
fn remote_owned_move_can_only_finish_by_remote_owner_or_human_cancellation() {
    let mut history = cube_history();
    let scene = history.scene.clone();
    let token = Uuid::new_v4();
    let id = history
        .begin_move(
            &target(&history),
            WorldAxis::Z,
            token,
            MoveProvenance::Remote,
            BUDGET,
        )
        .unwrap();
    history
        .update_move(token, id, MoveProvenance::Remote, -2.0)
        .unwrap();
    assert_eq!(
        history.finish_move(token, id, MoveProvenance::Native, MoveFinish::Commit),
        Err(MoveError::WrongOwner)
    );
    assert_eq!(history.active_move().unwrap().axis, WorldAxis::Z);
    assert_eq!(
        history.active_move().unwrap().provenance,
        MoveProvenance::Remote
    );
    history.cancel_active_move().unwrap();
    assert_eq!(history.scene, scene);
    assert_eq!(
        history.cancel_active_move().unwrap(),
        EditOutcome::Unchanged
    );
}

#[test]
fn new_history_and_fresh_scene_token_reject_the_old_token_id_pair() {
    let mut history = cube_history();
    let old_token = Uuid::new_v4();
    let old_id = start(&mut history, old_token);
    let mut replacement = cube_history();
    let token = Uuid::new_v4();
    let id = start(&mut replacement, token);
    assert_eq!(old_id, id);
    assert_eq!(
        replacement.update_move(old_token, old_id, MoveProvenance::Native, 3.0),
        Err(MoveError::StaleScene)
    );
    assert_eq!(
        replacement.finish_move(
            old_token,
            old_id,
            MoveProvenance::Native,
            MoveFinish::Commit
        ),
        Err(MoveError::StaleScene)
    );
    replacement.cancel_active_move().unwrap();
}
