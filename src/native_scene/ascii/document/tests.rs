use super::*;
use crate::native_scene::ascii::{decode_inner, write::emit_geometry_prefix};
use crate::{
    native_scene::{
        DirectedEdge, NativeEdge, NativeFace, NativeMesh, ascii::write::AsciiWriteRefusal,
    },
    primitive_options::SINGLE_QUAD_PLANE,
    scene::Primitive,
};

const CC: &str =
    "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 -0 0 1 2 3 4 -3 2 6 0 -1;";

fn cube() -> NativeScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    scene
}

fn bytes_of(scene: &NativeScene) -> Vec<u8> {
    emit_geometry_prefix(scene).unwrap()
}

pub(super) fn decode(bytes: &[u8], adoption: bool) -> Result<AsciiDecoded, AsciiDocumentError> {
    decode_document(Arc::from(bytes.to_vec()), adoption)
}

pub(super) fn authored(bytes: &[u8]) -> AuthoredProjection {
    match decode(bytes, true).unwrap() {
        AsciiDecoded::Authored(projection) => projection,
        other => panic!("not authored: {other:?}"),
    }
}

pub(super) fn refused(bytes: &[u8]) -> AsciiDocumentError {
    decode(bytes, true).err().unwrap_or_else(|| {
        panic!("accepted: {}", String::from_utf8_lossy(bytes));
    })
}

pub(super) fn replace(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1).into_bytes()
}

fn triangle_with_extreme_points() -> NativeMesh {
    let points = vec![
        [-0.0, f32::MAX, f32::from_bits(1)],
        [f32::MIN, 0.1, 1.0 / 3.0],
        [1e-30, -0.0, std::f32::consts::PI],
    ];
    let edges = [[0, 1], [1, 2], [0, 2]]
        .map(|endpoints| NativeEdge {
            endpoints,
            smooth: Some(false),
        })
        .to_vec();
    let face = NativeFace {
        outer: [(0, false), (1, false), (2, true)]
            .map(|(edge, reversed)| DirectedEdge { edge, reversed })
            .to_vec(),
        holes: Vec::new(),
    };
    NativeMesh::from_parts(points, edges, vec![face], Vec::new(), Some(false)).unwrap()
}

#[test]
fn emitted_scenes_decode_to_a_projection_that_re_emits_the_same_bytes() {
    let mut plane = NativeScene::default();
    plane.create(Primitive::Cube, "plane").unwrap();
    plane.shapes[0].mesh = NativeMesh::with_options(&SINGLE_QUAD_PLANE).unwrap();

    let mut many = NativeScene::default();
    for name in ["first", "second"] {
        many.create(Primitive::Cube, name).unwrap();
    }
    many.shapes[0].mesh = triangle_with_extreme_points();
    many.nodes[0].transform_uuid = Some(uuid::Uuid::from_u128(0x6c1b2d4e0a3f4b5c8d7e9f0a1b2c3d4e));
    many.shapes[1].uuid = Some(uuid::Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa));
    many.shapes[0].name = "storedFirst".into();
    let values = [
        -0.0,
        1e-9,
        1e15,
        0.1,
        1.0 / 3.0,
        std::f64::consts::PI,
        -2.0,
        1.5,
        3.0,
    ];
    for (channel, value) in many.nodes[1].channels.iter_mut().zip(values) {
        channel.value = value;
    }

    for scene in [cube(), plane, many] {
        let bytes = bytes_of(&scene);
        let projection = authored(&bytes);
        assert!(projection.scene().source.is_none());
        assert_eq!(bytes_of(projection.scene()), bytes);
        projection.scene().validate().unwrap();
        assert_eq!(projection.scene().nodes.len(), scene.nodes.len());
        for (got, want) in projection.scene().nodes.iter().zip(&scene.nodes) {
            let bits = |n: &crate::native_scene::NativeTransformNode| {
                n.channels
                    .iter()
                    .map(|c| c.value.to_bits())
                    .collect::<Vec<_>>()
            };
            assert_eq!((got.name(), bits(got)), (want.name(), bits(want)));
        }
        assert_eq!(
            projection.clone().into_scene().shapes.len(),
            scene.shapes.len()
        );
    }
}

#[test]
fn the_raw_decode_is_sourced_and_only_the_projection_may_be_emitted() {
    let bytes = bytes_of(&cube());
    let raw = decode_inner(Arc::from(bytes.clone()), true).unwrap();
    assert!(raw.source.is_some());
    assert_eq!(
        emit_geometry_prefix(&raw),
        Err(AsciiWriteRefusal::SourcedScene)
    );
}

#[test]
fn anything_the_writer_would_not_have_produced_is_refused() {
    let good = bytes_of(&cube());
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("extra fileInfo", [good.clone(), b"fileInfo \"x\" \"y\";\n".to_vec()].concat()),
        (
            "extra supported node",
            [good.clone(), b"createNode lightLinker -s -n \"lightLinker1\";\n".to_vec()].concat(),
        ),
        ("hand-edited float", replace(&good, "-0.5", "-0.50")),
        (
            "CRLF",
            String::from_utf8(good.clone()).unwrap().replace('\n', "\r\n").into_bytes(),
        ),
        ("missing final newline", good[..good.len() - 1].to_vec()),
        ("blank line", replace(&good, "createNode mesh", "\ncreateNode mesh")),
        ("other requires", replace(&good, "requires maya \"2026\"", "requires maya \"2027\"")),
        (
            "curve beside the mesh",
            [
                good.clone(),
                format!("createNode transform -n \"c\";\ncreateNode nurbsCurve -n \"cs\" -p \"c\";\n{CC}\n")
                    .into_bytes(),
            ]
            .concat(),
        ),
    ];
    // Each of these is accepted by the decoder, so the refusal below is the gate's.
    for (name, bytes) in cases {
        assert!(
            decode_inner(Arc::from(bytes.clone()), true).is_ok(),
            "{name}: the decoder must accept it for the gate to be what refuses it"
        );
        let error = refused(&bytes);
        assert!(
            matches!(error, AsciiDocumentError::Ascii(_)),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn what_the_decoder_itself_refuses_never_reaches_the_gate() {
    let good = bytes_of(&cube());
    let cases: Vec<(&str, Vec<u8>)> = vec![
        (
            "soft edge",
            replace(&good, " 0\n\t0 4 0\n", " 0\n\t0 4 1\n"),
        ),
        (
            "UV row",
            replace(&good, "\tf 4 0 1 2 3\n", "\tf 4 0 1 2 3 mu 0 4 0 1 2 3\n"),
        ),
        ("truncated", good[..good.len() / 2].to_vec()),
    ];
    for (name, bytes) in cases {
        assert!(
            decode_inner(Arc::from(bytes.clone()), true).is_err(),
            "{name}: the decoder must refuse it"
        );
        assert!(
            matches!(refused(&bytes), AsciiDocumentError::Ascii(_)),
            "{name}"
        );
    }
}

#[test]
fn a_mesh_free_ascii_scene_stays_retained_and_read_only() {
    let source = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; \
         createNode transform -n \"curve1\"; createNode nurbsCurve -n \"curveShape1\" -p \"curve1\"; {CC}"
    );
    for adoption in [true, false] {
        match decode(source.as_bytes(), adoption).unwrap() {
            AsciiDecoded::Retained(scene) => assert!(scene.source.is_some()),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn an_empty_scene_that_fails_the_gate_stays_retained_and_read_only() {
    let empty = bytes_of(&NativeScene::default());
    assert!(matches!(
        decode(&empty, true),
        Ok(AsciiDecoded::Authored(_))
    ));
    // No shapes and no nodes, but not the writer's bytes: still a valid read-only scene.
    let reformatted = replace(&empty, "requires maya \"2026\"", "requires maya \"2027\"");
    assert_ne!(reformatted, empty);
    for adoption in [true, false] {
        match decode(&reformatted, adoption).unwrap() {
            AsciiDecoded::Retained(scene) => {
                assert!(scene.shapes.is_empty() && scene.nodes.is_empty());
                assert!(scene.source.is_some());
                assert!(scene.authoring_read_only_reason().is_some());
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn adoption_is_a_caller_decision_and_never_masks_a_gate_refusal() {
    let good = bytes_of(&cube());
    assert_eq!(
        decode(&good, false).unwrap_err(),
        AsciiDocumentError::AdoptionUnsupported
    );
    assert!(matches!(decode(&good, true), Ok(AsciiDecoded::Authored(_))));
    let tampered = [good, b"fileInfo \"x\" \"y\";\n".to_vec()].concat();
    assert!(matches!(
        decode(&tampered, false).unwrap_err(),
        AsciiDocumentError::Ascii(_)
    ));
    assert_eq!(
        AsciiDocumentError::AdoptionUnsupported.to_string(),
        "Maya ASCII scenes with meshes can be opened only on macOS and Linux"
    );
}

#[test]
fn a_refusal_unrelated_to_meshes_stands_without_reaching_the_gate() {
    let other = "//Maya ASCII 2026 scene\nrequires maya \"2026\";\ncurrentUnit -l centimeter -a degree -t film;\ncreateNode transform -n \"a\";\ncreateNode locator -n \"l\" -p \"a\";\n";
    assert!(matches!(
        decode(other.as_bytes(), true).unwrap_err(),
        AsciiDocumentError::Ascii(_)
    ));
}

#[test]
fn a_file_reference_record_in_writer_output_is_never_adopted_as_authored() {
    let mut bytes = bytes_of(&cube());
    let at = bytes.windows(8).position(|w| w == b"requires").unwrap();
    bytes.splice(at..at, b"file -r \"a.ma\";\n".iter().copied());
    // Without the record the same bytes are authored, so only the record can refuse.
    assert!(matches!(
        decode(&bytes_of(&cube()), true),
        Ok(AsciiDecoded::Authored(_))
    ));
    assert!(decode(&bytes, true).is_err());
}
