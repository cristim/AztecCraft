//! Default shading-group link statements (`connectAttr "<mesh>.iog" ":initialShadingGroup.dsm"
//! -na`, #156 S2a). The reader recognises and counts them, but the writer emits none yet, so the
//! read-back gate still refuses every file that carries one.
use super::tests::{authored, refused};
use super::*;
use crate::{native_scene::ascii::write::emit_geometry_prefix, scene::Primitive};

fn two_cubes() -> (NativeScene, Vec<u8>) {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    scene.create(Primitive::Cube, "b").unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    (scene, bytes)
}

fn link(shape: &str) -> String {
    format!("connectAttr \"{shape}.iog\" \":initialShadingGroup.dsm\" -na;\n")
}

fn with(bytes: &[u8], tail: &str) -> Vec<u8> {
    [bytes, tail.as_bytes()].concat()
}

fn read(bytes: &[u8]) -> Result<usize, AsciiError> {
    decode_with_uvs(Arc::from(bytes.to_vec()), true).map(|(_, _, links)| links)
}

fn unsupported_kind(result: Result<usize, AsciiError>) -> bool {
    matches!(
        result,
        Err(AsciiError {
            kind: ErrorKind::Unsupported,
            ..
        })
    )
}

#[test]
fn links_are_recognised_and_counted() {
    let (scene, bytes) = two_cubes();
    let names: Vec<&str> = scene.shapes().iter().map(|s| s.name()).collect();
    assert_eq!(read(&bytes), Ok(0), "a legacy file has no links");
    assert_eq!(read(&with(&bytes, &link(names[0]))), Ok(1));
    let both = format!("{}{}", link(names[1]), link(names[0]));
    assert_eq!(
        read(&with(&bytes, &both)),
        Ok(2),
        "order is the gate's business"
    );
}

#[test]
fn only_the_exact_statement_on_a_mesh_shape_once_is_read() {
    let (scene, bytes) = two_cubes();
    let shape = scene.shapes()[0].name();
    let transform = scene.nodes()[0].name();
    let ok = link(shape);
    let cases = [
        ("duplicate", format!("{ok}{ok}")),
        ("unknown node", link("nope")),
        ("transform, not a shape", link(transform)),
        ("indexed iog", ok.replace(".iog", ".iog[0]")),
        (
            "other group",
            ok.replace(":initialShadingGroup", ":lambert1SG"),
        ),
        ("other attr", ok.replace(".dsm", ".dsm[0]")),
        ("missing -na", ok.replace(" -na", "")),
        ("misspelt flag", ok.replace("-na", "-nx")),
        ("extra token", ok.replace(";\n", " -f;\n")),
        (
            "reversed",
            format!("connectAttr \":initialShadingGroup.dsm\" \"{shape}.iog\" -na;\n"),
        ),
        ("quoted flag", ok.replace("-na", "\"-na\"")),
    ];
    for (name, tail) in cases {
        let result = read(&with(&bytes, &tail));
        assert!(result.is_err(), "{name}: {result:?}");
        // Unsupported when well-formed but not the link; malformed when the tokens are not.
        let kind = result.map(|_| ()).unwrap_err().kind;
        assert!(
            matches!(kind, ErrorKind::Unsupported | ErrorKind::Malformed),
            "{name}"
        );
    }
    assert!(
        read(&with(&bytes, "connectAttr \"a.iog\"")).is_err(),
        "truncated"
    );
}

#[test]
fn a_link_before_its_node_or_in_a_mesh_free_decode_is_not_read() {
    let (scene, bytes) = two_cubes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let early = text.replacen(
        "createNode transform",
        &format!("{}createNode transform", link(scene.shapes()[0].name())),
        1,
    );
    assert!(
        unsupported_kind(read(early.as_bytes())),
        "node not yet created"
    );
    let plain = decode_inner_plain(&with(&bytes, &link(scene.shapes()[0].name())));
    assert!(plain.is_err(), "meshes stay inadmissible without the gate");
}

fn decode_inner_plain(bytes: &[u8]) -> Result<NativeScene, AsciiError> {
    crate::native_scene::ascii::decode(Arc::from(bytes.to_vec()))
}

#[test]
fn the_gate_still_behaves_as_before() {
    let (scene, bytes) = two_cubes();
    // A legacy file reopens Authored and re-emits its exact bytes.
    let projection = authored(&bytes);
    assert_eq!(emit_geometry_prefix(projection.scene()).unwrap(), bytes);
    assert_eq!(projection.scene(), authored(&bytes).scene());
    // Every file with links is refused as Unsupported: the writer would not reproduce it.
    let names: Vec<&str> = scene.shapes().iter().map(|s| s.name()).collect();
    for tail in [
        link(names[0]),
        format!("{}{}", link(names[0]), link(names[1])),
    ] {
        let error = refused(&with(&bytes, &tail));
        assert!(
            matches!(
                error,
                AsciiDocumentError::Ascii(AsciiError {
                    kind: ErrorKind::Unsupported,
                    ..
                })
            ),
            "{error:?}"
        );
        // Not adoptable on a platform that cannot adopt either way.
        assert!(super::tests::decode(&with(&bytes, &tail), false).is_err());
    }
}
