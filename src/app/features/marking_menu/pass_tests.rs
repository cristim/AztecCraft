use super::layout::DIAL_RADIUS;
use super::*;
use crate::app::features::move_tool::tests::{View, frame, oblique, raw, viewport};
use crate::editor_operations::Operation;
use crate::editor_operations::SelectionMode;
use azteccraft::marking_menu::{Target, Zone};
use azteccraft::scene::Primitive;
use eframe::egui::{Event, Vec2};
use glam::Vec3;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

pub(super) fn button(
    pos: Pos2,
    button: PointerButton,
    pressed: bool,
    modifiers: Modifiers,
) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        },
    ]
}

pub(super) fn at(
    editor: &mut Editor,
    ctx: &egui::Context,
    time: f64,
    events: Vec<Event>,
) -> egui::FullOutput {
    let mut input = raw(SIZE, Pos2::ZERO, 1.0, events);
    input.time = Some(time);
    frame(editor, ctx, input)
}

pub(super) fn rmb(pos: Pos2, pressed: bool) -> Vec<Event> {
    button(pos, PointerButton::Secondary, pressed, Modifiers::NONE)
}

pub(super) fn lmb(pos: Pos2, pressed: bool) -> Vec<Event> {
    button(pos, PointerButton::Primary, pressed, Modifiers::NONE)
}

/// A key press and release in one frame, so a repeated press is never read as a key repeat.
pub(super) fn key(key: Key, modifiers: Modifiers) -> Vec<Event> {
    [true, false]
        .into_iter()
        .map(|pressed| Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        })
        .collect()
}

pub(super) fn setup() -> (Editor, egui::Context, View) {
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: oblique(),
        ..Editor::default()
    };
    editor.create(Primitive::Cube);
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    at(&mut editor, &ctx, 0.0, vec![]);
    let output = at(&mut editor, &ctx, 0.0, vec![]);
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&output),
        scale: 1.0,
    };
    (editor, ctx, view)
}

pub(super) fn nowhere(view: &View) -> Pos2 {
    view.rect.left_top() + egui::vec2(300.0, 200.0)
}

pub(super) fn deselect(editor: &mut Editor) {
    editor
        .apply_operation(Operation::Select {
            node: None,
            instance_path: None,
        })
        .unwrap();
}

/// A sticky menu opened by an RMB click at `origin`.
pub(super) fn open_sticky(editor: &mut Editor, ctx: &egui::Context, origin: Pos2) {
    at(editor, ctx, 1.0, rmb(origin, true));
    at(editor, ctx, 1.05, rmb(origin, false));
    assert!(editor.marking_menu.is_active() && editor.marking_menu.is_drawn());
}

pub(super) fn offset(zone: Zone, radius: f32) -> Vec2 {
    let angle = -std::f32::consts::FRAC_PI_2 + zone.index() as f32 * std::f32::consts::FRAC_PI_4;
    radius * egui::vec2(angle.cos(), angle.sin())
}

#[test]
fn nav_keys_map_to_the_documented_set() {
    let none = Modifiers::NONE;
    assert_eq!(nav_key(Key::Tab, none), Some(NavKey::Next));
    assert_eq!(nav_key(Key::Tab, Modifiers::SHIFT), Some(NavKey::Prev));
    assert_eq!(nav_key(Key::ArrowUp, none), Some(NavKey::Prev));
    assert_eq!(nav_key(Key::Num9, none), Some(NavKey::Number(9)));
    assert_eq!(nav_key(Key::Num0, none), None);
    assert_eq!(nav_key(Key::A, none), None);
}

#[test]
fn an_rmb_click_opens_a_sticky_menu_at_the_press_point() {
    let (mut editor, ctx, view) = setup();
    assert!(!editor.marking_menu.holds_overlay());
    let origin = nowhere(&view);
    open_sticky(&mut editor, &ctx, origin);
    assert_eq!(editor.marking_menu.origin(), Some([origin.x, origin.y]));
    assert!(editor.overlay_open(&ctx));
}

#[test]
fn focus_loss_and_the_pointer_leaving_dismiss_a_held_or_sticky_menu() {
    for lost in [Event::PointerGone, Event::WindowFocused(false)] {
        let (mut editor, ctx, view) = setup();
        let origin = nowhere(&view);
        at(&mut editor, &ctx, 1.0, rmb(origin, true));
        assert!(editor.marking_menu.is_active());
        at(&mut editor, &ctx, 1.1, vec![lost.clone()]);
        assert!(!editor.marking_menu.is_active(), "held, {lost:?}");
        let (mut editor, ctx, view) = setup();
        open_sticky(&mut editor, &ctx, nowhere(&view));
        at(&mut editor, &ctx, 2.0, vec![lost.clone()]);
        assert!(!editor.marking_menu.holds_overlay(), "sticky, {lost:?}");
    }
}

/// A primary drag from `a` to `b` in four frames.
fn drag(editor: &mut Editor, ctx: &egui::Context, start: f64, a: Pos2, b: Pos2) {
    at(editor, ctx, start, lmb(a, true));
    at(
        editor,
        ctx,
        start + 0.1,
        vec![Event::PointerMoved(a.lerp(b, 0.5))],
    );
    at(editor, ctx, start + 0.2, vec![Event::PointerMoved(b)]);
    at(editor, ctx, start + 0.3, lmb(b, false));
}

#[test]
fn a_release_the_menu_took_raises_no_component_marquee_refusal() {
    let refusal = crate::viewport_ui::SelectionRefusal::ComponentMarquee.message();
    let (mut control, ctx, view) = setup();
    control.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let a = view.rect.left_top() + egui::vec2(100.0, 100.0);
    drag(&mut control, &ctx, 1.0, a, a + egui::vec2(200.0, 150.0));
    assert_eq!(
        control.status, refusal,
        "control: the drag is refused with no menu"
    );
    let (mut editor, ctx, view) = setup();
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    open_sticky(&mut editor, &ctx, nowhere(&view));
    // A press outside the dial dismisses it; the drag that follows belongs to the menu's swallow.
    drag(&mut editor, &ctx, 2.0, a, a + egui::vec2(200.0, 150.0));
    assert_ne!(
        editor.status, refusal,
        "the taken release must not report a marquee"
    );
}

#[test]
fn keys_the_open_menu_takes_never_reach_a_focused_widget() {
    // Focus the File button with Tab, then press Enter: with no menu it opens the File menu.
    let focus_file = |editor: &mut Editor, ctx: &egui::Context| {
        for step in 0..60 {
            let output = at(
                editor,
                ctx,
                0.1 * f64::from(step),
                key(Key::Tab, Modifiers::NONE),
            );
            let update = output.platform_output.accesskit_update.unwrap();
            let label = update
                .nodes
                .iter()
                .find(|(id, _)| *id == update.focus)
                .and_then(|(_, node)| node.label().map(str::to_owned));
            if label.as_deref() == Some("File") {
                return;
            }
        }
        panic!("Tab never reached File");
    };
    let (mut control, ctx, _) = setup();
    focus_file(&mut control, &ctx);
    at(&mut control, &ctx, 10.0, key(Key::Enter, Modifiers::NONE));
    assert!(
        egui::Popup::is_any_open(&ctx),
        "control: Enter opens the File menu"
    );

    let (mut editor, ctx, _) = setup();
    focus_file(&mut editor, &ctx);
    at(&mut editor, &ctx, 10.0, key(Key::F10, Modifiers::SHIFT));
    assert!(editor.marking_menu.is_active());
    at(&mut editor, &ctx, 10.1, key(Key::Enter, Modifiers::NONE));
    assert!(
        !egui::Popup::is_any_open(&ctx),
        "Enter belonged to the menu"
    );
    assert!(
        editor.marking_menu.is_active(),
        "and chose nothing: no entry was focused"
    );
}

#[test]
fn choosing_vertex_switches_the_mode_and_the_release_picks_nothing() {
    let (mut editor, ctx, view) = setup();
    // Start in Face mode with nothing selected and put the Object Mode wedge over the cube:
    // a release that leaked to the viewport would select it.
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    deselect(&mut editor);
    let cube = view.screen(Vec3::ZERO);
    let origin = cube - offset(Zone::E, 60.0);
    open_sticky(&mut editor, &ctx, origin);
    at(&mut editor, &ctx, 2.0, lmb(cube, true));
    assert_eq!(editor.marking_menu.armed(), Some(Target::Zone(Zone::E)));
    at(&mut editor, &ctx, 2.1, lmb(cube, false));
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(
        editor.selected().is_none(),
        "the release must not pick the cube"
    );
    assert!(!editor.marking_menu.holds_overlay());
    // The gate is open again: the same click now picks.
    at(&mut editor, &ctx, 3.0, lmb(cube, true));
    at(&mut editor, &ctx, 3.1, lmb(cube, false));
    assert!(editor.selected().is_some(), "control: a later click picks");
}

#[test]
fn a_press_elsewhere_dismisses_and_its_release_picks_nothing() {
    let (mut editor, ctx, view) = setup();
    deselect(&mut editor);
    let cube = view.screen(Vec3::ZERO);
    open_sticky(&mut editor, &ctx, nowhere(&view));
    let far = view.rect.right_bottom() - egui::vec2(20.0, 20.0);
    assert!(far.distance(nowhere(&view)) > DIAL_RADIUS);
    at(&mut editor, &ctx, 2.0, lmb(cube, true));
    assert!(editor.marking_menu.holds_overlay() && !editor.marking_menu.is_active());
    at(&mut editor, &ctx, 2.1, lmb(cube, false));
    assert!(
        editor.selected().is_none(),
        "the swallowed release must not pick"
    );
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn a_flick_chooses_without_drawing_the_dial() {
    let (mut editor, ctx, view) = setup();
    let origin = nowhere(&view);
    at(&mut editor, &ctx, 1.0, rmb(origin, true));
    let out = origin + offset(Zone::N, 40.0);
    at(&mut editor, &ctx, 1.05, vec![Event::PointerMoved(out)]);
    assert!(!editor.marking_menu.is_drawn());
    at(&mut editor, &ctx, 1.1, rmb(out, false));
    assert_eq!(editor.selection_mode, SelectionMode::Edge);
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn a_held_button_draws_the_dial_after_the_delay_without_any_motion() {
    let (mut editor, ctx, view) = setup();
    let origin = nowhere(&view);
    let output = at(&mut editor, &ctx, 1.0, rmb(origin, true));
    // The test context repaints at once, so the repaint request itself is not observable here;
    // the interaction's `repaint_after` is unit-tested, and the draw below needs no input.
    drop(output);
    assert!(!editor.marking_menu.is_drawn());
    at(&mut editor, &ctx, 1.3, vec![]);
    assert!(editor.marking_menu.is_drawn(), "drawn by the timer alone");
}

#[test]
fn shift_ctrl_and_alt_rmb_open_nothing() {
    for modifiers in [Modifiers::SHIFT, Modifiers::CTRL, Modifiers::ALT] {
        let (mut editor, ctx, view) = setup();
        at(
            &mut editor,
            &ctx,
            1.0,
            button(nowhere(&view), PointerButton::Secondary, true, modifiers),
        );
        assert!(!editor.marking_menu.holds_overlay(), "{modifiers:?}");
    }
}

#[test]
fn an_rmb_press_outside_the_viewport_opens_nothing() {
    let (mut editor, ctx, _) = setup();
    at(&mut editor, &ctx, 1.0, rmb(Pos2::new(40.0, 4.0), true));
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn a_modal_dismisses_an_open_menu() {
    let (mut editor, ctx, view) = setup();
    open_sticky(&mut editor, &ctx, nowhere(&view));
    editor.request_close();
    assert!(editor.files.is_modal());
    at(&mut editor, &ctx, 2.0, vec![]);
    assert!(!editor.marking_menu.is_active());
}

/// Drags the camera with the Secondary button, Alt held, in 6 steps; returns the distance change.
fn secondary_drag_with_alt(editor: &mut Editor, ctx: &egui::Context, origin: Pos2) -> f32 {
    let distance = editor.camera.distance();
    for step in 1..=6 {
        let mut input = raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                Event::PointerMoved(origin + egui::vec2(0.0, 8.0 * step as f32)),
            ],
        );
        input.time = Some(1.0 + 0.01 * step as f64);
        frame(editor, ctx, input);
    }
    (editor.camera.distance() - distance).abs()
}

#[test]
fn alt_pressed_after_the_rmb_does_not_dolly_the_camera() {
    let (mut editor, ctx, view) = setup();
    let origin = nowhere(&view);
    at(&mut editor, &ctx, 1.0, rmb(origin, true));
    assert!(editor.marking_menu.holds_overlay());
    assert_eq!(secondary_drag_with_alt(&mut editor, &ctx, origin), 0.0);
    // Control: the same Alt drag with no menu dollies, so the harness can see a dolly.
    let (mut plain, ctx, view) = setup();
    let origin = nowhere(&view);
    let press = button(origin, PointerButton::Secondary, true, Modifiers::ALT);
    at(&mut plain, &ctx, 1.0, press);
    assert!(!plain.marking_menu.holds_overlay());
    assert!(secondary_drag_with_alt(&mut plain, &ctx, origin) > 0.0);
}

#[test]
fn keyboard_opens_navigates_and_chooses() {
    let (mut editor, ctx, view) = setup();
    at(
        &mut editor,
        &ctx,
        1.0,
        vec![Event::PointerMoved(nowhere(&view))],
    );
    at(&mut editor, &ctx, 1.1, key(Key::F10, Modifiers::SHIFT));
    assert!(editor.marking_menu.is_active());
    // Order: N Edge, NE Face, E Multi (unsupported), SE Paint, S Inputs, W UV, NW Vertex.
    at(&mut editor, &ctx, 1.2, key(Key::Num3, Modifiers::NONE));
    assert_eq!(editor.marking_menu.focused(), Some(Target::Zone(Zone::E)));
    at(&mut editor, &ctx, 1.3, key(Key::Enter, Modifiers::NONE));
    assert!(
        editor.marking_menu.is_active(),
        "a rejected choice stays open"
    );
    assert!(editor.status.contains("Multi selection is not implemented"));
    at(&mut editor, &ctx, 1.4, key(Key::Num1, Modifiers::NONE));
    assert_eq!(editor.marking_menu.focused(), Some(Target::Zone(Zone::N)));
    at(&mut editor, &ctx, 1.5, key(Key::Enter, Modifiers::NONE));
    assert_eq!(editor.selection_mode, SelectionMode::Edge);
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn escape_closes_the_menu_and_a_closed_menu_ignores_nav_keys() {
    let (mut editor, ctx, view) = setup();
    at(&mut editor, &ctx, 0.9, key(Key::ArrowDown, Modifiers::NONE));
    assert!(!editor.marking_menu.holds_overlay());
    open_sticky(&mut editor, &ctx, nowhere(&view));
    at(&mut editor, &ctx, 2.0, key(Key::Escape, Modifiers::NONE));
    assert!(!editor.marking_menu.holds_overlay());
}

#[test]
fn nav_keys_are_consumed_while_the_menu_is_open_and_left_alone_when_closed() {
    let tab_left = |editor: &mut Editor, ctx: &egui::Context, time: f64| {
        at(editor, ctx, time, key(Key::Tab, Modifiers::NONE));
        ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Key {
                        key: Key::Tab,
                        pressed: true,
                        ..
                    }
                )
            })
        })
    };
    let (mut editor, ctx, view) = setup();
    assert!(
        tab_left(&mut editor, &ctx, 0.9),
        "closed menu: Tab is not ours"
    );
    open_sticky(&mut editor, &ctx, nowhere(&view));
    assert!(
        !tab_left(&mut editor, &ctx, 2.0),
        "open menu: Tab is consumed"
    );
    assert_eq!(editor.marking_menu.focused(), Some(Target::Zone(Zone::N)));
}
