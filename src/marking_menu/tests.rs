use super::*;

/// Unit vector at `degrees` clockwise from north, in screen coordinates.
fn at(degrees: f64, len: f64) -> [f32; 2] {
    let r = degrees.to_radians();
    [(len * r.sin()) as f32, (-len * r.cos()) as f32]
}

#[test]
fn eight_centres_resolve_to_their_compass_zones() {
    for (i, zone) in Zone::ALL.into_iter().enumerate() {
        assert_eq!(zone_at(at(45.0 * i as f64, 50.0), DEAD_ZONE), Some(zone));
        assert_eq!(zone.index(), i);
    }
    assert_eq!(zone_at([0.0, -50.0], DEAD_ZONE), Some(Zone::N));
    assert_eq!(zone_at([50.0, 0.0], DEAD_ZONE), Some(Zone::E));
    assert_eq!(zone_at([0.0, 50.0], DEAD_ZONE), Some(Zone::S));
    assert_eq!(zone_at([-50.0, 0.0], DEAD_ZONE), Some(Zone::W));
}

#[test]
fn sector_edges_sit_at_twenty_two_and_a_half_degrees() {
    for k in 0..8 {
        let edge = 22.5 + 45.0 * f64::from(k);
        let before = Zone::ALL[k as usize];
        let after = Zone::ALL[(k as usize + 1) % 8];
        assert_eq!(zone_at(at(edge - 0.05, 50.0), DEAD_ZONE), Some(before));
        assert_eq!(zone_at(at(edge + 0.05, 50.0), DEAD_ZONE), Some(after));
    }
}

#[test]
fn dead_zone_boundary_and_zero_delta() {
    assert_eq!(zone_at([0.0, -11.9], DEAD_ZONE), None);
    assert_eq!(zone_at([0.0, -12.0], DEAD_ZONE), Some(Zone::N));
    assert_eq!(zone_at([0.0, 0.0], 0.0), None);
    assert_eq!(zone_at([0.0, 0.0], DEAD_ZONE), None);
}

#[test]
fn non_finite_input_resolves_to_none() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(zone_at([bad, 40.0], DEAD_ZONE), None);
        assert_eq!(zone_at([40.0, bad], DEAD_ZONE), None);
        assert_eq!(zone_at([40.0, 40.0], bad), None);
    }
}

#[test]
fn constants_are_pinned() {
    assert_eq!(DEAD_ZONE, 12.0);
}

#[derive(Debug, PartialEq)]
enum Act {
    Vertex,
}

fn sample() -> MarkingMenu<Act> {
    let mut menu = MarkingMenu::default();
    menu.zones[Zone::NW.index()] = Some(Entry::new("Vertex", Act::Vertex).checked(true));
    menu.zones[Zone::W.index()] = Some(Entry::unsupported("UV", "No UV mode yet"));
    menu.zones[Zone::S.index()] = Some(Entry::disabled(
        "Delete",
        "Component delete is not implemented",
    ));
    menu.list.push(Entry::new("Select All", Act::Vertex));
    menu
}

#[test]
fn resolve_separates_dispatch_disabled_and_empty() {
    let menu = sample();
    assert_eq!(
        menu.resolve(Target::Zone(Zone::NW)),
        Resolved::Dispatch(&Act::Vertex)
    );
    assert_eq!(
        menu.resolve(Target::Zone(Zone::W)),
        Resolved::Disabled("No UV mode yet")
    );
    assert_eq!(
        menu.resolve(Target::Zone(Zone::S)),
        Resolved::Disabled("Component delete is not implemented")
    );
    assert_eq!(menu.resolve(Target::Zone(Zone::N)), Resolved::Empty);
    assert_eq!(
        menu.resolve(Target::List(0)),
        Resolved::Dispatch(&Act::Vertex)
    );
    assert_eq!(menu.resolve(Target::List(1)), Resolved::Empty);
    assert_eq!(menu.resolve(Target::List(usize::MAX)), Resolved::Empty);
}

#[test]
fn only_enabled_dispatch_entries_are_enabled() {
    let menu = sample();
    assert!(menu.is_enabled(Target::Zone(Zone::NW)));
    assert!(!menu.is_enabled(Target::Zone(Zone::W)));
    assert!(!menu.is_enabled(Target::Zone(Zone::S)));
    assert!(!menu.is_enabled(Target::Zone(Zone::N)));
}

#[test]
fn disabled_and_unsupported_are_distinct_states_with_reasons() {
    let menu = sample();
    let uv = menu.entry(Target::Zone(Zone::W)).unwrap();
    let delete = menu.entry(Target::Zone(Zone::S)).unwrap();
    assert!(matches!(uv.state, EntryState::Unsupported(_)));
    assert!(matches!(delete.state, EntryState::Disabled(_)));
    assert_eq!(uv.disabled_reason(), Some("No UV mode yet"));
    assert_eq!(
        menu.entry(Target::Zone(Zone::NW))
            .unwrap()
            .disabled_reason(),
        None
    );
}

#[test]
fn accessible_names_carry_the_compass_label_state_and_reason() {
    let menu = sample();
    let labels: Vec<_> = Zone::ALL.iter().map(|z| z.label()).collect();
    assert_eq!(
        labels,
        [
            "North",
            "North-east",
            "East",
            "South-east",
            "South",
            "South-west",
            "West",
            "North-west"
        ]
    );
    let name = |t| menu.accessible_name(t);
    assert_eq!(
        name(Target::Zone(Zone::NW)).as_deref(),
        Some("North-west: Vertex, checked")
    );
    assert_eq!(
        name(Target::Zone(Zone::S)).as_deref(),
        Some("South: Delete, disabled: Component delete is not implemented")
    );
    assert_eq!(
        name(Target::Zone(Zone::W)).as_deref(),
        Some("West: UV, unsupported: No UV mode yet")
    );
    assert_eq!(name(Target::List(0)).as_deref(), Some("Select All"));
    assert_eq!(name(Target::Zone(Zone::N)), None);
    assert_eq!(name(Target::List(9)), None);
}

#[test]
fn an_empty_reason_leaves_no_trailing_colon() {
    let none: Entry<u8> = Entry::disabled("Merge", "");
    assert_eq!(none.accessible_name(), "Merge, disabled");
    let none: Entry<u8> = Entry::unsupported("UV", "").checked(true);
    assert_eq!(none.accessible_name(), "UV, checked, unsupported");
}
