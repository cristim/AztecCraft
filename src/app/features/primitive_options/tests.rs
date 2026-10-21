use super::*;
use crate::app::tests::frame_with;
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, accesskit};

const SIZE: Vec2 = egui::vec2(1200.0, 800.0);

struct Control {
    role: accesskit::Role,
    label: String,
    rect: Rect,
    disabled: bool,
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
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            Some(Control {
                role: node.role(),
                label: node.label()?.to_owned(),
                rect: Rect::from_min_max(
                    Pos2::new(bounds.x0 as f32, bounds.y0 as f32),
                    Pos2::new(bounds.x1 as f32, bounds.y1 as f32),
                ),
                disabled: node.is_disabled(),
            })
        })
        .collect()
}

fn find(output: &egui::FullOutput, label: &str) -> Option<Control> {
    controls(output).into_iter().find(|c| c.label == label)
}

/// The window's own button: the Create menu also has a "Create" control, but at the top.
fn window_button(output: &egui::FullOutput, label: &str) -> Control {
    controls(output)
        .into_iter()
        .filter(|c| c.label == label && c.role == accesskit::Role::Button)
        .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap_or_else(|| panic!("button {label}"))
}

fn menu_create(output: &egui::FullOutput) -> Control {
    controls(output)
        .into_iter()
        .filter(|c| c.label == "Create" && c.role == accesskit::Role::Button)
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .expect("Create menu")
}

fn setup() -> (Editor, egui::Context) {
    let context = egui::Context::default();
    context.enable_accesskit();
    let editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    (editor, context)
}

fn run(editor: &mut Editor, ctx: &egui::Context, events: Vec<Event>) -> egui::FullOutput {
    frame_with(editor, ctx, SIZE, 1.0, events)
}

fn click(editor: &mut Editor, ctx: &egui::Context, at: Pos2) -> egui::FullOutput {
    run(editor, ctx, vec![Event::PointerMoved(at)]);
    let mut last = None;
    for pressed in [true, false] {
        last = Some(run(
            editor,
            ctx,
            vec![Event::PointerButton {
                pos: at,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }],
        ));
    }
    last.unwrap()
}

fn open_menu_and_click(editor: &mut Editor, ctx: &egui::Context, label: &str) {
    let output = run(editor, ctx, vec![]);
    let menu = menu_create(&output);
    click(editor, ctx, menu.rect.center());
    let output = run(editor, ctx, vec![]);
    let target = find(&output, label).unwrap_or_else(|| panic!("menu control {label}"));
    click(editor, ctx, target.rect.center());
    run(editor, ctx, vec![]);
}

fn open_dialog(editor: &mut Editor, ctx: &egui::Context, primitive: Primitive) {
    editor.open_primitive_options(primitive);
    // The first frames size and place the window.
    for _ in 0..3 {
        run(editor, ctx, vec![]);
    }
}

fn set_field(editor: &mut Editor, ctx: &egui::Context, index: usize, text: &str) {
    editor.primitive_options.dialog.as_mut().unwrap().texts[index] = text.into();
    run(editor, ctx, vec![]);
}

fn press(editor: &mut Editor, ctx: &egui::Context, label: &str) {
    let output = run(editor, ctx, vec![]);
    let button = window_button(&output, label);
    click(editor, ctx, button.rect.center());
}

fn dialog_open(output: &egui::FullOutput, primitive: Primitive) -> bool {
    find(output, title(primitive)).is_some()
}

fn node_count(editor: &Editor) -> usize {
    editor.history.scene().nodes().len()
}

#[test]
fn drafts_round_trip_to_the_defaults_for_every_primitive() {
    for primitive in PRIMITIVES {
        let options = PrimitiveOptions::default_for(primitive);
        let rebuilt = assemble(primitive, &texts(&options), axis_of(&options)).unwrap();
        assert_eq!(rebuilt, options, "{primitive:?}");
    }
}

#[test]
fn assemble_names_the_bad_field_and_never_panics_on_odd_text() {
    let mut drafts = texts(&PrimitiveOptions::default_for(Primitive::Cube));
    drafts[0] = "wide".into();
    assert_eq!(
        assemble(Primitive::Cube, &drafts, Axis::Y).unwrap_err(),
        "Width must be a number"
    );
    drafts[0] = "1".into();
    drafts[3] = "2.5".into();
    assert_eq!(
        assemble(Primitive::Cube, &drafts, Axis::Y).unwrap_err(),
        "Subdivisions width must be a whole number"
    );
    drafts[3] = "-1".into();
    assert!(assemble(Primitive::Cube, &drafts, Axis::Y).is_err());
    drafts[3] = "1".into();
    for bad in ["NaN", "inf", "0", "-3", "1e7", ""] {
        drafts[1] = bad.into();
        assert!(
            assemble(Primitive::Cube, &drafts, Axis::Y).is_err(),
            "{bad}"
        );
    }
    assert!(assemble(Primitive::Cube, &drafts[..2], Axis::Y).is_err());
    assert!(assemble(Primitive::Sphere, &drafts, Axis::Y).is_err());
}

#[test]
fn option_box_in_the_create_menu_opens_that_primitives_window_and_closes_the_menu() {
    let (mut editor, ctx) = setup();
    open_menu_and_click(&mut editor, &ctx, "Polygon sphere option box");
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Sphere));
    assert!(find(&output, "Polygon cube").is_none(), "the menu closes");
    assert_eq!(node_count(&editor), 0, "opening the window creates nothing");
}

#[test]
fn plain_menu_item_still_creates_with_the_remembered_options() {
    let (mut editor, ctx) = setup();
    open_menu_and_click(&mut editor, &ctx, "Polygon cube");
    assert_eq!(node_count(&editor), 1);
    open_dialog(&mut editor, &ctx, Primitive::Plane);
    set_field(&mut editor, &ctx, 2, "3");
    set_field(&mut editor, &ctx, 3, "2");
    press(&mut editor, &ctx, "Create");
    let planes = node_count(&editor);
    assert_eq!(planes, 2);
    open_menu_and_click(&mut editor, &ctx, "Polygon plane");
    let faces = |index: usize| editor.history.scene().shapes()[index].mesh().faces().len();
    assert_eq!(faces(1), faces(2));
    assert_eq!(faces(1), 6);
}

#[test]
fn create_dispatches_the_options_and_keeps_the_window_open() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 1, "8");
    set_field(&mut editor, &ctx, 2, "6");
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 1);
    assert_eq!(
        editor.history.scene().shapes()[0].mesh().faces().len(),
        8 * 6
    );
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Sphere));
    assert!(editor.history.undo());
    assert_eq!(node_count(&editor), 0);
}

#[test]
fn invalid_text_keeps_the_window_open_with_a_message_and_changes_nothing() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Cylinder);
    let before = editor.history.scene().clone();
    for (index, text) in [(0, "abc"), (0, "0"), (2, "129"), (2, "2"), (3, "1.5")] {
        let saved = editor
            .primitive_options
            .dialog
            .as_ref()
            .unwrap()
            .texts
            .clone();
        set_field(&mut editor, &ctx, index, text);
        press(&mut editor, &ctx, "Create");
        assert_eq!(editor.history.scene(), &before, "{text}");
        let output = run(&mut editor, &ctx, vec![]);
        assert!(dialog_open(&output, Primitive::Cylinder));
        assert!(
            editor
                .primitive_options
                .dialog
                .as_ref()
                .unwrap()
                .message
                .is_some()
        );
        editor.primitive_options.dialog.as_mut().unwrap().texts = saved;
    }
    assert_eq!(
        editor.remembered_options(Primitive::Cylinder),
        PrimitiveOptions::default_for(Primitive::Cylinder)
    );
}

#[test]
fn close_discards_edits_and_x_button_closes_too() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Cone);
    set_field(&mut editor, &ctx, 0, "5");
    press(&mut editor, &ctx, "Close");
    let output = run(&mut editor, &ctx, vec![]);
    assert!(!dialog_open(&output, Primitive::Cone));
    open_dialog(&mut editor, &ctx, Primitive::Cone);
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[0],
        "1"
    );
}

#[test]
fn only_one_window_exists_and_reopening_keeps_unsent_edits() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Cube);
    set_field(&mut editor, &ctx, 0, "4");
    open_dialog(&mut editor, &ctx, Primitive::Cube);
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[0],
        "4"
    );
    open_dialog(&mut editor, &ctx, Primitive::Plane);
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Plane));
    assert!(!dialog_open(&output, Primitive::Cube));
    let creates = controls(&output)
        .iter()
        .filter(|c| c.label == "Create" && c.role == accesskit::Role::Button)
        .count();
    assert_eq!(creates, 2, "the Create menu plus one window button");
}

#[test]
fn unfinished_controls_are_disabled_with_a_reason() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Cylinder);
    let output = run(&mut editor, &ctx, vec![]);
    for label in [
        "Free",
        "Camera",
        "Interactive creation",
        "Create UVs",
        "Exit on completion",
        "Subdivisions caps",
        "Round cap",
    ] {
        let control = find(&output, label).unwrap_or_else(|| panic!("{label}"));
        assert!(control.disabled, "{label}");
    }
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    let output = run(&mut editor, &ctx, vec![]);
    assert!(find(&output, "Round cap").is_none());
}
