//! Issue #313: the Modify menu's Reset and Freeze Transformations rows reach the same
//! `Operation`s as the remote caller (Match needs two objects); the option boxes are visibly
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
fn the_option_boxes_are_visibly_unsupported() {
    let mut editor = two_cube_editor();
    let output = open_modify_menu(&mut editor, &editing_ctx());
    for label in [
        "Reset Transformations option box",
        "Freeze Transformations option box",
        "Match Transformations option box",
    ] {
        assert!(button_disabled(&output, label), "{label}");
        assert!(description(&output, label).is_some_and(|text| text.contains("#313")));
    }
}

#[test]
fn the_match_row_dispatches_one_named_undo_step_with_the_last_selected_as_target() {
    let ctx = editing_ctx();
    let mut editor = two_cube_editor();
    let ids: Vec<_> = editor.selection.members().iter().map(|m| m.node).collect();
    editor
        .apply_operation(Operation::EditNode {
            id: ids[1],
            name: None,
            translation: [Some(4.0), Some(5.0), Some(6.0)],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    let output = open_modify_menu(&mut editor, &ctx);
    assert!(!button_disabled(&output, "Match Transformations"));
    let at = face_text_position(&output, "Match Transformations");
    face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(
        editor.history.undo_label().as_deref(),
        Some("Match Transformations")
    );
    let scene = editor.history.scene();
    assert_eq!(
        scene.node(ids[0]).unwrap().transform().translation,
        [4.0, 5.0, 6.0]
    );
}

#[test]
fn the_match_row_says_why_it_is_disabled() {
    let mut one = two_cube_editor();
    let first = one.history.scene().transform_paths().unwrap()[0].clone();
    one.set_selection_members_for_test(vec![first]);
    let output = open_modify_menu(&mut one, &editing_ctx());
    assert!(button_disabled(&output, "Match Transformations"));
    assert_eq!(
        description(&output, "Match Transformations").as_deref(),
        Some("Unavailable: Select at least two objects; the last selected is the target")
    );
    let mut empty = two_cube_editor();
    empty.deselect_all();
    let output = open_modify_menu(&mut empty, &editing_ctx());
    assert_eq!(
        description(&output, "Match Transformations").as_deref(),
        Some("Unavailable: Select an object first")
    );
    let mut face = Editor::default();
    face.create(Primitive::Cube);
    face.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let output = open_modify_menu(&mut face, &editing_ctx());
    assert!(button_disabled(&output, "Match Transformations"));
    assert!(
        description(&output, "Match Transformations")
            .is_some_and(|text| text.contains("unsupported"))
    );
}

#[test]
fn the_match_row_is_disabled_on_a_read_only_scene() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.into_iter().take(2).collect());
    editor.sync_fields();
    let output = open_modify_menu(&mut editor, &editing_ctx());
    assert!(button_disabled(&output, "Match Transformations"));
}

#[test]
fn the_naming_and_align_rows_are_visibly_disabled_and_point_at_the_issue_and_the_tools() {
    let mut editor = two_cube_editor();
    let output = open_modify_menu(&mut editor, &editing_ctx());
    for (label, tool) in [
        ("Prefix Hierarchy Names", "prefix_hierarchy_names"),
        ("Search and Replace Names", "search_replace_names"),
        ("Align Objects", "align_objects"),
    ] {
        assert!(button_disabled(&output, label), "{label}");
        let reason = description(&output, label).unwrap_or_default();
        assert!(reason.contains("#313") && reason.contains(tool), "{reason}");
    }
}
