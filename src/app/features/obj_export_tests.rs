use super::*;
use crate::app::tests::{deep_group_editor, deep_group_editor_with_leaf};
use azteccraft::scene::Primitive;

fn object_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("o "))
        .map(str::to_owned)
        .collect()
}

fn select_the_root_group(editor: &mut Editor) {
    let group = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|target| target.path.len() == 1)
        .unwrap();
    editor.set_selection_members_for_test(vec![group]);
}

fn vertices(path: &Path) -> Vec<[f64; 3]> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("v "))
        .map(|line| {
            let mut values = line.split(' ').map(|value| value.parse().unwrap());
            std::array::from_fn(|_| values.next().unwrap())
        })
        .collect()
}

#[test]
fn export_selection_writes_every_selected_object_once() {
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(vec![all[2].clone(), all[0].clone()]);
    let before = (editor.history.scene().clone(), editor.selection.clone());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("two.obj");

    let reply = editor.export_obj(&path, true, &|| true).unwrap();

    assert!(matches!(
        reply,
        OperationReply::Exported {
            objects: 2,
            vertices: 16,
            faces: 12,
        }
    ));
    let name = |target: &azteccraft::native_scene::TransformTarget| {
        let node = editor.history.scene().node(target.node).unwrap();
        format!("o {}", node.name())
    };
    assert_eq!(object_lines(&path), [name(&all[0]), name(&all[2])]);
    assert_eq!(
        before,
        (editor.history.scene().clone(), editor.selection.clone())
    );
}

#[test]
fn export_selection_with_nothing_selected_refuses_without_writing() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.set_selection_members_for_test(Vec::new());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("none.obj");

    assert!(matches!(
        editor.export_obj(&path, true, &|| true),
        Err(OperationError::NoSelection)
    ));
    assert!(!path.exists());
}

#[test]
fn export_selection_of_a_parent_includes_its_nested_descendant_in_world_space() {
    let mut editor = deep_group_editor();
    select_the_root_group(&mut editor);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("group.obj");

    let reply = editor.export_obj(&path, true, &|| true).unwrap();

    assert!(matches!(
        reply,
        OperationReply::Exported {
            objects: 1,
            vertices: 3,
            faces: 1,
        }
    ));
    let view = ViewScene::from_native(editor.history.scene(), VIEW_BUDGET).unwrap();
    assert_eq!(view.instances()[0].world_matrix().w_axis.x, 2.0);
    let base = vertices(&path);
    assert_eq!(base.len(), 3);
    assert!(base.iter().all(|vertex| vertex[0] >= 2.0), "{base:?}");

    let mut translated = deep_group_editor_with_leaf([0.0, 3.0, 0.0], [0.0; 3], [0.0; 3]);
    select_the_root_group(&mut translated);
    let moved = directory.path().join("moved.obj");
    translated.export_obj(&moved, true, &|| true).unwrap();
    let expected: Vec<[f64; 3]> = base
        .iter()
        .map(|vertex| [vertex[0], vertex[1] + 3.0, vertex[2]])
        .collect();
    assert_eq!(vertices(&moved), expected);
}

#[test]
fn a_nested_rotation_about_a_pivot_exports_in_world_space() {
    let directory = tempfile::tempdir().unwrap();
    let mut plain = deep_group_editor();
    select_the_root_group(&mut plain);
    let base = directory.path().join("base.obj");
    plain.export_obj(&base, true, &|| true).unwrap();
    let mut rotated = deep_group_editor_with_leaf(
        [0.0; 3],
        [0.0, 0.0, std::f64::consts::FRAC_PI_2],
        [1.0, 0.0, 0.0],
    );
    select_the_root_group(&mut rotated);
    let path = directory.path().join("rotated.obj");

    rotated.export_obj(&path, true, &|| true).unwrap();

    let group_x = 2.0;
    let pivot_x = 1.0;
    let written = vertices(&path);
    for (vertex, source) in written.iter().zip(vertices(&base)) {
        let (x, y) = (source[0] - group_x - pivot_x, source[1]);
        let expected = [group_x + pivot_x - y, x, source[2]];
        for (value, wanted) in vertex.iter().zip(expected) {
            assert!((value - wanted).abs() < 1e-9, "{vertex:?} vs {expected:?}");
        }
    }
    assert_eq!(written.len(), 3);
}

#[test]
fn a_parent_and_its_child_both_selected_export_the_child_once() {
    let mut editor = deep_group_editor();
    let members = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(members);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("once.obj");

    editor.export_obj(&path, true, &|| true).unwrap();

    assert_eq!(object_lines(&path).len(), 1);
}

fn remote_export(
    editor: &mut Editor,
    path: PathBuf,
    may_install: &dyn Fn() -> bool,
) -> Result<OperationReply, OperationError> {
    editor.remote_operation_guarded(
        crate::editor_operations::RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation: Operation::ExportObj {
                path,
                selection_only: false,
            },
        },
        &eframe::egui::Context::default(),
        may_install,
    )
}

#[test]
fn a_remote_export_whose_install_permission_is_revoked_installs_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("revoked.obj");
    // Refused at the first check, and refused only after the staged file exists.
    for allowed in [0usize, 1] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let asked = std::cell::Cell::new(0usize);
        let may_install = || {
            asked.set(asked.get() + 1);
            asked.get() <= allowed
        };
        let result = remote_export(&mut editor, path.clone(), &may_install);
        assert!(
            matches!(
                result,
                Err(OperationError::ObjExport(ObjExportError::Cancelled))
            ),
            "allowed {allowed}: {result:?}"
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    assert!(remote_export(&mut editor, path.clone(), &|| true).is_ok());
    assert!(path.exists());
}
