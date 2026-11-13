//! The reader's UV block (PROVISIONAL layout, `.project-docs/uv-ascii-format.md`): what the
//! writer emits parses into a per-shape side table, and every hand edit of it is refused.
use super::super::{ErrorKind, decode_with_uvs, write::emit_geometry_prefix};
use crate::{
    native_scene::{NativeMesh, NativeScene, UvProjectAxis, UvSet},
    primitive_options::SINGLE_QUAD_PLANE,
    scene::Primitive,
};
use std::sync::Arc;

fn scene_with_uvs(plane: bool) -> NativeScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    if plane {
        scene.shapes[0].mesh = NativeMesh::with_options(&SINGLE_QUAD_PLANE).unwrap();
    }
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene
}

fn plane_text() -> String {
    String::from_utf8(emit_geometry_prefix(&scene_with_uvs(true)).unwrap()).unwrap()
}

fn read(text: &str) -> Result<Vec<Option<UvSet>>, super::super::AsciiError> {
    decode_with_uvs(Arc::from(text.as_bytes().to_vec()), true).map(|(_, uvs)| uvs)
}

#[track_caller]
fn refused(text: &str, kind: ErrorKind) {
    let error = read(text)
        .err()
        .unwrap_or_else(|| panic!("accepted:\n{text}"));
    assert_eq!(error.kind(), kind, "{text}");
}

#[test]
fn the_writers_uv_block_reads_back_into_the_side_table() {
    for plane in [true, false] {
        let scene = scene_with_uvs(plane);
        let bytes = emit_geometry_prefix(&scene).unwrap();
        let (decoded, uvs) = decode_with_uvs(Arc::from(bytes), true).unwrap();
        assert!(decoded.has_source(), "the sourced scene holds no UVs");
        assert!(decoded.uv_set(decoded.shapes()[0].id()).is_none());
        let want = scene.uv_set(scene.shapes()[0].id()).unwrap();
        assert_eq!(uvs, [Some(want.clone())]);
    }
}

#[test]
fn faces_without_mu_give_no_uv_set() {
    let mut scene = scene_with_uvs(false);
    scene.delete_uvs(scene.shapes()[0].id()).unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    assert_eq!(decode_with_uvs(Arc::from(bytes), true).unwrap().1, [None]);
}

#[test]
fn a_mu_block_that_is_not_the_single_running_set_is_refused() {
    let text = plane_text();
    for (from, to, kind) in [
        ("mu 0 4 0 1 2 3", "mu 1 4 0 1 2 3", ErrorKind::Unsupported),
        ("mu 0 4 0 1 2 3", "mu 0 4 1 0 2 3", ErrorKind::Unsupported),
        ("mu 0 4 0 1 2 3", "mu 0 4 0 1 2 4", ErrorKind::Unsupported),
        ("mu 0 4 0 1 2 3", "mu 0 5 0 1 2 3 4", ErrorKind::Malformed),
        ("mu 0 4 0 1 2 3", "mu 0 3 0 1 2", ErrorKind::Malformed),
        ("mu 0 4 0 1 2 3", "mu 0 4 0 1 2", ErrorKind::Malformed),
        (
            "mu 0 4 0 1 2 3",
            "mu 0 4 0 1 2 3 mu 0 4 4 5 6 7",
            ErrorKind::Unsupported,
        ),
        ("mu 0 4 0 1 2 3", "mf 4 0 1 2 3", ErrorKind::Unsupported),
        ("mu 0 4 0 1 2 3", "mu 0 4 0 1 2 -1", ErrorKind::Malformed),
    ] {
        assert!(text.contains(from));
        refused(&text.replacen(from, to, 1), kind);
    }
}

#[test]
fn mu_on_only_some_faces_is_refused() {
    let text = String::from_utf8(emit_geometry_prefix(&scene_with_uvs(false)).unwrap()).unwrap();
    let first = text.find(" mu 0 4 0 1 2 3").unwrap();
    let mut edited = text.clone();
    edited.replace_range(first..first + " mu 0 4 0 1 2 3".len(), "");
    refused(&edited, ErrorKind::Malformed);
}

#[test]
fn an_incomplete_or_altered_uv_statement_set_is_refused() {
    let text = plane_text();
    let uvsn = "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n";
    let cuvs = "\tsetAttr \".cuvs\" -type \"string\" \"map1\";\n";
    let uvsp_rows = "\t0 1\n\t0 0\n\t1 0\n\t1 1\n";
    for (from, to, kind) in [
        (uvsn, "", ErrorKind::Malformed),
        (cuvs, "", ErrorKind::Malformed),
        (uvsn, &uvsn.replace("map1", "map2"), ErrorKind::Unsupported),
        (cuvs, &cuvs.replace("map1", "map2"), ErrorKind::Unsupported),
        (
            uvsn,
            &uvsn.replace(" -type \"string\"", ""),
            ErrorKind::Unsupported,
        ),
        (uvsn, &format!("{uvsn}{uvsn}"), ErrorKind::Unsupported),
        (cuvs, &format!("{cuvs}{cuvs}"), ErrorKind::Unsupported),
        (
            uvsn,
            &uvsn.replace("setAttr ", "setAttr -s 1 "),
            ErrorKind::Unsupported,
        ),
        (uvsp_rows, "\t0 1\n\t0 0\n\t1 0\n", ErrorKind::Malformed),
        (
            uvsp_rows,
            &format!("{uvsp_rows}\t1 1\n"),
            ErrorKind::Malformed,
        ),
        (
            uvsp_rows,
            "\t0 1\n\t0 0\n\t1 0\n\t1\n",
            ErrorKind::Malformed,
        ),
        (
            uvsp_rows,
            "\t0 1\n\t0 0\n\t1 0\n\t1 nan\n",
            ErrorKind::Malformed,
        ),
        (
            uvsp_rows,
            "\t0 1\n\t0 0\n\t1 0\n\t1 1e39\n",
            ErrorKind::Malformed,
        ),
    ] {
        assert!(text.contains(from), "{from}");
        refused(&text.replacen(from, to, 1), kind);
    }
}

#[test]
fn uvsp_counts_must_match_the_ids_and_its_own_claim() {
    let text = plane_text();
    let head = "-s 4 \".uvst[0].uvsp[0:3]\"";
    for (to, kind) in [
        ("-s 4 \".uvst[0].uvsp[0:2]\"", ErrorKind::Unsupported),
        ("-s 3 \".uvst[0].uvsp[0:2]\"", ErrorKind::Malformed),
        ("-s 5 \".uvst[0].uvsp[0:4]\"", ErrorKind::Malformed),
        ("-s 0 \".uvst[0].uvsp[0:-1]\"", ErrorKind::Unsupported),
        (
            "-s 2147483647 \".uvst[0].uvsp[0:2147483646]\"",
            ErrorKind::Malformed,
        ),
        ("-s 4 \".uvst[1].uvsp[0:3]\"", ErrorKind::Unsupported),
    ] {
        assert!(text.contains(head));
        refused(&text.replacen(head, to, 1), kind);
    }
}

#[test]
fn uv_statements_need_faces_with_mu_first() {
    let text = plane_text();
    let start = text.find("\tsetAttr \".uvst[0].uvsn\"").unwrap();
    let (before, block) = text.split_at(start);
    let fc = before.find("\tsetAttr -s 1 \".fc").unwrap();
    // The block ahead of the faces, the block without mu rows, and a second block.
    refused(
        &format!("{}{block}{}", &before[..fc], &before[fc..]),
        ErrorKind::Unsupported,
    );
    refused(
        &(before.replace(" mu 0 4 0 1 2 3", "") + block),
        ErrorKind::Unsupported,
    );
    refused(&format!("{text}{block}"), ErrorKind::Unsupported);
}

#[test]
fn the_uv_reader_never_panics_on_a_truncated_block() {
    let bytes = emit_geometry_prefix(&scene_with_uvs(false)).unwrap();
    for end in (0..bytes.len()).step_by(7) {
        let _ = decode_with_uvs(Arc::from(bytes[..end].to_vec()), true);
    }
}

#[test]
fn a_set_name_without_mu_rows_is_refused_not_ignored() {
    let mut scene = scene_with_uvs(true);
    scene.delete_uvs(scene.shapes()[0].id()).unwrap();
    let plain = String::from_utf8(emit_geometry_prefix(&scene).unwrap()).unwrap();
    for statement in [
        "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n",
        "\tsetAttr \".cuvs\" -type \"string\" \"map1\";\n",
    ] {
        refused(&format!("{plain}{statement}"), ErrorKind::Unsupported);
    }
}

#[test]
fn a_consistent_uvsp_count_that_misses_the_ids_names_its_statement() {
    let text = plane_text()
        .replace("-s 4 \".uvst[0].uvsp[0:3]\"", "-s 3 \".uvst[0].uvsp[0:2]\"")
        .replace("\t1 1\n;", ";");
    let error = read(&text).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Malformed);
    assert!(error.offset().is_some(), "{error:?}");
}
