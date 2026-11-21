//! #458: the Attribute Editor Camera tab through the real Editor and real frames. Every control
//! must give the same film back and the same refusal as the MCP `set_camera_film_back` call for
//! the same input. Text is entered by focusing the field, setting its text and blurring it.
use super::image_plane_tab::{click_node, node_pos};
use super::rig::{Rig, SIZE};
use crate::app::Editor;
use crate::app::features::camera_attributes::{FIELDS, field_id};
use crate::app::tests::click_at;
use crate::editor_operations::{
    AttributeEditorTab, Operation, OperationReply, RemoteRequest, SidebarPanel,
};
use azteccraft::viewport::{FilmBack, FilmBackEdit, FilmFit, FilmRollOrder, Lens};
use eframe::egui::{self, accesskit::Role};

/// The field order of the tab (`camera_attributes` constants).
const FOCAL: usize = 0;
const SQUEEZE: usize = 3;
const OVERSCAN: usize = 14;

const LABELS: [&str; FIELDS] = [
    "Focal Length (mm)",
    "Horizontal Film Aperture (in)",
    "Vertical Film Aperture (in)",
    "Lens Squeeze Ratio",
    "Film Fit Offset (in)",
    "Horizontal Film Offset (in)",
    "Vertical Film Offset (in)",
    "Pre Scale",
    "Horizontal Film Translate",
    "Vertical Film Translate",
    "Horizontal Film Roll Pivot",
    "Vertical Film Roll Pivot",
    "Film Roll Value (degrees)",
    "Post Scale",
    "Overscan",
    "Camera Scale",
];

fn tab_rig() -> Rig {
    let mut rig = Rig::new(true);
    for operation in [
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Camera,
        },
    ] {
        rig.editor.native_operation(operation);
    }
    rig.settle();
    rig
}

fn film(editor: &Editor) -> FilmBack {
    match editor.camera.lens() {
        Lens::Film(film) => film,
        other => panic!("expected a film back, got {other:?}"),
    }
}

fn mcp(
    editor: &mut Editor,
    edit: FilmBackEdit,
) -> Result<OperationReply, crate::editor_operations::OperationError> {
    let token = editor.scene_token;
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SetCameraFilmBack(edit),
        },
        &egui::Context::default(),
    )
}

fn edit_text(rig: &mut Rig, index: usize, text: &str) {
    rig.ctx.memory_mut(|m| m.request_focus(field_id(index)));
    rig.settle();
    rig.editor.camera_drafts[index] = text.to_owned();
    rig.frame(vec![]);
    rig.ctx.memory_mut(|m| m.surrender_focus(field_id(index)));
    rig.settle();
}

fn edit_for(index: usize, value: f64) -> FilmBackEdit {
    let mut edit = FilmBackEdit::default();
    match index {
        0 => edit.focal_mm = Some(value),
        1 => edit.aperture_in[0] = Some(value),
        2 => edit.aperture_in[1] = Some(value),
        3 => edit.lens_squeeze = Some(value),
        4 => edit.film_fit_offset_in = Some(value),
        5 => edit.film_offset_in[0] = Some(value),
        6 => edit.film_offset_in[1] = Some(value),
        7 => edit.pre_scale = Some(value),
        8 => edit.film_translate[0] = Some(value),
        9 => edit.film_translate[1] = Some(value),
        10 => edit.film_roll_pivot[0] = Some(value),
        11 => edit.film_roll_pivot[1] = Some(value),
        12 => edit.film_roll_degrees = Some(value),
        13 => edit.post_scale = Some(value),
        14 => edit.overscan = Some(value),
        _ => edit.camera_scale = Some(value),
    }
    edit
}

fn label_pos(rig: &Rig, label: &str) -> egui::Pos2 {
    rig.update()
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        .next()
        .unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn each_text_control_gives_the_same_film_back_as_the_mcp_call() {
    let mut rig = tab_rig();
    let mut twin = Editor::default();
    for (index, label) in LABELS.iter().enumerate() {
        // A value no other field shares, valid for every field (a negative one for the pure offsets).
        let value = 0.5 + index as f64 / 16.0;
        edit_text(&mut rig, index, &value.to_string());
        assert!(!rig.editor.error, "{}: {}", label, rig.editor.status);
        mcp(&mut twin, edit_for(index, value)).unwrap();
    }
    assert_eq!(film(&rig.editor), film(&twin));
    // Spot-check that the slots are not all the same value.
    assert_eq!(film(&rig.editor).focal_mm, 0.5);
    assert_eq!(film(&rig.editor).camera_scale, 0.5 + 15.0 / 16.0);
    assert_eq!(rig.editor.status, "Film back set");
}

#[test]
fn an_untouched_field_never_rewrites_a_value_and_equal_text_is_no_change() {
    let mut rig = tab_rig();
    let exact = 35.0_f64.next_down();
    mcp(
        &mut rig.editor,
        FilmBackEdit {
            focal_mm: Some(exact),
            ..Default::default()
        },
    )
    .unwrap();
    rig.settle();
    for index in 0..FIELDS {
        rig.ctx.memory_mut(|m| m.request_focus(field_id(index)));
        rig.settle();
        rig.ctx.memory_mut(|m| m.surrender_focus(field_id(index)));
        rig.settle();
    }
    assert_eq!(film(&rig.editor).focal_mm.to_bits(), exact.to_bits());
    // The same number written differently is a numeric no-op, not a change.
    let before = rig.editor.camera.clone();
    edit_text(&mut rig, OVERSCAN, "1.0");
    assert_eq!(rig.editor.status, "No change");
    assert!(rig.editor.camera.same_capture(&before));
}

#[test]
fn text_that_is_not_a_number_or_that_the_operation_refuses_names_the_field_and_keeps_the_value() {
    for (index, bad, named) in [
        (FOCAL, "abc", "Focal Length (mm)"),
        (OVERSCAN, "", "Overscan"),
        // Parse as numbers; the operation refuses them with its own field name.
        (FOCAL, "0", "focal"),
        (FOCAL, "-5", "focal"),
        (SQUEEZE, "NaN", "squeeze"),
        (OVERSCAN, "inf", "overscan"),
    ] {
        let mut rig = tab_rig();
        let before = rig.editor.camera.clone();
        edit_text(&mut rig, index, bad);
        assert!(rig.editor.error, "{bad}");
        assert!(
            rig.editor
                .status
                .to_lowercase()
                .contains(&named.to_lowercase()),
            "{bad}: {}",
            rig.editor.status
        );
        assert!(rig.editor.camera.same_capture(&before), "{bad}");
        rig.settle();
        assert_ne!(rig.editor.camera_drafts[index], bad, "{bad}");
    }
}

#[test]
fn the_combos_and_the_gate_checkbox_send_the_typed_operation() {
    let mut rig = tab_rig();
    click_node(&mut rig, Role::CheckBox, "Display Film Gate");
    assert!(film(&rig.editor).display_film_gate);
    click_node(&mut rig, Role::ComboBox, "Film Fit");
    let at = label_pos(&rig, "Vertical");
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
    assert_eq!(film(&rig.editor).film_fit, FilmFit::Vertical);
    click_node(&mut rig, Role::ComboBox, "Film Roll Order");
    let at = label_pos(&rig, "Translate Rotate");
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
    assert_eq!(
        film(&rig.editor).film_roll_order,
        FilmRollOrder::TranslateRotate
    );
    let mut twin = Editor::default();
    mcp(
        &mut twin,
        FilmBackEdit {
            display_film_gate: Some(true),
            film_fit: Some(FilmFit::Vertical),
            film_roll_order: Some(FilmRollOrder::TranslateRotate),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(film(&rig.editor), film(&twin));
}

#[test]
fn every_control_is_named_and_a_file_prompt_disables_it_with_a_reason() {
    let mut rig = tab_rig();
    let update = rig.update();
    for label in LABELS {
        assert!(
            update
                .nodes
                .iter()
                .any(|(_, n)| n.role() == Role::TextInput && n.label() == Some(label)),
            "no text input named {label:?}"
        );
    }
    for (role, label) in [
        (Role::CheckBox, "Display Film Gate"),
        (Role::ComboBox, "Film Fit"),
        (Role::ComboBox, "Film Roll Order"),
    ] {
        let _ = node_pos(&rig, role, label);
    }
    let before = rig.editor.camera.clone();
    rig.editor.native_operation(Operation::NewScene);
    assert!(rig.editor.files.is_modal());
    rig.settle();
    let update = rig.update();
    let state = |role: Role, label: &str| {
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
    for label in LABELS {
        assert_eq!(
            state(Role::TextInput, label),
            (true, reason.clone()),
            "{label}"
        );
    }
    assert_eq!(
        state(Role::CheckBox, "Display Film Gate"),
        (true, reason.clone())
    );
    assert_eq!(state(Role::ComboBox, "Film Fit"), (true, reason.clone()));
    assert_eq!(state(Role::ComboBox, "Film Roll Order"), (true, reason));
    // The disabled controls cannot blur, so the guard itself is checked: both callers are refused.
    let refused = rig
        .editor
        .native_operation_result(Operation::SetCameraFilmBack(edit_for(FOCAL, 50.0)));
    assert!(
        matches!(
            refused,
            Err(crate::editor_operations::OperationError::BusyEditing)
        ),
        "{refused:?}"
    );
    assert!(mcp(&mut rig.editor, edit_for(FOCAL, 50.0)).is_err());
    assert!(rig.editor.camera.same_capture(&before));
}

#[test]
fn the_notes_report_the_session_only_rule_and_the_orthographic_state() {
    let mut rig = tab_rig();
    let texts = |rig: &Rig| -> Vec<String> {
        rig.update()
            .nodes
            .iter()
            .filter_map(|(_, n)| n.label().or(n.value()).map(str::to_owned))
            .collect()
    };
    let shown = texts(&rig);
    assert!(
        shown.iter().any(|t| t.starts_with("Session only:")),
        "{shown:?}"
    );
    assert!(!shown.iter().any(|t| t.starts_with("No effect until")));
    rig.editor.native_operation(Operation::SetView(
        crate::editor_operations::ViewPreset::Top,
    ));
    rig.settle();
    assert!(
        texts(&rig)
            .iter()
            .any(|t| t == "No effect until the view is perspective.")
    );
}

#[test]
fn the_tab_needs_no_selection_and_a_remote_edit_shows_in_the_fields() {
    let mut rig = tab_rig();
    rig.editor.native_operation(Operation::DeselectAll);
    rig.settle();
    assert_eq!(
        rig.editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Camera)
    );
    mcp(
        &mut rig.editor,
        FilmBackEdit {
            focal_mm: Some(50.0),
            ..Default::default()
        },
    )
    .unwrap();
    rig.settle();
    assert_eq!(rig.editor.camera_drafts[FOCAL], "50");
}

fn note_shown(rig: &Rig, starts_with: &str) -> bool {
    rig.update().nodes.iter().any(|(_, n)| {
        n.label()
            .or(n.value())
            .is_some_and(|text| text.starts_with(starts_with))
    })
}

#[test]
fn a_remote_edit_made_while_a_field_is_focused_but_untouched_survives_the_blur() {
    let mut rig = tab_rig();
    rig.ctx.memory_mut(|m| m.request_focus(field_id(FOCAL)));
    rig.settle();
    mcp(&mut rig.editor, edit_for(FOCAL, 50.0)).unwrap();
    rig.settle();
    rig.ctx.memory_mut(|m| m.surrender_focus(field_id(FOCAL)));
    rig.settle();
    assert_eq!(film(&rig.editor).focal_mm, 50.0);
    assert_eq!(rig.editor.camera_drafts[FOCAL], "50");
}

#[test]
fn escape_drops_the_typed_text_and_sends_nothing() {
    let mut rig = tab_rig();
    let before = rig.editor.camera.clone();
    rig.ctx.memory_mut(|m| m.request_focus(field_id(FOCAL)));
    rig.settle();
    rig.editor.camera_drafts[FOCAL] = "80".into();
    rig.frame(vec![]);
    rig.frame(vec![crate::app::tests::key_event(
        egui::Key::Escape,
        egui::Modifiers::NONE,
        false,
    )]);
    rig.settle();
    assert!(rig.editor.camera.same_capture(&before));
    assert_eq!(rig.editor.camera_drafts[FOCAL], "35");
    assert!(!rig.editor.error, "{}", rig.editor.status);
}

#[test]
fn a_held_drag_and_a_pending_draft_refuse_the_operation_natively() {
    use crate::editor_operations::OperationError;
    let mut rig = tab_rig();
    let before = rig.editor.camera.clone();
    // A pending invalid draft elsewhere: the native caller may only commit a valid one.
    rig.editor.channels[0] = "invalid".into();
    let refused = rig
        .editor
        .native_operation_result(Operation::SetCameraFilmBack(edit_for(FOCAL, 50.0)));
    assert!(refused.is_err(), "{refused:?}");
    assert!(rig.editor.camera.same_capture(&before));
    assert!(matches!(
        mcp(&mut rig.editor, edit_for(FOCAL, 50.0)),
        Err(OperationError::BusyEditing)
    ));
    rig.editor.channels = Default::default();
    rig.editor.sync_fields();
    // A held move: both callers are refused.
    let target = rig.editor.selected_move_target().unwrap();
    rig.editor
        .apply_operation_guarded(
            azteccraft::history::ManipulationProvenance::Remote,
            Operation::BeginMove {
                target,
                axis: azteccraft::native_scene::WorldAxis::X,
                expected_selection_generation: None,
            },
            &|| true,
        )
        .unwrap();
    assert!(matches!(
        rig.editor
            .native_operation_result(Operation::SetCameraFilmBack(edit_for(FOCAL, 50.0))),
        Err(OperationError::BusyManipulation)
    ));
    assert!(matches!(
        mcp(&mut rig.editor, edit_for(FOCAL, 50.0)),
        Err(OperationError::BusyManipulation)
    ));
    assert!(rig.editor.camera.same_capture(&before));
}

#[test]
fn the_session_lens_and_blocked_camera_notes_show_exactly_when_they_apply() {
    let mut rig = tab_rig();
    assert!(note_shown(&rig, "These rows show the session lens"));
    assert!(!note_shown(&rig, "The open file's film back for camera"));
    assert!(!note_shown(&rig, "The lens is set by field of view"));
    // A file stating a film back the editor cannot carry blocks its camera.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("partial_camera.ma");
    std::fs::write(
        &path,
        "//Maya ASCII 2026 scene\nrequires maya \"2026\";\ncurrentUnit -l centimeter -a degree -t film;\nfileInfo \"AztecCraft\" \"1\";\nselect -ne :perspShape;\n\tsetAttr \".hfa\" 1.2;\n",
    )
    .unwrap();
    let mut blocked = Rig::new(false);
    blocked
        .editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    assert_eq!(
        blocked.editor.history.scene().blocked_camera_names(),
        ["persp"]
    );
    for operation in [
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Camera,
        },
    ] {
        blocked.editor.native_operation(operation);
    }
    blocked.settle();
    assert!(note_shown(
        &blocked,
        "The open file's film back for camera persp is not applied"
    ));
    assert!(!blocked.update().nodes.iter().any(|(_, n)| {
        n.label()
            .or(n.value())
            .is_some_and(|t| t.contains("default lens"))
    }));
    // A field-of-view lens.
    rig.editor.camera =
        azteccraft::viewport::Camera::new(glam::Vec3::ZERO, 0.6, 0.4, 8.0, 0.8, 0.1, 1000.0)
            .unwrap();
    rig.settle();
    assert!(note_shown(&rig, "The lens is set by field of view"));
}
