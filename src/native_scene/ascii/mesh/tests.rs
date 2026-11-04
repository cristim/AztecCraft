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

/// The writer's cube output up to, and without, the face statement this task does not read.
fn cube_without_faces() -> (NativeScene, Vec<u8>) {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    let cut = bytes
        .windows(b"\tsetAttr -s 6 \".fc".len())
        .position(|w| w == b"\tsetAttr -s 6 \".fc")
        .unwrap();
    (scene, bytes[..cut].to_vec())
}

#[test]
fn the_writers_points_and_edges_read_back_bit_for_bit() {
    let (original, bytes) = cube_without_faces();
    let scene = decode_admitting_meshes(Arc::from(bytes)).unwrap();
    let (want, got) = (original.shapes()[0].mesh(), scene.shapes()[0].mesh());
    let (Points::Float3(want_points), Points::Float3(got_points)) = (want.points(), got.points())
    else {
        panic!("float points expected");
    };
    assert_eq!(want_points.len(), 8);
    for (a, b) in want_points
        .iter()
        .flatten()
        .zip(got_points.iter().flatten())
    {
        assert_eq!(a.to_bits(), b.to_bits());
    }
    assert_eq!(want.edges().len(), 12);
    for (a, b) in want.edges().iter().zip(got.edges()) {
        assert_eq!((a.endpoints(), a.smooth()), (b.endpoints(), b.smooth()));
    }
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
    let (_, bytes) = cube_without_faces();
    for end in 0..=bytes.len() {
        let _ = decode_admitting_meshes(Arc::from(bytes[..end].to_vec()));
    }
}

#[test]
fn public_decode_stays_closed_to_the_cube_arrays() {
    let (_, bytes) = cube_without_faces();
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
