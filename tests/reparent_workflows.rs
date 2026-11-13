//! Parent and unparent on opened (sourced) scenes: both formats refuse and keep the baseline.

use azteccraft::native_scene::{NativeError, NativeScene, ReparentKeep, ascii, binary};
use azteccraft::scene::Primitive;
use std::sync::Arc;

mod binary_fixture {
    include!("support/maya_binary.rs");
}
mod ascii_fixture {
    include!("support/maya_ascii.rs");
}

#[test]
fn opened_scenes_refuse_reparent_and_stay_unchanged() {
    let mut binary_scene = binary::decode(Arc::from(binary_fixture::specimen())).unwrap();
    let id = binary_scene.nodes()[0].id();
    let before = binary_scene.clone();
    assert_eq!(
        binary_scene.reparent(id, None, ReparentKeep::Local),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(binary_scene, before);

    let mut ascii_scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let id = ascii_scene.nodes()[0].id();
    let before = ascii_scene.clone();
    assert_eq!(
        ascii_scene.reparent(id, None, ReparentKeep::World),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(ascii_scene, before);

    let mut authored = NativeScene::default();
    let a = authored.create(Primitive::Cube, "a").unwrap();
    let b = authored.create(Primitive::Cube, "b").unwrap();
    authored.reparent(b, Some(a), ReparentKeep::Local).unwrap();
}
