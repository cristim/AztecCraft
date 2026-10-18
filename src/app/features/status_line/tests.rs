use super::*;
use crate::app::tests::frame_with;
use crate::app::{channel_edit_id, text_edit_focused};
use crate::editor_operations::{Operation, SelectionMode, SidebarPanel};
use azteccraft::scene::NodeId;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2, accesskit};

const SIZE: Vec2 = egui::vec2(960.0, 640.0);
const MODES: [&str; 3] = [
    "Hierarchy selection",
    "Object selection",
    "Component selection",
];
const COMPONENT_TYPES: [&str; 3] = ["Vertex selection", "Edge selection", "Face selection"];
const MENU_SET: &str = "Menu set";
const PRESETS: &str = "Selection mode menu";
const FILES: [&str; 3] = ["New scene", "Open...", "Save"];
const HIERARCHY: &str = "Hierarchy selection";
const CHANNEL_BOX: &str = "Channel Box / Layer Editor";
const ATTRIBUTE_EDITOR: &str = "Attribute Editor";
/// Toggles that are live; every other toggle is a disabled placeholder.
const LIVE: [&str; 2] = [CHANNEL_BOX, ATTRIBUTE_EDITOR];
const TOGGLES: [&str; 5] = [
    "Modeling Toolkit",
    "HumanIK",
    "Attribute Editor",
    "Tool Settings",
    "Channel Box / Layer Editor",
];

struct Control {
    role: accesskit::Role,
    label: String,
    id: accesskit::NodeId,
    rect: Rect,
    disabled: bool,
    selected: Option<bool>,
}

fn controls(output: &egui::FullOutput) -> Vec<Control> {
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
            Some(Control {
                role: node.role(),
                label: node.label()?.to_owned(),
                id: *id,
                rect: Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
                selected: node.toggled().map(|t| t == accesskit::Toggled::True),
            })
        })
        .collect()
}

fn strip(output: &egui::FullOutput) -> Vec<Control> {
    let mut found: Vec<Control> = controls(output)
        .into_iter()
        .filter(|c| {
            c.role == accesskit::Role::ComboBox && (c.label == MENU_SET || c.label == PRESETS)
                || FILES.contains(&c.label.as_str())
                || MODES.contains(&c.label.as_str())
                || COMPONENT_TYPES.contains(&c.label.as_str())
                || TOGGLES.contains(&c.label.as_str())
        })
        .collect();
    found.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
    found
}

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

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn click(editor: &mut Editor, context: &egui::Context, at: Pos2) {
    frame_with(editor, context, SIZE, 1.0, vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        frame_with(
            editor,
            context,
            SIZE,
            1.0,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        );
    }
}

/// Every state the strip must leave alone when a disabled control is used.
fn snapshot(editor: &Editor) -> (uuid::Uuid, String, bool, SelectionMode, Option<NodeId>) {
    (
        editor.scene_token,
        editor.status.clone(),
        editor.error,
        editor.selection_mode,
        editor.selected,
    )
}

#[test]
fn strip_fits_the_minimum_window_at_every_scale_and_enlarged_text() {
    for (ppp, font) in [(1.0, 14.0), (1.25, 14.0), (2.0, 14.0), (1.0, 18.0)] {
        let (mut editor, context) = setup();
        let mut style = egui::Style {
            override_font_id: Some(egui::FontId::proportional(font)),
            ..Default::default()
        };
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        context.set_style_of(egui::Theme::Dark, style);
        context.set_theme(egui::Theme::Dark);
        frame_with(&mut editor, &context, SIZE, ppp, vec![]);
        let output = frame_with(&mut editor, &context, SIZE, ppp, vec![]);
        let found = strip(&output);
        assert_eq!(found.len(), 16, "ppp {ppp} font {font}");
        let margin = egui::Frame::side_top_panel(&context.global_style()).total_margin();
        for control in &found {
            assert!(
                control.rect.left() >= margin.left - 0.5
                    && control.rect.right() <= SIZE.x - margin.right + 0.5,
                "ppp {ppp} font {font}: {} at {:?}",
                control.label,
                control.rect
            );
        }
        for pair in found.windows(2) {
            assert!(
                pair[0].rect.right() <= pair[1].rect.left() + 0.5,
                "ppp {ppp} font {font}: {} overlaps {}",
                pair[0].label,
                pair[1].label
            );
        }
    }
}

#[test]
fn strip_elements_are_in_documented_order() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let labels: Vec<String> = strip(&output).into_iter().map(|c| c.label).collect();
    let expected: Vec<&str> = [MENU_SET]
        .into_iter()
        .chain(FILES)
        .chain([PRESETS])
        .chain(MODES)
        .chain(COMPONENT_TYPES)
        .chain(TOGGLES)
        .collect();
    assert_eq!(labels, expected);
}

#[test]
fn sidebar_cluster_hugs_the_right_edge_at_a_wide_window() {
    let wide = egui::vec2(1440.0, 900.0);
    let (mut editor, context) = setup();
    let output = frame_with(&mut editor, &context, wide, 1.0, vec![]);
    let last = strip(&output).pop().expect("strip controls");
    assert_eq!(last.label, TOGGLES[4]);
    let margin = egui::Frame::side_top_panel(&context.global_style()).total_margin();
    let edge = wide.x - margin.right;
    let spacing = context.global_style().spacing.item_spacing.x;
    assert!(
        (edge - last.rect.right()).abs() <= spacing,
        "right edge {} vs strip edge {edge}",
        last.rect.right()
    );
}

#[test]
fn clicking_disabled_strip_buttons_changes_nothing() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let before = snapshot(&editor);
    for control in strip(&output)
        .iter()
        .filter(|c| TOGGLES.contains(&c.label.as_str()) && !LIVE.contains(&c.label.as_str()))
    {
        assert!(control.disabled, "{}", control.label);
        click(&mut editor, &context, control.rect.center());
        assert_eq!(snapshot(&editor), before, "{}", control.label);
    }
}

#[test]
fn presets_combo_is_inert_and_shows_the_disabled_hover_text() {
    let (mut editor, context) = file_editor(FileState::Clean);
    let output = strip_output(&mut editor, &context);
    let combo = strip(&output)
        .into_iter()
        .find(|c| c.label == PRESETS)
        .expect("presets combo");
    assert!(combo.disabled);
    let before = snapshot(&editor);
    let hover = hover_texts(&mut editor, &context, combo.rect.center());
    assert!(hover.contains(SELECTION_MODE_MENU_HOVER), "{hover}");
    click(&mut editor, &context, combo.rect.center());
    assert!(!egui::Popup::is_any_open(&context));
    assert_eq!(snapshot(&editor), before);
}

#[test]
fn open_combo_popup_blocks_shortcuts_and_disabled_sets_stay_disabled() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let combo = strip(&output).remove(0);
    let before = snapshot(&editor);
    click(&mut editor, &context, combo.rect.center());
    assert!(egui::Popup::is_any_open(&context));
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    for set in ["Rigging", "Animation", "FX", "Rendering"] {
        let entry = controls(&output)
            .into_iter()
            .find(|c| c.label == set)
            .unwrap_or_else(|| panic!("set {set}"));
        assert!(entry.disabled, "{set}");
    }
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F11)]);
    assert_eq!(editor.selection_mode, before.3, "F11 reached a shortcut");
    assert_eq!(snapshot(&editor), before);
}

#[test]
fn shortcut_baseline_changes_mode_without_the_popup() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let before = editor.selection_mode;
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F11)]);
    assert_ne!(editor.selection_mode, before);
}

#[test]
fn every_strip_control_has_its_label_and_disabled_state() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let found = strip(&output);
    for name in MODES.iter().chain(&COMPONENT_TYPES).chain(&TOGGLES) {
        let control = found
            .iter()
            .find(|c| c.label == *name)
            .unwrap_or_else(|| panic!("missing AccessKit label {name}"));
        assert_eq!(control.role, accesskit::Role::Button, "{name}");
        assert_eq!(
            control.disabled,
            TOGGLES.contains(name) && !LIVE.contains(name),
            "{name}"
        );
    }
    for (label, disabled) in [(MENU_SET, false), (PRESETS, true)] {
        let combo = found
            .iter()
            .find(|c| c.role == accesskit::Role::ComboBox && c.label == label)
            .unwrap_or_else(|| panic!("combo {label}"));
        assert_eq!(combo.disabled, disabled, "{label}");
    }
}

/// Tabs until the menu set combo has keyboard focus, failing if a disabled button takes it.
fn tab_to_combo(editor: &mut Editor, context: &egui::Context) {
    for _ in 0..60 {
        let output = frame_with(editor, context, SIZE, 1.0, vec![key(Key::Tab)]);
        let focus = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|update| update.focus);
        let found = strip(&output);
        let focused = found.iter().find(|c| Some(c.id) == focus);
        match focused {
            Some(c) if c.role == accesskit::Role::ComboBox && c.label == MENU_SET => return,
            Some(c) if c.label == PRESETS => panic!("Tab focused the disabled presets combo"),
            Some(c) if TOGGLES.contains(&c.label.as_str()) && !LIVE.contains(&c.label.as_str()) => {
                panic!("Tab focused disabled strip button {}", c.label)
            }
            Some(_) => {}
            None => {}
        }
    }
    panic!("Tab never reached the combo");
}

#[test]
fn keyboard_activates_the_enabled_combo_and_skips_disabled_buttons() {
    let (mut editor, context) = setup();
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    tab_to_combo(&mut editor, &context);
    let mut reached_shelf = false;
    for _ in 0..40 {
        let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::Tab)]);
        let focus = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|u| u.focus);
        let focused = controls(&output).into_iter().find(|c| Some(c.id) == focus);
        if let Some(c) = focused {
            assert!(
                (!TOGGLES.contains(&c.label.as_str()) || LIVE.contains(&c.label.as_str()))
                    && c.label != PRESETS,
                "Tab focused disabled {}",
                c.label
            );
            if c.label == "Cube" {
                reached_shelf = true;
                break;
            }
        }
    }
    assert!(
        reached_shelf,
        "Tab never passed the disabled buttons to the shelf"
    );
    let before = snapshot(&editor);
    for activate in [Key::Space, Key::Enter] {
        tab_to_combo(&mut editor, &context);
        frame_with(&mut editor, &context, SIZE, 1.0, vec![key(activate)]);
        assert!(egui::Popup::is_any_open(&context), "{activate:?} opens");
        frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::Escape)]);
        assert!(!egui::Popup::is_any_open(&context));
    }
    // Existing behaviour, tracked in #95: cancel_on_escape (panels.rs) is not gated on
    // Popup::is_any_open. A fix should flip this assertion deliberately.
    assert_eq!(editor.status, "Edit cancelled");
    let after = snapshot(&editor);
    assert_eq!(
        (after.0, after.2, after.3, after.4),
        (before.0, before.2, before.3, before.4)
    );
}

const OBJECT: &str = "Object selection";
const COMPONENT: &str = "Component selection";

fn button_center(output: &egui::FullOutput, label: &str) -> Pos2 {
    strip(output)
        .iter()
        .find(|c| c.label == label)
        .unwrap_or_else(|| panic!("strip control {label}"))
        .rect
        .center()
}

fn strip_output(editor: &mut Editor, context: &egui::Context) -> egui::FullOutput {
    frame_with(editor, context, SIZE, 1.0, vec![]);
    frame_with(editor, context, SIZE, 1.0, vec![])
}

/// Hovers first, then delivers keys, press and release in ONE frame.
fn click_with_keys(editor: &mut Editor, context: &egui::Context, at: Pos2, keys: Vec<Event>) {
    frame_with(editor, context, SIZE, 1.0, vec![Event::PointerMoved(at)]);
    let mut events = keys;
    for pressed in [true, false] {
        events.push(Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    frame_with(editor, context, SIZE, 1.0, events);
}

#[test]
fn component_and_object_clicks_set_the_mode_and_repeat_clicks_do_nothing() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    click(&mut editor, &context, button_center(&output, COMPONENT));
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    let after_face = snapshot(&editor);
    click(&mut editor, &context, button_center(&output, COMPONENT));
    assert_eq!(snapshot(&editor), after_face);
    click(&mut editor, &context, button_center(&output, OBJECT));
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    let after_object = snapshot(&editor);
    click(&mut editor, &context, button_center(&output, OBJECT));
    assert_eq!(snapshot(&editor), after_object);
}

#[test]
fn component_click_keeps_the_component_mode_and_enters_face_from_object_or_hierarchy() {
    for (_, mode, _) in TYPE_MODES {
        let (mut editor, context) = setup();
        let output = strip_output(&mut editor, &context);
        editor
            .apply_operation(Operation::SetSelectionMode(mode))
            .unwrap();
        let before = snapshot(&editor);
        click(&mut editor, &context, button_center(&output, COMPONENT));
        assert_eq!(snapshot(&editor), before, "{mode:?}");
        assert_eq!(editor.selection_mode, mode);
    }
    for start in [SelectionMode::Object, SelectionMode::Hierarchy] {
        let (mut editor, context) = setup();
        let output = strip_output(&mut editor, &context);
        editor
            .apply_operation(Operation::SetSelectionMode(start))
            .unwrap();
        click(&mut editor, &context, button_center(&output, COMPONENT));
        assert_eq!(editor.selection_mode, SelectionMode::Face, "{start:?}");
    }
}

#[test]
fn same_frame_shortcuts_beat_the_strip_click_exactly_like_the_select_toggle_menu_item() {
    // Select > Components > Face (menus.rs) assigns the intent directly and so beats a
    // same-frame F8, unlike the strip and Select > Toggle, which only fill an empty slot.
    for label in [HIERARCHY, OBJECT, COMPONENT] {
        let (mut reference, context) = setup();
        strip_output(&mut reference, &context);
        let reference_scene = reference.history.scene().clone();
        frame_with(
            &mut reference,
            &context,
            SIZE,
            1.0,
            vec![key(Key::F11), key(Key::Delete)],
        );
        let (mut editor, context) = setup();
        let output = strip_output(&mut editor, &context);
        let scene = editor.history.scene().clone();
        click_with_keys(
            &mut editor,
            &context,
            button_center(&output, label),
            vec![key(Key::F11), key(Key::Delete)],
        );
        assert_eq!(editor.selection_mode, SelectionMode::Face, "{label}");
        assert_eq!(editor.history.scene(), &scene, "{label}");
        assert_eq!(reference.history.scene(), &reference_scene, "{label}");
        assert_eq!(editor.status, reference.status, "{label}");
        assert_eq!(editor.error, reference.error, "{label}");
        assert_eq!(editor.selected, reference.selected, "{label}");
    }
}

#[test]
fn valid_draft_commits_to_the_old_target_and_invalid_draft_refuses() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    let target = editor.selected.expect("cube selected");
    editor.channels[0] = "2".into();
    editor.channel_intents[0] = true;
    click(&mut editor, &context, button_center(&output, COMPONENT));
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert!(!editor.has_draft());
    assert_eq!(editor.selected, Some(target));
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        2.0
    );

    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    let scene = editor.history.scene().clone();
    editor.channels[0] = "nan".into();
    editor.channel_intents[0] = true;
    click(&mut editor, &context, button_center(&output, COMPONENT));
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert_eq!(editor.channels[0], "nan");
    assert_eq!(editor.history.scene(), &scene);
    assert!(editor.error);
}

#[test]
fn clicking_a_mode_cancels_an_active_move() {
    cancels_an_active_move(COMPONENT, SelectionMode::Face);
}

#[test]
fn clicking_a_component_type_cancels_an_active_move() {
    for (label, mode, _) in TYPE_MODES {
        cancels_an_active_move(label, mode);
    }
}

fn cancels_an_active_move(button: &str, expected: SelectionMode) {
    use crate::app::features::move_tool::tests::{press_x, setup as move_setup};
    let (mut editor, context, rect) = move_setup(1.0);
    context.enable_accesskit();
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    frame_with(
        &mut editor,
        &context,
        egui::vec2(1440.0, 900.0),
        1.0,
        vec![Event::PointerMoved(
            begin + egui::vec2(rect.height() / 20.0, 0.0),
        )],
    );
    assert_ne!(editor.history.scene(), &before);
    // The pointer is still held (a click would first release and commit the move), so the strip
    // button is activated from the keyboard.
    let big = egui::vec2(1440.0, 900.0);
    let mut focused = false;
    for _ in 0..40 {
        let output = frame_with(&mut editor, &context, big, 1.0, vec![key(Key::Tab)]);
        let focus = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|u| u.focus);
        if strip(&output)
            .iter()
            .any(|c| c.label == button && Some(c.id) == focus)
        {
            focused = true;
            break;
        }
    }
    assert!(focused, "Tab never reached the {button} button");
    assert!(editor.native_move.is_some());
    frame_with(&mut editor, &context, big, 1.0, vec![key(Key::Space)]);
    assert_eq!(editor.selection_mode, expected);
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.status, "Move cancelled: selection mode changed");
}

#[test]
fn native_manipulation_without_a_native_drag_keeps_mode_buttons_enabled_and_a_click_cancels_it() {
    use crate::app::features::move_tool::tests::{press_x, setup as move_setup};
    let (mut editor, context, rect) = move_setup(1.0);
    context.enable_accesskit();
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let big = egui::vec2(1440.0, 900.0);
    frame_with(
        &mut editor,
        &context,
        big,
        1.0,
        vec![Event::PointerMoved(
            begin + egui::vec2(rect.height() / 20.0, 0.0),
        )],
    );
    assert_ne!(editor.history.scene(), &before);
    // The state left when starting the native drag fails after Begin succeeded.
    editor.native_move = None;
    let mut focused = false;
    for _ in 0..40 {
        let output = frame_with(&mut editor, &context, big, 1.0, vec![key(Key::Tab)]);
        let focus = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|u| u.focus);
        let found = strip(&output).into_iter().find(|c| c.label == COMPONENT);
        if let Some(control) = found
            && Some(control.id) == focus
        {
            assert!(!control.disabled);
            focused = true;
            break;
        }
    }
    assert!(focused, "Tab never reached the Component button");
    frame_with(&mut editor, &context, big, 1.0, vec![key(Key::Space)]);
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn mode_buttons_are_disabled_while_a_remote_manipulation_is_active() {
    use crate::app::tests::remote;
    use azteccraft::native_scene::WorldAxis;
    for label in [HIERARCHY, OBJECT, COMPONENT]
        .into_iter()
        .chain(COMPONENT_TYPES)
    {
        let (mut editor, context) = setup();
        let output = strip_output(&mut editor, &context);
        let target = editor.selected_move_target().unwrap();
        remote(
            &mut editor,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
        )
        .unwrap();
        let active = editor.history.active_manipulation().unwrap().id;
        let scene = editor.history.scene().clone();
        let before = snapshot(&editor);
        let output_after = strip_output(&mut editor, &context);
        let control = strip(&output_after)
            .into_iter()
            .find(|c| c.label == label)
            .unwrap();
        assert!(control.disabled, "{label}");
        click(&mut editor, &context, button_center(&output, label));
        assert_eq!(snapshot(&editor), before, "{label}");
        assert_eq!(
            editor.history.active_manipulation().map(|m| m.id),
            Some(active),
            "{label}"
        );
        assert_eq!(editor.history.scene(), &scene, "{label}");
    }
}

#[test]
fn modal_and_closing_disable_every_mode_button() {
    for closing in [false, true] {
        let (mut editor, context) = setup();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.new_document().unwrap();
        }
        let output = strip_output(&mut editor, &context);
        let before = snapshot(&editor);
        for label in [HIERARCHY, OBJECT, COMPONENT]
            .into_iter()
            .chain(COMPONENT_TYPES)
        {
            let control = strip(&output)
                .into_iter()
                .find(|c| c.label == label)
                .unwrap();
            assert!(control.disabled, "{label} closing={closing}");
            click(&mut editor, &context, control.rect.center());
            assert_eq!(snapshot(&editor), before, "{label} closing={closing}");
        }
    }
}

#[test]
fn accesskit_selected_follows_the_mode() {
    let (mut editor, context) = setup();
    let selected = |output: &egui::FullOutput| {
        let found = strip(output);
        let state = |label: &str| found.iter().find(|c| c.label == label).unwrap().selected;
        (state(HIERARCHY), state(OBJECT), state(COMPONENT))
    };
    let output = strip_output(&mut editor, &context);
    assert_eq!(selected(&output), (Some(false), Some(true), Some(false)));
    click(&mut editor, &context, button_center(&output, COMPONENT));
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(selected(&output), (Some(false), Some(false), Some(true)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F8)]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(selected(&output), (Some(false), Some(true), Some(false)));
    click(&mut editor, &context, button_center(&output, HIERARCHY));
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert_eq!(selected(&output), (Some(true), Some(false), Some(false)));
}

#[test]
fn hierarchy_click_sets_the_mode_and_repeat_clicks_do_nothing() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    click(&mut editor, &context, button_center(&output, HIERARCHY));
    assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
    let after = snapshot(&editor);
    click(&mut editor, &context, button_center(&output, HIERARCHY));
    assert_eq!(snapshot(&editor), after);
}

#[test]
fn f8_from_hierarchy_goes_to_face_and_back_to_object() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    click(&mut editor, &context, button_center(&output, HIERARCHY));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F8)]);
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F8)]);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
}

#[test]
fn same_frame_hierarchy_click_and_delete_deletes_the_selection_once() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    assert!(editor.selected.is_some());
    click_with_keys(
        &mut editor,
        &context,
        button_center(&output, HIERARCHY),
        vec![key(Key::Delete)],
    );
    assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
    assert!(!editor.error, "{}", editor.status);
    assert!(editor.history.scene().nodes().is_empty());
    assert_eq!(editor.selected, None);
}

#[test]
fn f8_cancels_an_active_move_from_hierarchy() {
    use crate::app::features::move_tool::tests::{press_x, setup as move_setup};
    let (mut editor, context, rect) = move_setup(1.0);
    context.enable_accesskit();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    frame_with(
        &mut editor,
        &context,
        egui::vec2(1440.0, 900.0),
        1.0,
        vec![Event::PointerMoved(
            begin + egui::vec2(rect.height() / 20.0, 0.0),
        )],
    );
    assert_ne!(editor.history.scene(), &before);
    frame_with(
        &mut editor,
        &context,
        egui::vec2(1440.0, 900.0),
        1.0,
        vec![key(Key::F8)],
    );
    assert_eq!(editor.selection_mode, SelectionMode::Face);
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn releasing_a_move_over_the_component_button_commits_the_move() {
    use crate::app::features::move_tool::tests::{press_x, setup as move_setup};
    let (mut editor, context, rect) = move_setup(1.0);
    context.enable_accesskit();
    let before = editor.history.scene().clone();
    let big = egui::vec2(1440.0, 900.0);
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let output = frame_with(
        &mut editor,
        &context,
        big,
        1.0,
        vec![Event::PointerMoved(
            begin + egui::vec2(rect.height() / 20.0, 0.0),
        )],
    );
    let target = button_center(&output, COMPONENT);
    frame_with(
        &mut editor,
        &context,
        big,
        1.0,
        vec![Event::PointerMoved(target)],
    );
    // A release over the strip commits the drag first, so keyboard activation and F8/F11 are
    // the only Move cancel routes.
    frame_with(
        &mut editor,
        &context,
        big,
        1.0,
        vec![Event::PointerButton {
            pos: target,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_ne!(editor.history.scene(), &before);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(
        !editor.status.contains("Move cancelled"),
        "{}",
        editor.status
    );
}

#[test]
fn strip_click_with_a_popup_open_is_dropped_like_the_menu_path() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    let combo = strip(&output).remove(0);
    click(&mut editor, &context, combo.rect.center());
    assert!(egui::Popup::is_any_open(&context));
    let before = snapshot(&editor);
    // A pointer press outside the popup would close it first, so activate from the keyboard.
    let mut focused = false;
    for _ in 0..40 {
        let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::Tab)]);
        let focus = output
            .platform_output
            .accesskit_update
            .as_ref()
            .map(|u| u.focus);
        assert!(egui::Popup::is_any_open(&context), "popup closed");
        if strip(&output)
            .iter()
            .any(|c| c.label == COMPONENT && Some(c.id) == focus)
        {
            focused = true;
            break;
        }
    }
    assert!(focused, "Tab never reached the Component button");
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::Space)]);
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(!editor.error);
    assert_eq!(snapshot(&editor).3, before.3);
}

fn channel_panel_drawn(output: &egui::FullOutput) -> bool {
    output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Channel Box")
    })
}

#[test]
fn channel_box_toggle_hides_and_shows_the_panel_after_the_click_frame() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    assert!(channel_panel_drawn(&output));
    let at = button_center(&output, CHANNEL_BOX);
    let token = editor.scene_token;
    for visible in [false, true] {
        click(&mut editor, &context, at);
        assert_eq!(
            editor.sidebar_panel_visible(SidebarPanel::ChannelBox),
            visible
        );
        let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
        assert_eq!(channel_panel_drawn(&output), visible);
        let control = strip(&output)
            .into_iter()
            .find(|c| c.label == CHANNEL_BOX)
            .unwrap();
        assert_eq!(control.selected, Some(visible));
        assert!(!control.disabled);
        assert_eq!(editor.scene_token, token);
        assert!(editor.sidebar_panel_request.is_none());
    }
}

#[test]
fn channel_box_toggle_is_disabled_for_modal_closing_and_move() {
    for blocked in 0..3 {
        let (mut editor, context) = setup();
        match blocked {
            0 => {
                editor.new_document().unwrap();
            }
            1 => {
                editor.close_document().unwrap();
            }
            _ => {
                let target = editor.selected_move_target().unwrap();
                editor
                    .apply_operation(crate::editor_operations::Operation::BeginMove {
                        target,
                        axis: azteccraft::native_scene::WorldAxis::X,
                    })
                    .unwrap();
            }
        }
        let output = strip_output(&mut editor, &context);
        let toggle = &SIDEBAR_TOGGLES[4];
        let expected = ["a dialog is open", "a dialog is open", "a Move is active"];
        assert_eq!(
            editor.sidebar_toggle_hover(toggle),
            format!(
                "Channel Box / Layer Editor: unavailable while {}",
                expected[blocked]
            )
        );
        let control = strip(&output)
            .into_iter()
            .find(|c| c.label == CHANNEL_BOX)
            .unwrap();
        assert!(control.disabled, "blocked {blocked}");
        click(&mut editor, &context, control.rect.center());
        assert!(
            editor.sidebar_panel_visible(SidebarPanel::ChannelBox),
            "blocked {blocked}"
        );
    }
}

fn press_release(at: Pos2) -> [Event; 2] {
    [true, false].map(|pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    })
}

fn translate_x(editor: &Editor) -> f64 {
    editor.history.scene().nodes()[0].transform().translation[0]
}

#[test]
fn same_frame_click_then_text_commits_the_draft_before_the_panel_hides() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    let at = button_center(&output, CHANNEL_BOX);
    // Pre-focus in earlier frames: the first click on a field can lose text (known-issues).
    context.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    frame_with(
        &mut editor,
        &context,
        SIZE,
        1.0,
        vec![Event::PointerMoved(at)],
    );
    assert!(text_edit_focused(&context));
    let command = Modifiers::COMMAND;
    let mut events = vec![
        Event::ModifiersChanged(command),
        Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: command,
        },
        Event::ModifiersChanged(Modifiers::NONE),
        Event::Text("7".into()),
    ];
    events.extend(press_release(at));
    frame_with(&mut editor, &context, SIZE, 1.0, events);
    assert_eq!(translate_x(&editor), 7.0);
    assert!(!editor.has_draft());
    assert_eq!(editor.channels[0], "7");
    assert!(!editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert!(editor.history.can_undo());
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(!channel_panel_drawn(&output));
    assert_eq!(translate_x(&editor), 7.0);
}

#[test]
fn new_scene_in_the_same_frame_drops_the_toggle_request() {
    // Fallback path, not the Space-plus-click frame. Spiked: Space alone activates the toggle
    // with the File popup open, but a pointer press on "New scene" in that frame moves focus
    // to that item first, so Space activates the item and no toggle request exists to drop.
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    let context = egui::Context::default();
    strip_output(&mut editor, &context);
    let old = editor.scene_token;
    editor.sidebar_panel_request = Some((SidebarPanel::ChannelBox, false));
    // The menu bar draws before viewport(), so New scene has run when the request is consumed.
    editor.request_new();
    assert_ne!(editor.scene_token, old, "a clean New replaces the document");
    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
        editor.viewport(ui, old, None, None, false, true)
    });
    output.textures_delta.clear();
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert!(editor.sidebar_panel_request.is_none());
}

#[test]
fn token_change_between_request_and_viewport_dispatches_nothing() {
    let (mut editor, context) = setup();
    strip_output(&mut editor, &context);
    let old = editor.scene_token;
    editor.sidebar_panel_request = Some((SidebarPanel::ChannelBox, false));
    // The scene changed after the frame began, as a menu item would.
    editor.scene_token = uuid::Uuid::new_v4();
    let current = editor.scene_token;
    for _ in 0..2 {
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            editor.viewport(ui, old, None, None, false, true)
        });
        output.textures_delta.clear();
        assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
        assert!(editor.sidebar_panel_request.is_none(), "request consumed");
        assert_eq!(editor.scene_token, current);
    }
}

#[test]
fn a_stale_request_does_not_survive_into_a_later_render() {
    let (mut editor, context) = setup();
    strip_output(&mut editor, &context);
    editor.sidebar_panel_request = Some((SidebarPanel::ChannelBox, false));
    let token = editor.scene_token;
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    assert!(channel_panel_drawn(&output));
    assert_eq!(editor.scene_token, token);
    assert!(editor.sidebar_panel_request.is_none());
}

#[test]
fn showing_again_restores_width_and_values_hiding_blurs_and_selected_follows() {
    let (mut editor, context) = setup();
    let output = strip_output(&mut editor, &context);
    let at = button_center(&output, CHANNEL_BOX);
    editor.channels[1] = "3".into();
    let width = |output: &egui::FullOutput| {
        controls(output)
            .into_iter()
            .find(|c| c.label == "Object name")
            .map(|c| c.rect.width())
    };
    let before = width(&frame_with(&mut editor, &context, SIZE, 1.0, vec![]));
    context.memory_mut(|memory| memory.request_focus(channel_edit_id(2)));
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(text_edit_focused(&context));
    click(&mut editor, &context, at);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(!channel_panel_drawn(&output));
    assert!(!text_edit_focused(&context), "hiding blurs the field");
    let control = strip(&output)
        .into_iter()
        .find(|c| c.label == CHANNEL_BOX)
        .unwrap();
    assert_eq!(control.selected, Some(false));
    click(&mut editor, &context, at);
    frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    let output = frame_with(&mut editor, &context, SIZE, 1.0, vec![]);
    assert!(channel_panel_drawn(&output));
    assert_eq!(editor.channels[1], "3");
    assert_eq!(width(&output), before);
    let control = strip(&output)
        .into_iter()
        .find(|c| c.label == CHANNEL_BOX)
        .unwrap();
    assert_eq!(control.selected, Some(true));
}

#[test]
fn closing_without_a_dialog_shows_the_closing_hover_on_the_toggle() {
    let mut editor = Editor::default();
    editor.close_document().unwrap();
    assert!(editor.files.is_closing() && !editor.files.is_modal());
    let toggle = &SIDEBAR_TOGGLES[4];
    assert_eq!(
        editor.sidebar_toggle_hover(toggle),
        "Channel Box / Layer Editor: unavailable while the window is closing"
    );
}

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[derive(Clone, Copy, Debug)]
enum FileState {
    Clean,
    Dirty,
    ReadOnlyAscii,
    Modal,
}

fn file_editor(state: FileState) -> (Editor, egui::Context) {
    let context = egui::Context::default();
    context.enable_accesskit();
    context.all_styles_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    match state {
        FileState::Clean => {}
        FileState::Dirty => editor.create(azteccraft::scene::Primitive::Cube),
        FileState::Modal => {
            editor.create(azteccraft::scene::Primitive::Cube);
            editor.new_document().unwrap();
        }
        FileState::ReadOnlyAscii => {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("curve.ma");
            std::fs::write(&path, ascii_fixture::specimen()).unwrap();
            editor
                .apply_operation(crate::editor_operations::Operation::OpenScene { path })
                .unwrap();
        }
    }
    (editor, context)
}

const ALL_STATES: [FileState; 4] = [
    FileState::Clean,
    FileState::Dirty,
    FileState::ReadOnlyAscii,
    FileState::Modal,
];

fn text_shapes(output: &egui::FullOutput) -> Vec<String> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

/// Text that appears only because the pointer rests on `at`: the hovered frame's text shapes
/// minus those of the same state with the pointer parked on the empty viewport.
fn hover_texts(editor: &mut Editor, context: &egui::Context, at: Pos2) -> String {
    let settle = |editor: &mut Editor, at: Pos2| {
        frame_with(editor, context, SIZE, 1.0, vec![Event::PointerMoved(at)]);
        frame_with(editor, context, SIZE, 1.0, vec![]);
        text_shapes(&frame_with(editor, context, SIZE, 1.0, vec![]))
    };
    let mut baseline = settle(editor, Pos2::new(480.0, 600.0));
    let mut new = Vec::new();
    for text in settle(editor, at) {
        match baseline.iter().position(|t| *t == text) {
            Some(index) => {
                baseline.swap_remove(index);
            }
            None => new.push(text),
        }
    }
    new.join("|")
}

/// Menu item controls (label, rect, disabled) of the open File menu, told apart from the
/// strip buttons that share their labels by their rectangles.
fn open_file_menu(
    editor: &mut Editor,
    context: &egui::Context,
    strip_rects: &[Rect],
) -> Vec<Control> {
    let output = strip_output(editor, context);
    let file = controls(&output)
        .into_iter()
        .find(|c| c.label == "File")
        .expect("File menu button");
    click(editor, context, file.rect.center());
    let output = frame_with(editor, context, SIZE, 1.0, vec![]);
    controls(&output)
        .into_iter()
        .filter(|c| {
            FILES.contains(&c.label.as_str())
                && !strip_rects.contains(&c.rect)
                && (c.rect.left() - file.rect.left()).abs() < 20.0
        })
        .collect()
}

fn strip_files(output: &egui::FullOutput) -> Vec<Control> {
    let found = strip(output);
    let row = found[0].rect.center().y;
    found
        .into_iter()
        .filter(|c| FILES.contains(&c.label.as_str()) && (c.rect.center().y - row).abs() < 8.0)
        .collect()
}

#[test]
fn strip_file_buttons_match_the_file_menu_in_enable_state_and_hover_reason() {
    for state in ALL_STATES {
        let (mut editor, context) = file_editor(state);
        let strip_output = strip_output(&mut editor, &context);
        let buttons = strip_files(&strip_output);
        assert_eq!(buttons.len(), 3, "{state:?}");
        let rects: Vec<Rect> = buttons.iter().map(|b| b.rect).collect();
        let reason = editor
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|r| r.to_string());
        let actions = editor.file_actions();
        let mut strip_hovers = Vec::new();
        for button in &buttons {
            strip_hovers.push(hover_texts(&mut editor, &context, button.rect.center()));
        }
        let items = open_file_menu(&mut editor, &context, &rects);
        if matches!(state, FileState::Modal) {
            // The modal blocks the menu bar, so the strip must be as inert as the menu.
            assert!(items.is_empty());
            for (button, action) in buttons.iter().zip(&actions) {
                assert_eq!(button.disabled, !action.enabled, "{state:?}");
            }
            continue;
        }
        assert_eq!(items.len(), 3, "{state:?} menu items");
        for (index, label) in FILES.iter().enumerate() {
            let button = buttons.iter().find(|b| b.label == *label).unwrap();
            let item = items.iter().find(|i| i.label == *label).unwrap();
            assert_eq!(button.disabled, item.disabled, "{state:?} {label} enable");
            assert_eq!(
                button.disabled, !actions[index].enabled,
                "{state:?} {label}"
            );
            let menu_hover = hover_texts(&mut editor, &context, item.rect.center());
            let strip_hover =
                &strip_hovers[buttons.iter().position(|b| b.label == *label).unwrap()];
            match (&reason, *label) {
                (Some(reason), "Save") => {
                    assert!(button.disabled, "{state:?}");
                    assert_eq!(actions[index].reason.as_ref(), Some(reason), "{state:?}");
                    assert_eq!(actions[index].hover(), reason.as_str(), "{state:?}");
                    assert!(menu_hover.contains(reason.as_str()), "{state:?} menu");
                    assert!(strip_hover.contains(reason.as_str()), "{state:?} strip");
                }
                _ => assert!(
                    strip_hover.contains(actions[index].tooltip),
                    "{state:?} {label}: {strip_hover}"
                ),
            }
        }
    }
}

#[test]
fn strip_save_enablement_comes_from_file_actions() {
    let (mut editor, context) = file_editor(FileState::Clean);
    let output = strip_output(&mut editor, &context);
    let save = strip_files(&output)
        .into_iter()
        .find(|b| b.label == "Save")
        .unwrap();
    assert_eq!(save.disabled, !editor.files.save_available());
}

type Outcome = (String, bool, Option<String>, bool, bool);

fn outcome(editor: &Editor) -> Outcome {
    (
        editor.status.clone(),
        editor.error,
        editor
            .files
            .pending_inspection()
            .map(|p| format!("{:?}", p.kind)),
        editor.is_dirty(),
        editor.files.is_modal(),
    )
}

#[test]
fn strip_and_menu_clicks_dispatch_the_same_handlers() {
    // Clean, Dirty and ReadOnlyAscii compare real strip and menu clicks for New and Save.
    // Modal only compares "no change": the modal blocks the File menu. Open is not clicked
    // outside Modal because open_dialog calls rfd directly (src/file_ui.rs), with no seam.
    for state in ALL_STATES {
        for label in FILES {
            if label == "Open..." && !matches!(state, FileState::Modal) {
                // The unmodal Open handler would block on a native dialog.
                continue;
            }
            let (mut by_strip, context) = file_editor(state);
            let output = strip_output(&mut by_strip, &context);
            let buttons = strip_files(&output);
            let rects: Vec<Rect> = buttons.iter().map(|b| b.rect).collect();
            let target = buttons.iter().find(|b| b.label == label).unwrap();
            click(&mut by_strip, &context, target.rect.center());

            let (mut by_menu, context) = file_editor(state);
            if matches!(state, FileState::Modal) {
                // The menu bar is blocked: the strip click must leave the modal state as it was.
                assert_eq!(outcome(&by_strip), outcome(&by_menu), "{state:?} {label}");
                continue;
            }
            let items = open_file_menu(&mut by_menu, &context, &rects);
            let item = items.iter().find(|i| i.label == label).unwrap();
            click(&mut by_menu, &context, item.rect.center());

            assert_eq!(outcome(&by_strip), outcome(&by_menu), "{state:?} {label}");
        }
    }
}

#[test]
fn dirty_new_from_the_strip_raises_the_unsaved_changes_modal() {
    let (mut editor, context) = file_editor(FileState::Dirty);
    let output = strip_output(&mut editor, &context);
    let new = strip_files(&output)
        .into_iter()
        .find(|b| b.label == "New scene")
        .unwrap();
    click(&mut editor, &context, new.rect.center());
    assert!(editor.files.is_modal());
    assert_eq!(
        format!("{:?}", editor.files.pending_inspection().unwrap().kind),
        "New"
    );
}

const TYPE_MODES: [(&str, SelectionMode, Key); 3] = [
    (COMPONENT_TYPES[0], SelectionMode::Vertex, Key::F9),
    (COMPONENT_TYPES[1], SelectionMode::Edge, Key::F10),
    (COMPONENT_TYPES[2], SelectionMode::Face, Key::F11),
];

#[test]
fn component_type_clicks_set_their_mode_and_repeat_clicks_do_nothing() {
    for (label, mode, _) in TYPE_MODES {
        let (mut editor, context) = setup();
        let output = strip_output(&mut editor, &context);
        click(&mut editor, &context, button_center(&output, label));
        assert_eq!(editor.selection_mode, mode, "{label}");
        let after = snapshot(&editor);
        click(&mut editor, &context, button_center(&output, label));
        assert_eq!(snapshot(&editor), after, "{label}");
    }
}

#[test]
fn component_type_buttons_match_their_hotkeys() {
    for (label, mode, hotkey) in TYPE_MODES {
        let (mut by_button, context) = setup();
        let output = strip_output(&mut by_button, &context);
        click(&mut by_button, &context, button_center(&output, label));
        let (mut by_key, key_context) = setup();
        frame_with(&mut by_key, &key_context, SIZE, 1.0, vec![key(hotkey)]);
        assert_eq!(by_key.selection_mode, mode, "{label}");
        assert_eq!(snapshot(&by_button).3, snapshot(&by_key).3, "{label}");
    }
}

#[test]
fn accesskit_pressed_state_tracks_every_mode() {
    let (mut editor, context) = setup();
    let pressed = |editor: &mut Editor, context: &egui::Context| {
        let found = strip(&strip_output(editor, context));
        [HIERARCHY, OBJECT, COMPONENT]
            .into_iter()
            .chain(COMPONENT_TYPES)
            .map(|label| found.iter().find(|c| c.label == label).unwrap().selected)
            .collect::<Vec<_>>()
    };
    let expected = |on: &[usize]| (0..6).map(|i| Some(on.contains(&i))).collect::<Vec<_>>();
    assert_eq!(pressed(&mut editor, &context), expected(&[1]));
    for (label, mode, _) in TYPE_MODES {
        let output = strip_output(&mut editor, &context);
        click(&mut editor, &context, button_center(&output, label));
        assert_eq!(editor.selection_mode, mode);
        let type_index = 3 + COMPONENT_TYPES.iter().position(|l| *l == label).unwrap();
        assert_eq!(
            pressed(&mut editor, &context),
            expected(&[2, type_index]),
            "{label}"
        );
    }
    frame_with(&mut editor, &context, SIZE, 1.0, vec![key(Key::F8)]);
    assert_eq!(pressed(&mut editor, &context), expected(&[1]));
}

#[test]
fn component_type_hover_states_hotkey_exclusivity_and_uv() {
    let (mut editor, context) = file_editor(FileState::Dirty);
    let output = strip_output(&mut editor, &context);
    for (label, _, hotkey) in TYPE_MODES {
        let hover = hover_texts(&mut editor, &context, button_center(&output, label));
        let name = label.split(' ').next().unwrap();
        assert!(hover.contains(name), "{label}: {hover}");
        assert!(hover.contains(&format!("{hotkey:?}")), "{label}: {hover}");
        assert!(hover.contains("exclusive"), "{label}: {hover}");
        assert!(hover.contains("UV is unsupported"), "{label}: {hover}");
    }
}
