use super::*;
use crate::native_scene::{
    ALL_CHANNELS, ascii, binary,
    test_fixtures::{cube, pivoted_extras_shape, restrict_edits},
};
use crate::scene::{Primitive, SceneError};
use std::sync::Arc;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}
#[allow(dead_code)]
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

fn set(id: NodeId, rotate: Option<[f64; 3]>, scale: Option<[f64; 3]>, preserve: bool) -> Command {
    Command::SetPivots {
        id,
        rotate_pivot: rotate,
        scale_pivot: scale,
        preserve,
    }
}

fn reset(ids: &[NodeId], mode: PivotReset) -> Command {
    Command::ResetPivots {
        ids: ids.to_vec(),
        mode,
    }
}

/// Everything a refused or Unchanged command must leave alone.
#[derive(PartialEq, Debug)]
struct Snapshot {
    scene: NativeScene,
    revision: u64,
    undo: usize,
    redo: usize,
}

fn snapshot(history: &History) -> Snapshot {
    Snapshot {
        scene: history.scene().clone(),
        revision: history.revision(),
        undo: history.undo.len(),
        redo: history.redo.len(),
    }
}

/// A pivoted cube and a second cube with raw pivots, after one edit that was undone so the redo
/// stack is not empty.
fn pivoted_history() -> (History, NodeId, NodeId) {
    let (scene, a) = pivoted_extras_shape();
    let mut history = History::new(scene);
    let EditOutcome::Created(b) = history
        .execute(Command::Create {
            primitive: Primitive::Cube,
            options: None,
            name: "b".into(),
        })
        .unwrap()
    else {
        panic!("create");
    };
    history
        .execute(set(b, Some([1.0, 2.0, 3.0]), Some([-1.0, 0.5, 2.0]), false))
        .unwrap();
    history
        .execute(Command::Rename {
            id: b,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    assert!(history.can_redo());
    (history, a, b)
}

fn assert_one_pivot_step(history: &mut History, command: Command) {
    let before = history.scene().clone();
    let depth = history.undo.len();
    assert_eq!(history.execute(command).unwrap(), EditOutcome::Changed);
    assert_ne!(history.scene(), &before);
    assert_eq!(history.undo.len(), depth + 1);
    assert!(history.can_undo() && !history.can_redo());
    assert_eq!(history.undo_label().as_deref(), Some("Pivot"));
    let after = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
    assert_eq!(history.redo_label().as_deref(), Some("Pivot"));
    assert!(history.redo());
    assert_eq!(history.scene(), &after);
    assert_eq!(history.undo.len(), depth + 1);
}

#[test]
fn set_pivots_is_one_pivot_step_that_undoes_and_redoes_bit_identically() {
    for preserve in [true, false] {
        let (mut history, a, _) = pivoted_history();
        assert_one_pivot_step(
            &mut history,
            set(a, Some([0.3, -4.0, 2.5]), Some([1.0, 1.0, -1.0]), preserve),
        );
    }
}

#[test]
fn reset_pivots_is_one_pivot_step_for_center_and_zero() {
    for mode in [PivotReset::Center, PivotReset::Zero] {
        let (mut history, a, _) = pivoted_history();
        assert_one_pivot_step(&mut history, reset(&[a], mode));
    }
}

#[test]
fn a_multi_object_reset_is_one_undo_step() {
    for mode in [PivotReset::Center, PivotReset::Zero] {
        let (mut history, a, b) = pivoted_history();
        // b was undone to its unrenamed state; both objects carry pivots to reset.
        assert_one_pivot_step(&mut history, reset(&[a, b], mode));
        let depth = history.undo.len();
        assert!(history.undo());
        assert_eq!(history.undo.len(), depth - 1);
        assert_eq!(
            history.scene().node(b).unwrap().rotate_pivot(),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            history.scene().node(a).unwrap().rotate_pivot(),
            [1.0, 0.25, -2.0]
        );
    }
}

#[test]
fn a_raw_edit_of_an_opened_binary_restores_its_replaced_flags_on_undo_and_redo() {
    let mut transform = wire::creation(b"object", None);
    transform.extend(wire::triple(b"t", [1.0, 2.0, 3.0]));
    transform.extend(wire::triple(b"rp", [0.5, 0.25, -1.0]));
    let bytes = wire::scene_with_units(
        &[
            wire::group(b"XFRM", &transform, 4),
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
        b"cm",
        b"deg",
    );
    let scene = binary::decode(bytes).unwrap();
    let id = scene.nodes()[0].id();
    let mut history = History::new(scene);
    assert_one_pivot_step(&mut history, set(id, Some([9.0, 8.0, 7.0]), None, false));
    // The patched words are replaced; undo must put the untouched flags back.
    assert!(history.undo());
    assert_eq!(
        history.scene().node(id).unwrap().rotate_pivot(),
        [0.5, 0.25, -1.0]
    );
}

#[test]
fn refusals_leave_scene_history_and_redo_stack_untouched() {
    let (mut history, a, b) = pivoted_history();
    let (mut ma, ma_node) = {
        let scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
        let id = scene.nodes()[0].id();
        (History::new(scene), id)
    };
    let missing = NodeId::new(999);
    let cases: Vec<(Command, NativeError)> = vec![
        (
            set(a, Some([f64::NAN, 0.0, 0.0]), None, true),
            NativeError::NonFinite,
        ),
        (
            set(a, None, Some([0.0, f64::INFINITY, 0.0]), false),
            NativeError::NonFinite,
        ),
        (
            set(a, Some([1.7e308; 3]), None, true),
            NativeError::NonFinite,
        ),
        (
            set(missing, Some([1.0; 3]), None, true),
            SceneError::MissingNode(missing).into(),
        ),
        (
            reset(&[a, a], PivotReset::Zero),
            SceneError::InvalidIds.into(),
        ),
        (
            reset(&[a, b, a], PivotReset::Center),
            SceneError::InvalidIds.into(),
        ),
        (
            reset(&[a, missing], PivotReset::Zero),
            SceneError::MissingNode(missing).into(),
        ),
        (
            reset(&[missing, a], PivotReset::Center),
            SceneError::MissingNode(missing).into(),
        ),
    ];
    for (command, error) in cases {
        let before = snapshot(&history);
        assert_eq!(history.execute(command.clone()), Err(error), "{command:?}");
        assert_eq!(snapshot(&history), before, "{command:?}");
    }
    for command in [
        set(ma_node, Some([1.0; 3]), None, true),
        reset(&[ma_node], PivotReset::Zero),
        reset(&[ma_node], PivotReset::Center),
    ] {
        let before = snapshot(&ma);
        assert_eq!(ma.execute(command), Err(NativeError::ReadOnlyFormat));
        assert_eq!(snapshot(&ma), before);
    }
}

fn key(id: NodeId) -> Command {
    Command::SetKeys {
        nodes: vec![id],
        frame: 1,
        channels: ALL_CHANNELS,
        current_frame: 1,
    }
}

#[test]
fn a_keyed_or_restricted_member_refuses_the_whole_command() {
    let (mut history, a, b) = pivoted_history();
    history.execute(key(a)).unwrap();
    leave_redo(&mut history, b);
    let before = snapshot(&history);
    for command in [
        set(a, Some([1.0; 3]), None, true),
        set(a, None, None, false),
        reset(&[a], PivotReset::Zero),
        reset(&[b, a], PivotReset::Zero),
        reset(&[b, a], PivotReset::Center),
    ] {
        assert_eq!(history.execute(command), Err(NativeError::PivotOnKeyedNode));
        assert_eq!(snapshot(&history), before);
    }

    let (mut scene, restricted) = cube();
    let other = scene.create(Primitive::Cube, "other").unwrap();
    restrict_edits(&mut scene, restricted);
    let mut history = History::new(scene);
    let before = snapshot(&history);
    for command in [
        set(restricted, Some([1.0; 3]), None, true),
        reset(&[other, restricted], PivotReset::Zero),
    ] {
        assert_eq!(history.execute(command), Err(NativeError::UnsupportedEdit));
        assert_eq!(snapshot(&history), before);
    }
}

#[test]
fn a_session_key_on_another_node_does_not_block() {
    let (mut history, a, b) = pivoted_history();
    history.execute(key(a)).unwrap();
    assert_one_pivot_step(&mut history, set(b, Some([4.0, 5.0, 6.0]), None, false));
    assert_one_pivot_step(&mut history, reset(&[b], PivotReset::Zero));
}

/// Renames `b` and undoes it, so the redo stack is not empty.
fn leave_redo(history: &mut History, b: NodeId) {
    history
        .execute(Command::Rename {
            id: b,
            name: "tmp".into(),
        })
        .unwrap();
    assert!(history.undo());
    assert!(history.can_redo());
}

#[test]
fn a_command_that_changes_nothing_makes_no_entry_and_keeps_the_redo_stack() {
    for mode in [PivotReset::Center, PivotReset::Zero] {
        let (mut history, a, b) = pivoted_history();
        history.execute(reset(&[a], mode)).unwrap();
        leave_redo(&mut history, b);
        let before = snapshot(&history);
        assert_eq!(
            history.execute(reset(&[a], mode)).unwrap(),
            EditOutcome::Unchanged,
            "{mode:?} twice"
        );
        assert_eq!(snapshot(&history), before);
    }
    let (mut history, a, b) = pivoted_history();
    history.execute(reset(&[a], PivotReset::Zero)).unwrap();
    leave_redo(&mut history, b);
    let before = snapshot(&history);
    let node = history.scene().node(a).unwrap();
    let (rp, sp) = (node.rotate_pivot(), node.scale_pivot());
    for command in [
        set(a, Some(rp), Some(sp), true),
        set(a, Some(rp), Some(sp), false),
        set(a, None, None, true),
    ] {
        assert_eq!(history.execute(command).unwrap(), EditOutcome::Unchanged);
        assert_eq!(snapshot(&history), before);
    }
}

#[test]
fn a_failing_member_after_a_valid_one_leaves_the_valid_one_unapplied() {
    // Atomicity: the first id is valid and would change; the second is unknown.
    let (mut history, a, _) = pivoted_history();
    let before = snapshot(&history);
    let missing = NodeId::new(999);
    for mode in [PivotReset::Center, PivotReset::Zero] {
        assert!(history.execute(reset(&[a, missing], mode)).is_err());
        assert_eq!(snapshot(&history), before);
        assert_ne!(
            history.scene().node(a).unwrap().rotate_pivot(),
            [0.0; 3],
            "a keeps its pivot"
        );
    }
}

#[test]
fn a_pivot_command_is_refused_during_a_manipulation() {
    let (mut history, a, _) = pivoted_history();
    let node = history.scene().node(a).unwrap();
    let path = vec![node.transform_id()];
    history
        .begin_move(
            &crate::native_scene::TransformTarget { node: a, path },
            crate::native_scene::WorldAxis::X,
            uuid::Uuid::new_v4(),
            ManipulationProvenance::Native,
            1024 * 1024,
        )
        .unwrap();
    let before = history.revision();
    for command in [
        set(a, Some([1.0; 3]), None, true),
        reset(&[a], PivotReset::Zero),
    ] {
        assert_eq!(history.execute(command), Err(NativeError::BusyManipulation));
    }
    assert_eq!(history.revision(), before);
}
