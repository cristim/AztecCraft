use super::write_pruned;
use crate::history::{Command, History};
use crate::native_scene::{
    Channel, DeleteUnsupported, NativeError, NativeId,
    binary::{
        ErrorKind,
        admission::Budget,
        decode, encode,
        removal::{
            PruneRange, RemovalRefusal, assert_pruned, check_removal,
            tests::{child_transform, connections, link, shape, transform, tree, two_objects},
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

#[test]
fn a_bound_numeric_payload_that_spells_a_removed_name_neither_blocks_delete_nor_save() {
    // A finite double whose bytes read `\0as\0`: the delete-time and save-time scans must agree.
    let spelled = f64::from_be_bytes([0, b'a', b's', 0, 0, 0, 0, 0]);
    assert!(spelled.is_finite());
    let kept = |t: [f64; 3]| {
        group(
            b"XFRM",
            &[creation(b"b", None), triple(b"t", t)].concat(),
            4,
        )
    };
    let with_a = scene(
        &[
            transform(b"a"),
            shape(b"as", b"a"),
            kept([spelled, 0.0, 0.0]),
            shape(b"bs", b"b"),
        ]
        .concat(),
    );
    let saved = save_without(with_a, "a");
    let expected = scene(&[kept([spelled, 0.0, 0.0]), shape(b"bs", b"b")].concat());
    assert_eq!(saved, expected.to_vec());
    assert!(decode(saved.into()).is_ok());
}

fn id_of(opened: &crate::native_scene::NativeScene, name: &str) -> crate::scene::NodeId {
    opened
        .nodes()
        .iter()
        .find(|n| n.name() == name)
        .unwrap()
        .id()
}

fn save_removing(bytes: Arc<[u8]>, names: &[&str]) -> Vec<u8> {
    let mut opened = decode(bytes).unwrap();
    let ids: Vec<_> = names.iter().map(|name| id_of(&opened, name)).collect();
    opened.remove_many(&ids).unwrap();
    encode(&opened).unwrap()
}

/// `g` > `c` > `cs`, then `b` > `bs`, as written by `tree`.
fn tree_parts(keep: &[&str]) -> Arc<[u8]> {
    let parts = [
        ("g", transform(b"g")),
        ("c", child_transform(b"c", b"g")),
        ("cs", shape(b"cs", b"c")),
        ("b", transform(b"b")),
        ("bs", shape(b"bs", b"b")),
    ];
    let kept: Vec<u8> = parts
        .into_iter()
        .filter(|(name, _)| keep.contains(name))
        .flat_map(|(_, bytes)| bytes)
        .collect();
    scene(&kept)
}

#[test]
fn a_saved_group_subtree_delete_equals_the_file_that_never_had_it() {
    let expected = tree_parts(&["b", "bs"]).to_vec();
    assert_eq!(save_removing(tree(&[]), &["g"]), expected);
    // The descendant listed too, in either order, is taken once.
    assert_eq!(save_removing(tree(&[]), &["g", "c"]), expected);
    assert_eq!(save_removing(tree(&[]), &["c", "g"]), expected);
    let reopened = decode(expected.into()).unwrap();
    assert_eq!(reopened.nodes()[0].name(), "b");
    assert_eq!(reopened.nodes()[0].transform_id, NativeId(1));
    assert_eq!(reopened.shapes()[0].id, NativeId(2));
}

#[test]
fn a_saved_non_root_delete_keeps_the_parent_and_renumbers_what_follows() {
    let saved = save_removing(tree(&[]), &["c"]);
    assert_eq!(saved, tree_parts(&["g", "b", "bs"]).to_vec());
    let saved_again = saved.clone();
    let reopened = decode(saved.into()).unwrap();
    let names: Vec<_> = reopened.nodes().iter().map(|n| n.name()).collect();
    assert_eq!(names, ["g", "b"]);
    assert_eq!(reopened.nodes()[1].transform_id, NativeId(2));
    assert_eq!(reopened.shapes()[0].id, NativeId(3));
    let mut deleted = decode(tree(&[])).unwrap();
    let c = id_of(&deleted, "c");
    deleted.remove(c).unwrap();
    assert_eq!(encode(&deleted).unwrap(), encode(&deleted).unwrap());
    assert_eq!(encode(&deleted).unwrap(), saved_again);
}

#[test]
fn undo_of_a_non_root_delete_restores_the_scene_and_the_original_bytes() {
    let original = tree(&[]);
    let opened = decode(original.clone()).unwrap();
    let c = id_of(&opened, "c");
    let mut history = History::new(opened.clone());
    history.execute(Command::Remove(c)).unwrap();
    let deleted = history.scene().clone();
    assert_eq!(
        encode(&deleted).unwrap(),
        tree_parts(&["g", "b", "bs"]).to_vec()
    );
    assert!(history.undo());
    assert_eq!(*history.scene(), opened);
    assert_eq!(encode(history.scene()).unwrap(), original.to_vec());
    assert!(history.redo());
    assert_eq!(*history.scene(), deleted);
    assert_eq!(
        encode(history.scene()).unwrap(),
        tree_parts(&["g", "b", "bs"]).to_vec()
    );
}

#[test]
fn a_refused_subtree_delete_leaves_the_scene_and_its_saved_bytes_alone() {
    let named = tree(&connections(&[link(
        1,
        "cs.iog.og[0]",
        ":initialShadingGroup.dsm[0]",
    )]));
    let mut opened = decode(named.clone()).unwrap();
    let before = opened.clone();
    let ids = [id_of(&opened, "g"), id_of(&opened, "c")];
    assert_eq!(
        opened.remove_many(&ids),
        Err(NativeError::DeleteMemberRefused {
            node: "g".into(),
            reason: Box::new(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
                RemovalRefusal::NextAvailableIndex
            ))),
        })
    );
    assert_eq!(opened, before);
    assert_eq!(encode(&opened).unwrap(), named.to_vec());
}
