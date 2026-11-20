//! `create_uvs` on the create path (#453 T8a): the UV set is built before anything is added, so a
//! failure rejects with no mutation, and undo, redo and delete treat it like any shape data.
use super::*;
use azteccraft::native_scene::{NativeError, NativeId};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn create(
    editor: &mut Editor,
    primitive: Primitive,
    options: Option<PrimitiveOptions>,
    create_uvs: bool,
) -> Result<OperationReply, crate::editor_operations::OperationError> {
    editor.apply_operation(Operation::Create {
        primitive,
        options,
        name: None,
        create_uvs,
    })
}

fn shape_of(editor: &Editor, index: usize) -> NativeId {
    editor.history.scene().shapes()[index].id()
}

#[test]
fn create_uvs_false_builds_none_and_true_builds_a_matching_valid_set() {
    for primitive in PRIMITIVES {
        let mut editor = Editor::default();
        create(&mut editor, primitive, None, false).unwrap();
        create(&mut editor, primitive, None, true).unwrap();
        let scene = editor.history.scene();
        assert!(
            scene.uv_set(shape_of(&editor, 0)).is_none(),
            "{primitive:?}"
        );
        let set = scene.uv_set(shape_of(&editor, 1)).expect("uvs");
        let mesh = scene.shapes()[1].mesh();
        assert_eq!(set.faces().len(), mesh.faces().len(), "{primitive:?}");
        for (face, corners) in mesh.faces().iter().zip(set.faces()) {
            assert_eq!(face.outer().len(), corners.len());
        }
        scene.validate().unwrap();
        // The same mesh either way: UVs add data, never change geometry.
        assert!(scene.shapes()[0].mesh() == mesh, "{primitive:?}");
    }
}

#[test]
fn a_failing_uv_build_rejects_before_any_mutation() {
    // A plane facing X has no extent along the Y-projection's width, so the placeholder layout
    // refuses it; the build runs before the node is added.
    let options = PrimitiveOptions::Plane {
        width: 1.0,
        height: 1.0,
        sub_width: 1,
        sub_height: 1,
        axis: Axis::X,
    };
    let mut editor = Editor::default();
    create(&mut editor, Primitive::Cube, None, false).unwrap();
    let before = editor.history.scene().clone();
    let (undo, redo) = (editor.history.undo_len(), editor.history.redo_len());
    let selected = editor.selected();
    assert!(create(&mut editor, Primitive::Plane, Some(options), true).is_err());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        (undo, redo)
    );
    assert_eq!(editor.selected(), selected);
    // The same options without UVs still create: the refusal is the UV build's alone.
    create(&mut editor, Primitive::Plane, Some(options), false).unwrap();
}

#[test]
fn undo_removes_the_uv_set_and_redo_restores_it() {
    let mut editor = Editor::default();
    create(&mut editor, Primitive::Cube, None, true).unwrap();
    let shape = shape_of(&editor, 0);
    let with_uvs = editor.history.scene().clone();
    assert!(editor.history.undo());
    assert!(editor.history.scene().uv_set(shape).is_none());
    editor.history.scene().validate().unwrap();
    assert!(editor.history.redo());
    assert_eq!(editor.history.scene(), &with_uvs);
    assert!(editor.history.scene().uv_set(shape).is_some());
}

#[test]
fn removing_the_node_prunes_its_uv_set() {
    let mut editor = Editor::default();
    create(&mut editor, Primitive::Sphere, None, true).unwrap();
    let shape = shape_of(&editor, 0);
    let node = editor.history.scene().nodes()[0].id();
    editor.history.execute(Command::Remove(node)).unwrap();
    assert!(editor.history.scene().uv_set(shape).is_none());
    editor.history.scene().validate().unwrap();
}

mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

/// An opened Maya ASCII scene is read-only; an opened Maya Binary one refuses structural edits.
#[test]
fn a_create_on_an_opened_scene_is_refused_with_its_typed_error_and_no_mutation() {
    let directory = tempfile::tempdir().unwrap();
    for (file, bytes, expected) in [
        (
            "curve.ma",
            ascii_fixture::specimen().to_vec(),
            NativeError::ReadOnlyFormat,
        ),
        (
            "synthetic.mb",
            binary_fixture::specimen().to_vec(),
            NativeError::UnsupportedEdit,
        ),
    ] {
        let path = directory.path().join(file);
        std::fs::write(&path, bytes).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let before = editor.history.scene().clone();
        let refused = create(&mut editor, Primitive::Cube, None, true).unwrap_err();
        assert!(
            matches!(&refused, crate::editor_operations::OperationError::Scene(e) if *e == expected),
            "{file}: {refused:?}"
        );
        assert_eq!(editor.history.scene(), &before, "{file}");
        assert!(
            before
                .shapes()
                .iter()
                .all(|s| before.uv_set(s.id()).is_none())
        );
    }
}

#[test]
fn an_explicitly_named_create_also_builds_the_uv_set() {
    let mut editor = Editor::default();
    for (name, create_uvs) in [("named_plain", false), ("named_uvs", true)] {
        editor
            .apply_operation(Operation::Create {
                primitive: Primitive::Cube,
                options: None,
                name: Some(name.into()),
                create_uvs,
            })
            .unwrap();
    }
    let scene = editor.history.scene();
    assert!(scene.uv_set(shape_of(&editor, 0)).is_none());
    assert!(scene.uv_set(shape_of(&editor, 1)).is_some());
    scene.validate().unwrap();
}
