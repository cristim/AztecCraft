use super::*;
use crate::native_scene::{
    DeleteUnsupported, NativeError, binary::tests::appearance_fixture,
    binary::tests::wire_fixture::*,
};

pub(in crate::native_scene::binary) fn link(mode: u8, from: &str, to: &str) -> Vec<u8> {
    chunk(
        b"CWFL",
        &[&[mode], from.as_bytes(), &[0], to.as_bytes(), &[0]].concat(),
        8,
    )
}

pub(in crate::native_scene::binary) fn connections(links: &[Vec<u8>]) -> Vec<u8> {
    group(b"CONS", &group(b"CONN", &links.concat(), 4), 4)
}

pub(in crate::native_scene::binary) fn transform(name: &[u8]) -> Vec<u8> {
    group(
        b"XFRM",
        &[creation(name, None), triple(b"t", [1.0, 2.0, 3.0])].concat(),
        4,
    )
}

pub(in crate::native_scene::binary) fn shape(name: &[u8], parent: &[u8]) -> Vec<u8> {
    group(
        b"DMSH",
        &[
            creation(name, Some(parent)),
            chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
        ]
        .concat(),
        4,
    )
}

/// `a` with shape `as` (ids 1 and 2) and a survivor `b` with shape `bs` (ids 3 and 4).
pub(in crate::native_scene::binary) fn two_objects(extra: &[u8]) -> Arc<[u8]> {
    scene(
        &[
            transform(b"a"),
            shape(b"as", b"a"),
            transform(b"b"),
            shape(b"bs", b"b"),
            extra.to_vec(),
        ]
        .concat(),
    )
}

fn relation(head: &str, tail: &str) -> Vec<u8> {
    let mut bytes = b"link\0".to_vec();
    bytes.extend(head.as_bytes());
    bytes.push(0);
    bytes.extend(2u32.to_be_bytes());
    bytes.extend(format!("{tail}\0other\0").as_bytes());
    chunk(b"RELA", &bytes, 8)
}

fn selection(target: &[u8]) -> Vec<u8> {
    group(b"SLCT", &chunk(b"SLCT", target, 8), 4)
}

fn removal(bytes: &Arc<[u8]>, ids: &[NativeId]) -> Result<(), RemovalRefusal> {
    check_removal(bytes, ids).map(drop)
}

pub(in crate::native_scene::binary) fn ids(ids: &[u64]) -> Vec<NativeId> {
    ids.iter().copied().map(NativeId).collect()
}

fn check(extra: &[u8]) -> Result<(), RemovalRefusal> {
    removal(&two_objects(extra), &ids(&[1, 2]))
}

fn check_links(links: &[Vec<u8>]) -> Result<(), RemovalRefusal> {
    check(&connections(links))
}

#[test]
fn a_transform_and_shape_nothing_else_names_is_removable() {
    assert_eq!(check(&[]), Ok(()));
    assert_eq!(removal(&two_objects(&[]), &ids(&[3, 4])), Ok(()));
}

#[test]
fn ids_must_name_removable_nodes_that_belong_together() {
    let bytes = two_objects(&[]);
    for unreadable in [&[1, 2, 2][..], &[9], &[1, 9], &[0], &[]] {
        assert_eq!(
            removal(&bytes, &ids(unreadable)),
            Err(RemovalRefusal::Unreadable),
            "{unreadable:?}"
        );
    }
    for foreign in [&[2][..], &[1, 4], &[3]] {
        assert_eq!(
            removal(&bytes, &ids(foreign)),
            Err(RemovalRefusal::ForeignChild),
            "{foreign:?}"
        );
    }
    for fine in [&[1, 2, 3, 4][..], &[4, 3]] {
        assert_eq!(removal(&bytes, &ids(fine)), Ok(()));
    }
    assert_eq!(
        removal(&Arc::from(&b"junk"[..]), &ids(&[1])),
        Err(RemovalRefusal::Unreadable)
    );
}

#[test]
fn any_other_node_parented_under_the_transform_is_refused() {
    let child = group(b"XFRM", &creation(b"c", Some(b"a")), 4);
    let second_shape = shape(b"as2", b"a");
    let camera = group(b"DCAM", &creation(b"cam", Some(b"a")), 4);
    for extra in [child, second_shape, camera] {
        assert_eq!(check(&extra), Err(RemovalRefusal::ForeignChild));
    }
}

#[test]
fn long_and_absolute_paths_to_a_removed_node_are_refused() {
    for parent in [&b"|a"[..], b":|a", b"|a|as"] {
        let camera = group(b"DCAM", &creation(b"cam", Some(parent)), 4);
        assert_eq!(check(&camera), Err(RemovalRefusal::ForeignChild));
    }
    let other = group(b"DCAM", &creation(b"cam", Some(b"|b|bs")), 4);
    assert_eq!(check(&other), Ok(()));
    for target in [&b":|a"[..], b":|a|as.f[1]"] {
        assert_eq!(
            check(&selection(target)),
            Err(RemovalRefusal::NamedElsewhere)
        );
    }
    for tail in ["|a|as.message", ":|a", "|a"] {
        let linked = connections(&[relation("x", tail)]);
        assert_eq!(check(&linked), Err(RemovalRefusal::NamedElsewhere));
    }
    for links in [link(0, "|a.t", "x.y"), link(0, "x.y", ":|a|as.iog")] {
        assert_eq!(check_links(&[links]), Err(RemovalRefusal::NamedElsewhere));
    }
    assert_eq!(check(&selection(b":|b|bs.f")), Ok(()));
}

#[test]
fn links_between_removed_nodes_and_to_a_zero_mode_set_are_dropped() {
    let set = group(b"SHAD", &creation(b"sg", None), 4);
    assert_eq!(check_links(&[link(0, "a.t", "as.foo")]), Ok(()));
    assert_eq!(
        check_links(&[link(0, "as.iog.og[0]", ":initialShadingGroup.dsm[2]")]),
        Ok(())
    );
    assert_eq!(
        check_links(&[link(0, "as.iog", ":initialShadingGroup.dsm[2]")]),
        Ok(())
    );
    assert_eq!(
        check(&[set, connections(&[link(0, "as.iog.og[1]", "sg.dsm[3]")])].concat()),
        Ok(())
    );
    assert_eq!(check_links(&[link(1, "bs.iog.og[0]", "sg.dsm")]), Ok(()));
}

#[test]
fn a_set_whose_namespaced_name_contains_the_removed_name_is_another_node() {
    let set = group(b"SHAD", &creation(b"ns:a", None), 4);
    let linked = connections(&[link(0, "as.iog", "ns:a.dsm[0]")]);
    assert_eq!(check(&[set, linked].concat()), Ok(()));
}

#[test]
fn mode_one_set_links_follow_the_unindexed_set_rule() {
    let set = ":initialShadingGroup";
    let unindexed = |mode, shape: &str| link(mode, shape, &format!("{set}.dsm"));
    let indexed = |mode, shape: &str| link(mode, shape, &format!("{set}.dsm[0]"));
    // An unindexed mode-1 member is an unordered set entry (PROVISIONAL), alone or beside others.
    assert_eq!(check_links(&[unindexed(1, "as.iog")]), Ok(()));
    assert_eq!(
        check_links(&[unindexed(1, "as.iog"), unindexed(1, "bs.iog")]),
        Ok(())
    );
    for refused in [
        vec![indexed(1, "as.iog.og[0]")],
        vec![unindexed(1, "as.iog"), indexed(0, "bs.iog")],
        vec![unindexed(1, "as.iog"), indexed(1, "bs.iog")],
        vec![indexed(0, "as.iog.og[0]"), unindexed(1, "bs.iog.og[0]")],
    ] {
        assert_eq!(
            check_links(&refused),
            Err(RemovalRefusal::NextAvailableIndex)
        );
    }
    let other = link(0, "as.iog.og[0]", ":otherSet.dsm[0]");
    assert_eq!(check_links(&[other, unindexed(1, "bs.iog.og[0]")]), Ok(()));
    let alias = link(0, "bs.iog", &format!("{set}.dagSetMembers[0]"));
    for dropped in [indexed(0, "as.iog"), unindexed(1, "as.iog")] {
        assert_eq!(
            check_links(&[dropped, alias.clone()]),
            Err(RemovalRefusal::NamedElsewhere)
        );
    }
}

#[test]
fn per_face_and_custom_links_naming_a_removed_node_are_refused() {
    for (links, reason) in [
        (
            link(0, "identity.id", "as.iog.og[0].gid"),
            RemovalRefusal::PerFaceShape,
        ),
        (
            link(0, "surface.mwc", "as.iog.og[0].gco"),
            RemovalRefusal::PerFaceShape,
        ),
        (
            link(0, "a.custom", "pcuu.i"),
            RemovalRefusal::NamedElsewhere,
        ),
        (
            link(0, "pcuu.o", "a.custom"),
            RemovalRefusal::NamedElsewhere,
        ),
        (
            link(0, "as.iog.og[0]", ":set.other"),
            RemovalRefusal::NamedElsewhere,
        ),
        (
            link(0, "as.iog.og[0]", "b.dsm"),
            RemovalRefusal::NamedElsewhere,
        ),
    ] {
        assert_eq!(check_links(&[links]), Err(reason));
    }
}

#[test]
fn relations_and_selections_naming_a_removed_node_are_refused() {
    let linked = |head, tail| connections(&[relation(head, tail)]);
    assert_eq!(check(&linked("lightLinker1", "b")), Ok(()));
    for extra in [
        linked("a", "x"),
        linked("x", "as.iog"),
        linked("x", ":a"),
        selection(b":a"),
        selection(b":as.f[1]"),
    ] {
        assert_eq!(check(&extra), Err(RemovalRefusal::NamedElsewhere));
    }
    assert_eq!(check(&selection(b":b")), Ok(()));
}

#[test]
fn a_removed_range_that_would_change_an_ancestor_padding_is_refused() {
    let nested = |inner: Vec<u8>| group(b"DMTI", &group(b"CONS", &inner, 8), 4);
    let bytes = scene(&[nested([transform(b"a"), shape(b"as", b"a")].concat())].concat());
    assert_eq!(
        removal(&bytes, &ids(&[1, 2])),
        Err(RemovalRefusal::Alignment)
    );
    let plain = scene(&[nested(transform(b"b")), transform(b"a"), shape(b"as", b"a")].concat());
    assert_eq!(removal(&plain, &ids(&[2, 3])), Ok(()));
}

#[test]
fn a_repeated_shape_id_overlaps_its_own_range_and_is_refused() {
    assert_eq!(
        removal(&two_objects(&[]), &ids(&[1, 2, 2])),
        Err(RemovalRefusal::Unreadable)
    );
}

#[test]
fn an_opened_scene_reports_the_specific_reason_and_stays_untouched() {
    let mut scene = crate::native_scene::binary::decode(two_objects(&connections(&[link(
        1,
        "as.iog.og[0]",
        ":initialShadingGroup.dsm[0]",
    )])))
    .unwrap();
    let baseline = scene.clone();
    let id = |scene: &crate::native_scene::NativeScene, at: usize| scene.nodes()[at].id();
    assert_eq!(
        scene.remove(id(&baseline, 0)),
        Err(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
            RemovalRefusal::NextAvailableIndex
        )))
    );
    assert_eq!(scene, baseline);
    assert_eq!(scene.remove(id(&baseline, 1)), Ok(()));
    assert_ne!(scene, baseline);
}

/// Measurement for issue #272: the recognised shading-group graph links a shape to `dsm` with mode
/// 1 and an unindexed `dsm`, so a mode-1 refusal makes every such shape undeletable. PROVISIONAL.
#[test]
fn the_recognised_shading_group_membership_link_is_mode_one() {
    use crate::native_scene::AppearanceState;
    let bytes: Arc<[u8]> = appearance_fixture::appearance_specimen().into();
    let container = container::inspect(bytes.clone()).unwrap();
    let facts = semantic::decode_records(&container).unwrap();
    let membership: Vec<_> = facts
        .records
        .iter()
        .filter_map(|record| match &record.data {
            Data::Connection(c) if text(&facts, &c.destination) == b"surface.dsm" => Some(c.mode),
            _ => None,
        })
        .collect();
    assert_eq!(membership, [1]);
    let ready = |bytes: Arc<[u8]>| {
        crate::native_scene::binary::decode(bytes)
            .unwrap()
            .appearance_readiness()
            .state
    };
    assert_eq!(ready(bytes.clone()), AppearanceState::CheckedViewerSource);
    let mut mode_zero = bytes.to_vec();
    let at = mode_zero
        .windows(b"shape.iog.og[0]\0surface.dsm\0".len())
        .position(|w| w == b"shape.iog.og[0]\0surface.dsm\0")
        .unwrap();
    assert_eq!(mode_zero[at - 1], 1);
    mode_zero[at - 1] = 0;
    assert_ne!(
        ready(mode_zero.into()),
        AppearanceState::CheckedViewerSource
    );
}

#[test]
fn a_material_is_not_removed_with_its_only_or_first_user() {
    let appearance = |users| {
        let scene = crate::native_scene::binary::decode(
            appearance_fixture::appearance_specimen_with_users(users).into(),
        )
        .unwrap();
        let ids: Vec<_> = scene.shapes().iter().map(|shape| shape.id()).collect();
        (scene.source.unwrap().appearance.unwrap(), ids)
    };
    let (single, ids) = appearance(1);
    assert_eq!(
        check_materials(&single, &ids),
        Err(RemovalRefusal::LastMaterialUser)
    );
    let (own, ids) = appearance(2);
    assert_eq!(
        check_materials(&own, &ids[..1]),
        Err(RemovalRefusal::LastMaterialUser)
    );
    assert_eq!(
        check_materials(&own, &ids[1..]),
        Err(RemovalRefusal::LastMaterialUser)
    );
    // The fixture gives each shape its own shading group; point the second at the first's material.
    let mut shared = (*own).clone();
    let material = shared.shapes[0].binding.as_ref().unwrap().material;
    shared.shapes[1].binding.as_mut().unwrap().material = material;
    let pair = &shared;
    assert_eq!(
        check_materials(pair, &ids[..1]),
        Err(RemovalRefusal::FirstMaterialUser)
    );
    assert_eq!(check_materials(pair, &ids[1..]), Ok(()));
    assert_eq!(
        check_materials(pair, &ids),
        Err(RemovalRefusal::LastMaterialUser)
    );
    assert_eq!(check_materials(pair, &[]), Ok(()));
}

fn text_chunk(value: &str) -> Vec<u8> {
    chunk(
        b"STR ",
        &[&b"k\0\x20"[..], value.as_bytes(), &[0]].concat(),
        4,
    )
}

#[test]
fn text_mentioning_a_removed_name_is_refused_on_identifier_boundaries() {
    for value in ["see a.t", "a", ":a", "|a|as", "x,a;", "setAttr a.t 1"] {
        assert_eq!(
            check(&text_chunk(value)),
            Err(RemovalRefusal::TextReference),
            "{value}"
        );
    }
    for value in ["xa", "a1", "a_b", "ba", "map1", ""] {
        assert_eq!(check(&text_chunk(value)), Ok(()), "{value}");
    }
    let script = |value: &str| group(b"SCRP", &chunk(b"TEXT", value.as_bytes(), 8), 4);
    assert_eq!(
        check(&script("move a.t")),
        Err(RemovalRefusal::TextReference)
    );
    assert_eq!(check(&script("move b.t")), Ok(()));
}

#[test]
fn only_surviving_text_is_scanned_and_numeric_payloads_never() {
    let note = chunk(b"STR ", b"k\0\x20a.t\0", 8);
    let shape_with_text = group(
        b"DMSH",
        &[creation(b"as", Some(b"a")), note.clone()].concat(),
        4,
    );
    let numeric = group(
        b"XFRM",
        &[creation(b"c", None), triple(b"a", [1.0; 3])].concat(),
        4,
    );
    let bytes = scene(&[transform(b"a"), shape_with_text, numeric].concat());
    assert_eq!(removal(&bytes, &ids(&[1, 2])), Ok(()));
}

#[test]
fn the_production_text_limit_refuses_a_large_script_before_scanning_it() {
    // Two removed names times this text exceed the limit, so the chunk is refused unread.
    let big = text_chunk(&" ".repeat(MAX_TEXT_WORK / 2 + 16));
    assert_eq!(check(&big), Err(RemovalRefusal::TextScanLimit));
}

#[test]
fn text_work_beyond_the_limit_is_refused_not_skipped() {
    let bytes = two_objects(&text_chunk("quite long unrelated text"));
    let container = container::inspect(bytes).unwrap();
    let names: [&[u8]; 2] = [b"a", b"as"];
    assert_eq!(scan_text(&container, &[], &names, 1000), Ok(()));
    assert_eq!(
        scan_text(&container, &[], &names, 10),
        Err(RemovalRefusal::TextScanLimit)
    );
    assert_eq!(
        scan_text(&container, &[], &[b""], 1000),
        Err(RemovalRefusal::Unreadable)
    );
}

#[test]
fn a_range_that_is_not_part_of_the_removed_nodes_is_not_accepted() {
    let kept = link(0, ":time1.o", ":x.i");
    let own = link(0, "as.iog.og[0]", ":initialShadingGroup.dsm[0]");
    let bytes = two_objects(&connections(&[own, kept]));
    let ranges = check_removal(&bytes, &ids(&[1, 2])).unwrap();
    let container = container::inspect(bytes.clone()).unwrap();
    let unrelated = container
        .chunks()
        .iter()
        .find(|c| c.tag() == *b"CWFL" && bytes[c.payload_range()].starts_with(b"\0:time1"))
        .unwrap();
    let wide = PruneRange {
        range: unrelated.header_range().start..unrelated.padding_range().end,
        ancestors: Vec::new(),
    };
    let both: Vec<_> = ranges
        .iter()
        .map(|r| PruneRange {
            range: r.range.clone(),
            ancestors: Vec::new(),
        })
        .chain([wide])
        .collect();
    assert_eq!(
        check_dropped(&bytes, &ids(&[1, 2]), &both, &bytes),
        Err(RemovalRefusal::UnexpectedDrop)
    );
    // Bytes that really lack both the node and the unrelated link: only the range check refuses.
    let lacking = scene(&[transform(b"b"), shape(b"bs", b"b")].concat());
    assert_eq!(
        check_dropped(&bytes, &ids(&[1, 2]), &both, &lacking),
        Err(RemovalRefusal::UnexpectedDrop)
    );
    // The right ranges with bytes that kept every record: the count disagrees.
    assert_eq!(
        check_dropped(&bytes, &ids(&[1, 2]), &ranges, &bytes),
        Err(RemovalRefusal::UnexpectedDrop)
    );
}
