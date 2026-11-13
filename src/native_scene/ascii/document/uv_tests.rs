//! The reopen gate with UVs (PROVISIONAL layout, `.project-docs/uv-ascii-format.md`): a UV block
//! is admitted only when re-emitting the decoded scene with its UVs gives the same bytes.
use super::tests::{authored, decode, refused, replace};
use super::*;
use crate::{
    native_scene::{
        NativeMesh, UvProjectAxis, UvSet,
        ascii::write::{AsciiWriteRefusal, emit_geometry_prefix, emit_geometry_prefix_with_uvs},
    },
    primitive_options::SINGLE_QUAD_PLANE,
    scene::Primitive,
};

fn with_uvs(plane: bool) -> NativeScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    scene.create(Primitive::Cube, "b").unwrap();
    if plane {
        scene.shapes[0].mesh = NativeMesh::with_options(&SINGLE_QUAD_PLANE).unwrap();
    }
    let first = scene.shapes()[0].id();
    scene.project_uv_planar(first, UvProjectAxis::Y).unwrap();
    scene
}

fn emitted(scene: &NativeScene) -> Vec<u8> {
    emit_geometry_prefix_with_uvs(scene).unwrap()
}

fn uv_sets(scene: &NativeScene) -> Vec<Option<UvSet>> {
    scene
        .shapes()
        .iter()
        .map(|shape| scene.uv_set(shape.id()).cloned())
        .collect()
}

#[test]
fn a_saved_uv_scene_reopens_authored_with_its_uvs_and_re_emits_the_same_bytes() {
    for plane in [true, false] {
        let scene = with_uvs(plane);
        let bytes = emitted(&scene);
        let projection = authored(&bytes);
        let reopened = projection.scene();
        assert!(reopened.source.is_none());
        let want = uv_sets(&scene);
        assert!(want[0].is_some() && want[1].is_none(), "one shape has UVs");
        assert_eq!(uv_sets(reopened), want);
        assert_eq!(emitted(reopened), bytes);
        reopened.validate().unwrap();
    }
}

#[test]
fn non_canonical_uv_text_decodes_but_the_gate_refuses_it() {
    let bytes = emitted(&with_uvs(true));
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.contains("\t0 1\n") && text.contains("mu 0 4 0 1 2 3"));
    for (name, edited) in [
        ("value text", replace(&bytes, "\t0 1\n", "\t0.0 1\n")),
        (
            "signed id",
            replace(&bytes, "mu 0 4 0 1 2 3", "mu 0 4 +0 1 2 3"),
        ),
        ("padded count", replace(&bytes, "mu 0 4 0", "mu 0 04 0")),
        ("exponent", replace(&bytes, "\t0 1\n", "\t0 1e0\n")),
    ] {
        assert!(
            decode_with_uvs(Arc::from(edited.clone()), true).is_ok(),
            "{name}: the reader must accept it for the gate to be what refuses it"
        );
        assert_eq!(
            refused(&edited),
            AsciiDocumentError::Ascii(AsciiError {
                kind: ErrorKind::Unsupported,
                offset: None
            }),
            "{name}"
        );
    }
}

#[test]
fn a_uv_file_follows_the_adoption_decision_and_a_maya_file_with_other_nodes_stays_refused() {
    let bytes = emitted(&with_uvs(true));
    assert_eq!(
        decode(&bytes, false).unwrap_err(),
        AsciiDocumentError::AdoptionUnsupported
    );
    let extra = [bytes, b"createNode locator -n \"l\";\n".to_vec()].concat();
    assert!(matches!(
        refused(&extra),
        AsciiDocumentError::Ascii(AsciiError {
            kind: ErrorKind::Unsupported,
            ..
        })
    ));
}

#[test]
fn the_authored_uv_scene_still_refuses_the_public_prefix_and_keeps_its_uvs_valid() {
    let projection = authored(&emitted(&with_uvs(true)));
    assert_eq!(
        emit_geometry_prefix(projection.scene()),
        Err(AsciiWriteRefusal::SessionUvs)
    );
}
