//! The session-material section of the Attribute Editor material tab (#467 T5a).
use super::*;
use crate::app::tests::remote;
use eframe::egui::accesskit::Role;

fn new_lambert() -> MaterialChoice {
    MaterialChoice::New {
        color: DEFAULT_LAMBERT_COLOR,
    }
}

fn cube_with_editor_open() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::AttributeEditor,
        visible: true,
    });
    editor
}

fn shape_id(editor: &Editor) -> NativeId {
    editor.session_material_target().unwrap()
}

fn select_material_tab(editor: &mut Editor) -> Result<OperationReply, OperationError> {
    let shape = shape_id(editor);
    remote(
        editor,
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        },
    )
}

fn frame(editor: &mut Editor) -> egui::FullOutput {
    let context = egui::Context::default();
    context.enable_accesskit();
    frame_in(editor, &context, vec![])
}

fn frame_in(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(420.0, 900.0),
            )),
            events,
            ..Default::default()
        },
        |ui| editor.attribute_editor(ui),
    );
    output.textures_delta.clear();
    output
}

/// The accesskit node with this role and label, once.
fn node_bounds(output: &egui::FullOutput, role: Role, label: &str) -> egui::Rect {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    let found: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role && node.label() == Some(label))
        .collect();
    assert_eq!(found.len(), 1, "{role:?} {label:?}");
    let bounds = found[0].1.bounds().expect("node bounds");
    egui::Rect::from_min_max(
        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
    )
}

fn texts(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

fn has_text(texts: &[String], wanted: &str) -> bool {
    texts.iter().any(|text| text == wanted)
}

#[test]
fn an_authored_shape_gets_a_material_tab_that_remote_selection_resolves() {
    let mut editor = cube_with_editor_open();
    assert!(select_material_tab(&mut editor).is_ok());
    let shape = shape_id(&editor);
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Material(shape))
    );
    let title = format!(
        "{} material",
        editor.history.scene().shape(shape).unwrap().name()
    );
    assert!(has_text(&texts(&frame(&mut editor)), &title));
}

#[test]
fn the_section_names_the_material_group_users_and_colour() {
    let mut editor = cube_with_editor_open();
    select_material_tab(&mut editor).unwrap();
    let shown = texts(&frame(&mut editor));
    assert!(has_text(&shown, "Material: standardSurface1"), "{shown:?}");
    assert!(has_text(&shown, "1 shape user"), "{shown:?}");
    editor.assign_material_to_selection(new_lambert()).unwrap();
    let shown = texts(&frame(&mut editor));
    assert!(has_text(&shown, "Material: lambert2"), "{shown:?}");
    assert!(has_text(&shown, "Shading group: lambert2SG"), "{shown:?}");
    assert!(has_text(&shown, "1 shape user"), "{shown:?}");
    assert!(has_text(&shown, "0.5"), "{shown:?}");
    assert!(
        !shown.iter().any(|text| text.contains("Phong")),
        "{shown:?}"
    );
}

#[test]
fn the_swatch_is_a_named_non_interactive_image_filled_with_the_viewport_byte() {
    let mut editor = cube_with_editor_open();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    select_material_tab(&mut editor).unwrap();
    let output = frame(&mut editor);
    // 0.5 is 127.5 of 255: the viewport rounds to 128, a truncating conversion would give 127.
    let want = egui::Color32::from_rgb(128, 128, 128);
    let swatch = node_bounds(&output, Role::Image, "Color, lambert2");
    let filled: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == want => Some(rect.rect),
            _ => None,
        })
        .collect();
    assert!(
        filled.contains(&swatch),
        "swatch {swatch:?} not among filled rects {filled:?}"
    );
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    assert!(
        update
            .nodes
            .iter()
            .all(|(_, node)| node.label() != Some("Color, lambert2") || node.role() == Role::Image),
        "the swatch must not be a control"
    );
}

#[test]
fn unstored_lambert_attributes_are_disabled_rows_with_their_reason_on_hover() {
    let mut editor = cube_with_editor_open();
    select_material_tab(&mut editor).unwrap();
    let context = egui::Context::default();
    context.enable_accesskit();
    context.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let output = frame_in(&mut editor, &context, vec![]);
    for row in UNSUPPORTED_ROWS {
        assert!(crate::app::tests::button_disabled(&output, row), "{row}");
        let at = node_bounds(&output, Role::Button, row).center();
        frame_in(&mut editor, &context, vec![egui::Event::PointerMoved(at)]);
        frame_in(&mut editor, &context, vec![]);
        let hovered = frame_in(&mut editor, &context, vec![]);
        assert!(
            has_text(&texts(&hovered), NOT_EDITABLE_REASON),
            "{row}: {:?}",
            texts(&hovered)
        );
        frame_in(&mut editor, &context, vec![egui::Event::PointerGone]);
    }
}

#[test]
fn users_count_shapes_sharing_the_initial_group_and_use_the_singular() {
    let mut editor = Editor::default();
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::AttributeEditor,
        visible: true,
    });
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let scene = editor.history.scene();
    let initial = SessionMaterialId::from_index(0);
    assert_eq!(scene.session_material_shape_count(initial), 3);
    editor.assign_material_to_selection(new_lambert()).unwrap();
    let scene = editor.history.scene();
    assert_eq!(scene.session_material_shape_count(initial), 2);
    assert_eq!(
        scene.session_material_shape_count(SessionMaterialId::from_index(1)),
        1
    );
    select_material_tab(&mut editor).unwrap();
    assert!(has_text(&texts(&frame(&mut editor)), "1 shape user"));
    editor
        .assign_material_to_selection(MaterialChoice::Existing(initial))
        .unwrap();
    select_material_tab(&mut editor).unwrap();
    assert!(has_text(&texts(&frame(&mut editor)), "3 shape users"));
}

#[test]
fn the_tab_does_not_resolve_without_a_selection_and_refusal_keeps_the_tab() {
    let mut editor = cube_with_editor_open();
    let shape = shape_id(&editor);
    editor.native_operation(Operation::Select {
        node: None,
        instance_path: None,
    });
    let before = editor.effective_attribute_editor_tab();
    let result = remote(
        &mut editor,
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        },
    );
    assert!(
        matches!(result, Err(OperationError::InvalidInput(_))),
        "{result:?}"
    );
    assert_eq!(editor.effective_attribute_editor_tab(), before);
}

fn dock_and_tab(editor: &Editor) -> (bool, Option<AttributeEditorTab>) {
    (
        editor.sidebar_panel_visible(SidebarPanel::AttributeEditor),
        editor.effective_attribute_editor_tab(),
    )
}

#[test]
fn material_attributes_shows_the_panel_on_the_material_tab_like_the_two_remote_operations() {
    let mut native = Editor::default();
    native.create(Primitive::Cube);
    assert!(!native.sidebar_panel_visible(SidebarPanel::AttributeEditor));
    let dirty = native.is_dirty();
    native.show_material_attributes().unwrap();
    let shape = shape_id(&native);
    assert_eq!(
        dock_and_tab(&native),
        (true, Some(AttributeEditorTab::Material(shape)))
    );
    let mut other = Editor::default();
    other.create(Primitive::Cube);
    for operation in [
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape_id(&other)),
        },
    ] {
        remote(&mut other, operation).unwrap();
    }
    assert_eq!(dock_and_tab(&native), dock_and_tab(&other));
    assert_eq!(
        native.is_dirty(),
        dirty,
        "view state never changes the dirty flag"
    );
}

#[test]
fn material_attributes_is_refused_without_one_shaped_object_and_changes_nothing() {
    let mut editor = Editor::default();
    let refused = |editor: &mut Editor| {
        let before = dock_and_tab(editor);
        let result = editor.show_material_attributes();
        assert!(result.is_err());
        assert_eq!(dock_and_tab(editor), before);
        assert!(editor.error);
    };
    assert!(editor.material_attributes_target().is_err());
    refused(&mut editor);
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.native_operation(Operation::SelectByRule(SelectRule::All));
    assert_eq!(
        editor.material_attributes_target(),
        Err(OperationError::MultipleSelection.to_string())
    );
    refused(&mut editor);
    let mut modal = Editor::default();
    modal.create(Primitive::Cube);
    modal.request_new();
    assert!(matches!(
        modal.show_material_attributes(),
        Err(OperationError::BusyEditing)
    ));
    assert!(!modal.sidebar_panel_visible(SidebarPanel::AttributeEditor));
}

#[test]
fn material_attributes_follows_whether_the_sourced_shape_has_a_phong_binding() {
    let mut phong = crate::app::tests::material_editor();
    assert!(phong.material_attributes_target().is_ok());
    phong.show_material_attributes().unwrap();
    assert!(matches!(
        phong.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Material(_))
    ));
    let (mut plain, _directory) =
        crate::app::features::attribute_editor::ui_tests::open_with_attribute_editor(
            "plain.mb",
            &crate::app::features::attribute_editor::ui_tests::many_shapes(1),
        );
    assert_eq!(
        plain.material_attributes_target(),
        Err(NO_MATERIAL_REASON.to_string())
    );
    assert!(plain.show_material_attributes().is_err());
    assert!(matches!(
        plain.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(_))
    ));
}

const SAME: [f64; 3] = [0.5; 3];

fn colour_editor() -> (Editor, egui::Context) {
    let mut editor = cube_with_editor_open();
    editor.assign_material_to_selection(new_lambert()).unwrap();
    select_material_tab(&mut editor).unwrap();
    let context = egui::Context::default();
    context.enable_accesskit();
    frame_in(&mut editor, &context, vec![]);
    (editor, context)
}

fn key(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Focuses channel `index` and replaces its text, without committing.
fn type_into(editor: &mut Editor, context: &egui::Context, index: usize, text: &str) {
    context.memory_mut(|memory| memory.request_focus(colour_edit_id(index)));
    frame_in(editor, context, vec![]);
    type_focused(editor, context, text);
}

/// Replaces the text of the focused field; empty text deletes the selection.
fn type_focused(editor: &mut Editor, context: &egui::Context, text: &str) {
    let mut events = vec![key(egui::Key::A, egui::Modifiers::COMMAND)];
    events.push(if text.is_empty() {
        key(egui::Key::Backspace, egui::Modifiers::NONE)
    } else {
        egui::Event::Text(text.into())
    });
    frame_in(editor, context, events);
}

fn first_text_input_centre(output: &egui::FullOutput) -> egui::Pos2 {
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    let bounds = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::TextInput)
        .and_then(|(_, node)| node.bounds())
        .unwrap();
    egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    )
}

fn focused(context: &egui::Context) -> Option<egui::Id> {
    context.memory(|memory| memory.focused())
}

fn press_enter(editor: &mut Editor, context: &egui::Context) {
    frame_in(
        editor,
        context,
        vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
    );
}

fn colour(editor: &Editor) -> [f64; 3] {
    let scene = editor.history.scene();
    scene.session_materials()[1].color()
}

#[test]
fn a_typed_colour_commits_one_undo_step_and_undo_restores_it() {
    let (mut editor, context) = colour_editor();
    let before = editor.history.scene().clone();
    type_into(&mut editor, &context, 0, "1");
    assert_eq!(editor.session_color_draft.as_ref().unwrap().text[0], "1");
    assert!(editor.has_draft());
    assert_eq!(colour(&editor), SAME, "typing alone commits nothing");
    press_enter(&mut editor, &context);
    assert_eq!(colour(&editor), [1.0, 0.5, 0.5]);
    assert_eq!(editor.status, "Material colour edited");
    assert!(!editor.has_draft());
    editor.native_operation(Operation::Undo);
    assert_eq!(editor.history.scene(), &before);
    editor.native_operation(Operation::Redo);
    assert_eq!(colour(&editor), [1.0, 0.5, 0.5]);
}

#[test]
fn invalid_text_keeps_the_draft_the_status_and_the_scene() {
    let (mut editor, context) = colour_editor();
    let before = editor.history.scene().clone();
    for bad in ["abc", "1.5", "-0.1", "NaN", "inf", ""] {
        type_into(&mut editor, &context, 1, bad);
        assert_eq!(editor.session_color_draft.as_ref().unwrap().text[1], bad);
        press_enter(&mut editor, &context);
        assert!(editor.error, "{bad:?}");
        assert_eq!(
            editor.status, "Colour channels must be numbers from 0 to 1",
            "{bad:?}"
        );
        assert_eq!(editor.history.scene(), &before, "{bad:?}");
        assert!(editor.session_color_draft.is_some(), "{bad:?}");
    }
}

#[test]
fn a_commit_refuses_when_the_material_changed_since_the_draft_began() {
    let (mut editor, context) = colour_editor();
    type_into(&mut editor, &context, 2, "0.25");
    editor
        .history
        .execute(Command::SetSessionMaterialColor {
            material: SessionMaterialId::from_index(1),
            color: [0.1, 0.1, 0.1],
        })
        .unwrap();
    assert!(!editor.commit_pending());
    assert_eq!(editor.status, "The material changed since this edit began");
    assert_eq!(colour(&editor), [0.10000000149011612; 3]);
    assert_eq!(editor.session_color_draft.as_ref().unwrap().text[2], "0.25");
}

#[test]
fn escape_reverts_the_draft() {
    let (mut editor, context) = colour_editor();
    type_into(&mut editor, &context, 0, "0.9");
    frame_in(
        &mut editor,
        &context,
        vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
    );
    assert!(!editor.has_draft());
    assert_eq!(editor.session_color_draft.as_ref().unwrap().text[0], "0.5");
    assert_eq!(colour(&editor), SAME);
}

#[test]
fn a_pending_native_colour_draft_blocks_the_remote_recolour_and_survives_it() {
    let (mut editor, context) = colour_editor();
    type_into(&mut editor, &context, 0, "0.9");
    let before = editor.history.scene().clone();
    let result = remote(
        &mut editor,
        Operation::SetSessionMaterialColor {
            material: SessionMaterialId::from_index(1),
            color: [0.0, 1.0, 0.0],
        },
    );
    assert!(
        matches!(result, Err(OperationError::BusyEditing)),
        "{result:?}"
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.session_color_draft.as_ref().unwrap().text[0], "0.9");
}

#[test]
fn the_fields_are_labelled_text_inputs_and_disabled_for_the_locked_initial_material() {
    let (mut editor, context) = colour_editor();
    let output = frame_in(&mut editor, &context, vec![]);
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    let mut names = Vec::new();
    for (_, node) in update
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::TextInput)
    {
        assert!(!node.is_disabled());
        let [label] = node.labelled_by() else {
            panic!("a colour field needs exactly one label");
        };
        let (_, label_node) = update.nodes.iter().find(|(id, _)| id == label).unwrap();
        names.push(label_node.value().unwrap_or_default().to_owned());
    }
    names.sort();
    assert_eq!(names, ["Blue", "Green", "Red"]);
    editor
        .assign_material_to_selection(MaterialChoice::Existing(SessionMaterialId::from_index(0)))
        .unwrap();
    assert!(editor.session_color_draft.is_none());
    context.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let output = frame_in(&mut editor, &context, vec![]);
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    let bounds = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::TextInput)
        .and_then(|(_, node)| node.bounds())
        .unwrap();
    let at = egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    );
    frame_in(&mut editor, &context, vec![egui::Event::PointerMoved(at)]);
    frame_in(&mut editor, &context, vec![]);
    let output = frame_in(&mut editor, &context, vec![]);
    assert!(
        has_text(&texts(&output), "The initial material is locked"),
        "{:?}",
        texts(&output)
    );
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    let inputs: Vec<_> = update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == Role::TextInput)
        .collect();
    assert_eq!(inputs.len(), 3);
    assert!(inputs.iter().all(|(_, node)| node.is_disabled()));
}

#[test]
fn a_sourced_scene_refuses_with_the_typed_reason() {
    let (editor, _directory) =
        crate::app::features::attribute_editor::ui_tests::open_with_attribute_editor(
            "one.mb",
            &crate::app::features::attribute_editor::ui_tests::many_shapes(1),
        );
    let shape = editor.history.scene().shapes()[0].id();
    assert_eq!(
        editor.session_color_refusal(shape).as_deref(),
        Some("native ownership or edit effects are unsupported")
    );
    assert!(editor.new_session_color_draft(shape).is_none());
}

#[test]
fn tab_between_the_fields_keeps_one_draft_and_enter_commits_all_three_as_one_step() {
    let (mut editor, context) = colour_editor();
    let before = editor.history.scene().clone();
    type_into(&mut editor, &context, 0, "1");
    frame_in(
        &mut editor,
        &context,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    assert_eq!(focused(&context), Some(colour_edit_id(1)));
    assert_eq!(
        editor.history.scene(),
        &before,
        "Tab to the next field commits nothing"
    );
    type_focused(&mut editor, &context, "0.25");
    frame_in(
        &mut editor,
        &context,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    assert_eq!(focused(&context), Some(colour_edit_id(2)));
    assert_eq!(editor.history.scene(), &before);
    type_focused(&mut editor, &context, "0.75");
    press_enter(&mut editor, &context);
    assert_eq!(colour(&editor), [1.0, 0.25, 0.75]);
    editor.native_operation(Operation::Undo);
    assert_eq!(
        editor.history.scene(),
        &before,
        "one undo restores the whole edit"
    );
}

#[test]
fn leaving_the_group_by_blur_commits_once() {
    let (mut editor, context) = colour_editor();
    let before = editor.history.scene().clone();
    type_into(&mut editor, &context, 0, "1");
    frame_in(
        &mut editor,
        &context,
        vec![key(egui::Key::Tab, egui::Modifiers::NONE)],
    );
    type_focused(&mut editor, &context, "0.25");
    assert_eq!(editor.history.scene(), &before);
    context.memory_mut(|memory| memory.surrender_focus(colour_edit_id(1)));
    frame_in(&mut editor, &context, vec![]);
    frame_in(&mut editor, &context, vec![]);
    assert_eq!(colour(&editor), [1.0, 0.25, 0.5]);
    assert_eq!(editor.status, "Material colour edited");
    editor.native_operation(Operation::Undo);
    assert_eq!(
        editor.history.scene(),
        &before,
        "blur made exactly one history step"
    );
}

#[test]
fn a_pending_channel_draft_disables_the_fields_with_a_reason_and_refuses_the_commit() {
    let (mut editor, context) = colour_editor();
    type_into(&mut editor, &context, 0, "0.9");
    editor.channels[0] = "5".into();
    assert!(editor.has_channel_draft());
    let before = editor.history.scene().clone();
    assert!(!editor.commit_pending());
    assert_eq!(
        editor.status,
        "Finish the channel edit before editing material data"
    );
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.session_color_draft.is_some());
    context.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let output = frame_in(&mut editor, &context, vec![]);
    let at = first_text_input_centre(&output);
    frame_in(&mut editor, &context, vec![egui::Event::PointerMoved(at)]);
    frame_in(&mut editor, &context, vec![]);
    let output = frame_in(&mut editor, &context, vec![]);
    assert!(
        has_text(&texts(&output), "Finish the channel edit first"),
        "{:?}",
        texts(&output)
    );
}

#[test]
fn a_sourced_phong_scene_shows_no_colour_fields() {
    let mut editor = crate::app::tests::material_editor();
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::AttributeEditor,
        visible: true,
    });
    let shape = editor.history.scene().shapes()[0].id();
    remote(
        &mut editor,
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        },
    )
    .unwrap();
    assert!(editor.session_color_draft.is_none());
    let output = frame(&mut editor);
    assert!(texts(&output).iter().any(|text| text.starts_with("Phong")));
    let update = output.platform_output.accesskit_update.as_ref().unwrap();
    assert!(
        !update
            .nodes
            .iter()
            .any(|(_, node)| node.value() == Some("Red") || node.value() == Some("Green"))
    );
}
