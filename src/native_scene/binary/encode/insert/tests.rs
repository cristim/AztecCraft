use super::*;
use crate::native_scene::binary::ErrorKind;
use crate::{
    maya_binary::container,
    native_scene::binary::tests::{
        site_fixture::{SITE_AUX, other_node_with_triple, sited_transform, triple_aux},
        wire_fixture::scene,
    },
};

fn record(values: [f64; 3]) -> Vec<u8> {
    let mut bytes = b"DBL3".to_vec();
    bytes.extend(SITE_AUX.to_be_bytes());
    bytes.extend(27u64.to_be_bytes());
    bytes.extend(b"t\0\x20");
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
    bytes.extend([0; 5]);
    bytes
}

fn budget() -> Budget {
    Budget::new(1024).unwrap()
}

fn word(bytes: &[u8], header: usize) -> u64 {
    u64::from_be_bytes(bytes[header + 8..header + 16].try_into().unwrap())
}

/// Two transforms (`a` with `r`, `b` with `s`) and a node that carries a canonical `DBL3`.
fn file() -> Vec<u8> {
    scene(
        &[
            sited_transform(b"a", &[triple_aux(b"r", SITE_AUX, [1.0; 3])]),
            sited_transform(b"b", &[triple_aux(b"s", SITE_AUX, [2.0; 3])]),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat(),
    )
    .to_vec()
}

/// `(xfrm header start, header start of its only DBL3)` for each transform, from chunk boundaries.
fn layout(bytes: &[u8]) -> Vec<(usize, usize)> {
    let inspected = container::inspect(bytes.to_vec().into()).unwrap();
    let chunks = inspected.chunks();
    let mut found = Vec::new();
    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.form() == Some(*b"XFRM") {
            let triple = chunks
                .iter()
                .find(|c| c.parent() == Some(index) && c.tag() == *b"DBL3")
                .unwrap();
            found.push((chunk.header_range().start, triple.header_range().start));
        }
    }
    found
}

#[test]
fn one_record_is_placed_and_only_the_two_size_words_change() {
    let original = file();
    let (xfrm, triple) = layout(&original)[0];
    let new = record([4.0, 5.0, 6.0]);
    assert_eq!(new.len(), 48);
    let plan = [Insertion {
        owner: NativeId(1),
        field: TransformField::Translation,
        at: triple,
        bytes: new.clone(),
        ancestors: vec![xfrm, 0],
    }];
    let output = splice(&original, &plan, &mut budget()).unwrap();

    let mut expected = original.clone();
    expected.splice(triple..triple, new);
    expected[xfrm + 8..xfrm + 16].copy_from_slice(&(word(&original, xfrm) + 48).to_be_bytes());
    expected[8..16].copy_from_slice(&(word(&original, 0) + 48).to_be_bytes());
    assert_eq!(output, expected);
    assert_eq!(output.len(), original.len() + 48);
    assert_eq!(word(&output, 0), word(&original, 0) + 48);
    assert_eq!(word(&output, xfrm), word(&original, xfrm) + 48);
    let decoded = crate::native_scene::binary::decode(output.into()).unwrap();
    assert_eq!(decoded.nodes()[0].transform().translation, [4.0, 5.0, 6.0]);
    assert_eq!(decoded.nodes()[1].transform().translation, [0.0; 3]);
}

#[test]
fn two_insertions_in_different_groups_grow_each_group_and_the_root_once_per_record() {
    let original = file();
    let layout = layout(&original);
    let (first, second) = (layout[0], layout[1]);
    let plan = [
        Insertion {
            owner: NativeId(1),
            field: TransformField::Translation,
            at: first.1,
            bytes: record([1.0; 3]),
            ancestors: vec![first.0, 0],
        },
        Insertion {
            owner: NativeId(1),
            field: TransformField::Translation,
            at: second.1,
            bytes: record([2.0; 3]),
            ancestors: vec![second.0, 0],
        },
    ];
    let output = splice(&original, &plan, &mut budget()).unwrap();
    assert_eq!(output.len(), original.len() + 96);
    assert_eq!(word(&output, 0), word(&original, 0) + 96);
    assert_eq!(word(&output, first.0), word(&original, first.0) + 48);
    assert_eq!(word(&output, second.0 + 48), word(&original, second.0) + 48);
    assert_eq!(&output[first.1..first.1 + 48], record([1.0; 3]));
    assert_eq!(&output[second.1 + 48..second.1 + 96], record([2.0; 3]));
    let decoded = crate::native_scene::binary::decode(output.into()).unwrap();
    assert_eq!(decoded.nodes()[0].transform().translation, [1.0; 3]);
    assert_eq!(decoded.nodes()[1].transform().translation, [2.0; 3]);
}

#[test]
fn an_empty_plan_returns_the_input() {
    let original = file();
    assert_eq!(splice(&original, &[], &mut budget()).unwrap(), original);
}

#[test]
fn malformed_plans_are_refused_without_panicking() {
    let original = file();
    let (xfrm, triple) = layout(&original)[0];
    let next = layout(&original)[1].0;
    let insertion = |at, ancestors: &[usize]| Insertion {
        owner: NativeId(1),
        field: TransformField::Translation,
        at,
        bytes: record([0.0; 3]),
        ancestors: ancestors.to_vec(),
    };
    let refused = |plan: &[Insertion]| splice(&original, plan, &mut budget()).unwrap_err().kind();
    assert_eq!(
        refused(&[insertion(triple, &[xfrm + 1, 0])]),
        ErrorKind::Unsupported
    );
    assert_eq!(
        refused(&[insertion(triple, &[triple, 0])]),
        ErrorKind::Unsupported
    );
    assert_eq!(
        refused(&[insertion(original.len() + 1, &[0])]),
        ErrorKind::Unsupported
    );
    assert_eq!(
        refused(&[insertion(next, &[next, 0])]),
        ErrorKind::Unsupported
    );
    assert_eq!(
        refused(&[insertion(triple, &[xfrm]), insertion(triple, &[xfrm])]),
        ErrorKind::Unsupported
    );
    assert_eq!(
        refused(&[insertion(triple + 48, &[xfrm]), insertion(triple, &[xfrm])]),
        ErrorKind::Unsupported
    );
}

#[test]
fn a_size_word_that_cannot_grow_is_a_resource_error() {
    let mut original = file();
    original[8..16].copy_from_slice(&(u64::MAX - 47).to_be_bytes());
    let (xfrm, triple) = layout(&file())[0];
    let plan = [Insertion {
        owner: NativeId(1),
        field: TransformField::Translation,
        at: triple,
        bytes: record([0.0; 3]),
        ancestors: vec![xfrm, 0],
    }];
    let error = splice(&original, &plan, &mut budget()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    original[8..16].copy_from_slice(&(u64::MAX - 48).to_be_bytes());
    let output = splice(&original, &plan, &mut budget()).unwrap();
    assert_eq!(word(&output, 0), u64::MAX);
}

#[test]
fn output_size_is_limited_to_the_container_maximum_at_the_boundary() {
    let fits = vec![0u8; MAX_BYTES - 48];
    let plan = |at| {
        [Insertion {
            owner: NativeId(1),
            field: TransformField::Translation,
            at,
            bytes: record([0.0; 3]),
            ancestors: vec![],
        }]
    };
    let mut budget = Budget::new(0).unwrap();
    assert_eq!(
        splice(&fits, &plan(16), &mut budget).unwrap().len(),
        MAX_BYTES
    );
    let over = vec![0u8; MAX_BYTES - 47];
    let error = splice(&over, &plan(16), &mut Budget::new(0).unwrap()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ResourceLimit);
}

#[test]
fn the_inverse_check_accepts_a_good_splice_and_rejects_tampering() {
    let original = file();
    let (xfrm, triple) = layout(&original)[0];
    let plan = [Insertion {
        owner: NativeId(1),
        field: TransformField::Translation,
        at: triple,
        bytes: record([4.0, 5.0, 6.0]),
        ancestors: vec![xfrm, 0],
    }];
    let good = splice(&original, &plan, &mut budget()).unwrap();
    assert!(check_inverse(&good, &original, &plan, &mut budget()).is_ok());
    let mut inserted = good.clone();
    inserted[triple + 20] ^= 1;
    let mut size = good.clone();
    size[xfrm + 15] ^= 1;
    let mut outside = good.clone();
    outside[good.len() - 1] ^= 1;
    let mut root = good.clone();
    root[15] ^= 1;
    for (name, bad) in [
        ("record bytes", inserted),
        ("group size word", size),
        ("byte outside", outside),
        ("root size word", root),
        ("truncated", good[..good.len() - 1].to_vec()),
    ] {
        assert!(
            check_inverse(&bad, &original, &plan, &mut budget()).is_err(),
            "{name}"
        );
    }
}
