use super::*;
use crate::maya_binary::container;
use std::sync::Arc;

const ALIGNS: [Align; 3] = [Align::One, Align::Four, Align::Eight];

#[test]
fn leaf_header_is_tag_aux_and_big_endian_size_then_payload_then_zero_padding() {
    let bytes = leaf(*b"DBL3", 0xb801_0000, &[1, 2, 3], Align::Eight).unwrap();
    assert_eq!(
        bytes,
        [
            b'D', b'B', b'L', b'3', // tag
            0xb8, 0x01, 0x00, 0x00, // aux, big endian
            0, 0, 0, 0, 0, 0, 0, 3, // payload size, big endian u64
            1, 2, 3, // payload
            0, 0, 0, 0, 0, // padding to 8
        ]
    );
}

#[test]
fn every_payload_length_pads_to_the_enclosing_alignment_with_zeros() {
    for align in ALIGNS {
        let step = align as usize;
        for length in 0..=17 {
            let payload = vec![0xa5; length];
            let bytes = leaf(*b"TEST", 7, &payload, align).unwrap();
            assert_eq!(bytes.len(), 16 + length.next_multiple_of(step), "{length}");
            assert_eq!(bytes.len(), chunk_len(length, align).unwrap());
            assert_eq!(bytes[8..16], (length as u64).to_be_bytes());
            assert_eq!(bytes[16..16 + length], payload[..]);
            assert!(bytes[16 + length..].iter().all(|byte| *byte == 0));
        }
    }
}

#[test]
fn group_size_counts_the_form_and_children_but_not_its_own_padding() {
    let child = leaf(*b"VERS", 3, b"2026", Align::Eight).unwrap();
    assert_eq!(child.len(), 24);
    let head = group(*b"HEAD", 0, std::slice::from_ref(&child), Align::Four).unwrap();
    assert_eq!(head[..4], *b"FOR8");
    assert_eq!(head[4..8], [0; 4]);
    assert_eq!(head[8..16], 28u64.to_be_bytes());
    assert_eq!(head[16..20], *b"HEAD");
    assert_eq!(head[20..], child[..]);
    let padded = group(*b"HEAD", 0, std::slice::from_ref(&child), Align::Eight).unwrap();
    assert_eq!(padded[8..16], 28u64.to_be_bytes());
    assert_eq!(padded.len(), 16 + 32);
    assert_eq!(padded[16 + 28..], [0; 4]);
}

#[test]
fn chunk_len_refuses_overflow_and_the_byte_after_the_container_limit() {
    for align in ALIGNS {
        assert_eq!(chunk_len(usize::MAX, align), Err(TooLarge));
        assert_eq!(chunk_len(usize::MAX - 16, align), Err(TooLarge));
    }
    let largest = MAX_BYTES - HEADER_BYTES;
    for align in ALIGNS {
        assert_eq!(chunk_len(largest, align), Ok(MAX_BYTES));
        assert_eq!(chunk_len(largest + 1, align), Err(TooLarge));
    }
    assert_eq!(chunk_len(largest - 7, Align::Eight), Ok(MAX_BYTES));
    assert_eq!(chunk_len(largest - 8, Align::Eight), Ok(MAX_BYTES - 8));
}

#[test]
fn a_leaf_at_the_container_limit_is_built_and_one_byte_more_is_refused() {
    let largest = MAX_BYTES - HEADER_BYTES;
    let bytes = leaf(*b"BIG_", 0, &vec![9; largest], Align::One).unwrap();
    assert_eq!(bytes.len(), MAX_BYTES);
    assert_eq!(
        leaf(*b"BIG_", 0, &vec![9; largest + 1], Align::One),
        Err(TooLarge)
    );
}

#[test]
fn children_that_together_pass_the_limit_are_refused() {
    let half = leaf(*b"HALF", 0, &vec![1; 9 * 1024 * 1024], Align::Eight).unwrap();
    assert!(group(*b"HEAD", 0, std::slice::from_ref(&half), Align::Four).is_ok());
    assert_eq!(
        group(*b"HEAD", 0, &[half.clone(), half], Align::Four),
        Err(TooLarge)
    );
}

#[test]
fn nested_output_is_accepted_by_the_strict_container_reader() {
    let t = leaf(*b"DBL3", 0xb801_0000, &[0x42; 27], Align::Eight).unwrap();
    let xfrm = group(*b"XFRM", 0, &[t], Align::Four).unwrap();
    let vers = leaf(*b"VERS", 0xb801_0000, b"2026", Align::Eight).unwrap();
    let head = group(*b"HEAD", 0, &[vers], Align::Four).unwrap();
    let file = group(*b"Maya", 0, &[head, xfrm], Align::One).unwrap();
    let container = container::inspect(Arc::from(file.as_slice())).unwrap();
    let chunks = container.chunks();
    let tags: Vec<_> = chunks.iter().map(|c| (c.tag(), c.form())).collect();
    assert_eq!(
        tags,
        [
            (*b"FOR8", Some(*b"Maya")),
            (*b"FOR8", Some(*b"HEAD")),
            (*b"VERS", None),
            (*b"FOR8", Some(*b"XFRM")),
            (*b"DBL3", None),
        ]
    );
    assert_eq!(chunks[0].padding_range().end, file.len());
    assert_eq!(container.payload(2), Some(&b"2026"[..]));
    assert_eq!(chunks[2].auxiliary(), 0xb801_0000);
    assert_eq!(chunks[1].auxiliary(), 0);
}
