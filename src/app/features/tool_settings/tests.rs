use super::*;
use crate::app::features::attribute_editor::ctrl_a_target;
use crate::app::tests::{click_at, frame_with};
use crate::editor_operations::{ManipulationSpace, Operation, SidebarPanel};
use azteccraft::native_scene::{MoveSpace, WorldAxis};
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, accesskit};

const SIZE: Vec2 = egui::vec2(1440.0, 900.0);

fn setup() -> (Editor, egui::Context) {
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    editor.create(azteccraft::scene::Primitive::Cube);
    (editor, context)
}

#[derive(Debug)]
struct Node {
    id: accesskit::NodeId,
    label: String,
    value: Option<String>,
    rect: Rect,
    disabled: bool,
    description: Option<String>,
}

fn nodes(output: &egui::FullOutput) -> Vec<Node> {
    let update = output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled");
    update
        .nodes
        .iter()
        .filter_map(|(id, node)| {
            let bounds = node.bounds()?;
            Some(Node {
                id: *id,
                label: node.label()?.to_owned(),
                value: node.value().map(str::to_owned),
                rect: Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
                description: node.description().map(str::to_owned),
            })
        })
        .collect()
}

fn named(output: &egui::FullOutput, label: &str) -> Vec<Node> {
    nodes(output)
        .into_iter()
        .filter(|node| node.label == label)
        .collect()
}

fn one(output: &egui::FullOutput, label: &str) -> Node {
    let mut found = named(output, label);
    assert_eq!(found.len(), 1, "nodes named {label:?}: {found:?}");
    found.remove(0)
}

fn text_drawn(output: &egui::FullOutput, text: &str) -> bool {
    output.shapes.iter().any(
        |shape| matches!(&shape.shape, egui::Shape::Text(drawn) if drawn.galley.job.text == text),
    )
}

fn settle(editor: &mut Editor, context: &egui::Context) -> egui::FullOutput {
    frame_with(editor, context, SIZE, 1.0, vec![]);
    frame_with(editor, context, SIZE, 1.0, vec![])
}

fn show_panel(editor: &mut Editor) {
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ToolSettings,
        visible: true,
    });
}

fn tool(editor: &mut Editor, tool: ActiveTool) {
    editor.native_operation(Operation::SetActiveTool(tool));
}

fn press_release(at: Pos2) -> Vec<Event> {
    [true, false]
        .map(|pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        })
        .into()
}

fn double_click(editor: &mut Editor, context: &egui::Context, at: Pos2) -> egui::FullOutput {
    frame_with(editor, context, SIZE, 1.0, vec![Event::PointerMoved(at)]);
    for _ in 0..2 {
        for event in press_release(at) {
            frame_with(editor, context, SIZE, 1.0, vec![event]);
        }
    }
    settle(editor, context)
}

/// The Tool Box button named `label`: the leftmost node with that name.
fn toolbox_button(output: &egui::FullOutput, label: &str) -> Node {
    named(output, label)
        .into_iter()
        .min_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
        .unwrap_or_else(|| panic!("no Tool Box button {label:?}"))
}

#[test]
fn status_line_toggle_shows_and_hides_the_panel_in_the_one_panel_dock() {
    let (mut editor, context) = setup();
    let output = settle(&mut editor, &context);
    let undo = editor.history.undo_label();
    let dirty = editor.is_dirty();
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    let toggle = one(&output, "Tool Settings");
    assert!(!toggle.disabled);
    assert!(!text_drawn(&output, PANEL_TITLE));
    for visible in [true, false] {
        click_at(&mut editor, &context, SIZE, toggle.rect.center());
        assert_eq!(
            editor.sidebar_panel_visible(SidebarPanel::ToolSettings),
            visible
        );
        assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        let output = settle(&mut editor, &context);
        assert_eq!(text_drawn(&output, PANEL_TITLE), visible);
    }
    assert_eq!(editor.sidebar_dock, None);
    // A dock switch is view state: no history entry and no dirty scene.
    assert_eq!(editor.history.undo_label(), undo);
    assert_eq!(editor.is_dirty(), dirty);
}

#[test]
fn the_panel_replaces_the_viewport_row_and_follows_the_active_tool() {
    let (mut editor, context) = setup();
    tool(&mut editor, ActiveTool::Move);
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, "Move tool"), "the row is drawn");
    show_panel(&mut editor);
    let output = settle(&mut editor, &context);
    assert!(!text_drawn(&output, "Move tool"), "the row is hidden");
    assert!(text_drawn(&output, "Move Tool"));
    assert!(text_drawn(&output, "Move Settings"));
    let combo = one(&output, AXIS_ORIENTATION_LABEL);
    assert_eq!(combo.value.as_deref(), Some("World"));
    assert!(!combo.disabled);
    assert!(named(&output, NUMERIC_FIELD_LABEL).is_empty());
    let reset = one(&output, RESET_TOOL_LABEL);
    assert!(reset.disabled);
    assert_eq!(
        reset.description.as_deref(),
        Some("Unavailable: Reset Tool is not implemented yet")
    );

    tool(&mut editor, ActiveTool::Rotate);
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, "Rotate Tool"));
    assert!(!text_drawn(&output, "Rotate tool"));
    assert_eq!(
        one(&output, AXIS_ORIENTATION_LABEL).value.as_deref(),
        Some("Object")
    );
    assert_eq!(
        one(&output, "Numeric entry axis").value.as_deref(),
        Some("X")
    );
    assert_eq!(one(&output, NUMERIC_FIELD_LABEL).value.as_deref(), Some(""));
    assert_eq!(named(&output, "Step Snap").len(), 1);

    tool(&mut editor, ActiveTool::Scale);
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, "Scale Tool"));
    assert_eq!(
        one(&output, AXIS_ORIENTATION_LABEL).value.as_deref(),
        Some("World")
    );
    // Scale's numeric entry starts on the Uniform handle (`ManipulationOptions` default).
    assert_eq!(
        one(&output, "Numeric entry handle").value.as_deref(),
        Some("Uniform")
    );
    assert_eq!(named(&output, "Scale Step Snap").len(), 1);

    tool(&mut editor, ActiveTool::Select);
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, "Select Tool"));
    assert!(named(&output, AXIS_ORIENTATION_LABEL).is_empty());

    // Hiding the panel brings the row back.
    tool(&mut editor, ActiveTool::Move);
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ToolSettings,
        visible: false,
    });
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, "Move tool"));
    assert!(named(&output, AXIS_ORIENTATION_LABEL).is_empty());
}

#[test]
fn axis_orientation_in_the_panel_sets_the_move_object_space() {
    let (mut editor, context) = setup();
    tool(&mut editor, ActiveTool::Move);
    show_panel(&mut editor);
    let output = settle(&mut editor, &context);
    let undo = editor.history.undo_label();
    let combo = one(&output, AXIS_ORIENTATION_LABEL);
    let opened = click_at(&mut editor, &context, SIZE, combo.rect.center());
    let object = named(&opened, "Object")
        .into_iter()
        .find(|node| !node.disabled && node.rect.top() >= combo.rect.top())
        .expect("the popup's Object row");
    // Maya's unimplemented orientations are listed disabled.
    assert!(
        named(&opened, "Normal")
            .iter()
            .any(|node| node.disabled && node.rect.top() >= combo.rect.top())
    );
    click_at(&mut editor, &context, SIZE, object.rect.center());
    assert_eq!(editor.manipulation.move_space(), MoveSpace::Object);
    let output = settle(&mut editor, &context);
    assert_eq!(
        one(&output, AXIS_ORIENTATION_LABEL).value.as_deref(),
        Some("Object")
    );
    // Tool settings are session state, like the row's selector.
    assert_eq!(editor.history.undo_label(), undo);
}

#[test]
fn numeric_entry_in_the_panel_rotates_through_one_history_entry() {
    let (mut editor, context) = setup();
    tool(&mut editor, ActiveTool::Rotate);
    show_panel(&mut editor);
    let output = settle(&mut editor, &context);
    assert!(text_drawn(&output, NUMERIC_HEADING));
    let field = one(&output, NUMERIC_FIELD_LABEL);
    let apply = named(&output, "Apply")
        .into_iter()
        .find(|node| (node.rect.center().y - field.rect.center().y).abs() < 4.0)
        .expect("the panel's Apply button beside its field");
    click_at(&mut editor, &context, SIZE, field.rect.center());
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        vec![Event::Text("30".into())],
    );
    let output = settle(&mut editor, &context);
    assert_eq!(
        one(&output, NUMERIC_FIELD_LABEL).value.as_deref(),
        Some("30")
    );
    click_at(&mut editor, &context, SIZE, apply.rect.center());
    settle(&mut editor, &context);
    let rotation = editor.history.scene().nodes()[0].transform().rotation;
    assert!((rotation[0] - 30.0).abs() < 1e-4, "{rotation:?}");
    assert_eq!(editor.history.undo_label().as_deref(), Some("Rotate"));
    editor.history_step(false);
    let rotation = editor.history.scene().nodes()[0].transform().rotation;
    assert_eq!(rotation[0], 0.0);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Create"));
}

#[test]
fn double_clicking_a_tool_box_tool_opens_its_settings() {
    let (mut editor, context) = setup();
    let output = settle(&mut editor, &context);
    let rotate = toolbox_button(&output, "Rotate");
    let output = double_click(&mut editor, &context, rotate.rect.center());
    assert_eq!(editor.active_tool, ActiveTool::Rotate);
    assert!(editor.sidebar_panel_visible(SidebarPanel::ToolSettings));
    assert!(text_drawn(&output, "Rotate Tool"));
    // A single click only selects the tool and leaves the dock alone.
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ChannelBox,
        visible: true,
    });
    let output = settle(&mut editor, &context);
    let select = toolbox_button(&output, "Select");
    click_at(&mut editor, &context, SIZE, select.rect.center());
    settle(&mut editor, &context);
    assert_eq!(editor.active_tool, ActiveTool::Select);
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    // Select has a Tool Box icon too, and its settings say what is missing. Let egui's 0.3 s
    // double-click window pass first so the pair is not read as part of a triple click.
    for _ in 0..30 {
        frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    }
    let output = double_click(&mut editor, &context, select.rect.center());
    assert!(editor.sidebar_panel_visible(SidebarPanel::ToolSettings));
    assert!(text_drawn(&output, "Select Tool"));
}

#[test]
fn windows_editors_rows_show_each_implemented_panel() {
    let (mut editor, context) = setup();
    let output = settle(&mut editor, &context);
    for (row, panel) in [
        ("Tool Settings", SidebarPanel::ToolSettings),
        ("Attribute Editor", SidebarPanel::AttributeEditor),
        ("Channel Box / Layer Editor", SidebarPanel::ChannelBox),
    ] {
        let before: Vec<_> = named(&output, row).iter().map(|node| node.id).collect();
        let windows = one(&settle(&mut editor, &context), "Windows");
        let opened = click_at(&mut editor, &context, SIZE, windows.rect.center());
        let editors = one(&opened, "Editors");
        frame_with(
            &mut editor,
            &context,
            SIZE,
            1.0,
            vec![Event::PointerMoved(editors.rect.center())],
        );
        let opened = settle(&mut editor, &context);
        let item = named(&opened, row)
            .into_iter()
            .find(|node| !before.contains(&node.id))
            .unwrap_or_else(|| panic!("no Windows > Editors > {row} row"));
        assert!(!item.disabled);
        click_at(&mut editor, &context, SIZE, item.rect.center());
        settle(&mut editor, &context);
        assert!(editor.sidebar_panel_visible(panel), "{row}");
    }
}

#[test]
fn panel_controls_are_disabled_with_a_reason_during_a_manipulation() {
    let (mut editor, context) = setup();
    tool(&mut editor, ActiveTool::Move);
    show_panel(&mut editor);
    settle(&mut editor, &context);
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X,
            expected_selection_generation: None,
        })
        .unwrap();
    let output = settle(&mut editor, &context);
    let combo = one(&output, AXIS_ORIENTATION_LABEL);
    assert!(combo.disabled);
    assert_eq!(
        combo.description.as_deref(),
        Some("Unavailable: a manipulation is in progress")
    );
    let clicked = click_at(&mut editor, &context, SIZE, combo.rect.center());
    assert!(
        named(&clicked, "Object")
            .iter()
            .all(|node| node.rect.top() < combo.rect.top()),
        "a disabled combo opens no popup"
    );
    // The dock cannot switch away mid-drag either.
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ChannelBox,
        visible: true,
    });
    assert!(editor.sidebar_panel_visible(SidebarPanel::ToolSettings));
}

#[test]
fn every_entry_point_is_disabled_behind_a_dialog_and_dispatch_refuses() {
    let (mut editor, context) = setup();
    // A dirty scene: Close asks to save, which opens the modal file prompt.
    assert!(editor.is_dirty());
    editor.request_close();
    assert!(editor.files.is_modal());
    let output = settle(&mut editor, &context);
    let toggle = one(&output, "Tool Settings");
    assert!(toggle.disabled, "the Status Line toggle");
    for tool in ["Select", "Move", "Rotate", "Scale"] {
        assert!(
            toolbox_button(&output, tool).disabled,
            "the {tool} Tool Box button"
        );
    }
    // The menu bar is behind the modal too: the Windows menu cannot open.
    let windows = one(&output, "Windows");
    let clicked = click_at(&mut editor, &context, SIZE, windows.rect.center());
    assert!(named(&clicked, "Editors").is_empty());
    for (dock, visible) in [
        (SidebarPanel::ToolSettings, true),
        (SidebarPanel::ChannelBox, false),
    ] {
        editor.native_operation(Operation::SetSidebarPanelVisible {
            panel: dock,
            visible,
        });
    }
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
}

#[test]
fn the_status_line_toggle_and_editors_rows_are_disabled_during_a_manipulation() {
    let (mut editor, context) = setup();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X,
            expected_selection_generation: None,
        })
        .unwrap();
    let output = settle(&mut editor, &context);
    assert!(one(&output, "Tool Settings").disabled);
    let windows = one(&output, "Windows");
    click_at(&mut editor, &context, SIZE, windows.rect.center());
    let opened = settle(&mut editor, &context);
    let editors = one(&opened, "Editors");
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        vec![Event::PointerMoved(editors.rect.center())],
    );
    let opened = settle(&mut editor, &context);
    let row = named(&opened, "Tool Settings")
        .into_iter()
        .find(|node| node.id != toggle_id(&output))
        .expect("the Windows > Editors > Tool Settings row");
    assert!(row.disabled);
    assert_eq!(
        row.description.as_deref(),
        Some("Unavailable: a manipulation is in progress")
    );
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
}

fn toggle_id(output: &egui::FullOutput) -> accesskit::NodeId {
    one(output, "Tool Settings").id
}

#[test]
fn ctrl_a_from_the_tool_settings_opens_the_attribute_editor() {
    assert_eq!(
        ctrl_a_target(Some(SidebarPanel::ToolSettings)),
        SidebarPanel::AttributeEditor
    );
    assert_eq!(
        ctrl_a_target(Some(SidebarPanel::AttributeEditor)),
        SidebarPanel::ChannelBox
    );
    assert_eq!(ctrl_a_target(None), SidebarPanel::AttributeEditor);
}

#[test]
fn scale_space_chosen_in_the_panel_is_the_one_set_tool_space_reads() {
    let (mut editor, context) = setup();
    tool(&mut editor, ActiveTool::Scale);
    show_panel(&mut editor);
    let output = settle(&mut editor, &context);
    let combo = one(&output, AXIS_ORIENTATION_LABEL);
    let opened = click_at(&mut editor, &context, SIZE, combo.rect.center());
    let parent = named(&opened, "Parent")
        .into_iter()
        .find(|node| !node.disabled && node.rect.top() >= combo.rect.top())
        .expect("the popup's Parent row");
    click_at(&mut editor, &context, SIZE, parent.rect.center());
    assert_eq!(
        editor.manipulation.space(ActiveTool::Scale),
        ManipulationSpace::Parent
    );
    let json = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert_eq!(json["manipulation"]["scale_space"], "parent");
    assert_eq!(json["sidebar_panels"]["tool_settings"], true);
}
