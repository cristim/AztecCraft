//! Oracle rejections and byte budgets of the UV block.

use super::*;

#[test]
fn oracle_rejects_variants_of_the_uv_block() {
    let mut scene = plane_scene();
    project(&mut scene);
    let bytes = emit_uvs(&scene).unwrap();
    let tail = bytes[bytes.len() - PLANE_UV_TAIL.len() - 1..].to_vec();
    parse_uv_block(&tail);
    let text = String::from_utf8(tail).unwrap();
    for (old, new) in [
        ("mu 0 4", "mu 1 4"),
        ("mu 0 4", "mu 0 3"),
        ("mu 0 4 0 1 2 3", "mu 0 4 0 1 2 4"),
        ("mu 0 4 0 1 2 3", "mu 0 4 0 2 1 3"),
        ("mu 0 4 0 1 2 3", "mu 0 4 1 2 3 4"),
        ("0 1 2 3\n", "0 1 2 3\r\n"),
        ("[0:3]\"", "[0:4]\""),
        ("-s 4 \".uvst", "-s 3 \".uvst"),
        ("\"map1\";\n\tsetAttr -s", "\"map2\";\n\tsetAttr -s"),
        ("\tsetAttr \".cuvs\" -type \"string\" \"map1\";\n", ""),
        (
            "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n",
            "\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n\tsetAttr \".uvst[0].uvsn\" -type \"string\" \"map1\";\n",
        ),
        ("\t0 1\n", "\t0 nan\n"),
    ] {
        assert!(text.contains(old), "missing anchor {old}");
        let changed = text.replacen(old, new, 1);
        assert!(
            std::panic::catch_unwind(|| parse_uv_block(changed.as_bytes())).is_err(),
            "accepted {old} -> {new}"
        );
    }
}

#[test]
fn uv_bounds_cover_the_longest_f32_text_and_the_signed_domain() {
    let mut line = ChannelLine {
        bytes: [0; CHANNEL_BYTES],
        used: 0,
    };
    for value in [
        f32::MAX,
        f32::MIN,
        f32::from_bits(1),
        -f32::from_bits(1),
        -0.0,
    ] {
        line.used = 0;
        line.format(format_args!("\t{value} {value}\n")).unwrap();
        assert!(line.used <= UV_ROW_BYTES, "{value}");
    }
    line.uv_header(i32::MAX as usize).unwrap();
    assert_eq!(
        &line.bytes[..line.used],
        b"\tsetAttr -s 2147483647 \".uvst[0].uvsp[0:2147483646]\" -type \"float2\"\n"
    );
    assert!(line.used <= UV_HEADER_BYTES);
    assert_eq!(line.uv_header(0), Err(AsciiWriteRefusal::EmptyMesh));
    for count in [i32::MAX as usize + 1, usize::MAX] {
        assert_eq!(
            line.uv_header(count),
            Err(AsciiWriteRefusal::IndexOutOfRange)
        );
    }
    assert!(mesh_uv_work(usize::MAX, 0).is_err());
    assert!(mesh_uv_work(0, usize::MAX).is_err());
    assert_eq!(
        (
            UV_HEADER_BYTES,
            UV_ROW_BYTES,
            MU_FACE_BYTES,
            MU_ID_BYTES,
            UV_STATEMENT_BYTES
        ),
        (68, 115, 16, 11, 88)
    );
    assert_eq!(
        mesh_uv_work(6, 24).unwrap(),
        2 * (68 + 24 * 126 + 6 * 16 + 2 + 88)
    );
}

#[test]
fn uv_tail_refuses_one_byte_over_the_cap_and_propagates_each_consumer() {
    let mut scene = plane_scene();
    project(&mut scene);
    let shape = scene.shapes()[0].mesh();
    let set = scene.uv_set(scene.shapes()[0].id());
    let mut total = 0;
    mesh_arrays(shape, set, |bytes| {
        total += bytes.len();
        Ok(())
    })
    .unwrap();
    for (available, expected) in [
        (total, Ok(())),
        (total - 1, Err(AsciiWriteRefusal::TooLarge)),
    ] {
        let mut length = MAX_BYTES - available;
        let result = mesh_arrays(shape, set, |bytes| {
            length = add_prefix_bytes(length, bytes.len())?;
            Ok(())
        });
        assert_eq!(result, expected);
        let mut out = Vec::new();
        let result = mesh_arrays(shape, set, |bytes| append(&mut out, available, bytes));
        assert_eq!(result, expected);
    }
}

#[test]
fn uv_work_is_charged_before_emission() {
    let mut scene = NativeScene::default();
    for index in 0..100 {
        let name = format!("{}{index}", "a".repeat(1190));
        scene.create(Primitive::Cube, &name).unwrap();
    }
    emit_uvs(&scene).unwrap();
    project(&mut scene);
    assert_eq!(emit_uvs(&scene), Err(AsciiWriteRefusal::ResourceLimit));
    assert_eq!(prefix(&scene), Err(AsciiWriteRefusal::SessionUvs));
}
