use crate::native_scene::{
    Channel, NativeId,
    binary::{
        ErrorKind, decode, encode,
        removal::tests::{connections, link, shape, transform, two_objects},
        tests::{
            appearance_fixture,
            site_fixture::{SITE_AUX, other_node_with_triple, sited_transform, triple_aux},
            wire_fixture::{creation, group, scene, triple},
        },
    },
};
use std::sync::Arc;

const SET: &str = ":initialShadingGroup";

fn without_a(extra: &[u8]) -> Vec<u8> {
    scene(&[transform(b"b"), shape(b"bs", b"b"), extra.to_vec()].concat()).to_vec()
}

fn save_without(bytes: Arc<[u8]>, name: &str) -> Vec<u8> {
    let mut opened = decode(bytes).unwrap();
    let id = opened
        .nodes()
        .iter()
        .find(|n| n.name() == name)
        .unwrap()
        .id();
    opened.remove(id).unwrap();
    encode(&opened).unwrap()
}

#[test]
fn a_saved_scene_equals_the_file_that_never_had_the_node() {
    let saved = save_without(two_objects(&[]), "a");
    assert_eq!(saved, without_a(&[]));
    let reopened = decode(saved.into()).unwrap();
    assert_eq!(reopened.nodes()[0].name(), "b");
    assert_eq!(reopened.nodes()[0].transform_id, NativeId(1));
    assert_eq!(reopened.shapes()[0].id, NativeId(2));
    let tail = save_without(two_objects(&[]), "b");
    let expected = scene(&[transform(b"a"), shape(b"as", b"a")].concat());
    assert_eq!(tail, expected.to_vec());
}

#[test]
fn dropped_set_links_leave_the_surviving_links_and_their_groups_intact() {
    let links = |a: &[Vec<u8>], b: &[Vec<u8>]| connections(&[a, b].concat());
    let dsm = |mode, shape: &str, index: &str| link(mode, shape, &format!("{SET}.dsm{index}"));
    let both = links(
        &[dsm(0, "as.iog.og[0]", "[0]")],
        &[dsm(0, "bs.iog.og[0]", "[1]")],
    );
    let only_b = links(&[], &[dsm(0, "bs.iog.og[0]", "[1]")]);
    assert_eq!(save_without(two_objects(&both), "a"), without_a(&only_b));
    let both = links(&[dsm(1, "as.iog", "")], &[dsm(1, "bs.iog", "")]);
    let only_b = links(&[], &[dsm(1, "bs.iog", "")]);
    assert_eq!(save_without(two_objects(&both), "a"), without_a(&only_b));
}

#[test]
fn surviving_edits_are_written_into_the_pruned_file() {
    let mut opened = decode(two_objects(&[])).unwrap();
    let (a, b) = (opened.nodes()[0].id(), opened.nodes()[1].id());
    opened.set_channel(b, Channel::TranslateX, 9.0).unwrap();
    opened.remove(a).unwrap();
    let edited = group(
        b"XFRM",
        &[creation(b"b", None), triple(b"t", [9.0, 2.0, 3.0])].concat(),
        4,
    );
    let expected = scene(&[edited, shape(b"bs", b"b")].concat());
    assert_eq!(encode(&opened).unwrap(), expected.to_vec());
    assert_eq!(encode(&opened).unwrap(), expected.to_vec(), "repeat save");
}

#[test]
fn an_appearance_scene_prunes_an_unshaded_object_and_keeps_every_offset_consistent() {
    let extra = [transform(b"plain"), shape(b"plainShape", b"plain")].concat();
    let with = appearance_fixture::appearance_specimen_with_fields(appearance_fixture::Fields {
        groups: &extra,
        ..Default::default()
    });
    let saved = save_without(with.into(), "plain");
    assert_eq!(saved, appearance_fixture::appearance_specimen());
}

#[test]
fn an_edit_that_needs_an_insertion_cannot_be_saved_with_a_removal() {
    let t = triple_aux(b"t", SITE_AUX, [7.0; 3]);
    let children = [
        sited_transform(b"a", &[t]),
        sited_transform(b"c", &[triple_aux(b"r", SITE_AUX, [1.0; 3])]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut opened = decode(scene(&children)).unwrap();
    let (a, c) = (opened.nodes()[0].id(), opened.nodes()[1].id());
    opened.remove(a).unwrap();
    opened.set_channel(c, Channel::TranslateX, 5.0).unwrap();
    assert_eq!(encode(&opened).unwrap_err().kind(), ErrorKind::Unsupported);
}
