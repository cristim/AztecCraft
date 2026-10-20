use super::*;
use crate::native_scene::binary::write::{
    fixtures::{leaves, walk},
    tests::bare_numbered,
    transforms_file,
};

fn built(count: usize) -> (NativeScene, Vec<u8>) {
    let scene = bare_numbered(count);
    let bytes = transforms_file(&scene).unwrap();
    (scene, bytes)
}

#[test]
fn the_writers_own_output_passes() {
    let (scene, bytes) = built(3);
    verify(&scene, &bytes).unwrap();
}

#[test]
fn a_corrupted_dbl3_word_is_caught() {
    let (scene, bytes) = built(2);
    let dbl3 = leaves(&walk(&bytes), b"DBL3", b"XFRM");
    assert_eq!(dbl3.len(), 6);
    for chunk in dbl3 {
        for byte in [3, 10, 18] {
            let mut tampered = bytes.clone();
            tampered[chunk.payload.start + byte] ^= 0x01;
            assert_eq!(
                verify(&scene, &tampered),
                Err(AuthoredBinaryRefusal::SelfCheckMismatch),
                "chunk {:#x} byte {byte}",
                chunk.start
            );
        }
    }
}

#[test]
fn a_changed_name_unit_or_truncation_is_caught() {
    let (scene, bytes) = built(2);
    let entries = walk(&bytes);
    let crea = &leaves(&entries, b"CREA", b"XFRM")[0];
    let mut renamed = bytes.clone();
    renamed[crea.payload.start + 1] = b'x';
    let luni = &leaves(&entries, b"LUNI", b"HEAD")[0];
    let mut millimeters = bytes.clone();
    millimeters[luni.payload.start] = b'm';
    let truncated = bytes[..bytes.len() - 8].to_vec();
    for tampered in [renamed, millimeters, truncated] {
        assert_eq!(
            verify(&scene, &tampered),
            Err(AuthoredBinaryRefusal::SelfCheckMismatch)
        );
    }
}

#[test]
fn a_scene_that_differs_from_the_bytes_is_caught() {
    let (mut scene, bytes) = built(2);
    scene.nodes[0].channels[0].value += 1.0;
    assert_eq!(
        verify(&scene, &bytes),
        Err(AuthoredBinaryRefusal::SelfCheckMismatch)
    );
}

#[test]
fn create_identity_bytes_are_the_stored_uuids_in_rfc_order_and_distinct() {
    let (scene, bytes) = built(3);
    let crea = leaves(&walk(&bytes), b"CREA", b"XFRM");
    let mut seen = Vec::new();
    for (chunk, node) in crea.iter().zip(scene.nodes()) {
        let payload = chunk.bytes(&bytes);
        let identity = &payload[payload.len() - 16..];
        assert_eq!(identity, node.transform_uuid().unwrap().as_bytes());
        assert!(!seen.contains(&identity));
        seen.push(identity);
    }
    assert_eq!(seen.len(), 3);
}
