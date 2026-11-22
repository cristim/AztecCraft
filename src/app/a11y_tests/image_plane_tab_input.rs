//! #459 P9b-1 review: commit on focus loss only, the modal state, the native reset flow and the
//! name field's whitespace, through the real Editor and frames.
use super::image_plane_tab::{
    DEPTH, NAME, OFFSET, click_node, edit_text, mcp, node_pos, plane, png, tab_rig,
};
use crate::app::features::image_plane::{ATTRIBUTES_ROW_LABEL, MENU_PATH, RESET_COVERAGE_LABEL};
use crate::app::features::image_plane_attributes::field_id;
use crate::app::tests::key_event;
use crate::editor_operations::Operation;
use azteccraft::image_plane::model::{CoverageParts, ImagePlaneEdit};
use eframe::egui::{self, accesskit::Role};

#[test]
fn typing_commits_nothing_until_the_field_loses_focus_and_then_exactly_the_final_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    rig.ctx.memory_mut(|m| m.request_focus(field_id(DEPTH)));
    rig.settle();
    rig.frame(vec![key_event(
        egui::Key::A,
        egui::Modifiers::COMMAND,
        false,
    )]);
    // Character by character: 2, 25, 250. A per-keystroke commit would set the depth at once.
    for key in ["2", "5", "0"] {
        rig.frame(vec![egui::Event::Text(key.into())]);
        assert_eq!(plane(&rig).depth, 100.0, "committed while typing {key}");
        assert!(!rig.editor.error, "{}", rig.editor.status);
    }
    assert_eq!(rig.editor.plane_drafts[DEPTH], "250");
    rig.ctx.memory_mut(|m| m.surrender_focus(field_id(DEPTH)));
    rig.settle();
    assert_eq!(plane(&rig).depth, 250.0);
}

#[test]
fn the_controls_are_disabled_behind_a_file_prompt_and_a_forced_edit_is_not_applied() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    rig.editor.native_operation(Operation::NewScene);
    assert!(rig.editor.files.is_modal());
    rig.settle();
    let state = |rig: &super::rig::Rig, role: Role, label: &str| {
        let update = rig.update();
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == role && n.label() == Some(label))
            .unwrap_or_else(|| panic!("no {role:?} {label:?}"));
        (
            node.is_disabled(),
            node.description().unwrap_or("").to_owned(),
        )
    };
    let reason = "Unavailable: a dialog is open".to_owned();
    assert_eq!(
        state(&rig, Role::TextInput, "Depth (cm)"),
        (true, reason.clone())
    );
    assert_eq!(
        state(&rig, Role::CheckBox, "Use Image Sequence"),
        (true, String::new())
    );
    let before = rig.editor.image_plane.clone();
    edit_text(&mut rig, DEPTH, "250");
    edit_text(&mut rig, OFFSET, "5");
    assert_eq!(rig.editor.image_plane, before);
}

#[test]
fn the_reset_step_in_the_import_hint_works_and_the_import_then_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    mcp(
        &mut rig.editor,
        ImagePlaneEdit {
            coverage_parts: CoverageParts {
                width: Some(4),
                height: Some(2),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    rig.settle();
    let small = png(dir.path(), "small.png", 2, 2);
    rig.editor.run_import_image(small.clone());
    assert!(rig.editor.error);
    let hint = format!(
        " Use {RESET_COVERAGE_LABEL} in {MENU_PATH} > {ATTRIBUTES_ROW_LABEL}, then import again."
    );
    assert!(rig.editor.status.ends_with(&hint), "{}", rig.editor.status);
    assert_eq!(plane(&rig).image_name, dir.path().join("plate.png"));
    // The hint names a real control: click it, then import again.
    click_node(&mut rig, Role::Button, RESET_COVERAGE_LABEL);
    assert!(plane(&rig).coverage.is_none());
    rig.editor.run_import_image(small);
    assert!(!rig.editor.error, "{}", rig.editor.status);
    assert_eq!(rig.editor.image_plane.as_ref().unwrap().image_size, (2, 2));
    assert!(plane(&rig).coverage.is_none());
}

#[test]
fn the_name_field_passes_its_text_through_and_the_numeric_fields_ignore_edge_spaces() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    let before = rig.editor.image_plane.clone();
    // A trailing space is part of the name: the operation refuses it for its extension. A
    // trimmed name would have reached the file open and failed there instead.
    edit_text(&mut rig, NAME, "missing.png ");
    assert!(
        rig.editor.status.contains(".png, .jpg or .jpeg"),
        "{}",
        rig.editor.status
    );
    // Only spaces: the operation's extension rule speaks, not the empty-name rule.
    edit_text(&mut rig, NAME, "   ");
    assert!(
        rig.editor.status.contains(".png, .jpg or .jpeg"),
        "{}",
        rig.editor.status
    );
    assert_eq!(rig.editor.image_plane, before);
    // An untouched name is never rewritten.
    rig.ctx.memory_mut(|m| m.request_focus(field_id(NAME)));
    rig.settle();
    rig.ctx.memory_mut(|m| m.surrender_focus(field_id(NAME)));
    rig.settle();
    assert_eq!(rig.editor.image_plane, before);
    // Numbers accept surrounding spaces.
    edit_text(&mut rig, OFFSET, " 7 ");
    edit_text(&mut rig, DEPTH, "  2.5  ");
    assert_eq!(plane(&rig).frame_offset, 7);
    assert_eq!(plane(&rig).depth, 2.5);
    let _ = node_pos;
}
