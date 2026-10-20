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
