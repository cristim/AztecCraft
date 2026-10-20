use super::*;
use crate::{maya_binary::container, scene::Primitive};
use std::sync::Arc;

#[test]
fn the_stub_refuses_what_the_shared_eligibility_refuses() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "persp").unwrap();
    assert_eq!(
        encode_authored(&scene),
        Err(AuthoredBinaryRefusal::Ineligible(
            AuthoredIneligible::ReservedName
        ))
    );
}

#[test]
fn the_stub_output_is_an_empty_maya_root_the_container_accepts() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let bytes = encode_authored(&scene).unwrap();
    assert_eq!(bytes[..4], *b"FOR8");
    assert_eq!(bytes[4..8], [0; 4]);
    assert_eq!(bytes[8..16], 4u64.to_be_bytes());
    assert_eq!(bytes[16..], *b"Maya");
    container::inspect(Arc::from(bytes)).unwrap();
}

const FIXTURE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/mayascenekit/file_owner_delete.mb"
));

#[test]
fn the_vendored_fixture_is_the_pinned_file() {
    assert_eq!(FIXTURE.len(), 8096);
    let container = container::inspect(Arc::from(FIXTURE)).unwrap();
    assert_eq!(container.chunks()[0].form(), Some(*b"Maya"));
}

#[test]
fn dbl3_aux_is_what_maya_stored_on_every_dbl3_chunk_of_the_vendored_fixture() {
    let container = container::inspect(Arc::from(FIXTURE)).unwrap();
    let dbl3: Vec<_> = container
        .chunks()
        .iter()
        .filter(|chunk| chunk.tag() == *b"DBL3")
        .collect();
    assert_eq!(dbl3.len(), 7);
    assert_eq!(dbl3[0].header_range().start, 0x2e4);
    for chunk in &dbl3 {
        assert_eq!(chunk.auxiliary(), DBL3_AUX);
    }
    assert_eq!(DBL3_AUX, 0xb801_0000);
}

#[test]
fn group_aux_is_what_maya_stored_on_the_root_and_transform_groups_of_the_fixture() {
    let container = container::inspect(Arc::from(FIXTURE)).unwrap();
    let mut seen = 0;
    for chunk in container.chunks() {
        if matches!(chunk.form(), Some(form) if [*b"Maya", *b"XFRM"].contains(&form)) {
            assert_eq!(chunk.auxiliary(), GROUP_AUX);
            seen += 1;
        }
    }
    assert_eq!(seen, 5);
    assert_eq!(GROUP_AUX, 0);
}

#[test]
fn maya_version_matches_the_fixture_vers_chunk_and_the_ascii_header() {
    let container = container::inspect(Arc::from(FIXTURE)).unwrap();
    let vers = container
        .chunks()
        .iter()
        .position(|chunk| chunk.tag() == *b"VERS")
        .unwrap();
    assert_eq!(
        container.payload(vers),
        Some(authored::MAYA_VERSION.as_bytes())
    );
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let ascii = crate::native_scene::ascii::emit_geometry_prefix(&scene).unwrap();
    let expected = format!(
        "//Maya ASCII {v} scene\nrequires maya \"{v}\";\n",
        v = authored::MAYA_VERSION
    );
    assert!(ascii.starts_with(expected.as_bytes()));
}
