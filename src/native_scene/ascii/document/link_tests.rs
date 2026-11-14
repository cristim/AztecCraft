//! Default shading-group link statements (`connectAttr "<mesh>.iog" ":initialShadingGroup.dsm"
//! -na`, #156). The reader counts them; the gate admits none (files saved before links existed)
//! or exactly one per shape, in the writer's order, and refuses everything else.
use super::tests::{authored, refused, replace};
use super::*;
use crate::{
    native_scene::ascii::write::{DefaultLinks, emit_geometry_prefix, emit_with_links},
    scene::Primitive,
};

fn two_cubes() -> (NativeScene, Vec<u8>) {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    scene.create(Primitive::Cube, "b").unwrap();
    let bytes = emit_with_links(&scene, DefaultLinks::Omit).unwrap();
    (scene, bytes)
}

fn link(shape: &str) -> String {
    format!("connectAttr \"{shape}.iog\" \":initialShadingGroup.dsm\" -na;\n")
}

fn with(bytes: &[u8], tail: &str) -> Vec<u8> {
    [bytes, tail.as_bytes()].concat()
}

fn read(bytes: &[u8]) -> Result<usize, AsciiError> {
    decode_with_uvs(Arc::from(bytes.to_vec()), true).map(|decoded| decoded.links)
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
fn the_gate_admits_the_legacy_layout_and_the_linked_layout_only() {
    let (scene, legacy) = two_cubes();
    let names: Vec<&str> = scene.shapes().iter().map(|s| s.name()).collect();
    let linked = emit_geometry_prefix(&scene).unwrap();
    assert_eq!(
        linked,
        with(&legacy, &format!("{}{}", link(names[0]), link(names[1])))
    );
    // A legacy file reopens Authored, keeps its layout under the gate and re-saves with links.
    let old = authored(&legacy);
    assert_eq!(
        emit_with_links(old.scene(), DefaultLinks::Omit).unwrap(),
        legacy
    );
    assert_eq!(emit_geometry_prefix(old.scene()).unwrap(), linked);
    let new = authored(&linked);
    assert_eq!(new.scene(), old.scene());
    assert_eq!(emit_geometry_prefix(new.scene()).unwrap(), linked);
    // Partial, duplicate, misordered or non-final links are no layout the writer emits.
    let partial = with(&legacy, &link(names[0]));
    let misordered = with(&legacy, &format!("{}{}", link(names[1]), link(names[0])));
    let duplicate = with(&legacy, &format!("{}{}", link(names[0]), link(names[0])));
    let interleaved = replace(&linked, &link(names[0]), "");
    for (name, bytes) in [
        ("partial", partial),
        ("misordered", misordered),
        ("duplicate", duplicate),
        ("interleaved", with(&interleaved, &link(names[0]))),
    ] {
        let refused = refused(&bytes);
        assert!(
            matches!(
                refused,
                AsciiDocumentError::Ascii(AsciiError {
                    kind: ErrorKind::Unsupported,
                    ..
                })
            ),
            "{name}: {refused:?}"
        );
    }
    // Adoption stays a caller decision for both layouts.
    for bytes in [&legacy, &linked] {
        assert_eq!(
            super::tests::decode(bytes, false).unwrap_err(),
            AsciiDocumentError::AdoptionUnsupported
        );
    }
}

#[test]
fn the_writer_refuses_names_that_would_mislink_and_instanced_shapes() {
    use crate::native_scene::ascii::write::AsciiWriteRefusal as R;
    let (scene, _) = two_cubes();
    type Case = (&'static str, fn(&mut NativeScene), R);
    let cases: [Case; 4] = [
        (
            "shape named like a shape",
            |s| s.shapes[1].name = s.shapes[0].name.clone(),
            R::NameCollision,
        ),
        (
            "shape named like a transform",
            |s| s.shapes[1].name = s.nodes[0].name.clone(),
            R::NameCollision,
        ),
        (
            "transform named like a shape",
            |s| s.nodes[1].name = s.shapes[0].name.clone(),
            R::NameCollision,
        ),
        (
            "instanced shape",
            |s| {
                let child = s.dag.iter().position(|e| e.parent.is_some()).unwrap();
                s.dag[child].parent = Some(s.nodes[1].transform_id);
            },
            R::NonAuthoredHierarchy,
        ),
    ];
    for (name, mutate, expected) in cases {
        let mut broken = scene.clone();
        mutate(&mut broken);
        for links in [DefaultLinks::Emit, DefaultLinks::Omit] {
            assert_eq!(
                emit_with_links(&broken, links),
                Err(expected.clone()),
                "{name}"
            );
        }
    }
}

fn next(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

#[test]
fn generated_scenes_round_trip_in_both_layouts() {
    let primitives = [Primitive::Cube, Primitive::Plane, Primitive::Sphere];
    let mut state = 156_156u64;
    for round in 0..40 {
        let mut scene = NativeScene::default();
        for index in 0..(next(&mut state) % 7) {
            let primitive = primitives[(next(&mut state) % 3) as usize];
            scene
                .create(primitive, &format!("m{round}_{index}"))
                .unwrap();
        }
        for links in [DefaultLinks::Emit, DefaultLinks::Omit] {
            let bytes = emit_with_links(&scene, links).unwrap();
            let count = bytes
                .windows(".iog\"".len())
                .filter(|w| w == b".iog\"")
                .count();
            assert_eq!(
                count,
                if links == DefaultLinks::Emit {
                    scene.shapes().len()
                } else {
                    0
                }
            );
            let projection = authored(&bytes);
            assert_eq!(projection.scene().shapes(), scene.shapes(), "round {round}");
            assert_eq!(projection.scene().nodes(), scene.nodes(), "round {round}");
            assert_eq!(emit_with_links(projection.scene(), links).unwrap(), bytes);
        }
    }
}
