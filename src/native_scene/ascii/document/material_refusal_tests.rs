//! Session-material nodes and colours the writer would not emit (#156), names that collide with
//! the scene's own, the material limit, and files with no material statements at all.
use super::material_tests::{attached, block, file, read, read_err, scene};
use super::tests::replace;
use super::*;
use crate::{
    native_scene::{MAX_SESSION_MATERIALS, session_material::SessionMaterials},
    scene::Primitive,
};

#[test]
fn gapped_names_and_a_material_that_is_never_assigned_round_trip_through_the_table() {
    let mut model = scene();
    for (index, number) in [(1, 7), (3, 12)] {
        model.session_materials.materials[index].name = format!("lambert{number}");
        model.session_materials.groups[index].name = format!("lambert{number}SG");
    }
    let candidate = attached(&file(&model));
    assert_eq!(
        candidate.session_materials.materials,
        model.session_materials.materials
    );
    assert_eq!(
        candidate.session_materials.groups,
        model.session_materials.groups
    );
    assert_eq!(
        candidate.session_materials.assignments,
        model.session_materials.assignments
    );
}

#[test]
fn files_without_material_statements_read_none_and_the_gate_is_unchanged() {
    let mut plain = NativeScene::default();
    plain.create(Primitive::Cube, "a").unwrap();
    for links in [DefaultLinks::Omit, DefaultLinks::Emit] {
        let bytes = emit_with_links(&plain, links).unwrap();
        assert!(read(&bytes).unwrap().materials.is_none());
        assert_eq!(
            super::tests::authored(&bytes).scene().session_materials(),
            plain.session_materials()
        );
    }
}

#[test]
fn statements_the_writer_would_not_emit_are_refused() {
    let bytes = file(&scene());
    let sg = "createNode shadingEngine -n \"lambert2SG\";\n";
    let cases = [
        (
            "unknown node",
            replace(
                &bytes,
                "createNode lambert -n \"lambert2\";",
                "createNode blinn -n \"lambert2\";",
            ),
        ),
        (
            "materialInfo",
            [
                &bytes[..],
                b"createNode materialInfo -n \"materialInfo1\";\n",
            ]
            .concat(),
        ),
        (
            "Maya's own lambert",
            replace(&bytes, "-n \"lambert2\";", "-n \"lambert1\";"),
        ),
        (
            "initial shader",
            replace(&bytes, "-n \"lambert2\";", "-n \"standardSurface1\";"),
        ),
        (
            "leading zero",
            replace(&bytes, "-n \"lambert2\";", "-n \"lambert02\";"),
        ),
        (
            "group named as a lambert",
            replace(&bytes, "-n \"lambert2SG\";", "-n \"lambert9\";"),
        ),
        (
            "lambert named as a group",
            replace(
                &bytes,
                "createNode lambert -n \"lambert2\"",
                "createNode lambert -n \"lambert2SG\"",
            ),
        ),
        (
            "group of another number",
            replace(&bytes, sg, "createNode shadingEngine -n \"lambert9SG\";\n"),
        ),
        (
            "parented node",
            replace(&bytes, "-n \"lambert2\";", "-n \"lambert2\" -p \"a\";"),
        ),
        (
            "shared node",
            replace(&bytes, "-n \"lambert2\";", "-n \"lambert2\" -s;"),
        ),
        (
            "extra setAttr flag",
            replace(
                &bytes,
                ".c\" -type \"float3\"",
                ".c\" -k off -type \"float3\"",
            ),
        ),
        (
            "untyped colour",
            replace(&bytes, " -type \"float3\" 0.25", " 0.25"),
        ),
        (
            "double3 colour",
            replace(&bytes, "\"float3\" 0.25", "\"double3\" 0.25"),
        ),
        ("short colour", replace(&bytes, "0.25 0.5 1;", "0.25 0.5;")),
        (
            "long colour",
            replace(&bytes, "0.25 0.5 1;", "0.25 0.5 1 1;"),
        ),
        ("other attribute", replace(&bytes, "\".c\"", "\".ic\"")),
        ("not renderable", replace(&bytes, ".ro\" yes", ".ro\" no")),
        ("ihi", replace(&bytes, ".ro\" yes", ".ihi\" 0")),
        (
            "duplicate colour",
            replace(
                &bytes,
                "createNode shadingEngine -n \"lambert2SG\";",
                "\tsetAttr \".c\" -type \"float3\" 0 0 0;\ncreateNode shadingEngine -n \"lambert2SG\";",
            ),
        ),
        (
            "rename -uid",
            replace(
                &bytes,
                sg,
                &format!("{sg}\trename -uid \"6c1b2d4e-0a3f-4b5c-8d7e-9f0a1b2c3d4e\";\n"),
            ),
        ),
    ];
    for (name, edited) in cases {
        assert!(read(&edited).is_err(), "{name}");
    }
}

#[test]
fn a_material_or_group_named_like_a_transform_or_shape_is_a_name_collision() {
    for (collider, kind) in [
        ("lambert2", "transform"),
        ("lambert2SG", "transform"),
        ("lambert2", "shape"),
        ("lambert2SG", "shape"),
    ] {
        let mut model = NativeScene::default();
        model.create(Primitive::Cube, "a").unwrap();
        model.create_session_material([0.5, 0.5, 0.5]).unwrap();
        let mut plain = model.clone();
        plain.session_materials = SessionMaterials::default();
        if kind == "transform" {
            plain.nodes[0].name = collider.into();
        } else {
            plain.shapes[0].name = collider.into();
        }
        let mut bytes = emit_with_links(&plain, DefaultLinks::Omit).unwrap_or_else(|e| {
            panic!("{collider} {kind}: {e:?}");
        });
        // The material block's own shape link names whatever the shape is called.
        let mut named = model.clone();
        named.shapes[0].name = plain.shapes[0].name.clone();
        bytes.extend_from_slice(block(&named).as_bytes());
        assert_eq!(
            read_err(&bytes),
            ErrorKind::Unsupported,
            "{collider} {kind}"
        );
        // The same name declared after the material is a collision too.
        let text = emit_with_links(&plain, DefaultLinks::Omit).unwrap();
        let text = String::from_utf8(text).unwrap();
        let block = block(&named);
        let (nodes, connects) = block.split_once("connectAttr").unwrap();
        let at = text.find("createNode transform").unwrap();
        let late = format!("{}{nodes}{}connectAttr{connects}", &text[..at], &text[at..]);
        assert_eq!(
            read_err(late.as_bytes()),
            ErrorKind::Unsupported,
            "{collider} {kind} late"
        );
    }
}

#[test]
fn colours_must_be_the_exact_single_precision_text() {
    let bytes = file(&scene());
    for (name, text) in [
        ("f64 precision", "0.30000000000000004 0.5 1"),
        ("surplus digits", "0.30000001 0.5 1"),
        ("no leading zero", ".25 0.5 1"),
        ("exponent", "2.5e-1 0.5 1"),
        ("trailing zero", "0.25 0.50 1"),
        ("one point zero", "0.25 0.5 1.0"),
        ("above range", "0.25 0.5 1.5"),
        ("below range", "-0.25 0.5 1"),
        ("not finite", "0.25 inf 1"),
        ("not a number", "0.25 nan 1"),
        ("word", "0.25 half 1"),
    ] {
        let edited = replace(&bytes, "0.25 0.5 1;", &format!("{text};"));
        assert!(read(&edited).is_err(), "{name}");
    }
    // The text a writer prints for an f32 that has no short form is accepted.
    let mut model = scene();
    model.session_materials.materials[1].color =
        [0.1, 0.123_456_79, 1.0 / 3.0].map(|c| f64::from(c as f32));
    assert_eq!(
        attached(&file(&model)).session_materials,
        model.session_materials
    );
}

#[test]
fn the_table_is_bounded_by_the_session_material_limit() {
    // Index 0 is the default pair, so a file holds MAX - 1 materials at most.
    let mut model = NativeScene::default();
    model.create(Primitive::Cube, "a").unwrap();
    for _ in 1..MAX_SESSION_MATERIALS {
        model.create_session_material([0.5, 0.5, 0.5]).unwrap();
    }
    assert_eq!(model.session_materials().len(), MAX_SESSION_MATERIALS);
    let bytes = file(&model);
    let full = read(&bytes).unwrap();
    assert!(full.materials.is_some());
    let extra = "createNode lambert -n \"lambert600\";\n\tsetAttr \".c\" -type \"float3\" 0 0 0;\ncreateNode shadingEngine -n \"lambert600SG\";\n\tsetAttr \".ro\" yes;\nconnectAttr \"lambert600.oc\" \"lambert600SG.ss\";\nconnectAttr \"lambert600SG.pa\" \":renderPartition.st\" -na;\nconnectAttr \"lambert600.msg\" \":defaultShaderList1.s\" -na;\n";
    let over = [&bytes[..], extra.as_bytes()].concat();
    assert_eq!(read_err(&over), ErrorKind::ResourceLimit);
}

#[test]
fn more_connects_the_writer_would_not_emit_are_refused() {
    let bytes = file(&scene());
    let oc = "connectAttr \"lambert2.oc\" \"lambert2SG.ss\";\n";
    let pa = "connectAttr \"lambert2SG.pa\" \":renderPartition.st\" -na;\n";
    let msg = "connectAttr \"lambert2.msg\" \":defaultShaderList1.s\" -na;\n";
    let link = "connectAttr \"aShape.iog\" \"lambert2SG.dsm\" -na;\n";
    let cases = [
        (
            "oc into a lambert",
            replace(&bytes, oc, "connectAttr \"lambert2.oc\" \"lambert3.ss\";\n"),
        ),
        (
            "oc from a group",
            replace(
                &bytes,
                oc,
                "connectAttr \"lambert2SG.oc\" \"lambert2SG.ss\";\n",
            ),
        ),
        (
            "oc with -na",
            replace(
                &bytes,
                oc,
                "connectAttr \"lambert2.oc\" \"lambert2SG.ss\" -na;\n",
            ),
        ),
        (
            "pa of a lambert",
            replace(
                &bytes,
                pa,
                "connectAttr \"lambert2.pa\" \":renderPartition.st\" -na;\n",
            ),
        ),
        (
            "msg of a group",
            replace(
                &bytes,
                msg,
                "connectAttr \"lambert2SG.msg\" \":defaultShaderList1.s\" -na;\n",
            ),
        ),
        (
            "msg to another list",
            replace(
                &bytes,
                msg,
                "connectAttr \"lambert2.msg\" \":defaultShaderList2.s\" -na;\n",
            ),
        ),
        (
            "membership of a transform",
            replace(
                &bytes,
                link,
                "connectAttr \"a.iog\" \"lambert2SG.dsm\" -na;\n",
            ),
        ),
        (
            "membership of an unknown shape",
            replace(
                &bytes,
                link,
                "connectAttr \"zShape.iog\" \"lambert2SG.dsm\" -na;\n",
            ),
        ),
        (
            "membership into a lambert",
            replace(
                &bytes,
                link,
                "connectAttr \"aShape.iog\" \"lambert2.dsm\" -na;\n",
            ),
        ),
        (
            "membership into an unknown group",
            replace(
                &bytes,
                link,
                "connectAttr \"aShape.iog\" \"lambert9SG.dsm\" -na;\n",
            ),
        ),
        (
            "colon-qualified group",
            replace(
                &bytes,
                link,
                "connectAttr \"aShape.iog\" \":lambert2SG.dsm\" -na;\n",
            ),
        ),
    ];
    for (name, edited) in cases {
        assert!(read(&edited).is_err(), "{name}");
    }
}

#[test]
fn consistent_renames_and_crossed_pairings_are_refused_for_the_one_reason_each_has() {
    let bytes = file(&scene());
    let text = String::from_utf8(bytes.clone()).unwrap();
    // Every mention renamed alike, so only the name rule itself can refuse.
    for (name, from, to) in [
        ("Maya's own lambert", "lambert2", "lambert1"),
        ("initial shader", "lambert2", "standardSurface1"),
        ("group of another number", "lambert2SG", "lambert9SG"),
    ] {
        let edited = text.replace(from, to);
        assert!(read(edited.as_bytes()).is_err(), "{name}");
    }
    // Each lambert wired to the other's group: every owner is complete and only pairing is wrong.
    let crossed = text
        .replace(
            "\"lambert2.oc\" \"lambert2SG.ss\"",
            "\"lambert2.oc\" \"lambert3SG.ss\"",
        )
        .replace(
            "\"lambert3.oc\" \"lambert3SG.ss\"",
            "\"lambert3.oc\" \"lambert2SG.ss\"",
        );
    assert_ne!(crossed, text);
    assert!(read(crossed.as_bytes()).is_err(), "crossed");
    // A transform with no shape, as a curve file has, does not make the file retained.
    let mut empty = NativeScene::default();
    empty.create_session_material([0.5, 0.5, 0.5]).unwrap();
    let header_end = "fileInfo \"AztecCraft\" \"1\";\n";
    let bytes = replace(
        &file(&empty),
        header_end,
        &format!("{header_end}createNode transform -n \"t\";\n"),
    );
    assert!(read(&bytes).unwrap().materials.is_some());
    assert!(
        super::tests::refused(&bytes)
            == AsciiDocumentError::Ascii(AsciiError {
                kind: ErrorKind::Unsupported,
                offset: None,
            })
    );
}
