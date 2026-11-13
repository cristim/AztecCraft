//! Issue #39 slice A: Move, Rotate and Scale edit only the t, r or s words of an opened Maya
//! binary scene that carries rotate and scale pivots. Save, Save As and reopen keep every other
//! byte, rp and sp included, and undo restores the original bytes. Synthetic scene; the native
//! binary Save and Save As exist on unix and Windows.
use super::tests::remote;
use super::*;
use crate::editor_operations::{ManipulationSpace, ManipulationValue, MoveDistance};
use azteccraft::history::{ManipulationError, ManipulationFinish, ManipulationId};
use azteccraft::native_scene::{
    ChannelLeaf, NativeError, RotateAxis, ScaleHandle, TransformTarget, WorldAxis,
};
use std::ops::Range;

#[allow(dead_code)]
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

const T: [f64; 3] = [1.0, 2.0, 3.0];
const R: [f64; 3] = [10.0, 20.0, 30.0];
const S: [f64; 3] = [2.0, 3.0, 4.0];
const RP: [f64; 3] = [0.5, 0.25, -1.0];
const SP: [f64; 3] = [-1.0, 2.0, 0.5];

/// One transform with the given leaves and a triangle shape under it.
fn scene_bytes(leaves: &[(&[u8], [f64; 3])]) -> Vec<u8> {
    let mut transform = wire::creation(b"object", None);
    for (key, values) in leaves {
        transform.extend(wire::triple(key, *values));
    }
    wire::scene(
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
    )
    .to_vec()
}

fn pivoted() -> Vec<u8> {
    scene_bytes(&[(b"t", T), (b"r", R), (b"s", S), (b"rp", RP), (b"sp", SP)])
}

/// The three numeric words of the `DBL3` leaf `key`: its payload after the key, a zero and the
/// kind byte.
fn leaf(bytes: &[u8], key: &[u8]) -> Range<usize> {
    let mut at = 0;
    while let Some(found) = bytes[at..].windows(4).position(|w| w == b"DBL3") {
        let payload = at + found + 16;
        let head = &bytes[payload..payload + key.len() + 2];
        if &head[..key.len()] == key && head[key.len()] == 0 && head[key.len() + 1] == 0x20 {
            return payload + key.len() + 2..payload + key.len() + 26;
        }
        at += found + 4;
    }
    panic!("no {key:?} leaf");
}

fn open(path: &std::path::Path) -> Editor {
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene {
            path: path.to_owned(),
        })
        .unwrap();
    let node = editor.history.scene().nodes()[0].id();
    let instance = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .clone();
    editor
        .apply_operation(Operation::Select {
            node: Some(node),
            instance_path: Some(instance),
        })
        .unwrap();
    editor
}

#[derive(Clone, Copy, Debug)]
enum Edit {
    Move,
    Rotate,
    Scale,
}

impl Edit {
    fn key(self) -> &'static [u8] {
        match self {
            Self::Move => b"t",
            Self::Rotate => b"r",
            Self::Scale => b"s",
        }
    }
}

/// Begin, update and commit `edit` through the remote operations, as MCP does.
fn commit(editor: &mut Editor, edit: Edit) {
    let (node, path) = {
        let (node, path) = editor.single_target().unwrap();
        (node, path.to_vec())
    };
    let target = TransformTarget { node, path };
    let started = remote(
        editor,
        match edit {
            Edit::Move => Operation::BeginMove {
                target,
                axis: WorldAxis::X,
                expected_selection_generation: None,
            },
            Edit::Rotate => Operation::BeginRotate {
                target,
                axis: RotateAxis::X,
                space: ManipulationSpace::Object,
            },
            Edit::Scale => Operation::BeginScale {
                target,
                handle: ScaleHandle::X,
                space: ManipulationSpace::Object,
                expected_selection_generation: None,
            },
        },
    )
    .unwrap();
    let OperationReply::ManipulationStarted { manipulation_id } = started else {
        panic!("{started:?}");
    };
    let id = ManipulationId::from_decimal(&manipulation_id).unwrap();
    let update = match edit {
        Edit::Move => Operation::UpdateMove {
            id,
            distance: MoveDistance::Centimeters(5.0),
        },
        Edit::Rotate => Operation::UpdateManipulation {
            id,
            value: ManipulationValue::Degrees(25.0),
        },
        Edit::Scale => Operation::UpdateManipulation {
            id,
            value: ManipulationValue::Factor(1.5),
        },
    };
    remote(editor, update).unwrap();
    remote(
        editor,
        Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        },
    )
    .unwrap();
}

fn assert_only_differs_within(original: &[u8], saved: &[u8], allowed: Range<usize>, what: &str) {
    assert_eq!(original.len(), saved.len(), "{what}: length");
    for (offset, (a, b)) in original.iter().zip(saved).enumerate() {
        assert!(
            a == b || allowed.contains(&offset),
            "{what}: byte {offset} changed outside {allowed:?}"
        );
    }
    assert_ne!(original[allowed.clone()], saved[allowed], "{what}: no edit");
}

fn channels(editor: &Editor) -> ([f64; 9], [f64; 3], [f64; 3]) {
    let node = &editor.history.scene().nodes()[0];
    (
        node.transform().values(),
        node.rotate_pivot(),
        node.scale_pivot(),
    )
}

fn bits(values: &([f64; 9], [f64; 3], [f64; 3])) -> Vec<u64> {
    values
        .0
        .iter()
        .chain(&values.1)
        .chain(&values.2)
        .map(|v| v.to_bits())
        .collect()
}

#[test]
fn move_rotate_and_scale_patch_only_their_words_through_save_save_as_and_reopen() {
    for edit in [Edit::Move, Edit::Rotate, Edit::Scale] {
        let directory = tempfile::tempdir().unwrap();
        let original = pivoted();
        let path = directory.path().join("original.mb");
        std::fs::write(&path, &original).unwrap();
        let mut editor = open(&path);
        let before = channels(&editor);
        commit(&mut editor, edit);
        let edited = channels(&editor);
        assert_ne!(bits(&before), bits(&edited), "{edit:?}");
        // rp and sp are never touched by the edit itself.
        assert_eq!(before.1.map(f64::to_bits), edited.1.map(f64::to_bits));
        assert_eq!(before.2.map(f64::to_bits), edited.2.map(f64::to_bits));

        assert!(matches!(
            remote(&mut editor, Operation::SaveScene),
            Ok(OperationReply::Saved { .. })
        ));
        let saved = std::fs::read(&path).unwrap();
        assert_only_differs_within(&original, &saved, leaf(&original, edit.key()), "Save");
        let copy = directory.path().join("copy.mb");
        assert!(matches!(
            remote(&mut editor, Operation::SaveSceneAs { path: copy.clone() }),
            Ok(OperationReply::Saved { .. })
        ));
        assert_eq!(std::fs::read(&copy).unwrap(), saved, "{edit:?}: Save As");

        for reopened in [&path, &copy] {
            let again = open(reopened);
            assert_eq!(bits(&channels(&again)), bits(&edited), "{edit:?}: reopen");
        }

        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(
            azteccraft::native_scene::binary::encode(editor.history.scene()).unwrap(),
            original,
            "{edit:?}: undo"
        );
    }
}

/// Rotate, Scale and Move begins in turn; a begin that starts is cancelled at once.
fn begin_all(editor: &mut Editor) -> [Result<(), OperationError>; 3] {
    let (node, path) = {
        let (node, path) = editor.single_target().unwrap();
        (node, path.to_vec())
    };
    let target = TransformTarget { node, path };
    let operations = [
        Operation::BeginRotate {
            target: target.clone(),
            axis: RotateAxis::X,
            space: ManipulationSpace::Object,
        },
        Operation::BeginScale {
            target: target.clone(),
            handle: ScaleHandle::X,
            space: ManipulationSpace::Object,
            expected_selection_generation: None,
        },
        Operation::BeginMove {
            target,
            axis: WorldAxis::X,
            expected_selection_generation: None,
        },
    ];
    operations.map(|operation| {
        let started = remote(editor, operation)?;
        let OperationReply::ManipulationStarted { manipulation_id } = started else {
            panic!("{started:?}");
        };
        remote(
            editor,
            Operation::FinishMove {
                id: ManipulationId::from_decimal(&manipulation_id).unwrap(),
                finish: ManipulationFinish::Cancel,
            },
        )
        .map(|_| ())
    })
}

#[test]
fn an_absent_rotate_leaf_with_a_pivot_refuses_rotate_and_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("no_rotate.mb");
    std::fs::write(
        &path,
        scene_bytes(&[(b"t", T), (b"s", S), (b"rp", RP), (b"sp", SP)]),
    )
    .unwrap();
    let mut editor = open(&path);
    let before = editor.history.scene().clone();
    let [rotate, scale, mv] = begin_all(&mut editor);
    let error = rotate.unwrap_err();
    assert!(
        matches!(
            &error,
            OperationError::Move(ManipulationError::Native(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Rotate,
                component: 0
            }))
        ),
        "{error:?}"
    );
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.active_manipulation().is_none());
    // The leaves that exist are still editable, and a cancelled begin leaves the scene alone.
    scale.unwrap();
    mv.unwrap();
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn a_sourced_ascii_scene_with_a_pivot_refuses_every_tool_as_read_only() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("pivot.ma");
    std::fs::write(
        &path,
        "//Maya ASCII 2016 scene\nrequires maya \"2016\";\n\
         currentUnit -l centimeter -a degree -t film;\n\
         createNode transform -n \"object\";\n\
         setAttr \".t\" -type \"double3\" 1 2 3;\n\
         setAttr \".r\" -type \"double3\" 10 20 30;\n\
         setAttr \".s\" -type \"double3\" 2 3 4;\n\
         setAttr \".rp\" -type \"double3\" 0.5 0.25 -1;\n\
         setAttr \".sp\" -type \"double3\" -1 2 0.5;\n",
    )
    .unwrap();
    let mut editor = open(&path);
    let before = editor.history.scene().clone();
    for (tool, result) in ["rotate", "scale", "move"]
        .into_iter()
        .zip(begin_all(&mut editor))
    {
        let error = result.unwrap_err();
        assert!(
            matches!(
                &error,
                OperationError::Scene(NativeError::ReadOnlyFormat)
                    | OperationError::Move(ManipulationError::Native(NativeError::ReadOnlyFormat))
            ),
            "{tool}: {error:?}"
        );
    }
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.active_manipulation().is_none());
}

/// The untitled-scene Save As is unix only (`save_as_available`). Slice A gives an authored
/// node no pivot; slice B owns writing one, so the writer must refuse it and write nothing.
#[cfg(all(unix, feature = "mcp-test-client"))]
#[test]
fn save_as_of_a_pivoted_authored_scene_is_refused_and_writes_no_file() {
    use azteccraft::document::NativeSaveError;
    use azteccraft::native_scene::binary::AuthoredRefusalCode;
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("pivoted.mb");
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let node = editor.single_target().unwrap().0;
    let mut scene = editor.history.scene().clone();
    scene
        .set_pivots_for_test(node, [1.0, 0.25, -2.0], [-0.5, 1.0, 0.0])
        .unwrap();
    editor.history = azteccraft::history::History::new(scene);
    // The planted pivots must not read as a typed pivot draft.
    editor.sync_fields();
    let result = remote(
        &mut editor,
        Operation::SaveSceneAs {
            path: destination.clone(),
        },
    );
    assert!(
        matches!(
            result,
            Err(OperationError::NativeSave(NativeSaveError::Unwritable(
                AuthoredRefusalCode::NonDefaultPivot
            )))
        ),
        "{result:?}"
    );
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
