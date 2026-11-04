use super::write_pruned;
use crate::native_scene::{
    Channel, NativeError, NativeId,
    binary::{
        ErrorKind,
        admission::Budget,
        decode, encode,
        removal::{
            PruneRange, RemovalRefusal, assert_pruned, check_removal,
            tests::{connections, link, shape, transform, two_objects},
        },
        tests::{
            appearance_fixture,
            site_fixture::{SITE_AUX, other_node_with_triple, sited_transform, triple_aux},
            wire_fixture::{chunk, creation, group, scene, triple},
        },
    },
    source_removal::IdRemap,
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
    let time = link(0, ":time1.o", ":x.i");
    let b_dsm = dsm(0, "bs.iog.og[0]", "[1]");
    let survivors = [time, b_dsm];
    let both = links(&[dsm(0, "as.iog.og[0]", "[0]")], &survivors);
    let only_b = links(&[], &survivors);
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
fn an_edit_that_needs_an_insertion_is_refused_before_it_can_reach_save() {
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
    assert_eq!(
        opened.set_channel(c, Channel::TranslateX, 5.0),
        Err(NativeError::InsertionAfterDelete)
    );
    assert!(encode(&opened).is_ok());
}

#[test]
fn the_pruned_bytes_are_checked_for_any_surviving_reference() {
    let named = two_objects(&connections(&[link(0, "a.t", "bs.f")]));
    let names: [&[u8]; 2] = [b"a", b"as"];
    assert_eq!(
        assert_pruned(&named, &names),
        Err(RemovalRefusal::NamedElsewhere)
    );
    assert_eq!(assert_pruned(&two_objects(&[]), &[b"zz"]), Ok(()));
}

#[test]
fn a_prune_that_leaves_a_reference_would_be_caught_on_the_written_bytes() {
    let saved = save_without(two_objects(&[]), "a");
    let kept: [&[u8]; 1] = [b"b"];
    assert_eq!(
        assert_pruned(&Arc::from(saved.as_slice()), &kept),
        Err(RemovalRefusal::NamedElsewhere)
    );
}

/// Removes `a` and its shape from `bytes`, lets `adjust` doctor the ranges the analysis chose on
/// `analysed` (a file whose groups before the doctored part sit at the same offsets), and writes them.
fn write_doctored(
    bytes: &Arc<[u8]>,
    analysed: &Arc<[u8]>,
    adjust: impl FnOnce(&mut Vec<PruneRange>, &crate::maya_binary::container::Container),
) -> Result<Vec<u8>, crate::native_scene::binary::BinaryError> {
    let original = decode(bytes.clone()).unwrap();
    let mut opened = original.clone();
    opened.apply_removal(&[NativeId(1), NativeId(2)]);
    let mut ranges = check_removal(analysed, &opened.removed).unwrap();
    let container = crate::maya_binary::container::inspect(bytes.clone()).unwrap();
    adjust(&mut ranges, &container);
    ranges.sort_by_key(|prune| prune.range.start);
    let source = opened.source.as_ref().unwrap();
    let remap = IdRemap::new(&opened.removed, &original);
    let names = ["a".to_string(), "as".to_string()];
    let mut budget = Budget::new(source.bytes.len() * 4).unwrap();
    write_pruned(
        &opened,
        source,
        bytes,
        &ranges,
        (&remap, &names),
        (0, 0),
        &mut budget,
    )
}

/// A range for the `CWFL` whose payload starts with `prefix`, sharing the ancestors of `sibling`.
fn link_range(
    container: &crate::maya_binary::container::Container,
    bytes: &[u8],
    prefix: &[u8],
    sibling: &PruneRange,
) -> PruneRange {
    let chunk = container
        .chunks()
        .iter()
        .find(|c| c.tag() == *b"CWFL" && bytes[c.payload_range()].starts_with(prefix))
        .unwrap();
    PruneRange {
        range: chunk.header_range().start..chunk.padding_range().end,
        ancestors: sibling.ancestors.clone(),
    }
}

#[test]
fn an_over_wide_range_never_yields_bytes() {
    let kept = link(0, ":time1.o", ":x.i");
    let own = link(0, "as.iog.og[0]", &format!("{SET}.dsm[0]"));
    let bytes = two_objects(&connections(&[own, kept]));
    let written = write_doctored(&bytes, &bytes, |ranges, container| {
        let wide = link_range(container, &bytes, b"\0:time1", ranges.last().unwrap());
        ranges.push(wide);
    });
    assert_eq!(written.unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn a_range_that_ends_beyond_the_removed_group_is_refused() {
    let junk = group(b"DPLM", &chunk(b"FLGS", b"k\0", 8), 4);
    let bytes = scene(
        &[
            transform(b"a"),
            shape(b"as", b"a"),
            junk,
            transform(b"b"),
            shape(b"bs", b"b"),
        ]
        .concat(),
    );
    let written = write_doctored(&bytes, &bytes, |ranges, container| {
        // The shape's group plus the group after it, which has no record and is not removed.
        let shape_range = ranges.pop().unwrap();
        let next = container
            .chunks()
            .iter()
            .find(|c| c.header_range().start == shape_range.range.end)
            .unwrap();
        ranges.push(PruneRange {
            range: shape_range.range.start..next.padding_range().end,
            ancestors: shape_range.ancestors,
        });
    });
    assert_eq!(written.unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn a_link_whose_node_only_shares_a_namespace_prefix_is_not_a_removed_link() {
    let own = link(0, "as.iog.og[0]", &format!("{SET}.dsm[0]"));
    let prefixed = link(0, "ns:a.msg", ":y.z");
    let other = group(b"SHAD", &creation(b"ns:a", None), 4);
    let file = |links: &[Vec<u8>]| two_objects(&[other.clone(), connections(links)].concat());
    // The removal analysis refuses the prefixed link, so it runs on the file without it.
    let (bytes, clean) = (file(&[own.clone(), prefixed]), file(&[own]));
    let written = write_doctored(&bytes, &clean, |ranges, container| {
        let wide = link_range(container, &bytes, b"\0ns:a", ranges.last().unwrap());
        ranges.push(wide);
    });
    assert_eq!(written.unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn a_surviving_text_that_names_a_removed_node_is_caught_on_the_written_bytes() {
    // The analysis ran on the file without the note, so only the scan of the written bytes sees it.
    let note = chunk(b"STR ", b"k\0\x20see as.t\0", 4);
    let analysed = two_objects(&[]);
    let bytes = two_objects(&note);
    let written = write_doctored(&bytes, &analysed, |_, _| {});
    assert_eq!(written.unwrap_err().kind(), ErrorKind::Unsupported);
    assert!(write_doctored(&analysed, &analysed, |_, _| {}).is_ok());
}

#[test]
fn a_planned_insertion_is_refused_when_a_removal_is_pending() {
    use super::super::{encode_with, insert::Insertion};
    let mut opened = decode(two_objects(&[])).unwrap();
    let a = opened.nodes()[0].id();
    opened.remove(a).unwrap();
    let plan = [Insertion {
        owner: NativeId(3),
        field: crate::native_scene::source::TransformField::Translation,
        at: 16,
        bytes: Vec::new(),
        ancestors: vec![0],
    }];
    let error = encode_with(&opened, &plan, &[]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
    assert!(encode_with(&opened, &[], &[]).is_ok());
}
