use super::*;

const O: [f32; 2] = [100.0, 100.0];

#[derive(Debug, PartialEq)]
struct Act;

/// Zones NW (enabled), N (disabled), E (enabled), S (unsupported), list row 0 (enabled).
fn menu() -> MarkingMenu<Act> {
    let mut m = MarkingMenu::default();
    m.zones[Zone::NW.index()] = Some(Entry::new("Vertex", Act).checked(true));
    m.zones[Zone::N.index()] = Some(Entry::disabled("Edge", "Needs a mesh"));
    m.zones[Zone::E.index()] = Some(Entry::new("Face", Act));
    m.zones[Zone::S.index()] = Some(Entry::unsupported("UV", "No UV mode yet"));
    m.list.push(Entry::new("Select All", Act));
    m
}

fn opened() -> Interaction {
    let mut i = Interaction::default();
    assert_eq!(i.key_open(O, 0.0), Outcome::None);
    i
}

const N: Target = Target::Zone(Zone::N);
const E: Target = Target::Zone(Zone::E);
const S: Target = Target::Zone(Zone::S);
const NW: Target = Target::Zone(Zone::NW);
const ROW: Target = Target::List(0);

fn nav(i: &mut Interaction, key: NavKey, m: &MarkingMenu<Act>, t: f64) -> Outcome {
    i.key_nav(key, &m.nav_order(), t, &|x| m.is_enabled(x))
}

#[test]
fn nav_order_is_clockwise_zones_then_list_rows_and_skips_empty_zones() {
    assert_eq!(menu().nav_order(), vec![N, E, S, NW, ROW]);
    assert!(MarkingMenu::<Act>::default().nav_order().is_empty());
}

#[test]
fn key_open_opens_a_sticky_menu_with_nothing_focused_and_only_from_closed() {
    let mut i = opened();
    assert!(i.is_active() && i.is_drawn());
    assert_eq!(i.origin(), Some(O));
    assert_eq!(i.focused(), None);
    i.key_open([5.0, 5.0], 1.0);
    assert_eq!(
        i.origin(),
        Some(O),
        "an open menu does not reopen elsewhere"
    );
    let mut h = Interaction::default();
    h.press(Button::Secondary, O, None, 0.0, &|_| true);
    h.key_open([5.0, 5.0], 0.1);
    assert_eq!(
        h.origin(),
        Some(O),
        "a held menu does not turn into a key menu"
    );
    let mut bad = Interaction::default();
    bad.key_open([f32::NAN, 0.0], 0.0);
    bad.key_open(O, f64::NAN);
    assert!(!bad.holds_overlay());
}

#[test]
fn next_and_prev_skip_disabled_entries_and_wrap() {
    let m = menu();
    let mut i = opened();
    let mut seen = vec![];
    for k in 0..4 {
        nav(&mut i, NavKey::Next, &m, f64::from(k));
        seen.push(i.focused().unwrap());
    }
    assert_eq!(
        seen,
        vec![E, NW, ROW, E],
        "N and S are skipped; ROW wraps to E"
    );
    let mut j = opened();
    nav(&mut j, NavKey::Prev, &m, 1.0);
    assert_eq!(
        j.focused(),
        Some(ROW),
        "Prev from nothing starts at the end"
    );
    nav(&mut j, NavKey::Prev, &m, 2.0);
    assert_eq!(j.focused(), Some(NW));
    nav(&mut j, NavKey::Prev, &m, 3.0);
    nav(&mut j, NavKey::Prev, &m, 4.0);
    assert_eq!(j.focused(), Some(ROW), "Prev wraps from E to ROW");
}

#[test]
fn nav_with_no_enabled_entry_leaves_focus_empty() {
    let mut m = MarkingMenu::<Act>::default();
    m.zones[Zone::W.index()] = Some(Entry::unsupported("UV", "No UV mode yet"));
    let mut i = opened();
    assert_eq!(nav(&mut i, NavKey::Next, &m, 1.0), Outcome::None);
    assert_eq!(i.focused(), None);
}

#[test]
fn number_focuses_by_visible_position_and_rejects_disabled() {
    let m = menu();
    let mut i = opened();
    assert_eq!(nav(&mut i, NavKey::Number(2), &m, 1.0), Outcome::None);
    assert_eq!(i.focused(), Some(E));
    assert_eq!(
        nav(&mut i, NavKey::Number(1), &m, 2.0),
        Outcome::Rejected(N),
        "the disabled entry reports its reason"
    );
    assert_eq!(i.focused(), Some(E), "focus stays where it was");
    assert_eq!(
        nav(&mut i, NavKey::Number(3), &m, 3.0),
        Outcome::Rejected(S)
    );
    for out_of_range in [0, 6, usize::MAX] {
        assert_eq!(
            nav(&mut i, NavKey::Number(out_of_range), &m, 4.0),
            Outcome::None
        );
    }
    assert_eq!(i.focused(), Some(E));
}

#[test]
fn activate_dispatches_the_focused_entry_and_closes() {
    let m = menu();
    let mut i = opened();
    assert_eq!(
        i.key_activate(1.0, &|x| m.is_enabled(x)),
        Outcome::None,
        "nothing focused"
    );
    nav(&mut i, NavKey::Number(4), &m, 2.0);
    assert_eq!(
        i.key_activate(3.0, &|x| m.is_enabled(x)),
        Outcome::Dispatch(NW)
    );
    assert!(!i.holds_overlay());
}

#[test]
fn activate_of_an_entry_that_became_disabled_is_rejected_and_stays_open() {
    let m = menu();
    let mut i = opened();
    nav(&mut i, NavKey::Number(2), &m, 1.0);
    assert_eq!(i.key_activate(2.0, &|_| false), Outcome::Rejected(E));
    assert!(i.is_active());
    assert_eq!(i.focused(), None);
}

#[test]
fn escape_dismisses_a_key_menu_and_keys_do_nothing_when_not_sticky() {
    let m = menu();
    let mut i = opened();
    assert_eq!(i.escape(), Outcome::Dismissed);
    assert_eq!(nav(&mut i, NavKey::Next, &m, 1.0), Outcome::None);
    assert_eq!(i.key_activate(1.0, &|_| true), Outcome::None);
    let mut h = Interaction::default();
    h.press(Button::Secondary, O, None, 0.0, &|_| true);
    nav(&mut h, NavKey::Next, &m, 0.1);
    assert_eq!(h.focused(), None);
    let mut s = opened();
    s.press(Button::Primary, [500.0, 500.0], None, 1.0, &|_| true);
    nav(&mut s, NavKey::Next, &m, 1.1);
    assert_eq!(s.focused(), None, "a swallowing menu ignores keys");
}

#[test]
fn keys_count_as_human_input_for_idle() {
    let m = menu();
    let quiet = FrameInput {
        human_input: false,
        primary_down: false,
        secondary_down: false,
    };
    let mut i = opened();
    nav(&mut i, NavKey::Next, &m, 29.0);
    assert_eq!(
        i.tick(30.5, quiet),
        Outcome::None,
        "navigation touched idle"
    );
    assert_eq!(i.key_activate(58.0, &|_| false), Outcome::Rejected(E));
    assert_eq!(
        i.tick(59.0, quiet),
        Outcome::None,
        "a rejected activate touched idle"
    );
}

#[test]
fn a_keyboard_action_clears_a_pointer_armed_entry() {
    let m = menu();
    let enabled = |x: Target| m.is_enabled(x);
    // Navigation disarms: a later release over the entry must not dispatch.
    let mut i = opened();
    i.press(Button::Primary, O, Some(ROW), 1.0, &enabled);
    assert_eq!(i.armed(), Some(ROW));
    nav(&mut i, NavKey::Number(1), &m, 2.0);
    assert_eq!(i.armed(), None);
    assert_eq!(
        i.release(Button::Primary, O, Some(ROW), 3.0, &enabled),
        Outcome::None
    );
    // A rejected activation disarms too.
    let mut j = opened();
    nav(&mut j, NavKey::Number(2), &m, 1.0);
    j.press(Button::Primary, O, Some(ROW), 2.0, &enabled);
    assert_eq!(j.key_activate(3.0, &|_| false), Outcome::Rejected(E));
    assert_eq!(j.armed(), None);
    assert_eq!(
        j.release(Button::Primary, O, Some(ROW), 4.0, &enabled),
        Outcome::None,
        "the earlier press no longer dispatches"
    );
}
