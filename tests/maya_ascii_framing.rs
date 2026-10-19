#[path = "support/maya_ascii_structure/fixtures.rs"]
mod fixtures;
#[path = "support/maya_ascii_structure/framing.rs"]
mod framing;

use fixtures::{GEOMETRY, HEADER, golden};
use framing::{Kind, frame};

fn rejects(text: &str) {
    assert!(frame(text.as_bytes()).is_err(), "accepted: {text:?}");
}

#[test]
fn handwritten_scene_has_complete_kind_sequence_and_first_lambert_seam() {
    let document = golden();
    let (kinds, boundary) = frame(document.as_bytes()).unwrap();
    assert_eq!(boundary, HEADER.len() + GEOMETRY.len());
    assert_eq!(
        kinds,
        [
            Kind::Header,
            Kind::Header,
            Kind::Header,
            Kind::Transform,
            Kind::Mesh,
            Kind::Array,
            Kind::Array,
            Kind::Array,
            Kind::Lambert,
            Kind::Color,
            Kind::Group,
            Kind::Lambert,
            Kind::Color,
            Kind::Group,
            Kind::Connection,
            Kind::Connection,
            Kind::Connection,
        ]
    );
    rejects(&format!("{document}evalDeferred \"script\";\n"));
}

#[test]
fn empty_and_no_material_documents_have_eof_and_first_connection_seams() {
    assert_eq!(
        frame(HEADER.as_bytes()).unwrap(),
        (vec![Kind::Header; 3], HEADER.len())
    );
    let geometry = format!("{HEADER}{GEOMETRY}");
    let (mut kinds, boundary) = frame(geometry.as_bytes()).unwrap();
    assert_eq!(boundary, geometry.len());
    assert_eq!(kinds.len(), 8);
    let initial = "connectAttr \"plainShape.iog\" \":initialShadingGroup.dsm\" -na;\n";
    let document = format!("{geometry}{initial}");
    kinds.push(Kind::Connection);
    assert_eq!(frame(document.as_bytes()).unwrap(), (kinds, geometry.len()));
}

#[test]
fn framing_and_unknown_executable_mutations_fail() {
    let document = golden();
    rejects(&document.replace('\n', "\r\n"));
    rejects(&document.replace("createNode lambert", "// extra\ncreateNode lambert"));
    rejects(&document.replace("createNode lambert", "\ncreateNode lambert"));
    rejects(&document.replace(
        "createNode lambert",
        "evalDeferred \"script\";\ncreateNode lambert",
    ));
    rejects(&document.replace(
        "createNode lambert -n \"red\";\n",
        "createNode lambert -n \"red\"; connectAttr \"red.oc\" \"redSG.ss\";\n",
    ));
    rejects(document.trim_end_matches('\n'));
    rejects(&document.replace("\tf 4 0 1 2 3\n;\n", "\tf 4 0 1 2 3;\n"));
    rejects(&document.replace(
        "\tsetAttr -s 4 \".vt[0:3]\"\n",
        "\tsetAttr -s 4 \".vt[0:3]\";\n",
    ));
    rejects(&document.replace("\tf 4 0 1 2 3\n;\n", "\tf 4 0 1 2 3\n ;\n"));
}

#[test]
fn lone_cr_inside_an_array_row_is_refused_on_its_own() {
    let document = golden().replace("\t-0.5 0 -0.5\n", "\t-0.5 0 -0.5\r\n");
    assert_eq!(document.matches('\r').count(), 1);
    assert_eq!(
        frame(document.as_bytes()).err().as_deref(),
        Some("canonical LF framing")
    );
}

#[test]
fn uuid_and_transform_channel_lines_are_classified_in_their_positions() {
    let uuid = "\trename -uid \"id\";\n";
    let channel = "\tsetAttr \".t\" -type \"double3\" 0 0 0;\n";
    let geometry = GEOMETRY
        .replace(
            "createNode mesh",
            &format!("{uuid}{channel}createNode mesh"),
        )
        .replace(
            "\tsetAttr -s 4 \".vt",
            &format!("{uuid}\tsetAttr -s 4 \".vt"),
        );
    let document = format!("{HEADER}{geometry}");
    let (kinds, boundary) = frame(document.as_bytes()).unwrap();
    assert_eq!(boundary, document.len());
    assert_eq!(
        kinds,
        [
            Kind::Header,
            Kind::Header,
            Kind::Header,
            Kind::Transform,
            Kind::Uuid,
            Kind::Channel,
            Kind::Mesh,
            Kind::Uuid,
            Kind::Array,
            Kind::Array,
            Kind::Array,
        ]
    );
    rejects(&format!("{}{uuid}", golden()));
    rejects(&format!("{}{channel}", golden()));
}

#[test]
fn input_bound_is_checked_before_line_collection() {
    let oversized = vec![b'x'; 16 * 1024 * 1024 + 1];
    assert_eq!(frame(&oversized).err().as_deref(), Some("input byte limit"));
}

#[test]
fn exact_byte_limit_is_inclusive_before_utf8_validation() {
    let maximum = 16 * 1024 * 1024;
    let mut document = format!("{HEADER}createNode transform -n \"\";\n");
    let name_offset = document.len() - 3;
    document.insert_str(name_offset, &"x".repeat(maximum - document.len()));
    assert_eq!(document.len(), maximum);
    assert_eq!(
        frame(document.as_bytes()).unwrap(),
        (
            vec![Kind::Header, Kind::Header, Kind::Header, Kind::Transform],
            maximum
        )
    );
    let mut oversized = document.into_bytes();
    oversized.push(0xff);
    assert_eq!(frame(&oversized).err().as_deref(), Some("input byte limit"));
}

#[test]
fn malformed_headers_arrays_and_geometry_after_tail_fail() {
    rejects("");
    rejects("//Maya ASCII 2026 scene\n");
    rejects(&HEADER.replace("2026", "2025"));
    assert!(frame(&[0xff, b'\n']).is_err());
    for count in ["0", "5", "2147483648", "999999999999999999999999", "x"] {
        rejects(&golden().replace("-s 4 \".vt", &format!("-s {count} \".vt")));
    }
    rejects(&golden().replace(".vt[0:3]", ".uv[0:3]"));
    rejects(&golden().replace("-type \"polyFaces\"", "-type \"other\""));
    rejects(&golden().replace("\tf 4 0 1 2 3", "\t4 0 1 2 3"));
    rejects(&golden().replace("\t-0.5 0 -0.5", "\t-0.5 0 -0.5 // row"));
    rejects(&format!("{}{}", golden(), GEOMETRY));
    rejects(&format!("{}createNode transform -n \"late\";\n", golden()));
    rejects(&format!(
        "{}\tsetAttr -s 1 \".vt[0:0]\"\n\t0 0 0\n;\n",
        golden()
    ));
}

#[test]
fn array_headers_require_one_attribute_and_decimal_last_index() {
    for (attribute, last, suffix) in [
        (".vt", "3", ""),
        (".ed", "3", ""),
        (".fc", "0", " -type \"polyFaces\""),
    ] {
        let header = format!("\"{attribute}[0:{last}]\"{suffix}\n");
        let extra = format!("\"{attribute}[0:{last}]\" \".vt[0:3]\"{suffix}\n");
        assert_eq!(
            frame(golden().replace(&header, &extra).as_bytes())
                .err()
                .as_deref(),
            Some("array header")
        );
        for invalid in ["", "-1", "+3", "3x", " 3"] {
            let malformed = format!("\"{attribute}[0:{invalid}]\"{suffix}\n");
            assert_eq!(
                frame(golden().replace(&header, &malformed).as_bytes())
                    .err()
                    .as_deref(),
                Some("array header")
            );
        }
    }
}

#[test]
fn array_rows_require_nonempty_payload_after_each_exact_prefix() {
    for (row, prefix) in [
        ("\t-0.5 0 -0.5\n", "\t"),
        ("\t0 1 0\n", "\t"),
        ("\tf 4 0 1 2 3\n", "\tf "),
    ] {
        for whitespace in ["", " \t "] {
            let empty = format!("{prefix}{whitespace}\n");
            assert_eq!(
                frame(golden().replace(row, &empty).as_bytes())
                    .err()
                    .as_deref(),
                Some("array row framing")
            );
        }
    }
}
