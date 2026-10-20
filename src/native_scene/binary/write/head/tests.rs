use super::*;
use crate::native_scene::binary::write::{
    fixtures::{MAYA_2025, groups, leaves, walk},
    profile::ROOT_AUX,
};

fn wrapped() -> Vec<u8> {
    group(*b"Maya", ROOT_AUX, &[head().unwrap()], Align::One).unwrap()
}

/// Header, payload and padding of the first `tag` child of a `HEAD` group.
fn first_head_child(file: &[u8], tag: &[u8; 4]) -> Vec<u8> {
    leaves(&walk(file), tag, b"HEAD")[0].whole(file).to_vec()
}

#[test]
fn every_emitted_head_child_is_byte_identical_to_the_fixtures() {
    let emitted = wrapped();
    for (tag, _, _) in CHILDREN {
        assert_eq!(
            first_head_child(&emitted, &tag),
            first_head_child(MAYA_2025, &tag),
            "{}",
            String::from_utf8_lossy(&tag)
        );
    }
}

#[test]
fn the_head_group_lists_exactly_the_documented_kinds_in_fixture_order() {
    let emitted = wrapped();
    let entries = walk(&emitted);
    let tags: Vec<_> = entries
        .iter()
        .filter(|e| e.parent_form == Some(*b"HEAD"))
        .map(|e| e.tag)
        .collect();
    let expected: Vec<_> = CHILDREN.iter().map(|(tag, _, _)| *tag).collect();
    assert_eq!(tags, expected);
    for omitted in [b"CHNG", b"FINF", b"PLUG", b"BCSZ"] {
        assert!(!tags.contains(omitted));
    }
}

#[test]
fn the_head_group_header_and_position_match_the_fixture() {
    let emitted = wrapped();
    let ours = &groups(&walk(&emitted), b"HEAD")[0];
    let theirs = &groups(&walk(MAYA_2025), b"HEAD")[0];
    assert_eq!(ours.aux, HEAD_GROUP_AUX);
    assert_eq!(ours.aux, theirs.aux);
    assert_eq!(ours.start, theirs.start);
    assert_eq!(ours.end % 4, 0);
}

#[test]
fn the_emitted_head_is_a_valid_container() {
    let emitted = wrapped();
    crate::maya_binary::container::inspect(std::sync::Arc::from(emitted)).unwrap();
}

#[test]
fn each_unit_payload_is_what_the_default_units_decode_to() {
    let units = crate::native_scene::Units::default();
    assert_eq!(units.linear, crate::native_scene::LinearUnit::Centimeter);
    assert_eq!(units.angular, crate::native_scene::AngularUnit::Degree);
    assert_eq!(units.time, "film");
    let payload = |tag: &[u8; 4]| CHILDREN.iter().find(|c| c.0 == *tag).unwrap().2;
    assert_eq!(payload(b"LUNI"), b"cm");
    assert_eq!(payload(b"AUNI"), b"deg");
    assert_eq!(payload(b"TUNI"), b"film");
}
