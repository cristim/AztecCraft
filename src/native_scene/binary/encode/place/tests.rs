use super::*;
use crate::maya_binary::container::inspect;
use crate::native_scene::{
    NativeId,
    binary::{
        decode,
        encode::insert::splice,
        tests::{
            site_fixture::{
                SITE_AUX, double_aux, other_node_with_triple, sited_transform, triple_aux,
            },
            wire_fixture::scene,
        },
    },
};
use std::sync::Arc;

fn bytes() -> Arc<[u8]> {
    scene(
        &[
            sited_transform(
                b"a",
                &[
                    double_aux(b"v", 0x24, SITE_AUX, 1.0),
                    triple_aux(b"r", SITE_AUX, [1.0; 3]),
                ],
            ),
            sited_transform(b"b", &[]),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat(),
    )
}

/// Header starts and payload ends read from the chunk boundaries of the original file.
struct Marks {
    owner: NativeId,
    a: usize,
    a_end: usize,
    visibility: usize,
    rotation: usize,
    b: usize,
    b_end: usize,
}

fn marks(scene: &NativeScene, source: &Arc<[u8]>) -> Marks {
    let tree = inspect(source.clone()).unwrap();
    let chunks = tree.chunks();
    let groups: Vec<_> = chunks
        .iter()
        .filter(|c| c.form() == Some(*b"XFRM"))
        .collect();
    let in_a = |tag: &[u8; 4]| {
        chunks
            .iter()
            .find(|c| {
                c.tag() == *tag
                    && c.parent().map(|p| chunks[p].header_range())
                        == Some(groups[0].header_range())
            })
            .unwrap()
            .header_range()
            .start
    };
    Marks {
        owner: scene.nodes[0].transform_id,
        a: groups[0].header_range().start,
        a_end: groups[0].payload_range().end,
        visibility: in_a(b"DBLE"),
        rotation: in_a(b"DBL3"),
        b: groups[1].header_range().start,
        b_end: groups[1].payload_range().end,
    }
}

fn insertion(owner: NativeId, at: usize, group: usize) -> Insertion {
    Insertion {
        owner,
        field: TransformField::Translation,
        at,
        bytes: triple_aux(b"t", SITE_AUX, [5.0, 0.0, 0.0]),
        ancestors: vec![group, 0],
    }
}

fn verdict(plan: &[Insertion], tamper: impl FnOnce(&mut Vec<u8>)) -> Result<(), Violation> {
    verdict_in(bytes(), plan, tamper)
}

fn verdict_in(
    source: Arc<[u8]>,
    plan: &[Insertion],
    tamper: impl FnOnce(&mut Vec<u8>),
) -> Result<(), Violation> {
    let decoded = decode(source.clone()).unwrap();
    let mut budget = Budget::new(source.len()).unwrap();
    let mut spliced = splice(&source, plan, &mut budget).unwrap();
    tamper(&mut spliced);
    let old = inspect(source).unwrap();
    let new = inspect(spliced.into()).unwrap();
    check(&decoded, &old, &new, plan, &mut budget)
}

fn setup() -> (Arc<[u8]>, Marks) {
    let source = bytes();
    let decoded = decode(source.clone()).unwrap();
    let marks = marks(&decoded, &source);
    (source, marks)
}

#[test]
fn records_before_the_first_triple_and_at_an_empty_group_end_are_placed() {
    let (_, m) = setup();
    assert_eq!(
        verdict(&[insertion(m.owner, m.rotation, m.a)], |_| {}),
        Ok(())
    );
    let second = Insertion {
        owner: NativeId(m.owner.0 + 1),
        ..insertion(m.owner, m.b_end, m.b)
    };
    let both = [insertion(m.owner, m.rotation, m.a), second];
    assert_eq!(verdict(&both, |_| {}), Ok(()));
}

#[test]
fn a_record_after_an_existing_triple_or_before_visibility_breaks_the_order() {
    let (_, m) = setup();
    for (name, at) in [("after r", m.a_end), ("before v", m.visibility)] {
        let found = verdict(&[insertion(m.owner, at, m.a)], |_| {});
        assert_eq!(found, Err(Violation::Order), "{name}");
    }
}

#[test]
fn a_record_in_another_group_is_not_the_owners() {
    let (_, m) = setup();
    let found = verdict(&[insertion(m.owner, m.b_end, m.b)], |_| {});
    assert_eq!(found, Err(Violation::Parent));
}

#[test]
fn a_missing_ancestor_never_reaches_the_placement_check_in_the_root_parent_layout() {
    let (_, m) = setup();
    let source = bytes();
    let mut budget = Budget::new(source.len()).unwrap();
    // Without the root the grown group overruns it.
    let unrooted = Insertion {
        ancestors: vec![m.a],
        ..insertion(m.owner, m.rotation, m.a)
    };
    let spliced = splice(&source, &[unrooted], &mut budget).unwrap();
    assert!(inspect(spliced.into()).is_err());
    // Without the group the 48-byte record would be the group's sibling, but the root pads its
    // children to 4, so the leftover bytes read as a truncated header.
    let sibling = Insertion {
        ancestors: vec![0],
        ..insertion(m.owner, m.a_end, m.a)
    };
    let spliced = splice(&source, &[sibling], &mut budget).unwrap();
    assert_eq!(
        inspect(spliced.into()).unwrap_err().kind(),
        crate::maya_binary::container::ErrorKind::Truncated
    );
}

#[test]
fn a_record_with_a_non_zero_pad_byte_is_refused() {
    let (_, m) = setup();
    let plan = [insertion(m.owner, m.rotation, m.a)];
    let pad = m.rotation + 47;
    assert_eq!(
        verdict(&plan, |bytes| bytes[pad] = 1),
        Err(Violation::Record)
    );
}

#[test]
fn a_record_that_is_not_the_planned_translation_with_the_files_aux_is_refused() {
    let (_, m) = setup();
    let wrong_aux = Insertion {
        bytes: triple_aux(b"t", SITE_AUX ^ 1, [0.0; 3]),
        ..insertion(m.owner, m.rotation, m.a)
    };
    assert_eq!(verdict(&[wrong_aux], |_| {}), Err(Violation::Record));
    let wrong_key = Insertion {
        bytes: triple_aux(b"s", SITE_AUX, [0.0; 3]),
        ..insertion(m.owner, m.rotation, m.a)
    };
    assert_eq!(verdict(&[wrong_key], |_| {}), Err(Violation::Record));
}

#[test]
fn an_unknown_owner_or_a_misreported_position_is_refused() {
    let (_, m) = setup();
    let stranger = Insertion {
        owner: NativeId(m.owner.0 + 99),
        ..insertion(m.owner, m.rotation, m.a)
    };
    assert_eq!(verdict(&[stranger], |_| {}), Err(Violation::Owner));
    let source = bytes();
    let decoded = decode(source.clone()).unwrap();
    let mut budget = Budget::new(source.len()).unwrap();
    let real = insertion(m.owner, m.rotation, m.a);
    let spliced = splice(&source, std::slice::from_ref(&real), &mut budget).unwrap();
    let old = inspect(source.clone()).unwrap();
    let new = inspect(spliced.into()).unwrap();
    let lied = Insertion {
        at: m.rotation + 1,
        ..real
    };
    assert_eq!(
        check(
            &decoded,
            &old,
            &new,
            std::slice::from_ref(&lied),
            &mut budget
        ),
        Err(Violation::Position)
    );
    assert_eq!(
        check(&decoded, &old, &new, &[], &mut budget),
        Err(Violation::Count)
    );
}

#[test]
fn any_other_changed_byte_is_refused() {
    let (_, m) = setup();
    let plan = [insertion(m.owner, m.rotation, m.a)];
    let last = |bytes: &mut Vec<u8>| *bytes.last_mut().unwrap() ^= 1;
    let anchor_payload = |bytes: &mut Vec<u8>| {
        let at = bytes.windows(4).rposition(|w| w == b"DBL3").unwrap();
        bytes[at + 20] ^= 1;
    };
    let visibility = |bytes: &mut Vec<u8>| {
        let at = bytes.windows(4).position(|w| w == b"DBLE").unwrap();
        bytes[at + 5] ^= 1;
    };
    for (name, tamper) in [
        ("final byte", &last as &dyn Fn(&mut Vec<u8>)),
        ("a triple's payload", &anchor_payload),
        ("a visibility aux", &visibility),
    ] {
        let found = verdict(&plan, |b| tamper(b));
        assert_eq!(found, Err(Violation::Chunk), "{name}");
    }
}

/// A transform inside an `AMBL` group (children aligned to 8), so a 48-byte record can be a valid
/// sibling of the transform: the layout in which only the placement check can tell.
mod nested {
    use super::*;
    use crate::native_scene::binary::tests::site_fixture::{creation_zero_pad, group_zero_pad};

    fn source() -> Arc<[u8]> {
        let inner = group_zero_pad(
            b"XFRM",
            &[
                creation_zero_pad(b"inner", None),
                triple_aux(b"r", SITE_AUX, [1.0; 3]),
            ]
            .concat(),
            8,
        );
        let holder = group_zero_pad(
            b"AMBL",
            &[creation_zero_pad(b"holder", None), inner].concat(),
            4,
        );
        scene(&[holder, other_node_with_triple(b"anchor", SITE_AUX)].concat())
    }

    /// `(owner, inner header, holder header, end of the inner group with its padding, first triple)`.
    fn marks() -> (NativeId, usize, usize, usize, usize) {
        let source = source();
        let decoded = decode(source.clone()).unwrap();
        let tree = inspect(source).unwrap();
        let chunks = tree.chunks();
        let inner = chunks
            .iter()
            .position(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        let triple = chunks
            .iter()
            .find(|c| c.parent() == Some(inner) && c.tag() == *b"DBL3")
            .unwrap();
        let holder = chunks[inner].parent().unwrap();
        (
            decoded.nodes()[0].transform_id,
            chunks[inner].header_range().start,
            chunks[holder].header_range().start,
            chunks[inner].padding_range().end,
            triple.header_range().start,
        )
    }

    #[test]
    fn the_site_names_the_whole_chain_and_the_proper_record_is_placed() {
        let (owner, inner, holder, _, triple) = marks();
        let found = decode(source()).unwrap();
        let site = &found.source.as_ref().unwrap().sites[0];
        assert_eq!(
            (site.at, site.ancestors.clone()),
            (triple, vec![inner, holder, 0])
        );
        let good = insertion(owner, triple, inner);
        let good = Insertion {
            ancestors: vec![inner, holder, 0],
            ..good
        };
        assert_eq!(verdict_in(source(), &[good], |_| {}), Ok(()));
    }

    #[test]
    fn dropping_the_innermost_ancestor_yields_a_valid_container_that_only_placement_refuses() {
        let (owner, _, holder, inner_end, _) = marks();
        let sibling = Insertion {
            ancestors: vec![holder, 0],
            ..insertion(owner, inner_end, holder)
        };
        assert_eq!(
            verdict_in(source(), std::slice::from_ref(&sibling), |_| {}),
            Err(Violation::Parent)
        );
    }

    #[test]
    fn dropping_a_middle_ancestor_leaves_a_group_too_small_for_its_child() {
        let (owner, inner, _, _, triple) = marks();
        let source = source();
        let mut budget = Budget::new(source.len()).unwrap();
        let skipped = Insertion {
            ancestors: vec![inner, 0],
            ..insertion(owner, triple, inner)
        };
        let spliced = splice(&source, &[skipped], &mut budget).unwrap();
        assert!(inspect(spliced.into()).is_err());
    }
}
