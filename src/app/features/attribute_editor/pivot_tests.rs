//! Issue #39 slice B, T7: the Attribute Editor Pivots drafts. They share the channel drafts'
//! guards and commit policy, and a field commits as one raw `SetPivots` ("Pivot") entry.
use super::*;
use crate::app::tests::remote;
use azteccraft::native_scene::{ALL_CHANNELS, NativeError};

#[allow(dead_code)]
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

/// A cube rotated 45 degrees about Y, so a pivot edit that preserved the matrix would show in
/// the pivot translates and a raw one would not.
fn rotated_cube() -> (Editor, NodeId) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.history.scene().nodes()[0].id();
    editor
        .apply_operation(Operation::EditNode {
            id,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None, Some(45.0), None],
            scale: [None; 3],
        })
        .unwrap();
    (editor, id)
}

fn type_pivot(editor: &mut Editor, index: usize, text: &str) {
    editor.pivot_drafts[index] = text.into();
    editor.pivot_intents[index] = true;
}

fn pivots(editor: &Editor, id: NodeId) -> ([f64; 3], [f64; 3]) {
    let node = editor.history.scene().node(id).unwrap();
    (node.rotate_pivot(), node.scale_pivot())
}

fn first_channel(editor: &Editor, id: NodeId) -> f64 {
    editor
        .history
        .scene()
        .node(id)
        .unwrap()
        .transform()
        .values()[0]
}

/// An opened Maya binary transform with the given leaves and a triangle shape under it.
fn open_binary(directory: &std::path::Path, leaves: &[(&[u8], [f64; 3])]) -> Editor {
    let mut transform = wire::creation(b"object", None);
    for (key, values) in leaves {
        transform.extend(wire::triple(key, *values));
    }
    let bytes = wire::scene(
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
    );
    let path = directory.join("scene.mb");
    std::fs::write(&path, bytes).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let id = editor.history.scene().nodes()[0].id();
    editor.select(Some(id));
    editor
}

#[test]
fn selecting_a_node_fills_the_pivot_drafts_from_its_pivots() {
    let (mut editor, id) = rotated_cube();
    assert_eq!(editor.pivot_drafts, ["0"; 6]);
    editor
        .apply_operation(Operation::SetPivots {
            id,
            rotate_pivot: Some([1.5, -2.0, 0.25]),
            scale_pivot: Some([3.0, 4.0, 5.0]),
            preserve: false,
        })
        .unwrap();
    assert_eq!(editor.pivot_drafts, ["1.5", "-2", "0.25", "3", "4", "5"]);
    assert_eq!(editor.pivot_intents, [false; 6]);
    assert!(!editor.has_draft());
    editor.select(None);
    assert_eq!(editor.pivot_drafts, <[String; 6]>::default());
}

#[test]
fn a_typed_pivot_is_a_draft_and_commits_raw_as_one_pivot_undo_step() {
    let (mut editor, id) = rotated_cube();
    let before = editor.history.scene().clone();
    type_pivot(&mut editor, 0, "2");
    type_pivot(&mut editor, 5, "-3");
    assert!(editor.has_channel_draft() && editor.has_draft());
    assert!(editor.commit_pending(), "{}", editor.status);
    assert_eq!(pivots(&editor, id), ([2.0, 0.0, 0.0], [0.0, 0.0, -3.0]));
    // Raw: the pivot translates stay untouched, so the rotated cube moves (Q5, PROVISIONAL).
    let attributes = editor.history.scene().transform_attributes(id).unwrap();
    assert_eq!(attributes.rotate_pivot_translate, [0.0; 3]);
    assert!(!editor.has_draft());
    assert_eq!(editor.pivot_intents, [false; 6]);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.pivot_drafts, ["0"; 6]);
    editor.history_step(true);
    assert_eq!(pivots(&editor, id).0, [2.0, 0.0, 0.0]);
}

#[test]
fn only_the_touched_pivot_is_sent() {
    let (mut editor, id) = rotated_cube();
    assert!(matches!(editor.pivot_command(id), Ok(None)));
    type_pivot(&mut editor, 4, "7");
    let Ok(Some(Command::SetPivots {
        rotate_pivot,
        scale_pivot,
        preserve,
        ..
    })) = editor.pivot_command(id)
    else {
        panic!("no SetPivots");
    };
    assert_eq!(rotate_pivot, None, "an untouched pivot is not re-sent");
    assert_eq!(scale_pivot, Some([0.0, 7.0, 0.0]));
    assert!(!preserve, "an Attribute Editor field edit is raw (Q5)");
    // A changed draft with no recorded keystroke is touched too, like a changed channel.
    editor.pivot_intents = [false; 6];
    editor.pivot_drafts[4] = "0".into();
    editor.pivot_drafts[1] = "1".into();
    assert!(matches!(
        editor.pivot_command(id),
        Ok(Some(Command::SetPivots {
            rotate_pivot: Some([0.0, 1.0, 0.0]),
            scale_pivot: None,
            ..
        }))
    ));
}

#[test]
fn bad_pivot_text_keeps_the_draft_and_changes_nothing() {
    let (mut editor, _) = rotated_cube();
    let before = editor.history.scene().clone();
    let undo = editor.history.undo_label();
    for bad in ["", "abc", "1e999", "NaN"] {
        type_pivot(&mut editor, 2, bad);
        assert!(!editor.commit_pending(), "{bad}");
        assert!(editor.error);
        assert_eq!(editor.status, "Enter a valid number in the edited pivot");
        assert_eq!(editor.pivot_drafts[2], bad);
        assert!(editor.pivot_intents[2]);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.history.undo_label(), undo);
    }
}

#[test]
fn a_refused_pivot_commit_shows_the_reason_and_keeps_the_draft() {
    let (mut editor, _) = rotated_cube();
    editor
        .apply_operation(Operation::SetKeyframe {
            nodes: None,
            time: None,
            channels: ALL_CHANNELS,
        })
        .unwrap();
    let before = editor.history.scene().clone();
    type_pivot(&mut editor, 0, "2");
    assert!(!editor.commit_pending());
    assert!(editor.error);
    assert_eq!(editor.status, NativeError::PivotOnKeyedNode.to_string());
    assert_eq!(editor.pivot_drafts[0], "2");
    assert!(editor.pivot_intents[0] && editor.has_draft());
    assert_eq!(editor.history.scene(), &before);

    // An opened binary node without the rp leaf refuses a nonzero pivot with the typed reason.
    let directory = tempfile::tempdir().unwrap();
    let mut editor = open_binary(
        directory.path(),
        &[(b"t", [1.0; 3]), (b"s", [1.0; 3]), (b"sp", [0.0; 3])],
    );
    let before = editor.history.scene().clone();
    type_pivot(&mut editor, 1, "4");
    assert!(!editor.commit_pending());
    assert!(
        editor.status.contains("stores no rotate pivot"),
        "{}",
        editor.status
    );
    assert!(editor.error);
    assert_eq!(editor.pivot_drafts[1], "4");
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_channel_draft_and_a_pivot_draft_commit_as_two_undo_entries() {
    let (mut editor, id) = rotated_cube();
    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    type_pivot(&mut editor, 3, "9");
    assert!(editor.commit_pending(), "{}", editor.status);
    assert_eq!(first_channel(&editor, id), 5.0);
    assert_eq!(pivots(&editor, id).1, [9.0, 0.0, 0.0]);
    assert!(!editor.has_draft());
    assert_eq!(editor.history.undo_label().as_deref(), Some("Pivot"));
    editor.history_step(false);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Edit"));
    assert_eq!(pivots(&editor, id).1, [0.0; 3]);
    editor.history_step(false);
    assert_eq!(first_channel(&editor, id), 0.0);
}

#[test]
fn bad_pivot_text_beside_a_channel_draft_commits_neither() {
    let (mut editor, id) = rotated_cube();
    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    type_pivot(&mut editor, 0, "abc");
    assert!(!editor.commit_pending());
    assert_eq!(editor.status, "Enter a valid number in the edited pivot");
    assert_eq!(first_channel(&editor, id), 0.0);
    assert!(editor.channel_intents[0] && editor.pivot_intents[0]);
}

#[test]
fn a_refused_pivot_after_a_committed_channel_edit_keeps_only_the_pivot_draft() {
    let directory = tempfile::tempdir().unwrap();
    let mut editor = open_binary(
        directory.path(),
        &[(b"t", [1.0; 3]), (b"s", [1.0; 3]), (b"sp", [0.0; 3])],
    );
    let id = editor.history.scene().nodes()[0].id();
    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    type_pivot(&mut editor, 1, "4");
    assert!(!editor.commit_pending());
    assert!(editor.status.contains("stores no rotate pivot"));
    assert_eq!(first_channel(&editor, id), 5.0, "the channel edit landed");
    assert_eq!(editor.channel_intents, [false; 9]);
    assert!(editor.pivot_intents[1]);
    assert_eq!(editor.pivot_drafts[1], "4");
}

#[test]
fn a_pivot_only_draft_refuses_a_remote_operation_like_a_channel_draft() {
    let create = || Operation::Create {
        primitive: Primitive::Cube,
        options: None,
        name: None,
    };
    let (mut editor, _) = rotated_cube();
    let before = editor.history.scene().clone();
    type_pivot(&mut editor, 0, "2");
    assert!(editor.channel_intents.iter().all(|v| !v) && !editor.name_intent);
    assert!(matches!(
        remote(&mut editor, create()),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.pivot_intents[0], "the draft survives the refusal");
    // The same cell with a channel draft, which the pivot draft must match.
    let (mut channel, _) = rotated_cube();
    channel.channels[0] = "5".into();
    assert!(matches!(
        remote(&mut channel, create()),
        Err(OperationError::BusyEditing)
    ));
}
