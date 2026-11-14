//! The UV block of the ASCII writer: bytes, strict oracle, byte budgets. The text layout is
//! PROVISIONAL (`.project-docs/uv-ascii-format.md`).

use super::*;
use crate::native_scene::UvProjectAxis;

const PLANE_UV_TAIL: &[u8] = b"\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"\n\
\tf 4 0 1 2 3 mu 0 4 0 1 2 3\n\
;\n\
\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n\
\tsetAttr -s 4 \".uvst[0].uvsp[0:3]\" -type \"float2\"\n\
\t0 1\n\
\t0 0\n\
\t1 0\n\
\t1 1\n\
;\n\
\tsetAttr \".cuvs\" -type \"string\" \"map1\";\n";

fn plane_scene() -> NativeScene {
    let mut scene = scene(&["plane"]);
    scene.shapes[0].mesh = NativeMesh::with_options(&SINGLE_QUAD_PLANE).unwrap();
    scene
}
fn project(scene: &mut NativeScene) {
    for index in 0..scene.shapes.len() {
        let shape = scene.shapes()[index].id();
        scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    }
}

/// One shape's UV block as the strict oracle reads it: each face's `mu` ids and the `uvsp` values.
struct UvBlock {
    mu: Vec<Vec<usize>>,
    uvsp: Vec<[f32; 2]>,
}

/// Strictly reads the single-shape UV tail: panics on anything but the exact statements.
fn parse_uv_block(bytes: &[u8]) -> UvBlock {
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(!text.contains('\r') && text.ends_with('\n'));
    let (_, fc) = text.split_once("\".fc[0:").unwrap();
    let mut lines = fc.split_terminator('\n');
    let face_count = {
        let head = lines.next().unwrap();
        let (last, rest) = head.split_once("]\" -type \"polyFaces\"").unwrap();
        assert_eq!(rest, "");
        last.parse::<usize>().unwrap() + 1
    };
    let mut mu = Vec::new();
    for _ in 0..face_count {
        let row = lines.next().unwrap().strip_prefix('\t').unwrap();
        let tokens = row.split(' ').collect::<Vec<_>>();
        assert_eq!(tokens[0], "f");
        let corners = tokens[1].parse::<usize>().unwrap();
        assert_eq!(tokens[2 + corners], "mu");
        assert_eq!(tokens[3 + corners], "0", "only set 0 exists");
        assert_eq!(tokens[4 + corners].parse::<usize>().unwrap(), corners);
        assert_eq!(tokens.len(), 5 + 2 * corners);
        mu.push(
            tokens[5 + corners..]
                .iter()
                .map(|id| id.parse::<usize>().unwrap())
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(lines.next(), Some(";"));
    assert_eq!(
        lines.next(),
        Some(UVSN_STATEMENT.strip_suffix('\n').unwrap())
    );
    let head = lines.next().unwrap();
    let (count, rest) = head
        .strip_prefix("\tsetAttr -s ")
        .unwrap()
        .split_once(" \".uvst[0].uvsp[0:")
        .unwrap();
    let (last, rest) = rest.split_once("]\"").unwrap();
    assert_eq!(rest, " -type \"float2\"");
    let count = count.parse::<usize>().unwrap();
    assert_eq!(last.parse::<usize>().unwrap() + 1, count);
    let mut uvsp = Vec::new();
    for _ in 0..count {
        let row = lines.next().unwrap().strip_prefix('\t').unwrap();
        let (u, v) = row.split_once(' ').unwrap();
        let (u, v) = (u.parse::<f32>().unwrap(), v.parse::<f32>().unwrap());
        assert!(u.is_finite() && v.is_finite());
        uvsp.push([u, v]);
    }
    assert_eq!(lines.next(), Some(";"));
    assert_eq!(
        lines.next(),
        Some(CUVS_STATEMENT.strip_suffix('\n').unwrap())
    );
    assert_eq!(lines.next(), None, "exactly one UV block");
    let running = mu.iter().flatten().copied().collect::<Vec<_>>();
    assert_eq!(running, (0..count).collect::<Vec<_>>(), "running ids");
    UvBlock { mu, uvsp }
}

#[test]
fn plane_with_uvs_emits_the_exact_provisional_block() {
    let mut scene = plane_scene();
    project(&mut scene);
    let bytes = prefix(&scene).unwrap();
    assert!(
        bytes.ends_with(PLANE_UV_TAIL),
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let block = parse_uv_block(&bytes[bytes.len() - PLANE_UV_TAIL.len() - 1..]);
    assert_eq!(block.mu, [[0, 1, 2, 3]]);
    assert_eq!(block.uvsp, [[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
}

#[test]
fn cube_values_ids_and_length_match_the_stored_set() {
    let mut scene = scene(&["cube"]);
    project(&mut scene);
    let bytes = prefix(&scene).unwrap();
    assert_eq!(
        prefix_bytes(&scene, DefaultLinks::Omit).unwrap(),
        bytes.len()
    );
    let text = std::str::from_utf8(&bytes).unwrap();
    let start = text.find("\tsetAttr -s 6 \".fc").unwrap();
    let block = parse_uv_block(&bytes[start..]);
    let set = scene.uv_set(scene.shapes()[0].id()).unwrap();
    assert_eq!(block.uvsp.len(), set.corner_count());
    let stored = set.faces().iter().flatten().collect::<Vec<_>>();
    for (written, stored) in block.uvsp.iter().zip(stored) {
        assert_eq!(written.map(f64::from), *stored, "f32 text round-trips");
    }
    for (face, ids) in set.faces().iter().zip(&block.mu) {
        assert_eq!(face.len(), ids.len());
    }
}

#[test]
fn shapes_without_uvs_keep_todays_bytes() {
    let mut scene = scene(&["a", "b"]);
    let plain = prefix(&scene).unwrap();
    assert_eq!(prefix(&scene).unwrap(), plain);
    let shape = scene.shapes()[1].id();
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let mixed = prefix(&scene).unwrap();
    let text = std::str::from_utf8(&mixed).unwrap();
    assert_eq!(text.matches(".uvst[0].uvsn").count(), 1);
    assert_eq!(text.matches(" mu 0 4 ").count(), 6);
    let first = std::str::from_utf8(&plain).unwrap();
    let boundary = first.find("createNode transform -n \"b\"").unwrap();
    assert_eq!(&text[..boundary], &first[..boundary]);
}

mod limits;
