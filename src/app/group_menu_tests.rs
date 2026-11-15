//! Issue #42: Edit > Group, Edit > Ungroup and Ctrl/Cmd+G reach the same `Operation`s as the
//! remote caller; the option boxes are visibly unsupported and G, Shift+G stay unbound.
use super::tests::{
    button_disabled, editing_ctx, face_pointer_click, face_text_position, frame, key_event,
    shortcuts_in, two_cube_editor,
};
use super::*;
use eframe::egui::Key;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn open_edit_menu(editor: &mut Editor, ctx: &egui::Context) -> egui::FullOutput {
    let output = frame(editor, ctx, vec![]);
    let edit = face_text_position(&output, "Edit");
    face_pointer_click(editor, ctx, edit, egui::Modifiers::NONE, vec![])
}

fn names_under(editor: &Editor, group: &str) -> Vec<String> {
    let scene = editor.history.scene();
    let group = scene.nodes().iter().find(|n| n.name() == group).unwrap();
    let transform = group.transform_id();
    scene
        .dag_edges()
        .iter()
        .filter(|edge| edge.parent() == Some(transform))
        .filter_map(|edge| {
            scene
                .nodes()
                .iter()
                .find(|n| n.transform_id() == edge.child())
                .map(|n| n.name().to_string())
        })
        .collect()
}

#[test]
fn the_edit_menu_group_and_ungroup_rows_work_on_the_selection_and_undo_together() {
    let ctx = editing_ctx();
    let mut editor = two_cube_editor();
    let output = open_edit_menu(&mut editor, &ctx);
    assert!(!button_disabled(&output, "Group   Ctrl/Cmd G"));
    assert!(!button_disabled(&output, "Ungroup"));
    let at = face_text_position(&output, "Group   Ctrl/Cmd G");
    face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(names_under(&editor, "group1").len(), 2);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Group"));
    assert_eq!(editor.selection.len(), 1);

    let output = open_edit_menu(&mut editor, &ctx);
    let at = face_text_position(&output, "Ungroup");
    face_pointer_click(&mut editor, &ctx, at, egui::Modifiers::NONE, vec![]);
    assert!(
        editor
            .history
            .scene()
            .nodes()
            .iter()
            .all(|n| n.name() != "group1")
    );
    assert_eq!(editor.selection.len(), 2);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Ungroup"));
}

#[test]
fn the_chord_and_the_menu_row_make_the_same_group() {
    let mut by_chord = two_cube_editor();
    let chord = egui::Modifiers::COMMAND;
    shortcuts_in(
        &egui::Context::default(),
        &mut by_chord,
        chord,
        vec![key_event(Key::G, chord, false)],
    );
    let ctx = editing_ctx();
    let mut by_menu = two_cube_editor();
    let output = open_edit_menu(&mut by_menu, &ctx);
    let at = face_text_position(&output, "Group   Ctrl/Cmd G");
    face_pointer_click(&mut by_menu, &ctx, at, egui::Modifiers::NONE, vec![]);
    assert_eq!(
        names_under(&by_chord, "group1"),
        names_under(&by_menu, "group1")
    );
    assert_eq!(by_chord.history.undo_label(), by_menu.history.undo_label());
    assert_eq!(by_chord.selection.len(), by_menu.selection.len());
}

#[test]
fn rows_are_disabled_without_an_object_selection_in_component_modes_and_in_read_only_scenes() {
    let ctx = editing_ctx();
    let mut empty = two_cube_editor();
    empty.deselect_all();
    let output = open_edit_menu(&mut empty, &ctx);
    assert!(button_disabled(&output, "Group   Ctrl/Cmd G"));
    assert!(button_disabled(&output, "Ungroup"));

    for mode in [
        SelectionMode::Face,
        SelectionMode::Vertex,
        SelectionMode::Edge,
    ] {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.native_operation(Operation::SetSelectionMode(mode));
        assert_eq!(editor.selection_mode, mode);
        assert!(!editor.selection.is_empty());
        let output = open_edit_menu(&mut editor, &editing_ctx());
        assert!(button_disabled(&output, "Group   Ctrl/Cmd G"), "{mode:?}");
        assert!(button_disabled(&output, "Ungroup"), "{mode:?}");
    }

    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let output = open_edit_menu(&mut editor, &editing_ctx());
    assert!(button_disabled(&output, "Group   Ctrl/Cmd G"));
    assert!(button_disabled(&output, "Ungroup"));
}

#[test]
fn the_option_boxes_are_visibly_unsupported_and_inert() {
    let ctx = editing_ctx();
    let mut editor = two_cube_editor();
    let output = open_edit_menu(&mut editor, &ctx);
    for label in ["Group option box", "Ungroup option box"] {
        assert!(button_disabled(&output, label), "{label}");
    }
}

#[test]
fn only_command_g_groups_and_both_modifier_states_are_pinned() {
    let ctrl_on_windows_and_linux = egui::Modifiers::COMMAND | egui::Modifiers::CTRL;
    let cmd_on_macos = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..Default::default()
    };
    for (modifiers, key, grouped) in [
        (ctrl_on_windows_and_linux, Key::G, true),
        (cmd_on_macos, Key::G, true),
        (egui::Modifiers::NONE, Key::G, false),
        (egui::Modifiers::SHIFT, Key::G, false),
        (
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            Key::G,
            false,
        ),
        (
            egui::Modifiers::COMMAND | egui::Modifiers::ALT,
            Key::G,
            false,
        ),
        (egui::Modifiers::ALT, Key::G, false),
        // Control on macOS is not Command.
        (egui::Modifiers::CTRL, Key::G, false),
        (egui::Modifiers::COMMAND, Key::H, false),
    ] {
        let mut editor = two_cube_editor();
        let before = editor.history.scene().clone();
        shortcuts_in(
            &egui::Context::default(),
            &mut editor,
            modifiers,
            vec![key_event(key, modifiers, false)],
        );
        assert_eq!(
            editor.history.scene() != &before,
            grouped,
            "{modifiers:?} {key:?}"
        );
    }
    // A held key repeats nothing: egui marks the second event of one context as a repeat.
    let mut editor = two_cube_editor();
    let ctx = egui::Context::default();
    let chord = egui::Modifiers::COMMAND;
    shortcuts_in(
        &ctx,
        &mut editor,
        chord,
        vec![key_event(Key::G, chord, false)],
    );
    let grouped = editor.history.scene().clone();
    shortcuts_in(
        &ctx,
        &mut editor,
        chord,
        vec![key_event(Key::G, chord, true)],
    );
    assert_eq!(editor.history.scene(), &grouped);
}

#[test]
fn the_chord_is_ignored_while_a_dialog_or_the_close_prompt_is_open() {
    for closing in [false, true] {
        let mut editor = two_cube_editor();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.new_document().unwrap();
            assert!(editor.files.is_modal());
        }
        let before = editor.history.scene().clone();
        let chord = egui::Modifiers::COMMAND;
        shortcuts_in(
            &egui::Context::default(),
            &mut editor,
            chord,
            vec![key_event(Key::G, chord, false)],
        );
        assert_eq!(editor.history.scene(), &before, "closing={closing}");
    }
}
