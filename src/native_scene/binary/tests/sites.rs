use super::{
    site_fixture::{
        SITE_AUX, chunk_zero_pad, creation_zero_pad, double_aux, group_zero_pad,
        other_node_with_triple, sited_transform, triple_aux,
    },
    wire_fixture::scene,
};
use crate::native_scene::binary::decode;

#[test]
fn zero_pad_builders_emit_exact_bytes() {
    let record = triple_aux(b"r", SITE_AUX, [1.0, 2.0, 3.0]);
    assert_eq!(record.len(), 48);
    assert_eq!(&record[..4], b"DBL3");
    assert_eq!(record[4..8], SITE_AUX.to_be_bytes());
    assert_eq!(record[8..16], 27u64.to_be_bytes());
    assert_eq!(&record[16..19], b"r\0\x20");
    assert_eq!(record[19..27], 1.0f64.to_be_bytes());
    assert_eq!(record[43..], [0; 5]);
    let visible = double_aux(b"v", 0x24, SITE_AUX, 1.0);
    assert_eq!(visible.len(), 32);
    assert_eq!(visible[16..19], *b"v\0\x24");
    assert_eq!(visible[27..], [0; 5]);
    let group = group_zero_pad(b"XFRM", &creation_zero_pad(b"a", None), 4);
    assert_eq!(&group[..4], b"FOR8");
    assert_eq!(group[4..8], [0; 4]);
    assert_eq!(chunk_zero_pad(b"ATTR", 7, b"abc", 8)[19..], [0; 5]);
}

#[test]
fn zero_pad_scene_decodes_with_translation_and_unrelated_node_triple() {
    let children = [
        sited_transform(
            b"object",
            &[
                double_aux(b"v", 0x24, SITE_AUX, 1.0),
                triple_aux(b"t", SITE_AUX, [4.0, 5.0, 6.0]),
            ],
        ),
        other_node_with_triple(b"light", SITE_AUX ^ 1),
    ]
    .concat();
    let decoded = decode(scene(&children)).unwrap();
    assert_eq!(decoded.nodes()[0].transform().translation, [4.0, 5.0, 6.0]);
    assert_eq!(decoded.nodes().len(), 1);
}
