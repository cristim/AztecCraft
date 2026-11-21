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
    assert!(has_text(&shown, "R 0.500 G 0.500 B 0.500"), "{shown:?}");
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
