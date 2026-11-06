use super::*;
use crate::native_scene::{
    NativeTransform, RotateAxis, RotateSpace, RotateUnsupported, ScaleHandle, ScaleSpace,
    ScaleUnsupported, binary, test_fixtures,
};
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

fn start(history: &mut History, token: Uuid) -> ManipulationId {
    history
        .begin_move(
            &target(history),
            WorldAxis::X,
            token,
            ManipulationProvenance::Native,
            BUDGET,
        )
        .unwrap()
}

fn cube_history() -> History {
    History::new(test_fixtures::cube().0)
}

#[test]
fn every_manipulation_write_to_the_scene_changes_the_revision() {
    let token = Uuid::new_v4();
    let finish = |finish| {
        let mut history = cube_history();
        let id = start(&mut history, token);
        let begun = history.revision();
        history
            .update_move(token, id, ManipulationProvenance::Native, 2.0)
            .unwrap();
        let previewed = history.revision();
        assert_ne!(previewed, begun);
        history
            .finish_move(token, id, ManipulationProvenance::Native, finish)
            .unwrap();
        assert_ne!(history.revision(), previewed);
    };
    finish(ManipulationFinish::Commit);
    finish(ManipulationFinish::Cancel);
    let mut history = cube_history();
    let id = start(&mut history, token);
    history
        .update_move(token, id, ManipulationProvenance::Native, 2.0)
        .unwrap();
    let previewed = history.revision();
    history.cancel_active_move().unwrap();
    assert_ne!(history.revision(), previewed);
}

#[test]
fn many_absolute_previews_make_exactly_one_undo_and_redo() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    for distance in [0.5, 2.0, -4.0, 3.0, 3.0, -1.25] {
        history
            .update_move(token, id, ManipulationProvenance::Native, distance)
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
            .finish_move(
                token,
                id,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.scene, preview);
    assert!(history.active_manipulation().is_none());
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
        .update_move(token, first, ManipulationProvenance::Native, 7.0)
        .unwrap();
    assert_eq!(
        history
            .update_move(token, first, ManipulationProvenance::Native, 7.0)
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(
        history
            .finish_move(
                token,
                first,
                ManipulationProvenance::Native,
                ManipulationFinish::Cancel
            )
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.scene, original);
    assert_eq!(history.redo, redo);
    assert!(history.undo.is_empty());
    let second = start(&mut history, token);
    assert_ne!(first, second);
    history
        .update_move(token, second, ManipulationProvenance::Native, -3.0)
        .unwrap();
    history
        .update_move(token, second, ManipulationProvenance::Native, -0.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(
                token,
                second,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(history.scene, original);
    assert_eq!(history.redo, redo);
    assert!(history.undo.is_empty());
    let third = start(&mut history, token);
    history
        .update_move(token, third, ManipulationProvenance::Native, 1.0)
        .unwrap();
    history
        .finish_move(
            token,
            third,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit,
        )
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
            options: None,
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
        .update_move(token, id, ManipulationProvenance::Native, 2.0)
        .unwrap();
    let scene = history.scene.clone();
    let undo = history.undo.clone();
    let redo = history.redo.clone();
    for command in [
        Command::Create {
            primitive: Primitive::Plane,
            options: None,
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
        Command::SetMaterialDiffuse { shape, value: 0.25 },
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
        .update_move(token, id, ManipulationProvenance::Native, 2.0)
        .unwrap();
    let preview = history.scene.clone();
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::Y,
            token,
            ManipulationProvenance::Remote,
            BUDGET
        ),
        Err(ManipulationError::Busy)
    );
    for finish in [ManipulationFinish::Commit, ManipulationFinish::Cancel] {
        assert_eq!(
            history.finish_move(Uuid::new_v4(), id, ManipulationProvenance::Native, finish),
            Err(ManipulationError::StaleScene)
        );
        assert_eq!(
            history.finish_move(
                token,
                ManipulationId(0),
                ManipulationProvenance::Native,
                finish
            ),
            Err(ManipulationError::StaleManipulation)
        );
        assert_eq!(
            history.finish_move(token, id, ManipulationProvenance::Remote, finish),
            Err(ManipulationError::WrongOwner)
        );
    }
    assert_eq!(
        history.update_move(Uuid::new_v4(), id, ManipulationProvenance::Native, 9.0),
        Err(ManipulationError::StaleScene)
    );
    assert_eq!(
        history.update_move(
            token,
            ManipulationId(0),
            ManipulationProvenance::Native,
            9.0
        ),
        Err(ManipulationError::StaleManipulation)
    );
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Remote, 9.0),
        Err(ManipulationError::WrongOwner)
    );
    assert_eq!(history.scene, preview);
    assert_eq!(
        history
            .active_manipulation()
            .unwrap()
            .move_distance_cm()
            .unwrap(),
        2.0
    );
    assert!(history.undo.is_empty() && history.redo.is_empty());
    history.cancel_active_move().unwrap();
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Native, 9.0),
        Err(ManipulationError::StaleManipulation)
    );
}

#[test]
fn failed_numeric_or_view_preview_preserves_the_exact_previous_preview() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, ManipulationProvenance::Native, 1.25)
        .unwrap();
    let preview = history.scene.clone();
    for distance in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            history.update_move(token, id, ManipulationProvenance::Native, distance),
            Err(ManipulationError::Native(NativeError::NonFinite))
        );
        assert_eq!(history.scene, preview);
    }
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Native, 1e40),
        Err(ManipulationError::View(ViewError::NonFiniteWorld))
    );
    assert_eq!(history.scene, preview);
    assert_eq!(
        history
            .active_manipulation()
            .unwrap()
            .move_distance_cm()
            .unwrap(),
        1.25
    );
    assert!(history.undo.is_empty() && history.redo.is_empty());
    history
        .finish_move(
            token,
            id,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit,
        )
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
            ManipulationProvenance::Native,
            0
        ),
        Err(ManipulationError::View(ViewError::ResourceLimit))
    );
    assert_eq!(history.scene, original);
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.next_manipulation_id, 0);
    let id = start(&mut history, token);
    assert_eq!(id, ManipulationId(1));
    history.cancel_active_move().unwrap();
    history.next_manipulation_id = u64::MAX;
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::X,
            token,
            ManipulationProvenance::Native,
            BUDGET
        ),
        Err(ManipulationError::IdsExhausted)
    );
    assert_eq!(history.scene, original);
    assert!(history.active_manipulation().is_none());
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
            .update_move(token, first, ManipulationProvenance::Native, distance)
            .unwrap();
    }
    assert_eq!(history.scene, scene);
    history
        .finish_move(
            token,
            first,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit,
        )
        .unwrap();
    assert_eq!(history.scene, scene);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    let second = start(&mut history, token);
    history
        .update_move(token, second, ManipulationProvenance::Native, -1.0)
        .unwrap();
    history
        .finish_move(
            token,
            second,
            ManipulationProvenance::Native,
            ManipulationFinish::Cancel,
        )
        .unwrap();
    assert_eq!(history.scene, scene);
    let third = start(&mut history, token);
    history
        .update_move(token, third, ManipulationProvenance::Native, 1.0)
        .unwrap();
    let final_scene = history.scene.clone();
    history
        .finish_move(
            token,
            third,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit,
        )
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
        .update_move(token, id, ManipulationProvenance::Native, 1.0)
        .unwrap();
    history.scene = binary::decode(wire::specimen().into()).unwrap();
    let changed_identity = history.scene.clone();
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Native, 2.0),
        Err(ManipulationError::Native(NativeError::InvalidRetention))
    );
    assert_eq!(
        history.finish_move(
            token,
            id,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit
        ),
        Err(ManipulationError::Native(NativeError::InvalidRetention))
    );
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, changed_identity);
    assert!(history.undo.is_empty() && history.redo.is_empty());
    assert_eq!(history.cancel_active_move(), Ok(EditOutcome::Unchanged));
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Native, 2.0),
        Err(ManipulationError::StaleManipulation)
    );
}

#[test]
fn failed_cancel_still_clears_the_active_move_so_a_new_move_can_begin() {
    let scene = binary::decode(wire::specimen().into()).unwrap();
    let mut history = History::new(scene);
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, ManipulationProvenance::Native, 1.0)
        .unwrap();
    history.scene = binary::decode(wire::specimen().into()).unwrap();
    let changed_identity = history.scene.clone();
    assert_eq!(
        history.cancel_active_move(),
        Err(ManipulationError::Native(NativeError::InvalidRetention))
    );
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.scene, changed_identity);
    assert_eq!(
        start(&mut history, token),
        ManipulationId(2),
        "the failed cancel must not leave the transaction owning the history"
    );
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
        .update_move(token, first, ManipulationProvenance::Native, 2.0)
        .unwrap();
    history
        .update_move(token, first, ManipulationProvenance::Native, -0.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(
                token,
                first,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(history.scene, authored);
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
    let second = start(&mut history, token);
    history
        .update_move(token, second, ManipulationProvenance::Native, -4.0)
        .unwrap();
    history
        .finish_move(
            token,
            second,
            ManipulationProvenance::Native,
            ManipulationFinish::Cancel,
        )
        .unwrap();
    assert_eq!(history.scene, authored);
    assert_eq!(history.undo, undo);
    assert_eq!(history.redo, redo);
    let third = start(&mut history, token);
    history
        .update_move(token, third, ManipulationProvenance::Native, 1.25)
        .unwrap();
    history
        .finish_move(
            token,
            third,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit,
        )
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
            ManipulationProvenance::Remote,
            BUDGET,
        )
        .unwrap();
    history
        .update_move(token, id, ManipulationProvenance::Remote, -2.0)
        .unwrap();
    assert_eq!(
        history.finish_move(
            token,
            id,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit
        ),
        Err(ManipulationError::WrongOwner)
    );
    assert_eq!(
        history.active_manipulation().unwrap().move_axis().unwrap(),
        WorldAxis::Z
    );
    assert_eq!(
        history.active_manipulation().unwrap().provenance,
        ManipulationProvenance::Remote
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
        replacement.update_move(old_token, old_id, ManipulationProvenance::Native, 3.0),
        Err(ManipulationError::StaleScene)
    );
    assert_eq!(
        replacement.finish_move(
            old_token,
            old_id,
            ManipulationProvenance::Native,
            ManipulationFinish::Commit
        ),
        Err(ManipulationError::StaleScene)
    );
    replacement.cancel_active_move().unwrap();
}

fn start_rotate(history: &mut History, axis: RotateAxis, token: Uuid) -> ManipulationId {
    history
        .begin_rotate(
            &target(history),
            axis,
            RotateSpace::Object,
            token,
            ManipulationProvenance::Native,
            BUDGET,
        )
        .unwrap()
}

fn rotation(history: &History) -> [f64; 3] {
    history
        .scene
        .node(target(history).node)
        .unwrap()
        .transform()
        .rotation
}

#[test]
fn rotate_previews_make_exactly_one_undo_and_leave_other_channels_alone() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    let id = start_rotate(&mut history, RotateAxis::Z, token);
    let active = history.active_manipulation().unwrap();
    assert_eq!(
        active.kind,
        ManipulationKindState::Rotate {
            axis: RotateAxis::Z,
            space: RotateSpace::Object
        }
    );
    assert_eq!((active.parameter, active.move_axis()), (0.0, None));
    for degrees in [10.0, -35.0, 80.0] {
        history
            .update_move(token, id, ManipulationProvenance::Native, degrees)
            .unwrap();
        assert!((rotation(&history)[2] - degrees).abs() < 1e-9);
        assert!(history.undo.is_empty());
    }
    let preview = history.scene.clone();
    assert_eq!(
        history
            .finish_move(
                token,
                id,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.scene, preview);
    assert_eq!(history.undo.len(), 1);
    assert!(history.undo());
    assert_eq!(history.scene, original);
    assert!(history.redo());
    assert_eq!(history.scene, preview);
}

#[test]
fn rotate_shares_token_owner_stale_and_busy_guards_and_cancels_exactly() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    let id = start_rotate(&mut history, RotateAxis::X, token);
    let again = history.begin_rotate(
        &target(&history),
        RotateAxis::Y,
        RotateSpace::Object,
        token,
        ManipulationProvenance::Native,
        BUDGET,
    );
    assert_eq!(again, Err(ManipulationError::Busy));
    let moving = history.begin_move(
        &target(&history),
        WorldAxis::X,
        token,
        ManipulationProvenance::Native,
        BUDGET,
    );
    assert_eq!(moving, Err(ManipulationError::Busy));
    let other = ManipulationId(id.0 + 1);
    for (scene_token, id, provenance, expected) in [
        (
            Uuid::new_v4(),
            id,
            ManipulationProvenance::Native,
            ManipulationError::StaleScene,
        ),
        (
            token,
            other,
            ManipulationProvenance::Native,
            ManipulationError::StaleManipulation,
        ),
        (
            token,
            id,
            ManipulationProvenance::Remote,
            ManipulationError::WrongOwner,
        ),
    ] {
        assert_eq!(
            history.update_move(scene_token, id, provenance, 5.0),
            Err(expected)
        );
        assert!(history.active_manipulation().is_some());
    }
    history
        .update_move(token, id, ManipulationProvenance::Native, 25.0)
        .unwrap();
    assert_eq!(history.cancel_active_move().unwrap(), EditOutcome::Changed);
    assert_eq!(history.scene, original);
    assert!(history.active_manipulation().is_none() && history.undo.is_empty());
}

#[test]
fn rotate_refusal_in_drag_keeps_the_last_valid_preview() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = start_rotate(&mut history, RotateAxis::Y, token);
    history
        .update_move(token, id, ManipulationProvenance::Native, 30.0)
        .unwrap();
    let preview = history.scene.clone();
    for refused in [90.0, f64::NAN, 1e308] {
        assert!(
            history
                .update_move(token, id, ManipulationProvenance::Native, refused)
                .is_err()
        );
        assert_eq!(history.scene, preview);
        assert_eq!(history.active_manipulation().unwrap().parameter, 30.0);
    }
    history
        .update_move(token, id, ManipulationProvenance::Native, 0.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(
                token,
                id,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert!(history.undo.is_empty());
}

#[test]
fn refused_rotate_begin_consumes_no_id_and_leaves_no_transaction() {
    let mut history = History::new(test_fixtures::pivoted_shape().0);
    let before = history.scene.clone();
    let result = history.begin_rotate(
        &target(&history),
        RotateAxis::X,
        RotateSpace::Object,
        Uuid::new_v4(),
        ManipulationProvenance::Native,
        BUDGET,
    );
    assert!(matches!(
        result,
        Err(ManipulationError::Native(NativeError::UnsupportedRotate(
            RotateUnsupported::SelectedPivot
        )))
    ));
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.next_manipulation_id, 0);
    assert_eq!(history.scene, before);
}

fn scale_of(history: &History) -> [f64; 3] {
    history
        .scene
        .node(target(history).node)
        .unwrap()
        .transform()
        .scale
}

#[test]
fn scale_previews_make_one_undo_and_identity_is_the_initial_parameter() {
    let mut history = cube_history();
    let original = history.scene.clone();
    let token = Uuid::new_v4();
    let id = history
        .begin_scale(
            &target(&history),
            ScaleHandle::Uniform,
            ScaleSpace::Object,
            token,
            ManipulationProvenance::Native,
            BUDGET,
        )
        .unwrap();
    let active = history.active_manipulation().unwrap();
    assert_eq!(
        active.kind,
        ManipulationKindState::Scale {
            handle: ScaleHandle::Uniform,
            space: ScaleSpace::Object
        }
    );
    assert_eq!((active.parameter, active.move_axis()), (1.0, None));
    for factor in [2.0, 0.5, 3.0] {
        history
            .update_move(token, id, ManipulationProvenance::Native, factor)
            .unwrap();
        assert_eq!(scale_of(&history), [factor; 3]);
        assert!(history.undo.is_empty());
    }
    let preview = history.scene.clone();
    assert_eq!(
        history
            .finish_move(
                token,
                id,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Changed
    );
    assert_eq!(history.undo.len(), 1);
    assert!(history.undo());
    assert_eq!(history.scene, original);
    assert!(history.redo());
    assert_eq!(history.scene, preview);
}

#[test]
fn scale_refusal_in_drag_keeps_the_last_valid_preview_and_identity_is_unchanged() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = history
        .begin_scale(
            &target(&history),
            ScaleHandle::X,
            ScaleSpace::Object,
            token,
            ManipulationProvenance::Remote,
            BUDGET,
        )
        .unwrap();
    assert_eq!(
        history.begin_move(
            &target(&history),
            WorldAxis::X,
            token,
            ManipulationProvenance::Remote,
            BUDGET
        ),
        Err(ManipulationError::Busy)
    );
    history
        .update_move(token, id, ManipulationProvenance::Remote, 4.0)
        .unwrap();
    let preview = history.scene.clone();
    for refused in [0.0, -2.0, f64::NAN, f64::MAX * 4.0, 1e308] {
        assert!(
            history
                .update_move(token, id, ManipulationProvenance::Remote, refused)
                .is_err()
        );
        assert_eq!(history.scene, preview);
    }
    assert_eq!(
        history.update_move(token, id, ManipulationProvenance::Native, 2.0),
        Err(ManipulationError::WrongOwner)
    );
    history
        .update_move(token, id, ManipulationProvenance::Remote, 1.0)
        .unwrap();
    assert_eq!(
        history
            .finish_move(
                token,
                id,
                ManipulationProvenance::Remote,
                ManipulationFinish::Commit
            )
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert!(history.undo.is_empty());
}

#[test]
fn refused_scale_begin_consumes_no_id() {
    let mut history = History::new(test_fixtures::pivoted_shape().0);
    let result = history.begin_scale(
        &target(&history),
        ScaleHandle::Y,
        ScaleSpace::Object,
        Uuid::new_v4(),
        ManipulationProvenance::Native,
        BUDGET,
    );
    assert!(matches!(
        result,
        Err(ManipulationError::Native(NativeError::UnsupportedScale(
            ScaleUnsupported::SelectedPivot
        )))
    ));
    assert!(history.active_manipulation().is_none());
    assert_eq!(history.next_manipulation_id, 0);
}

#[test]
fn each_manipulation_kind_names_its_undo_and_redo_entry() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let node = target(&history).node;
    assert_eq!((history.undo_label(), history.redo_label()), (None, None));
    let commit = |history: &mut History, id, value| {
        history
            .update_move(token, id, ManipulationProvenance::Native, value)
            .unwrap();
        history
            .finish_move(
                token,
                id,
                ManipulationProvenance::Native,
                ManipulationFinish::Commit,
            )
            .unwrap();
    };
    let id = start(&mut history, token);
    commit(&mut history, id, 2.0);
    assert_eq!(history.undo_label(), Some("Move".to_string()));
    let id = history
        .begin_rotate(
            &target(&history),
            RotateAxis::X,
            RotateSpace::Object,
            token,
            ManipulationProvenance::Native,
            BUDGET,
        )
        .unwrap();
    commit(&mut history, id, 30.0);
    assert_eq!(history.undo_label(), Some("Rotate".to_string()));
    let id = history
        .begin_scale(
            &target(&history),
            ScaleHandle::Uniform,
            ScaleSpace::Object,
            token,
            ManipulationProvenance::Native,
            BUDGET,
        )
        .unwrap();
    commit(&mut history, id, 2.0);
    assert_eq!(history.undo_label(), Some("Scale".to_string()));
    history
        .execute(Command::Rename {
            id: node,
            name: "renamed".into(),
        })
        .unwrap();
    assert_eq!(history.undo_label(), Some("Rename".to_string()));
    // Undo names the edit it reverts and Redo the one it reapplies, in the same order.
    for expected in ["Rename", "Scale", "Rotate", "Move"] {
        assert_eq!(history.undo_label(), Some(expected.to_string()));
        assert!(history.undo());
        assert_eq!(history.redo_label(), Some(expected.to_string()));
    }
    assert_eq!(history.undo_label(), None);
    for expected in ["Move", "Rotate"] {
        assert!(history.redo());
        assert_eq!(history.undo_label(), Some(expected.to_string()));
    }
}

#[test]
fn a_cancelled_manipulation_leaves_no_label() {
    let mut history = cube_history();
    let token = Uuid::new_v4();
    let id = start(&mut history, token);
    history
        .update_move(token, id, ManipulationProvenance::Native, 1.0)
        .unwrap();
    history
        .finish_move(
            token,
            id,
            ManipulationProvenance::Native,
            ManipulationFinish::Cancel,
        )
        .unwrap();
    assert_eq!(history.undo_label(), None);
}

#[test]
fn active_state_reports_the_baseline_and_live_channels_of_each_kind() {
    let token = Uuid::new_v4();
    let mut history = cube_history();
    let node = target(&history).node;
    history
        .scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 2.0, 3.0],
                rotation: [10.0, 0.0, 0.0],
                scale: [1.5, 1.0, 1.0],
            },
        )
        .unwrap();
    let rotate = history
        .begin_rotate(
            &target(&history),
            RotateAxis::X,
            RotateSpace::Object,
            token,
            ManipulationProvenance::Remote,
            BUDGET,
        )
        .unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.baseline_channels, [10.0, 0.0, 0.0]);
    assert_eq!(state.current_channels, Some([10.0, 0.0, 0.0]));
    history
        .update_move(token, rotate, ManipulationProvenance::Remote, 30.0)
        .unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.baseline_channels, [10.0, 0.0, 0.0]);
    assert_eq!(state.current_channels, Some([40.0, 0.0, 0.0]));
    history
        .finish_move(
            token,
            rotate,
            ManipulationProvenance::Remote,
            ManipulationFinish::Cancel,
        )
        .unwrap();

    let scale = history
        .begin_scale(
            &target(&history),
            ScaleHandle::X,
            ScaleSpace::Object,
            token,
            ManipulationProvenance::Remote,
            BUDGET,
        )
        .unwrap();
    history
        .update_move(token, scale, ManipulationProvenance::Remote, 2.0)
        .unwrap();
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.baseline_channels, [1.5, 1.0, 1.0]);
    assert_eq!(state.current_channels, Some([3.0, 1.0, 1.0]));
    history.cancel_active_move().unwrap();

    let moved = start(&mut history, token);
    let state = history.active_manipulation().unwrap();
    assert_eq!(state.baseline_channels, [1.0, 2.0, 3.0]);
    assert_eq!(state.id, moved);
}

// Extras only arrive with read-only Maya ASCII sources today, which the editor refuses first, so
// the typed policy reasons are checked at the History seam that the native and MCP begin paths
// both call. Both surface a `ManipulationError::Native` and keep the baseline.
#[test]
fn extras_policy_refusals_leave_no_transaction_and_no_consumed_id() {
    use crate::native_scene::TransformExtras;
    let cases = [
        (
            TransformExtras {
                rotate_order: Some(1),
                ..TransformExtras::default()
            },
            NativeError::UnsupportedRotate(RotateUnsupported::RotateOrder),
            false,
        ),
        (
            TransformExtras {
                rotate_axis: Some([0.0, 0.0, 90.0]),
                ..TransformExtras::default()
            },
            NativeError::UnsupportedRotate(RotateUnsupported::RotateAxis),
            false,
        ),
        (
            TransformExtras {
                shear: Some([0.5, 0.0, 0.0]),
                ..TransformExtras::default()
            },
            NativeError::UnsupportedScale(ScaleUnsupported::Shear),
            true,
        ),
    ];
    for (extras, expected, scale) in cases {
        let (mut scene, node) = test_fixtures::cube();
        test_fixtures::set_extras(&mut scene, node, extras);
        let mut history = History::new(scene);
        let before = history.scene.clone();
        let selected = target(&history);
        let result = if scale {
            history.begin_scale(
                &selected,
                ScaleHandle::X,
                ScaleSpace::World,
                Uuid::new_v4(),
                ManipulationProvenance::Native,
                BUDGET,
            )
        } else {
            history.begin_rotate(
                &selected,
                RotateAxis::Z,
                RotateSpace::Object,
                Uuid::new_v4(),
                ManipulationProvenance::Native,
                BUDGET,
            )
        };
        assert_eq!(result.unwrap_err(), ManipulationError::Native(expected));
        assert!(history.active_manipulation().is_none());
        assert_eq!(history.next_manipulation_id, 0);
        assert_eq!(history.scene, before);
    }
}
