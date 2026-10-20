use super::*;
use crate::native_scene::binary::write::{
    fixtures::{MAYA_2025, leaves, walk},
    profile::ROOT_AUX,
};

const UUID_A: Uuid = Uuid::from_bytes([
    0xb2, 0xb4, 0xef, 0x70, 0x34, 0xbb, 0x42, 0x49, 0x89, 0x08, 0x32, 0xa0, 0x64, 0xf1, 0xe2, 0x2f,
]);
const UUID_B: Uuid = Uuid::from_bytes([0x11; 16]);

const CHANNELS: Channels = Channels {
    translation: [1.5, -2.0, 0.25],
    rotation: [0.5, 0.0, -0.75],
    scale: [1.0, 2.0, 3.0],
};

fn in_root(children: &[Vec<u8>]) -> Vec<u8> {
    group(*b"Maya", ROOT_AUX, children, Align::One).unwrap()
}

#[test]
fn a_transform_is_create_then_translate_rotate_scale_in_that_order() {
    let file = in_root(&[xfrm("rig", UUID_A, CHANNELS).unwrap()]);
    let entries = walk(&file);
    let kids: Vec<_> = entries
        .iter()
        .filter(|e| e.parent_form == Some(*b"XFRM"))
        .collect();
    let tags: Vec<_> = kids.iter().map(|e| e.tag).collect();
    assert_eq!(tags, [*b"CREA", *b"DBL3", *b"DBL3", *b"DBL3"]);
    assert_eq!(kids[0].aux, XFRM_CREA_AUX);
    assert!(kids[1..].iter().all(|e| e.aux == DBL3_AUX));
    let keys: Vec<_> = kids[1..].iter().map(|e| e.bytes(&file)[0]).collect();
    assert_eq!(keys, [b't', b'r', b's']);
}

#[test]
fn the_create_chunk_is_flag_name_nul_and_the_uuid_in_guid_byte_order() {
    let file = in_root(&[xfrm("rig", UUID_A, CHANNELS).unwrap()]);
    let crea = &leaves(&walk(&file), b"CREA", b"XFRM")[0];
    let mut expected = vec![0x8c];
    expected.extend_from_slice(b"rig\0");
    expected.extend_from_slice(&UUID_A.to_bytes_le());
    assert_eq!(crea.bytes(&file), expected);
    assert_eq!(
        expected[expected.len() - 16..][..4],
        [0x70, 0xef, 0xb4, 0xb2]
    );
}

#[test]
fn double3_values_are_big_endian_after_key_nul_and_kind_0x20() {
    let file = in_root(&[xfrm("rig", UUID_A, CHANNELS).unwrap()]);
    let dbl3 = leaves(&walk(&file), b"DBL3", b"XFRM");
    let mut expected = vec![b'r', 0, 0x20];
    for value in CHANNELS.rotation {
        expected.extend_from_slice(&value.to_be_bytes());
    }
    assert_eq!(dbl3[1].bytes(&file), expected);
    assert_eq!(expected.len(), 27);
}

#[test]
fn the_double3_layout_reproduces_every_xfrm_dbl3_of_the_fixture() {
    let entries = walk(MAYA_2025);
    let all = leaves(&entries, b"DBL3", b"XFRM");
    assert_eq!(all.len(), 7);
    for chunk in all {
        let bytes = chunk.bytes(MAYA_2025);
        let key = std::str::from_utf8(&bytes[..1]).unwrap();
        let values: [f64; 3] = std::array::from_fn(|i| {
            f64::from_be_bytes(bytes[3 + 8 * i..11 + 8 * i].try_into().unwrap())
        });
        assert_eq!(double3(key, values), bytes, "chunk at {:#x}", chunk.start);
    }
}

#[test]
fn the_create_layout_reproduces_the_fixture_mesh_transform() {
    let entries = walk(MAYA_2025);
    let crea = leaves(&entries, b"CREA", b"XFRM")
        .into_iter()
        .find(|c| c.bytes(MAYA_2025)[1..].starts_with(b"Dummy_Mesh\0"))
        .unwrap();
    let bytes = crea.bytes(MAYA_2025);
    let uuid = Uuid::from_bytes_le(bytes[bytes.len() - 16..].try_into().unwrap());
    assert_eq!(create("Dummy_Mesh", uuid), bytes);
}

#[test]
fn consecutive_groups_pad_to_four_and_the_container_accepts_them() {
    let first = xfrm("a", UUID_A, CHANNELS).unwrap();
    let second = xfrm("bb", UUID_B, CHANNELS).unwrap();
    assert_eq!(first.len() % 4, 0);
    assert_eq!(second.len() % 4, 0);
    let file = in_root(&[first, second]);
    let entries = walk(&file);
    let groups: Vec<_> = entries
        .iter()
        .filter(|e| e.form == Some(*b"XFRM"))
        .collect();
    assert_eq!(groups.len(), 2);
    crate::maya_binary::container::inspect(std::sync::Arc::from(file)).unwrap();
}

/// Evidence for the layout: in the Maya 2025 fixture every `XFRM` and `DMSH` `CREA` ends in
/// 16 identity bytes with the v4 version nibble in byte 7 and the variant bits in byte 8.
#[test]
fn the_fixtures_identity_bytes_carry_version_and_variant_in_guid_positions() {
    let entries = walk(MAYA_2025);
    let mut identities = leaves(&entries, b"CREA", b"XFRM");
    identities.extend(leaves(&entries, b"CREA", b"DMSH"));
    assert_eq!(identities.len(), 7);
    for crea in identities {
        let payload = crea.bytes(MAYA_2025);
        let id: [u8; 16] = payload[payload.len() - 16..].try_into().unwrap();
        assert_eq!(id[7] >> 4, 4, "version nibble at byte 7: {id:02x?}");
        assert_eq!(id[8] >> 6, 0b10, "variant bits at byte 8: {id:02x?}");
        let uuid = Uuid::from_bytes_le(id);
        assert_eq!(uuid.get_version_num(), 4);
        assert_eq!(uuid.to_bytes_le(), id);
    }
}

#[test]
fn the_written_identity_has_version_and_variant_where_the_fixture_does() {
    let uuid = Uuid::from_bytes([
        0xb2, 0xb4, 0xef, 0x70, 0x34, 0xbb, 0x4a, 0x29, 0x98, 0x08, 0x32, 0xa0, 0x64, 0xf1, 0xe2,
        0x2f,
    ]);
    assert_eq!(uuid.get_version_num(), 4);
    let payload = create("rig", uuid);
    let id = &payload[payload.len() - 16..];
    assert_eq!(id[7] >> 4, 4);
    assert_eq!(id[8] >> 6, 0b10);
}
