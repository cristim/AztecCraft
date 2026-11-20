use super::*;

const O: [f32; 2] = [100.0, 100.0];
const NORTH: [f32; 2] = [100.0, 40.0];
const NEAR: [f32; 2] = [100.0, 91.0];
const YES: &dyn Fn(Target) -> bool = &|_| true;
const NO: &dyn Fn(Target) -> bool = &|_| false;
const VERTEX: Target = Target::Zone(Zone::NW);
const ROW: Target = Target::List(0);

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
fn constants_are_pinned() {
    assert_eq!(DRAW_DELAY, 0.25);
    assert_eq!(FLICK_DISTANCE, 24.0);
    assert_eq!(IDLE_DISMISS, 30.0);
}

#[test]
fn flick_dispatches_without_drawing() {
    let mut i = held(0.0);
    i.moved(NORTH, 0.05);
    assert!(!i.is_drawn());
    assert_eq!(
        i.release(Button::Secondary, NORTH, None, 0.1, YES),
        Outcome::Dispatch(Target::Zone(Zone::N))
    );
    assert!(!i.holds_overlay());
}

#[test]
fn release_before_the_flick_distance_goes_sticky() {
    let mut i = held(0.0);
    let p = [100.0, 80.0];
    i.moved(p, 0.05);
    assert_eq!(
        i.release(Button::Secondary, p, None, 0.1, YES),
        Outcome::None
    );
    assert!(i.is_active() && i.is_drawn());
}

#[test]
fn hold_without_motion_draws_after_the_delay() {
    let mut i = held(1.0);
    i.tick(1.2, input(false, true));
    assert!(!i.is_drawn());
    let wait = i.repaint_after(1.2).expect("held schedules the draw");
    assert!((wait - 0.05).abs() < 1e-9);
    i.tick(1.25, input(false, true));
    assert!(i.is_drawn());
}

#[test]
fn hold_drag_release_on_an_entry_dispatches() {
    let mut i = held(0.0);
    i.moved([60.0, 60.0], 0.1);
    i.tick(0.4, input(true, true));
    assert!(i.is_drawn());
    assert_eq!(
        i.release(Button::Secondary, [60.0, 60.0], None, 0.5, YES),
        Outcome::Dispatch(VERTEX)
    );
}

#[test]
fn drawn_release_over_a_disabled_or_empty_zone_is_rejected_and_closes() {
    let mut i = held(0.0);
    i.tick(0.3, input(true, true));
    assert_eq!(
        i.release(Button::Secondary, NORTH, None, 0.4, NO),
        Outcome::Rejected(Target::Zone(Zone::N))
    );
    assert!(!i.holds_overlay());
}

#[test]
fn release_in_the_dead_zone_after_an_excursion_dismisses() {
    let mut i = held(0.0);
    i.moved(NORTH, 0.1);
    i.moved(NEAR, 0.2);
    i.tick(0.3, input(true, true));
    assert_eq!(
        i.release(Button::Secondary, NEAR, None, 0.4, YES),
        Outcome::Dismissed
    );
    assert!(!i.holds_overlay());
}

#[test]
fn release_in_the_dead_zone_without_an_excursion_goes_sticky() {
    let mut i = held(0.0);
    i.moved(NEAR, 0.1);
    i.tick(0.5, input(true, true));
    assert_eq!(
        i.release(Button::Secondary, NEAR, None, 0.6, YES),
        Outcome::None
    );
    assert!(i.is_active());
}

#[test]
fn release_at_exactly_the_draw_delay_counts_as_drawn() {
    // Drawn, not a flick; the same zone dispatches either way.
    let mut i = held(0.0);
    i.moved(NORTH, 0.1);
    assert_eq!(
        i.release(Button::Secondary, NORTH, None, DRAW_DELAY, YES),
        Outcome::Dispatch(Target::Zone(Zone::N))
    );
    let mut j = held(0.0);
    assert_eq!(
        j.release(Button::Secondary, O, None, DRAW_DELAY, YES),
        Outcome::None,
        "a click at the delay goes sticky"
    );
    let mut k = held(0.0);
    k.moved([100.0, 79.0], 0.1);
    assert_eq!(
        k.release(Button::Secondary, [100.0, 79.0], None, DRAW_DELAY, YES),
        Outcome::Dispatch(Target::Zone(Zone::N)),
        "drawn release dispatches below the flick distance"
    );
    let mut m = held(0.0);
    m.moved([100.0, 79.0], 0.1);
    assert_eq!(
        m.release(Button::Secondary, [100.0, 79.0], None, 0.2, YES),
        Outcome::None,
        "an undrawn release below the flick distance goes sticky"
    );
}

#[test]
fn sticky_press_and_release_on_an_entry_dispatches_on_release() {
    let mut i = sticky();
    assert_eq!(
        i.press(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.0, YES),
        Outcome::None
    );
    assert_eq!(i.armed(), Some(VERTEX));
    assert!(i.is_active(), "nothing dispatches on press");
    assert_eq!(
        i.release(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.1, YES),
        Outcome::Dispatch(VERTEX)
    );
    assert!(!i.holds_overlay());
}

#[test]
fn sticky_release_on_another_entry_disarms_and_stays_open() {
    let mut i = sticky();
    i.press(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.0, YES);
    assert_eq!(
        i.release(Button::Primary, [150.0, 150.0], Some(ROW), 1.1, YES),
        Outcome::None
    );
    assert!(i.is_active());
    assert_eq!(i.armed(), None);
}

#[test]
fn sticky_press_on_a_disabled_entry_is_rejected_and_stays_open() {
    let mut i = sticky();
    assert_eq!(
        i.press(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.0, NO),
        Outcome::Rejected(VERTEX)
    );
    assert!(i.is_active());
    assert_eq!(i.armed(), None);
}

#[test]
fn press_elsewhere_dismisses_and_swallows_the_release() {
    let mut i = sticky();
    assert_eq!(
        i.press(Button::Primary, [500.0, 500.0], None, 1.0, YES),
        Outcome::Dismissed
    );
    assert!(!i.is_active());
    assert!(i.holds_overlay(), "the release must not click through");
    assert_eq!(
        i.release(Button::Primary, [500.0, 500.0], None, 1.1, YES),
        Outcome::None
    );
    assert!(!i.holds_overlay());
}

#[test]
fn a_second_secondary_press_while_open_dismisses_and_swallows() {
    let mut i = sticky();
    assert_eq!(
        i.press(Button::Secondary, O, None, 1.0, YES),
        Outcome::Dismissed
    );
    assert!(i.holds_overlay());
    i.release(Button::Secondary, O, None, 1.1, YES);
    assert!(!i.holds_overlay());
}

#[test]
fn sticky_release_of_an_armed_entry_that_became_disabled_is_rejected_and_stays_open() {
    let mut i = sticky();
    i.press(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.0, YES);
    assert_eq!(
        i.release(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.1, NO),
        Outcome::Rejected(VERTEX)
    );
    assert!(i.is_active(), "a sticky menu stays open after a rejection");
    assert_eq!(i.armed(), None);
    i.press(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.2, YES);
    assert_eq!(
        i.release(Button::Primary, [60.0, 60.0], Some(VERTEX), 1.3, YES),
        Outcome::Dispatch(VERTEX)
    );
}

#[test]
fn flick_is_chosen_at_the_flick_distance_and_not_below_it() {
    let mut at = held(0.0);
    at.moved([100.0, 76.0], 0.05);
    assert_eq!(
        at.release(Button::Secondary, [100.0, 76.0], None, 0.1, YES),
        Outcome::Dispatch(Target::Zone(Zone::N))
    );
    let mut below = held(0.0);
    below.moved([100.0, 76.1], 0.05);
    assert_eq!(
        below.release(Button::Secondary, [100.0, 76.1], None, 0.1, YES),
        Outcome::None
    );
    assert!(below.is_active());
}
