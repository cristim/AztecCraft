use super::*;
use crate::app::tests::{
    click_at, create_menu_closed, frame_with, open_polygon_primitives, remote, submenu_item,
};
use crate::editor_operations::{OperationError, OperationReply};
use eframe::egui::{Event, Modifiers, PointerButton, Pos2, Rect, Vec2, accesskit};

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

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

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
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

/// Clicks a control of the Create > Polygon Primitives popup, found by its position there.
fn open_menu_and_click(editor: &mut Editor, ctx: &egui::Context, label: &str) {
    let output = open_polygon_primitives(editor, ctx, SIZE);
    let at = submenu_item(&output, label).rect.center();
    click_at(editor, ctx, SIZE, at);
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

fn focus_field(editor: &mut Editor, ctx: &egui::Context, index: usize) {
    ctx.memory_mut(|memory| memory.request_focus(option_field_id(index)));
    run(editor, ctx, vec![]);
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
    open_menu_and_click(&mut editor, &ctx, "Sphere option box");
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Sphere));
    assert!(create_menu_closed(&output), "the whole menu chain closes");
    assert_eq!(node_count(&editor), 0, "opening the window creates nothing");
}

#[test]
fn plain_menu_item_still_creates_with_the_remembered_options() {
    let (mut editor, ctx) = setup();
    open_menu_and_click(&mut editor, &ctx, "Cube");
    assert_eq!(node_count(&editor), 1);
    open_dialog(&mut editor, &ctx, Primitive::Plane);
    set_field(&mut editor, &ctx, 2, "3");
    set_field(&mut editor, &ctx, 3, "2");
    press(&mut editor, &ctx, "Create");
    let planes = node_count(&editor);
    assert_eq!(planes, 2);
    open_menu_and_click(&mut editor, &ctx, "Plane");
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

#[test]
fn typing_in_a_field_blocks_hotkeys_delete_and_viewport_clicks() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    editor.create(Primitive::Plane);
    let selected = editor.selected();
    let nodes = node_count(&editor);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    focus_field(&mut editor, &ctx, 0);
    assert!(text_edit_focused(&ctx));
    let tool = editor.active_tool;
    let mode = editor.selection_mode;
    run(
        &mut editor,
        &ctx,
        vec![key(Key::W), key(Key::F8), key(Key::Delete), key(Key::Num4)],
    );
    assert_eq!(editor.active_tool, tool);
    assert_eq!(editor.selection_mode, mode);
    assert_eq!(node_count(&editor), nodes);
    assert_eq!(editor.selected(), selected);
    // A viewport click while a field has focus must not select or change anything.
    let viewport = Pos2::new(700.0, 400.0);
    let before = (editor.selected(), editor.history.scene().clone());
    focus_field(&mut editor, &ctx, 1);
    run(&mut editor, &ctx, vec![Event::PointerMoved(viewport)]);
    run(
        &mut editor,
        &ctx,
        vec![Event::PointerButton {
            pos: viewport,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!((editor.selected(), editor.history.scene().clone()), before);
}

#[test]
fn clicking_a_dialog_field_prefocuses_it_so_the_first_text_lands() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    let field = ctx.read_response(option_field_id(0)).unwrap().rect.center();
    run(&mut editor, &ctx, vec![Event::PointerMoved(field)]);
    run(
        &mut editor,
        &ctx,
        vec![
            Event::PointerButton {
                pos: field,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::Text("7".into()),
        ],
    );
    assert_eq!(ctx.memory(|m| m.focused()), Some(option_field_id(0)));
    assert!(editor.primitive_options.dialog.as_ref().unwrap().texts[0].contains('7'));
}

#[test]
fn window_is_hidden_while_a_file_modal_is_open_and_returns_with_its_text() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 0, "3");
    editor.request_new();
    assert!(editor.files.is_modal());
    let output = run(&mut editor, &ctx, vec![]);
    assert!(!dialog_open(&output, Primitive::Sphere));
    let before = node_count(&editor);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    run(&mut editor, &ctx, vec![]);
    assert!(!editor.files.is_modal());
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Sphere));
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[0],
        "3"
    );
    assert_eq!(node_count(&editor), before);
}

#[test]
fn window_is_hidden_while_closing() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    editor.request_close();
    assert!(editor.files.is_closing());
    let output = run(&mut editor, &ctx, vec![]);
    assert!(!dialog_open(&output, Primitive::Sphere));
}

#[test]
fn read_only_scene_disables_create_and_the_option_box() {
    let (mut editor, ctx) = setup();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    assert!(
        editor
            .history
            .scene()
            .authoring_read_only_reason()
            .is_some()
    );
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    let output = run(&mut editor, &ctx, vec![]);
    assert!(window_button(&output, "Create").disabled);
    let before = editor.history.scene().clone();
    press(&mut editor, &ctx, "Create");
    assert_eq!(editor.history.scene(), &before);
    let output = open_polygon_primitives(&mut editor, &ctx, SIZE);
    assert!(submenu_item(&output, "Sphere option box").disabled);
}

#[test]
fn remote_create_succeeds_while_the_window_is_open_and_busy_only_while_typing() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 0, "4");
    let create = || Operation::Create {
        primitive: Primitive::Cube,
        options: None,
        name: None,
    };
    let reply = remote(&mut editor, create()).unwrap();
    assert!(matches!(reply, OperationReply::Created { .. }));
    assert_eq!(node_count(&editor), 1);
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[0],
        "4"
    );
    focus_field(&mut editor, &ctx, 0);
    let busy = editor
        .remote_operation(
            crate::editor_operations::RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation: create(),
            },
            &ctx,
        )
        .unwrap_err();
    assert!(matches!(busy, OperationError::BusyEditing));
}

#[test]
fn create_commits_a_valid_human_draft_first_and_reports_an_invalid_one() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Plane);
    editor.channels[0] = "5".into();
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 2);
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation,
        [5.0, 0.0, 0.0],
        "the valid draft was committed by the create"
    );
    editor.channels[0] = "nope".into();
    editor.set_selection_for_test(Some(editor.history.scene().nodes()[0].id()), Vec::new());
    run(&mut editor, &ctx, vec![]);
    editor.channels[0] = "nope".into();
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 2, "an invalid draft blocks the create");
    assert!(
        editor
            .primitive_options
            .dialog
            .as_ref()
            .unwrap()
            .message
            .is_some()
    );
}

#[test]
fn escape_closes_the_window_when_nothing_else_wants_it() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    let output = run(&mut editor, &ctx, vec![]);
    assert!(!dialog_open(&output, Primitive::Sphere));
    assert!(!editor.status.contains("Edit cancelled"));
}

#[test]
fn escape_cancels_a_draft_first_and_a_second_press_closes() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    editor.channels[0] = "9".into();
    assert!(editor.has_draft());
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(!editor.has_draft());
    assert!(
        editor.primitive_options.dialog.is_some(),
        "the draft took the key"
    );
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_none());
}

#[test]
fn escape_while_typing_in_a_field_keeps_the_window() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    focus_field(&mut editor, &ctx, 0);
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_some());
    assert_eq!(ctx.memory(|m| m.focused()), None);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_none());
}

#[test]
fn escape_cancels_a_manipulation_and_keeps_the_window() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    editor
        .apply_operation(Operation::BeginMove {
            target: editor.selected_move_target().unwrap(),
            axis: azteccraft::native_scene::WorldAxis::X,
        })
        .unwrap();
    assert!(editor.history.active_manipulation().is_some());
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.history.active_manipulation().is_none());
    assert!(editor.primitive_options.dialog.is_some());
}

#[test]
fn replacing_the_scene_keeps_the_window_the_drafts_and_the_remembered_options() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 1, "8");
    set_field(&mut editor, &ctx, 2, "6");
    press(&mut editor, &ctx, "Create");
    set_field(&mut editor, &ctx, 0, "3");
    let created_token = editor.scene_token;
    editor.request_new();
    assert!(editor.files.is_modal(), "the created scene is unsaved");
    let output = run(&mut editor, &ctx, vec![]);
    assert!(
        !dialog_open(&output, Primitive::Sphere),
        "hidden by the prompt"
    );
    let pending_id = uuid::Uuid::parse_str(&editor.files.pending_inspection().unwrap().id).unwrap();
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id,
            decision: crate::editor_operations::FileDecision::Discard,
        })
        .unwrap();
    assert_ne!(editor.scene_token, created_token);
    assert_eq!(node_count(&editor), 0);
    for _ in 0..2 {
        run(&mut editor, &ctx, vec![]);
    }
    let output = run(&mut editor, &ctx, vec![]);
    assert!(dialog_open(&output, Primitive::Sphere), "the window stays");
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[0],
        "3"
    );
    assert!(matches!(
        editor.remembered_options(Primitive::Sphere),
        PrimitiveOptions::Sphere {
            sub_axis: 8,
            sub_height: 6,
            ..
        }
    ));
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 1, "Create now targets the new scene");
}

#[test]
fn escape_with_a_focused_non_dialog_field_keeps_the_window() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    ctx.memory_mut(|memory| memory.request_focus(crate::app::name_edit_id()));
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![]);
    assert!(!editor.has_draft());
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_some());
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_none());
}

#[test]
fn typing_state_is_cleared_while_the_window_is_hidden_by_a_modal() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    focus_field(&mut editor, &ctx, 0);
    run(&mut editor, &ctx, vec![]);
    editor.request_new();
    assert!(editor.files.is_modal());
    run(&mut editor, &ctx, vec![]);
    let pending_id = uuid::Uuid::parse_str(&editor.files.pending_inspection().unwrap().id).unwrap();
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id,
            decision: crate::editor_operations::FileDecision::Cancel,
        })
        .unwrap();
    // The very next frame: no redraw of the window has refreshed the focus state yet.
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(
        editor.primitive_options.dialog.is_none(),
        "one Escape closes it"
    );
    assert!(!editor.status.contains("Edit cancelled"));
}

#[test]
fn text_events_reach_a_dialog_field() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 1, "");
    focus_field(&mut editor, &ctx, 1);
    run(&mut editor, &ctx, vec![Event::Text("1".into())]);
    run(&mut editor, &ctx, vec![Event::Text("2".into())]);
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts[1],
        "12"
    );
    press(&mut editor, &ctx, "Create");
    assert_eq!(
        editor.history.scene().shapes()[0].mesh().faces().len(),
        12 * 20
    );
}

#[test]
fn a_failed_create_message_clears_after_a_valid_create_and_options_are_remembered() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    set_field(&mut editor, &ctx, 1, "9");
    editor.channels[0] = "nope".into();
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 1);
    assert!(
        editor
            .primitive_options
            .dialog
            .as_ref()
            .unwrap()
            .message
            .is_some()
    );
    assert_eq!(
        editor.remembered_options(Primitive::Sphere),
        PrimitiveOptions::default_for(Primitive::Sphere)
    );
    editor.sync_fields();
    press(&mut editor, &ctx, "Create");
    assert_eq!(node_count(&editor), 2);
    assert!(
        editor
            .primitive_options
            .dialog
            .as_ref()
            .unwrap()
            .message
            .is_none()
    );
    assert!(matches!(
        editor.remembered_options(Primitive::Sphere),
        PrimitiveOptions::Sphere { sub_axis: 9, .. }
    ));
}

#[test]
fn a_disabled_option_box_shows_the_read_only_reason_on_hover() {
    let (mut editor, ctx) = setup();
    ctx.all_styles_mut(|style| style.interaction.tooltip_delay = 0.0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let output = open_polygon_primitives(&mut editor, &ctx, SIZE);
    let target = submenu_item(&output, "Sphere option box");
    assert!(target.disabled);
    // The Attribute Editor paints the same reason text, so look for a tooltip layer instead.
    let tooltip_layers = |ctx: &egui::Context| {
        ctx.memory(|memory| {
            memory
                .areas()
                .visible_layer_ids()
                .into_iter()
                .filter(|layer| layer.order == egui::Order::Tooltip)
                .count()
        })
    };
    assert_eq!(tooltip_layers(&ctx), 0, "no tooltip before the hover");
    let reason = editor
        .history
        .scene()
        .authoring_read_only_reason()
        .expect("an ASCII scene is read-only")
        .to_string();
    let reason_texts = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == reason)
            })
            .count()
    };
    // egui ignores a hover that follows a click too closely; give the frames real time.
    let frames = |editor: &mut Editor, first: f64, moved: bool| {
        let mut last = None;
        for step in 1..=6 {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SIZE)),
                time: Some(first + f64::from(step)),
                ..Default::default()
            };
            if step == 1 && moved {
                input.events.push(Event::PointerMoved(target.rect.center()));
            }
            let mut output = ctx.run_ui(input, |ui| editor.render(ui));
            output.textures_delta.clear();
            last = Some(output);
        }
        last.unwrap()
    };
    let control = frames(&mut editor, 10.0, false);
    assert_eq!(tooltip_layers(&ctx), 0, "no tooltip without the hover");
    let hovered = frames(&mut editor, 20.0, true);
    assert!(
        tooltip_layers(&ctx) > 0,
        "the disabled box shows its reason"
    );
    assert_eq!(
        reason_texts(&hovered),
        reason_texts(&control) + 1,
        "the tooltip adds one copy of the read-only reason"
    );
}

#[test]
fn clicking_a_read_only_option_box_opens_no_window() {
    let (mut editor, ctx) = setup();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    std::fs::write(&path, ascii_fixture::specimen()).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let output = open_polygon_primitives(&mut editor, &ctx, SIZE);
    let target = submenu_item(&output, "Sphere option box");
    assert!(target.disabled);
    let before = editor.history.scene().clone();
    click(&mut editor, &ctx, target.rect.center());
    let output = run(&mut editor, &ctx, vec![]);
    assert!(editor.primitive_options.dialog.is_none());
    assert!(!dialog_open(&output, Primitive::Sphere));
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn escape_closing_the_window_consumes_the_pointer_sequence() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    editor.select(None);
    run(&mut editor, &ctx, vec![]);
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    assert!(!editor.move_consumed_frame && !editor.move_suppress_primary);
    let at = Pos2::new(600.0, 400.0);
    run(&mut editor, &ctx, vec![Event::PointerMoved(at)]);
    run(
        &mut editor,
        &ctx,
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_none());
    assert!(editor.move_consumed_frame);
    assert!(
        editor.move_suppress_primary,
        "the held press is not replayed"
    );
    run(
        &mut editor,
        &ctx,
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        editor.selection_presentation().0,
        None,
        "releasing over the cube selects nothing"
    );
}

#[test]
fn dragging_over_a_field_does_not_scrub_its_value() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    let before = editor
        .primitive_options
        .dialog
        .as_ref()
        .unwrap()
        .texts
        .clone();
    run(&mut editor, &ctx, vec![]);
    let field = ctx
        .read_response(option_field_id(0))
        .expect("field response");
    let start = field.rect.center();
    run(&mut editor, &ctx, vec![Event::PointerMoved(start)]);
    run(
        &mut editor,
        &ctx,
        vec![Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
    );
    for step in 1..=5 {
        run(
            &mut editor,
            &ctx,
            vec![Event::PointerMoved(
                start + egui::vec2(20.0 * step as f32, 0.0),
            )],
        );
    }
    run(
        &mut editor,
        &ctx,
        vec![Event::PointerButton {
            pos: start + egui::vec2(100.0, 0.0),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert_eq!(
        editor.primitive_options.dialog.as_ref().unwrap().texts,
        before,
        "the fields are text-only (PROVISIONAL: Maya's option fields may scrub)"
    );
}

#[test]
fn escape_while_a_close_is_in_progress_keeps_the_hidden_window() {
    let (mut editor, ctx) = setup();
    open_dialog(&mut editor, &ctx, Primitive::Sphere);
    editor.apply_operation(Operation::RequestClose).unwrap();
    assert!(editor.files.is_closing() && !editor.files.is_modal());
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.primitive_options.dialog.is_some());
}
