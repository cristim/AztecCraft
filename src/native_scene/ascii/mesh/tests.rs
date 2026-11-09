use super::super::{ErrorKind, decode_admitting_meshes, write::emit_geometry_prefix};
use crate::{
    native_scene::{NativeScene, Points},
    scene::Primitive,
};
use std::sync::Arc;

const HEADER: &str = "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"a\"; createNode mesh -n \"aShape\" -p \"a\"; ";

fn decode(body: &str) -> Result<NativeScene, super::AsciiError> {
    decode_admitting_meshes(Arc::from(format!("{HEADER}{body}").into_bytes()))
}

fn refusal(body: &str, kind: ErrorKind) {
    let error = decode(body)
        .err()
        .unwrap_or_else(|| panic!("accepted: {body}"));
    assert_eq!(error.kind(), kind, "{body}");
    let start = HEADER.len();
    let offset = error
        .offset()
        .unwrap_or_else(|| panic!("no offset: {body}"));
    assert!(
        (start..=start + body.len()).contains(&offset),
        "offset {offset} outside the statement: {body}"
    );
}

/// The authored writer's whole output for a one-cube scene.
fn cube() -> (NativeScene, Vec<u8>) {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    (scene, bytes)
}

#[test]
fn the_writers_points_edges_and_faces_read_back_bit_for_bit() {
    let (original, bytes) = cube();
    let scene = decode_admitting_meshes(Arc::from(bytes)).unwrap();
    let (want, got) = (original.shapes()[0].mesh(), scene.shapes()[0].mesh());
    let (Points::Float3(want_points), Points::Float3(got_points)) = (want.points(), got.points())
    else {
        panic!("float points expected");
    };
    assert_eq!((want_points.len(), got_points.len()), (8, 8));
    for (a, b) in want_points
        .iter()
        .flatten()
        .zip(got_points.iter().flatten())
    {
        assert_eq!(a.to_bits(), b.to_bits());
    }
    assert_eq!((want.edges().len(), got.edges().len()), (12, 12));
    for (a, b) in want.edges().iter().zip(got.edges()) {
        assert_eq!((a.endpoints(), a.smooth()), (b.endpoints(), b.smooth()));
    }
    assert_eq!((want.faces().len(), got.faces().len()), (6, 6));
    for (a, b) in want.faces().iter().zip(got.faces()) {
        assert_eq!(a.outer(), b.outer());
        assert!(b.holes().is_empty());
    }
    // The cube has reversed edges, which are stored as negative tokens.
    assert!(
        got.faces()
            .iter()
            .flat_map(|f| f.outer())
            .any(|e| e.reversed)
    );
}

#[test]
fn point_values_keep_signed_zero_subnormals_and_the_f32_extremes() {
    let scene =
        decode("setAttr -s 2 \".vt[0:1]\" -0 1e-45 3.4028235e38 0.1 -3.4028235e38 1;").unwrap();
    let Points::Float3(points) = scene.shapes()[0].mesh().points() else {
        panic!("float points expected");
    };
    let bits: Vec<u32> = points.iter().flatten().map(|v| v.to_bits()).collect();
    assert_eq!(
        bits,
        [-0.0f32, 1e-45, f32::MAX, 0.1, f32::MIN, 1.0].map(f32::to_bits)
    );
}

#[test]
fn malformed_or_oversized_array_claims_allocate_nothing_and_name_the_statement() {
    use ErrorKind::{Malformed as M, Unsupported as U};
    for (body, kind) in [
        // a count far beyond the tokens present, a count that disagrees with the range
        ("setAttr -s 2147483647 \".vt[0:2147483646]\" 0 0 0;", M),
        ("setAttr -s 2 \".vt[0:2]\" 0 0 0 0 0 0;", U),
        ("setAttr -s 2 \".vt[0:0]\" 0 0 0;", U),
        ("setAttr -s 1 \".vt[0:1]\" 0 0 0 1 0 0;", U),
        // no size, no range, a split range, a leading zero, an empty array, a wrong name
        ("setAttr \".vt[0:0]\" 0 0 0;", U),
        ("setAttr -s 1 \".vt\" 0 0 0;", U),
        ("setAttr -s 2 \".vt[1:2]\" 0 0 0 0 0 0;", U),
        ("setAttr -s 1 \".vt[0:00]\" 0 0 0;", U),
        ("setAttr -s 0 \".vt[0:-1]\";", U),
        ("setAttr -s 1 \".pt[0:0]\" 0 0 0;", U),
        // missing, extra, non-finite and non-numeric values, typed and keyable forms
        ("setAttr -s 1 \".vt[0:0]\" 0 0;", M),
        ("setAttr -s 1 \".vt[0:0]\" 0 0 0 0;", M),
        ("setAttr -s 1 \".vt[0:0]\" 0 0 1e40;", M),
        ("setAttr -s 1 \".vt[0:0]\" 0 0 nan;", M),
        ("setAttr -s 1 \".vt[0:0]\" -type \"float3\" 0 0 0;", U),
        ("setAttr -k off -s 1 \".vt[0:0]\" 0 0 0;", U),
        // a duplicate array, edge values beyond hard, negative endpoints, a missing edge value
        (
            "setAttr -s 1 \".vt[0:0]\" 0 0 0; setAttr -s 1 \".vt[0:0]\" 0 0 0;",
            U,
        ),
        (
            "setAttr -s 1 \".ed[0:0]\" 0 0 0; setAttr -s 1 \".ed[0:0]\" 0 0 0;",
            U,
        ),
        (
            "setAttr -s 2 \".vt[0:1]\" 0 0 0 1 0 0; setAttr -s 1 \".ed[0:0]\" 0 1 1;",
            U,
        ),
        (
            "setAttr -s 2 \".vt[0:1]\" 0 0 0 1 0 0; setAttr -s 1 \".ed[0:0]\" -1 1 0;",
            M,
        ),
        ("setAttr -s 1 \".ed[0:0]\" 0 1;", M),
    ] {
        refusal(body, kind);
    }
}

#[test]
fn an_edge_past_the_point_count_is_a_topology_error_not_a_panic() {
    let error = decode("setAttr -s 1 \".vt[0:0]\" 0 0 0; setAttr -s 1 \".ed[0:0]\" 0 5 0;")
        .err()
        .unwrap();
    // The failure belongs to no single statement, so no offset is reported.
    assert_eq!((error.kind(), error.offset()), (ErrorKind::Malformed, None));
}

#[test]
fn every_truncation_of_the_cube_output_returns_without_panicking() {
    let (_, bytes) = cube();
    for end in 0..=bytes.len() {
        let _ = decode_admitting_meshes(Arc::from(bytes[..end].to_vec()));
    }
}

#[test]
fn public_decode_stays_closed_to_the_cube() {
    let (_, bytes) = cube();
    assert_eq!(
        super::super::decode(Arc::from(bytes)).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
}

#[test]
fn points_parse_straight_to_f32_without_double_rounding() {
    // Just above the midpoint of 1.0 and its f32 successor: parsed as f64 first, it rounds to the
    // midpoint and then ties to 1.0; parsed directly, it correctly rounds up.
    let scene = decode("setAttr -s 1 \".vt[0:0]\" 1.00000005960464477539062501 0 0;").unwrap();
    let Points::Float3(points) = scene.shapes()[0].mesh().points() else {
        panic!("float points expected");
    };
    assert_eq!(points[0][0].to_bits(), 0x3f80_0001);
}

const TRIANGLE: &str =
    "setAttr -s 3 \".vt[0:2]\" 0 0 0 1 0 0 0 1 0; setAttr -s 3 \".ed[0:2]\" 0 1 0 1 2 0 2 0 0; ";

fn faces(statement: &str) -> Result<NativeScene, super::AsciiError> {
    decode(&format!("{TRIANGLE}{statement}"))
}

#[test]
fn a_face_decodes_forward_and_reversed_edge_tokens_in_i64() {
    let scene = faces("setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" f 3 0 1 2;").unwrap();
    let outer: Vec<_> = scene.shapes()[0].mesh().faces()[0]
        .outer()
        .iter()
        .map(|e| (e.edge, e.reversed))
        .collect();
    assert_eq!(outer, [(0, false), (1, false), (2, false)]);
    // -1 is edge 0 reversed, -3 is edge 2 reversed: a reversed loop over the same edges.
    let scene = faces("setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" f 3 -3 -2 -1;").unwrap();
    let outer: Vec<_> = scene.shapes()[0].mesh().faces()[0]
        .outer()
        .iter()
        .map(|e| (e.edge, e.reversed))
        .collect();
    assert_eq!(outer, [(2, true), (1, true), (0, true)]);
}

#[test]
fn face_rows_refuse_bad_counts_tokens_and_unsupported_keywords() {
    use ErrorKind::{Malformed as M, Unsupported as U};
    let fc = |rest: &str| format!("setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" {rest};");
    for (body, kind) in [
        // fewer than three edges, a count that is not the token count, a missing row, extras
        (faces(&fc("f 2 0 1")), M),
        (faces(&fc("f 3 0 1")), M),
        (faces(&fc("f 3 0 1 2 2")), U),
        (faces(&fc("f 0")), M),
        // a short or oversized face inside a statement that has enough tokens overall
        (
            faces("setAttr -s 2 \".fc[0:1]\" -type \"polyFaces\" f 2 0 1 f 4 0 1 2 0;"),
            M,
        ),
        (faces(&fc("f 2147483647 0 1 2")), M),
        (faces(&fc("")), M),
        // an edge past the array, a token that cannot be negated in i32, a non-integer
        (faces(&fc("f 3 0 1 3")), M),
        (faces(&fc("f 3 0 1 -4")), M),
        (faces(&fc("f 3 0 1 -2147483648")), M),
        (faces(&fc("f 3 0 1 2147483647")), M),
        (faces(&fc("f 3 0 1 1.5")), M),
        // hole, UV and colour keywords, and a row that does not start with f
        (faces(&fc("f 3 0 1 2 h 3 0 1 2")), U),
        (faces(&fc("f 3 0 1 2 mu 0 3 0 1 2")), U),
        (faces(&fc("f 3 0 1 2 mc 0 3 0 1 2")), U),
        (faces(&fc("mf 3 0 1 2")), U),
        // no type, a different type, a wrong count or range, a duplicate, faces before edges
        (faces("setAttr -s 1 \".fc[0:0]\" f 3 0 1 2;"), U),
        (
            faces("setAttr -s 1 \".fc[0:0]\" -type \"long3\" f 3 0 1 2;"),
            U,
        ),
        (
            faces("setAttr -s 2 \".fc[0:0]\" -type \"polyFaces\" f 3 0 1 2;"),
            U,
        ),
        (
            faces("setAttr -s 2 \".fc[0:1]\" -type \"polyFaces\" f 3 0 1 2;"),
            M,
        ),
        (
            faces(&format!("{} {}", fc("f 3 0 1 2"), fc("f 3 0 1 2"))),
            U,
        ),
        (
            decode("setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" f 3 0 1 2;"),
            U,
        ),
    ] {
        let error = body.err().unwrap_or_else(|| panic!("accepted a bad face"));
        assert_eq!(error.kind(), kind, "{error:?}");
        assert!(error.offset().is_some(), "{error:?}");
    }
}

#[test]
fn an_open_face_loop_is_a_topology_error_without_an_offset() {
    let error = faces("setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" f 3 0 2 1;").unwrap_err();
    assert_eq!((error.kind(), error.offset()), (ErrorKind::Malformed, None));
}

#[test]
fn a_huge_face_count_claim_allocates_nothing() {
    let error = faces("setAttr -s 2147483647 \".fc[0:2147483646]\" -type \"polyFaces\" f 3 0 1 2;")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Malformed);
}
