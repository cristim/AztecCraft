//! Primitives created with UVs (#453 T8b) through the ASCII round trip and every writer: pins what
//! each writer does with session UVs today. `.ma` admits and reads them back, OBJ writes `vt`,
//! and `.mb`, glTF and USD refuse with their SessionUvs error and succeed without UVs.
use super::tests::authored;
use super::*;
use crate::{
    native_scene::ascii::write::{DefaultLinks, emit_with_links},
    primitive_options::PrimitiveOptions,
    scene::Primitive,
    view_scene::{VIEW_BUDGET, ViewScene},
};

const LAYOUT_PRIMITIVES: [Primitive; 4] = [
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Cone,
    Primitive::Torus,
];

fn scene_of(primitive: Primitive, create_uvs: bool) -> NativeScene {
    let mut options = PrimitiveOptions::default_for(primitive);
    if let PrimitiveOptions::Cylinder {
        sub_caps,
        round_cap,
        ..
    }
    | PrimitiveOptions::Cone {
        sub_caps,
        round_cap,
        ..
    } = &mut options
    {
        *sub_caps = 2;
        *round_cap = true;
    }
    let mut scene = NativeScene::default();
    scene
        .create_with_options(primitive, &options, "p", create_uvs)
        .unwrap();
    scene
}

#[test]
fn created_uvs_survive_an_ascii_save_and_reopen() {
    for primitive in LAYOUT_PRIMITIVES {
        let scene = scene_of(primitive, true);
        let bytes = emit_with_links(&scene, DefaultLinks::Omit).unwrap();
        let projection = authored(&bytes);
        let (before, after) = (&scene.shapes()[0], &projection.scene().shapes()[0]);
        let set = scene.uv_set(before.id()).expect("created UVs");
        assert_eq!(
            projection.scene().uv_set(after.id()),
            Some(set),
            "{primitive:?}"
        );
        assert_eq!(
            emit_with_links(projection.scene(), DefaultLinks::Omit).unwrap(),
            bytes
        );
    }
}

#[test]
fn each_writer_keeps_its_documented_behaviour_for_created_uvs() {
    use crate::{gltf::GltfError, obj, usd::UsdError};
    for primitive in LAYOUT_PRIMITIVES {
        for (with_uvs, label) in [(true, "uvs"), (false, "plain")] {
            let scene = scene_of(primitive, with_uvs);
            let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
            let instances: Vec<_> = view.instances().iter().collect();
            let dir = tempfile::tempdir().unwrap();
            let at = |name: &str| dir.path().join(name);
            let gltf = crate::gltf::export_native_new(&at("a.gltf"), &scene, &instances, &|| true);
            let usd = crate::usd::export_native_new(&at("a.usda"), &scene, &instances, &|| true);
            let obj = obj::export_native_new(&at("a.obj"), &scene, &instances, &|| true);
            assert!(obj.is_ok(), "{primitive:?} {label}: OBJ always writes");
            let text = std::fs::read_to_string(at("a.obj")).unwrap();
            assert_eq!(
                text.contains("\nvt "),
                with_uvs,
                "{primitive:?} OBJ vt rows"
            );
            if with_uvs {
                assert!(
                    matches!(gltf, Err(GltfError::SessionUvs)),
                    "{primitive:?} glTF"
                );
                assert!(
                    matches!(usd, Err(UsdError::SessionUvs)),
                    "{primitive:?} USD"
                );
            } else {
                assert!(gltf.is_ok(), "{primitive:?} glTF {label}");
                assert!(usd.is_ok(), "{primitive:?} USD {label}");
            }
            assert!(
                emit_with_links(&scene, DefaultLinks::Omit).is_ok(),
                "ASCII admits"
            );
            // The binary writer exists on Unix only.
            #[cfg(unix)]
            {
                let binary = crate::native_scene::binary::encode_authored(&scene);
                if with_uvs {
                    assert_eq!(
                        crate::native_scene::binary::AuthoredRefusalCode::from(
                            &binary.unwrap_err()
                        ),
                        crate::native_scene::binary::AuthoredRefusalCode::SessionUvs,
                        "{primitive:?} .mb"
                    );
                } else {
                    assert!(binary.is_ok(), "{primitive:?} .mb {label}");
                }
            }
        }
    }
}
