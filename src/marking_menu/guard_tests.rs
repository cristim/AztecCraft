use super::*;

const O: [f32; 2] = [100.0, 100.0];
const NORTH: [f32; 2] = [100.0, 40.0];
const YES: &dyn Fn(Target) -> bool = &|_| true;
const VERTEX: Target = Target::Zone(Zone::NW);

fn input(human: bool, secondary: bool) -> FrameInput {
    FrameInput {
        human_input: human,
        primary_down: false,
        secondary_down: secondary,
    }
}

fn held(t: f64) -> Interaction {
    let mut i = Interaction::default();
    assert_eq!(i.press(Button::Secondary, O, None, t, YES), Outcome::None);
    i
}

fn sticky() -> Interaction {
    let mut i = held(0.0);
    assert_eq!(
        i.release(Button::Secondary, O, None, 0.05, YES),
        Outcome::None
    );
    assert!(i.is_active() && i.is_drawn());
    i
}

#[test]
fn swallow_ends_when_the_button_is_no_longer_down() {
    let mut i = sticky();
    i.press(Button::Primary, [500.0, 500.0], None, 1.0, YES);
    i.tick(
        1.1,
        FrameInput {
            human_input: true,
            primary_down: true,
            secondary_down: false,
        },
    );
    assert!(i.holds_overlay());
    i.tick(
        1.2,
        FrameInput {
            human_input: true,
            primary_down: false,
            secondary_down: false,
        },
    );
    assert!(!i.holds_overlay());
}

#[test]
fn escape_and_focus_loss_close_held_and_open() {
    for close in [Interaction::escape, Interaction::focus_lost] {
        let mut h = held(0.0);
        assert_eq!(close(&mut h), Outcome::Dismissed);
        assert!(!h.holds_overlay());
        let mut s = sticky();
        assert_eq!(close(&mut s), Outcome::Dismissed);
        assert!(!s.holds_overlay());
        assert_eq!(
            close(&mut s),
            Outcome::None,
            "closing a closed menu is a no-op"
        );
    }
}

#[test]
fn escape_and_focus_loss_during_swallow_keep_swallowing_the_release() {
    for close in [Interaction::escape, Interaction::focus_lost] {
        let mut i = sticky();
        i.press(Button::Primary, [500.0, 500.0], None, 1.0, YES);
        assert_eq!(close(&mut i), Outcome::None, "already dismissed once");
        assert!(
            i.holds_overlay(),
            "the release must still not click through"
        );
        assert_eq!(
            i.release(Button::Primary, [500.0, 500.0], None, 1.1, YES),
            Outcome::None
        );
        assert!(!i.holds_overlay());
    }
}

#[test]
fn motion_while_held_counts_as_human_input_for_idle() {
    let mut i = held(0.0);
    i.moved(NORTH, 29.0);
    assert_eq!(i.tick(30.5, input(false, true)), Outcome::None);
    assert_eq!(i.tick(59.1, input(false, true)), Outcome::Dismissed);
}

#[test]
fn held_dismisses_when_secondary_is_no_longer_down() {
    let mut i = held(0.0);
    assert_eq!(i.tick(0.1, input(true, false)), Outcome::Dismissed);
    assert!(!i.holds_overlay());
}

#[test]
fn idle_dismisses_held_and_open() {
    let mut h = held(0.0);
    assert_eq!(h.tick(29.9, input(false, true)), Outcome::None);
    assert_eq!(h.tick(30.0, input(false, true)), Outcome::Dismissed);
    let mut s = sticky();
    assert_eq!(s.tick(30.04, input(false, false)), Outcome::None);
    assert_eq!(s.tick(30.05, input(false, false)), Outcome::Dismissed);
}

#[test]
fn a_frame_without_human_input_does_not_extend_idle() {
    let mut i = sticky();
    for t in [10.0, 20.0, 29.0] {
        assert_eq!(i.tick(t, input(false, false)), Outcome::None);
    }
    assert_eq!(i.tick(30.1, input(false, false)), Outcome::Dismissed);
    let mut j = sticky();
    j.tick(29.0, input(true, false));
    assert_eq!(j.tick(58.0, input(false, false)), Outcome::None);
    assert_eq!(j.tick(59.1, input(false, false)), Outcome::Dismissed);
}

#[test]
fn repaint_is_scheduled_only_while_the_menu_is_active() {
    let mut i = Interaction::default();
    assert_eq!(i.repaint_after(5.0), None);
    i.press(Button::Secondary, O, None, 5.0, YES);
    let held_wait = i.repaint_after(5.0).expect("held schedules the draw");
    assert!((held_wait - DRAW_DELAY).abs() < 1e-9);
    i.release(Button::Secondary, O, None, 5.1, YES);
    let open_wait = i.repaint_after(6.0).expect("open schedules the idle bound");
    assert!((open_wait - (IDLE_DISMISS - 0.9)).abs() < 1e-9);
    i.escape();
    assert_eq!(i.repaint_after(6.0), None);
}

#[test]
fn time_going_backwards_never_panics_or_flicks() {
    let mut i = held(10.0);
    i.moved(NORTH, 9.0);
    // Negative elapsed clamps to zero: undrawn and fast, so a flick.
    assert_eq!(
        i.release(Button::Secondary, NORTH, None, 5.0, YES),
        Outcome::Dispatch(Target::Zone(Zone::N))
    );
    let mut j = sticky();
    assert_eq!(j.tick(-3.0, input(false, false)), Outcome::None);
    assert!(j.repaint_after(-3.0).is_some());
}

#[test]
fn non_finite_events_are_ignored() {
    let mut i = Interaction::default();
    assert_eq!(
        i.press(Button::Secondary, [f32::NAN, 0.0], None, 0.0, YES),
        Outcome::None
    );
    assert_eq!(
        i.press(Button::Secondary, O, None, f64::NAN, YES),
        Outcome::None
    );
    assert!(!i.holds_overlay());
    let mut h = held(0.0);
    h.moved([f32::INFINITY, 0.0], 0.1);
    assert_eq!(
        h.release(Button::Secondary, [f32::NAN, 0.0], None, 0.1, YES),
        Outcome::None
    );
    assert!(h.is_active(), "a bad release leaves the menu held");
    assert_eq!(h.tick(f64::NAN, input(true, true)), Outcome::None);
}

#[test]
fn closed_menu_ignores_everything_but_a_secondary_press() {
    let mut i = Interaction::default();
    assert_eq!(
        i.press(Button::Primary, O, Some(VERTEX), 0.0, YES),
        Outcome::None
    );
    assert_eq!(
        i.release(Button::Secondary, O, None, 0.0, YES),
        Outcome::None
    );
    assert_eq!(i.tick(1.0, input(true, true)), Outcome::None);
    assert!(!i.holds_overlay());
}

#[test]
fn origin_is_the_press_position() {
    let mut i = Interaction::default();
    i.press(Button::Secondary, [7.0, 9.0], None, 0.0, YES);
    assert_eq!(i.origin(), Some([7.0, 9.0]));
    i.escape();
    assert_eq!(i.origin(), None);
}
