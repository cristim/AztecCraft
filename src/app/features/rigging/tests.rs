use super::*;
use crate::app::menus::{ALWAYS_ON_MENUS, HELP_MENU, MODELING_SET_MENUS, RIGGING_SET_MENUS};
use crate::app::tests::{frame_with, remote};
use crate::editor_operations::{MenuSet, SelectionMode};
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

fn control(output: &egui::FullOutput, label: &str) -> Control {
    controls(output)
        .into_iter()
        .filter(|c| c.label == label)
        .max_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
        .unwrap_or_else(|| panic!("control {label}"))
}

fn open_window(editor: &mut Editor, ctx: &egui::Context) {
    editor.open_joint_options();
    for _ in 0..3 {
        run(editor, ctx, vec![]);
    }
}

fn press(editor: &mut Editor, ctx: &egui::Context, label: &str) {
    let output = run(editor, ctx, vec![]);
    let at = control(&output, label).rect.center();
    click(editor, ctx, at);
}

fn set_field(editor: &mut Editor, ctx: &egui::Context, index: usize, text: &str) {
    editor.joint_options.dialog.as_mut().unwrap().texts[index] = text.into();
    run(editor, ctx, vec![]);
}

fn message(editor: &Editor) -> Option<String> {
    editor.joint_options.dialog.as_ref()?.message.clone()
}

fn joints(editor: &Editor) -> Vec<NodeId> {
    let scene = editor.history.scene();
    (scene.nodes().iter())
        .filter(|node| scene.joint(node.transform_id()).is_some())
        .map(|node| node.id())
        .collect()
}

fn lead(editor: &Editor) -> Option<NodeId> {
    editor.selection_presentation().0
}

fn bar_labels(output: &egui::FullOutput) -> Vec<String> {
    let mut bar: Vec<Control> = controls(output)
        .into_iter()
        .filter(|c| c.rect.top() < 30.0)
        .collect();
    bar.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()));
    bar.into_iter().map(|c| c.label).collect()
}

fn expected_bar(set: &[&str]) -> Vec<String> {
    ALWAYS_ON_MENUS
        .iter()
        .chain(set)
        .chain([&HELP_MENU])
        .map(|label| (*label).to_owned())
        .collect()
}

#[test]
fn f3_shows_the_rigging_menus_and_f2_returns_to_modeling() {
    let (mut editor, ctx) = setup();
    let output = run(&mut editor, &ctx, vec![]);
    assert_eq!(bar_labels(&output), expected_bar(&MODELING_SET_MENUS));
    run(&mut editor, &ctx, vec![key(Key::F3)]);
    assert_eq!(editor.menu_set, MenuSet::Rigging);
    let output = run(&mut editor, &ctx, vec![]);
    assert_eq!(bar_labels(&output), expected_bar(&RIGGING_SET_MENUS));
    assert!(!editor.is_dirty(), "a menu set is not a scene edit");
    run(&mut editor, &ctx, vec![key(Key::F2)]);
    assert_eq!(editor.menu_set, MenuSet::Modeling);
}

#[test]
fn set_hotkeys_wait_for_a_focused_field_and_a_file_modal() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    ctx.memory_mut(|memory| memory.request_focus(joint_field_id(0)));
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![key(Key::F3)]);
    assert_eq!(editor.menu_set, MenuSet::Modeling, "typing keeps the key");

    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    editor.request_new();
    assert!(editor.files.is_modal());
    run(&mut editor, &ctx, vec![key(Key::F3)]);
    assert_eq!(editor.menu_set, MenuSet::Modeling);
}

#[test]
fn a_new_scene_keeps_the_menu_set() {
    let (mut editor, ctx) = setup();
    run(&mut editor, &ctx, vec![key(Key::F3)]);
    remote(&mut editor, Operation::NewScene).unwrap();
    assert_eq!(editor.menu_set, MenuSet::Rigging);
}

#[test]
fn skeleton_lists_every_row_and_only_create_joints_is_live() {
    let (mut editor, ctx) = setup();
    run(&mut editor, &ctx, vec![key(Key::F3)]);
    let output = run(&mut editor, &ctx, vec![]);
    let at = control(&output, "Skeleton").rect.center();
    click(&mut editor, &ctx, at);
    // The popup's first frame is its sizing pass, which reports every row as disabled.
    let output = run(&mut editor, &ctx, vec![]);
    let mut rows: Vec<Control> = controls(&output)
        .into_iter()
        // The far right of the Status Line also has a HumanIK button.
        .filter(|c| c.role == accesskit::Role::Button && c.rect.top() > 30.0)
        .filter(|c| c.rect.left() < SIZE.x / 2.0)
        .filter(|c| {
            c.label == CREATE_JOINTS
                || c.label == "Create Joints option box"
                || JOINT_ROWS.contains(&c.label.as_str())
                || IK_ROWS.contains(&c.label.as_str())
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.rect.top().total_cmp(&b.rect.top())).then(a.rect.left().total_cmp(&b.rect.left()))
    });
    let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
    let mut expected = vec![CREATE_JOINTS, "Create Joints option box"];
    expected.extend(JOINT_ROWS);
    expected.extend(IK_ROWS);
    assert_eq!(labels, expected);
    for row in &rows {
        assert_eq!(row.disabled, row.label != CREATE_JOINTS, "{}", row.label);
    }
    let at = rows[0].rect.center();
    click(&mut editor, &ctx, at);
    assert!(editor.joint_options.dialog.is_some());
    assert!(!egui::Popup::is_any_open(&ctx));
}

#[test]
fn repeated_create_builds_a_chain_and_deselect_starts_a_new_one() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    set_field(&mut editor, &ctx, 1, "2");
    press(&mut editor, &ctx, "Create");
    let root = lead(&editor).expect("the new joint is selected");
    assert_eq!(joints(&editor), [root]);
    set_field(&mut editor, &ctx, 1, "5");
    press(&mut editor, &ctx, "Create");
    let child = lead(&editor).unwrap();
    assert_ne!(child, root);
    let scene = editor.history.scene();
    let paths = scene.transform_paths().unwrap();
    let path = paths.iter().find(|target| target.node == child).unwrap();
    assert_eq!(path.path.len(), 2, "the second joint is the root's child");
    assert!(
        editor.joint_options.dialog.is_some(),
        "the window stays open"
    );

    editor.deselect_all();
    press(&mut editor, &ctx, "Create");
    let second_root = lead(&editor).unwrap();
    let scene = editor.history.scene();
    let paths = scene.transform_paths().unwrap();
    let path = paths
        .iter()
        .find(|target| target.node == second_root)
        .unwrap();
    assert_eq!(path.path.len(), 1, "a new hierarchy");
    assert_eq!(joints(&editor).len(), 3);
}

#[test]
fn window_values_reach_the_operation_and_a_used_name_is_cleared() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    press(&mut editor, &ctx, "Create");
    set_field(&mut editor, &ctx, 0, "3");
    set_field(&mut editor, &ctx, 3, "0.25");
    set_field(&mut editor, &ctx, NAME_FIELD, " hip ");
    editor.joint_options.dialog.as_mut().unwrap().relative = true;
    press(&mut editor, &ctx, "Create");
    let id = lead(&editor).unwrap();
    let scene = editor.history.scene();
    let node = scene.node(id).unwrap();
    assert_eq!(node.name(), "hip");
    assert_eq!(scene.joint(node.transform_id()).unwrap().radius(), 0.25);
    assert_eq!(node.transform().values()[0], 3.0);
    assert_eq!(
        editor.joint_options.dialog.as_ref().unwrap().texts[NAME_FIELD],
        ""
    );
    assert_eq!(message(&editor), None);
}

#[test]
fn invalid_drafts_and_core_refusals_report_in_the_window_and_change_nothing() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    let before = editor.history.scene().clone();
    set_field(&mut editor, &ctx, 2, "abc");
    press(&mut editor, &ctx, "Create");
    assert_eq!(message(&editor).as_deref(), Some("Z must be a number"));
    assert_eq!(editor.history.scene(), &before);
    set_field(&mut editor, &ctx, 2, "0");
    set_field(&mut editor, &ctx, 3, "0");
    press(&mut editor, &ctx, "Create");
    assert!(message(&editor).is_some_and(|text| text.contains("radius")));
    assert_eq!(editor.history.scene(), &before);
    set_field(&mut editor, &ctx, 3, "1");
    press(&mut editor, &ctx, "Create");
    assert_eq!(message(&editor), None, "a success clears the old message");
    assert_eq!(joints(&editor).len(), 1);
}

#[test]
fn create_is_disabled_with_a_reason_for_a_selection_that_cannot_parent() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_window(&mut editor, &ctx);
    let reason = editor.joint_placement().err().unwrap();
    assert!(reason.contains("must be a joint"), "{reason}");
    let output = run(&mut editor, &ctx, vec![]);
    assert!(control(&output, "Create").disabled);
    let before = editor.history.scene().clone();
    press(&mut editor, &ctx, "Create");
    assert_eq!(editor.history.scene(), &before, "a disabled click is inert");

    editor.deselect_all();
    press(&mut editor, &ctx, "Create");
    let first = lead(&editor).unwrap();
    assert_eq!(joints(&editor), [first]);
    editor.create(Primitive::Cube);
    let cube = lead(&editor).unwrap();
    remote(
        &mut editor,
        Operation::ModifySelection {
            change: SelectionChange::Add,
            node: first,
            instance_path: None,
        },
    )
    .unwrap();
    assert!(editor.selection.len() == 2 && cube != first);
    let reason = editor.joint_placement().err().unwrap();
    assert!(reason.contains("Select one joint"), "{reason}");

    editor.deselect_all();
    remote(
        &mut editor,
        Operation::SetSelectionMode(SelectionMode::Face),
    )
    .unwrap();
    assert_eq!(
        editor.joint_placement().err().as_deref(),
        Some(JOINT_COMPONENT_REASON)
    );
}

#[test]
fn the_parent_readout_names_the_selected_joint() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    assert_eq!(
        editor.joint_placement().unwrap().parent,
        None,
        "nothing selected starts a chain"
    );
    press(&mut editor, &ctx, "Create");
    let placement = editor.joint_placement().unwrap();
    assert_eq!(placement.parent, lead(&editor));
    assert_eq!(placement.readout, "joint1");
}

#[test]
fn window_hides_during_a_file_modal_and_escape_closes_it_unless_typing() {
    let (mut editor, ctx) = setup();
    editor.create(Primitive::Cube);
    open_window(&mut editor, &ctx);
    set_field(&mut editor, &ctx, 0, "7");
    editor.request_new();
    assert!(editor.files.is_modal());
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    run(&mut editor, &ctx, vec![]);
    assert!(!editor.files.is_modal());
    let dialog = editor.joint_options.dialog.as_ref().unwrap();
    assert_eq!(dialog.texts[0], "7", "hidden, not discarded");

    ctx.memory_mut(|memory| memory.request_focus(joint_field_id(0)));
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(
        editor.joint_options.dialog.is_some(),
        "the first Escape leaves the field"
    );
    run(&mut editor, &ctx, vec![]);
    run(&mut editor, &ctx, vec![key(Key::Escape)]);
    assert!(editor.joint_options.dialog.is_none());
}

#[test]
fn a_focused_field_counts_as_a_text_field() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    ctx.memory_mut(|memory| memory.request_focus(joint_field_id(NAME_FIELD)));
    run(&mut editor, &ctx, vec![]);
    assert!(text_edit_focused(&ctx));
}

#[test]
fn outliner_marks_joint_rows_and_channel_box_shows_a_disabled_radius() {
    let (mut editor, ctx) = setup();
    open_window(&mut editor, &ctx);
    set_field(&mut editor, &ctx, 3, "0.25");
    press(&mut editor, &ctx, "Create");
    let output = run(&mut editor, &ctx, vec![]);
    let row = format!("{JOINT_ROW_GLYPH}  joint1");
    assert!(controls(&output).iter().any(|c| c.label == row));
    let radius = controls(&output)
        .into_iter()
        .find(|c| c.label == "0.25" && c.role == accesskit::Role::Button)
        .expect("the Radius value");
    assert!(radius.disabled);
}
