use super::*;
use crate::native_scene::binary::write::{
    fixtures::{leaves, walk},
    scene_file,
    tests::{bare_numbered, meshed},
};
use crate::scene::Primitive;

fn built(count: usize) -> (NativeScene, Vec<u8>) {
    let scene = bare_numbered(count);
    let bytes = scene_file(&scene).unwrap();
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
fn create_identity_bytes_are_the_stored_uuids_in_guid_byte_order_and_distinct() {
    let (scene, bytes) = built(3);
    let crea = leaves(&walk(&bytes), b"CREA", b"XFRM");
    let mut seen = Vec::new();
    for (chunk, node) in crea.iter().zip(scene.nodes()) {
        let payload = chunk.bytes(&bytes);
        let identity = &payload[payload.len() - 16..];
        assert_eq!(identity, node.transform_uuid().unwrap().to_bytes_le());
        assert!(!seen.contains(&identity));
        seen.push(identity);
    }
    assert_eq!(seen.len(), 3);
}

fn cube_and_plane() -> (NativeScene, Vec<u8>) {
    let scene = meshed(&[Primitive::Cube, Primitive::Plane]);
    let bytes = scene_file(&scene).unwrap();
    (scene, bytes)
}

/// Byte offsets of the first word of the point, endpoint and corner arrays of the first
/// `MESH` chunk, and the number of points.
fn mesh_arrays(bytes: &[u8]) -> ([usize; 3], usize) {
    let mesh = &leaves(&walk(bytes), b"MESH", b"DMSH")[0];
    let count = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let points = mesh.payload.start + 3 + 4;
    let endpoints = points + 4 * count(points - 4) + 4;
    let corners = endpoints + 4 * count(endpoints - 4) + 4;
    ([points, endpoints, corners], count(points - 4) / 3)
}

#[test]
fn a_meshed_scenes_own_output_passes() {
    let (scene, bytes) = cube_and_plane();
    verify(&scene, &bytes).unwrap();
}

#[test]
fn a_changed_point_endpoint_corner_or_shape_name_is_caught() {
    let (scene, bytes) = cube_and_plane();
    let ([points, endpoints, corners], point_count) = mesh_arrays(&bytes);
    assert_eq!(point_count, 8);
    let mut tampered = Vec::new();
    let mut point = bytes.clone();
    point[points + 3] ^= 0x01;
    tampered.push(point);
    let mut endpoint = bytes.clone();
    endpoint[endpoints + 7] = (endpoint[endpoints + 7] + 1) % 8;
    tampered.push(endpoint);
    let mut corner = bytes.clone();
    corner[corners + 3] = (corner[corners + 3] + 1) % 12;
    tampered.push(corner);
    let mut terminal = bytes.clone();
    terminal[corners + 4 * 3] ^= 0x60;
    tampered.push(terminal);
    let shape_crea = &leaves(&walk(&bytes), b"CREA", b"DMSH")[0];
    let mut renamed = bytes.clone();
    renamed[shape_crea.payload.start + 1] = b'x';
    tampered.push(renamed);
    for (index, bytes) in tampered.iter().enumerate() {
        assert_eq!(
            verify(&scene, bytes),
            Err(AuthoredBinaryRefusal::SelfCheckMismatch),
            "tamper {index}"
        );
    }
}

#[test]
fn bytes_for_another_scene_are_caught_even_when_every_name_matches() {
    let (_, bytes) = cube_and_plane();
    let swapped = meshed(&[Primitive::Plane, Primitive::Cube]);
    assert_eq!(
        verify(&swapped, &bytes),
        Err(AuthoredBinaryRefusal::SelfCheckMismatch)
    );
}

#[test]
fn shape_create_identity_bytes_are_the_stored_uuids_in_guid_byte_order() {
    let (scene, bytes) = cube_and_plane();
    let crea = leaves(&walk(&bytes), b"CREA", b"DMSH");
    assert_eq!(crea.len(), 2);
    for (chunk, shape) in crea.iter().zip(scene.shapes()) {
        let payload = chunk.bytes(&bytes);
        assert_eq!(
            &payload[payload.len() - 16..],
            shape.uuid().unwrap().to_bytes_le()
        );
    }
    assert_ne!(crea[0].bytes(&bytes)[..], crea[1].bytes(&bytes)[..]);
}
