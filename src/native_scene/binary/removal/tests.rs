use super::*;
use crate::native_scene::{DeleteUnsupported, NativeError, binary::tests::wire_fixture::*};

fn link(mode: u8, from: &str, to: &str) -> Vec<u8> {
    chunk(
        b"CWFL",
        &[&[mode], from.as_bytes(), &[0], to.as_bytes(), &[0]].concat(),
        8,
    )
}

fn connections(links: &[Vec<u8>]) -> Vec<u8> {
    group(b"CONS", &group(b"CONN", &links.concat(), 4), 4)
}

fn transform(name: &[u8]) -> Vec<u8> {
    group(
        b"XFRM",
        &[creation(name, None), triple(b"t", [1.0, 2.0, 3.0])].concat(),
        4,
    )
}

fn shape(name: &[u8], parent: &[u8]) -> Vec<u8> {
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
fn two_objects(extra: &[u8]) -> Arc<[u8]> {
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

fn check(extra: &[u8]) -> Result<(), RemovalRefusal> {
    check_removal(&two_objects(extra), NativeId(1), &[NativeId(2)])
}

fn check_links(links: &[Vec<u8>]) -> Result<(), RemovalRefusal> {
    check(&connections(links))
}

#[test]
fn a_transform_and_shape_nothing_else_names_is_removable() {
    assert_eq!(check(&[]), Ok(()));
    assert_eq!(
        check_removal(&two_objects(&[]), NativeId(3), &[NativeId(4)]),
        Ok(())
    );
}

#[test]
fn ids_must_name_the_transform_and_its_shapes() {
    let bytes = two_objects(&[]);
    let cases: [(u64, &[u64]); 5] = [(2, &[]), (1, &[1]), (9, &[]), (1, &[9]), (0, &[])];
    for (transform, shapes) in cases {
        let shapes: Vec<_> = shapes.iter().copied().map(NativeId).collect();
        assert_eq!(
            check_removal(&bytes, NativeId(transform), &shapes),
            Err(RemovalRefusal::Unreadable)
        );
    }
    assert_eq!(
        check_removal(&bytes, NativeId(1), &[NativeId(4)]),
        Err(RemovalRefusal::ForeignChild)
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
    let unrelated = group(b"DCAM", &creation(b"cam", Some(b"|b|bs")), 4);
    assert_eq!(check(&[unrelated, selection(b":|b|bs.f")].concat()), Ok(()));
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
            RemovalRefusal::NamedElsewhere
        )))
    );
    assert_eq!(scene, baseline);
    assert_eq!(
        scene.remove(id(&baseline, 1)),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, baseline);
}
