//! Issue #313: the Modify menu's Reset and Freeze Transformations rows reach the same
//! `Operation`s as the remote caller; Match Transformations and the option boxes are visibly
//! unsupported, and every disabled row says why to AccessKit.
use super::tests::{
    button_disabled, editing_ctx, face_pointer_click, face_text_position, frame, two_cube_editor,
};
use super::*;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

const ROWS: [&str; 2] = ["Reset Transformations", "Freeze Transformations"];

fn open_modify_menu(editor: &mut Editor, ctx: &egui::Context) -> egui::FullOutput {
    let output = frame(editor, ctx, vec![]);
    let modify = face_text_position(&output, "Modify");
    face_pointer_click(editor, ctx, modify, egui::Modifiers::NONE, vec![])
}

fn description(output: &egui::FullOutput, label: &str) -> Option<String> {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
        })
        .and_then(|(_, node)| node.description().map(str::to_owned))
}

#[test]
fn the_rows_dispatch_one_named_undo_step_each() {
    for (row, label) in [
        (ROWS[0], "Reset Transformations"),
        (ROWS[1], "Freeze Transformations"),
    ] {
        let ctx = editing_ctx();
        let mut editor = two_cube_editor();
        let ids: Vec<_> = editor
            .history
            .scene()
            .nodes()
            .iter()
            .map(|n| n.id())
            .collect();
        for id in ids {
            editor
                .apply_operation(Operation::EditNode {
                    id,
                    name: None,
                    translation: [Some(1.0), None, None],
                    rotation_degrees: [None; 3],
                    scale: [Some(2.0), Some(2.0), Some(2.0)],
                })
                .unwrap();
        }
        let output = open_modify_menu(&mut editor, &ctx);
        assert!(!button_disabled(&output, row));
        let at = face_text_position(&output, row);
        face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
        assert!(!editor.error, "{}", editor.status);
        assert_eq!(editor.history.undo_label().as_deref(), Some(label));
    }
}

#[test]
fn the_rows_are_disabled_with_a_reason_without_an_object_selection() {
    let ctx = editing_ctx();
    let mut empty = two_cube_editor();
    empty.deselect_all();
    let output = open_modify_menu(&mut empty, &ctx);
    for row in ROWS {
        assert!(button_disabled(&output, row), "{row}");
        assert_eq!(
            description(&output, row).as_deref(),
            Some("Unavailable: Select an object first"),
            "{row}"
        );
    }
}

#[test]
fn the_rows_are_disabled_in_component_modes_and_read_only_scenes() {
    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.native_operation(Operation::SetSelectionMode(mode));
        let output = open_modify_menu(&mut editor, &editing_ctx());
        for row in ROWS {
            assert!(button_disabled(&output, row), "{mode:?} {row}");
            assert!(description(&output, row).is_some_and(|text| text.contains("unsupported")));
        }
    }
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let output = open_modify_menu(&mut editor, &editing_ctx());
    for row in ROWS {
        assert!(button_disabled(&output, row), "{row}");
    }
}

#[test]
fn match_transformations_and_the_option_boxes_are_visibly_unsupported() {
    let mut editor = two_cube_editor();
    let output = open_modify_menu(&mut editor, &editing_ctx());
    for label in [
        "Match Transformations",
        "Reset Transformations option box",
        "Freeze Transformations option box",
    ] {
        assert!(button_disabled(&output, label), "{label}");
    }
    assert!(
        description(&output, "Match Transformations").is_some_and(|text| text.contains("#313"))
    );
}
