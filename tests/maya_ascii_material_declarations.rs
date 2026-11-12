#[path = "support/maya_ascii_structure/material_declarations.rs"]
mod declarations;
#[path = "support/maya_ascii_structure/fixtures.rs"]
mod fixtures;

use declarations::{inspect_declarations as inspect, inspect_header};
use fixtures::{CONNECTIONS, GEOMETRY, HEADER, MATERIALS, golden};

fn rejects(case: &str, text: &str, category: &str) {
    assert_eq!(
        inspect(text.as_bytes()).err().as_deref(),
        Some(category),
        "{case}"
    );
}

#[test]
fn golden_preserves_pairs_color_text_seam_shapes_and_raw_connections() {
    let report = inspect(golden().as_bytes()).unwrap();
    assert_eq!(report.boundary, HEADER.len() + GEOMETRY.len());
    assert_eq!(report.shapes, ["plainShape"]);
    assert_eq!(
        report.materials,
        [
            (
                "red".into(),
                "redSG".into(),
                ["-0".into(), "0.1".into(), "1".into()]
            ),
            (
                "unused".into(),
                "unusedSG".into(),
                ["0.5".into(), "0.5".into(), "0.5".into()]
            ),
        ]
    );
    let expected = inspect_header(&format!("{HEADER}{CONNECTIONS}")).unwrap();
    assert_eq!(report.connections.len(), 3);
    assert_eq!(
        report.connections,
        expected.into_iter().skip(3).collect::<Vec<_>>()
    );
}

#[test]
fn empty_geometry_unused_pairs_and_unvalidated_endpoints_are_accepted() {
    let initial = "connectAttr \"plainShape.iog\" \":initialShadingGroup.dsm\" -na;\n";
    let nonsense = "connectAttr \"missing.output\" \"unknown.input\";\n";
    for (case, body, shapes, materials, connections) in [
        ("empty", String::new(), 0, 0, 0),
        ("geometry", GEOMETRY.into(), 1, 0, 0),
        ("initial group", format!("{GEOMETRY}{initial}"), 1, 0, 1),
        ("unused pairs", MATERIALS.into(), 0, 2, 0),
        (
            "complete tail",
            format!("{MATERIALS}{CONNECTIONS}"),
            0,
            2,
            3,
        ),
        ("graph deferred", nonsense.into(), 0, 0, 1),
    ] {
        let text = format!("{HEADER}{body}");
        let report = inspect(text.as_bytes()).unwrap();
        assert_eq!(
            (
                report.shapes.len(),
                report.materials.len(),
                report.connections.len()
            ),
            (shapes, materials, connections),
            "{case}"
        );
        let expected = inspect_header(&format!("{HEADER}{initial}")).unwrap();
        if case == "initial group" {
            assert_eq!(
                report.connections,
                expected.into_iter().skip(3).collect::<Vec<_>>()
            );
        }
        if connections == 0 {
            let seam = if materials == 0 {
                text.len()
            } else {
                HEADER.len()
            };
            assert_eq!(report.boundary, seam, "{case}");
        }
    }
}

#[test]
fn rgb_values_and_type_syntax_use_exact_unquoted_f64_profile() {
    for payload in ["-0 0.1 1", "5e-324 0 1", "0.10000000000000001 0 1"] {
        let body = MATERIALS.replacen("-0 0.1 1", payload, 1);
        let report = inspect(format!("{HEADER}{body}").as_bytes()).unwrap();
        assert_eq!(
            report.materials[0].2,
            payload
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
                .as_slice()
        );
    }
    for (case, payload) in [
        ("nonnumeric", "bad 0 1"),
        ("negative", "-0.1 0 1"),
        ("above one", "1.1 0 1"),
        ("NaN", "NaN 0 1"),
        ("positive infinity", "inf 0 1"),
        ("negative infinity", "-inf 0 1"),
        ("quoted number", "\"0\" 0 1"),
        ("missing component", "0 1"),
        ("extra component", "0 0 0 1"),
    ] {
        let body = MATERIALS.replacen("-0 0.1 1", payload, 1);
        rejects(case, &format!("{HEADER}{body}"), "Lambert color");
    }
    for (case, from, to) in [
        ("type value", "\"float3\"", "\"double3\""),
        ("type quote", "\"float3\"", "float3"),
        ("flag", "-type", "\"-type\""),
    ] {
        let body = MATERIALS.replacen(from, to, 1);
        rejects(case, &format!("{HEADER}{body}"), "Lambert color");
    }
}

#[test]
fn both_declaration_roles_validate_names_flags_quotes_and_arity() {
    for (role, old, declaration, name_error, arity_error) in [
        (
            "Lambert",
            "red",
            "createNode lambert -n \"red\";",
            "Lambert name/order",
            "Lambert declaration",
        ),
        (
            "group",
            "redSG",
            "createNode shadingEngine -n \"redSG\";",
            "shadingEngine name/order",
            "shadingEngine declaration",
        ),
    ] {
        let original = format!("\"{old}\"");
        let valid = MATERIALS.replacen(&original, "\"valid_name2\"", 1);
        assert!(
            inspect(format!("{HEADER}{valid}").as_bytes()).is_ok(),
            "{role}"
        );
        for (case, replacement) in [
            ("empty", "\"\""),
            ("leading digit", "\"2name\""),
            ("invalid character", "\"bad-name\""),
            ("unquoted", "bare"),
        ] {
            let body = MATERIALS.replacen(&original, replacement, 1);
            rejects(
                &format!("{role}: {case}"),
                &format!("{HEADER}{body}"),
                name_error,
            );
        }
        let body = MATERIALS.replacen(declaration, &declaration.replace("-n", "-bad"), 1);
        rejects(
            &format!("{role}: flag"),
            &format!("{HEADER}{body}"),
            name_error,
        );
        let body = MATERIALS.replacen(declaration, &declaration.replace(';', " extra;"), 1);
        rejects(
            &format!("{role}: arity"),
            &format!("{HEADER}{body}"),
            arity_error,
        );
    }
}

#[test]
fn observed_names_are_unique_across_both_insertions_and_geometry() {
    for (case, from, to, category) in [
        (
            "Lambert-Lambert",
            "\"unused\"",
            "\"red\"",
            "Lambert name/order",
        ),
        (
            "group-group",
            "\"unusedSG\"",
            "\"redSG\"",
            "shadingEngine name/order",
        ),
        (
            "group-material",
            "\"redSG\"",
            "\"red\"",
            "shadingEngine name/order",
        ),
        (
            "later Lambert-earlier group",
            "\"unused\"",
            "\"redSG\"",
            "Lambert name/order",
        ),
        (
            "transform-Lambert",
            "\"red\"",
            "\"plain\"",
            "Lambert name/order",
        ),
        (
            "shape-Lambert",
            "\"red\"",
            "\"plainShape\"",
            "Lambert name/order",
        ),
        (
            "transform-group",
            "\"redSG\"",
            "\"plain\"",
            "shadingEngine name/order",
        ),
        (
            "shape-group",
            "\"redSG\"",
            "\"plainShape\"",
            "shadingEngine name/order",
        ),
    ] {
        let body = MATERIALS.replacen(from, to, 1);
        rejects(case, &format!("{HEADER}{GEOMETRY}{body}"), category);
    }
}

#[test]
fn declaration_pairs_require_one_color_then_one_group() {
    let color = "\tsetAttr \".c\" -type \"float3\" -0 0.1 1;\n";
    let group = "createNode shadingEngine -n \"redSG\";\n";
    for (case, body, category) in [
        (
            "missing color",
            MATERIALS.replacen(color, "", 1),
            "shadingEngine owner",
        ),
        (
            "duplicate color",
            MATERIALS.replacen(color, &format!("{color}{color}"), 1),
            "color owner",
        ),
        (
            "next Lambert",
            MATERIALS.replacen(group, "", 1),
            "Lambert name/order",
        ),
        (
            "group before color",
            MATERIALS.replacen(&format!("{color}{group}"), &format!("{group}{color}"), 1),
            "shadingEngine owner",
        ),
        ("extra color", format!("{MATERIALS}{color}"), "color owner"),
        (
            "extra group",
            format!("{MATERIALS}{group}"),
            "shadingEngine owner",
        ),
        (
            "orphan color inherited",
            color.into(),
            "tail before declaration",
        ),
        (
            "orphan group inherited",
            group.into(),
            "tail before declaration",
        ),
    ] {
        rejects(case, &format!("{HEADER}{body}"), category);
    }
}

#[test]
fn pending_pair_at_eof_and_after_connection_is_refused() {
    let pending = "createNode lambert -n \"red\";\n\tsetAttr \".c\" -type \"float3\" 0 0 0;\n";
    rejects(
        "isolated EOF",
        &format!("{HEADER}{pending}"),
        "incomplete material pair",
    );
    assert!(
        inspect(format!("{HEADER}{pending}createNode shadingEngine -n \"redSG\";\n").as_bytes())
            .is_ok()
    );
    rejects(
        "pending plus connection",
        &format!("{HEADER}{pending}{CONNECTIONS}"),
        "incomplete material pair",
    );
}

#[test]
fn completed_declarations_after_first_connection_are_refused_by_phase() {
    let connection = "connectAttr \"missing.output\" \"unknown.input\";\n";
    let baseline = format!("{HEADER}{MATERIALS}{connection}");
    assert!(inspect(baseline.as_bytes()).is_ok());
    let fresh = MATERIALS.replace("red", "fresh").replace("unused", "other");
    rejects(
        "late complete pair",
        &format!("{baseline}{fresh}"),
        "declaration after connection",
    );
}

#[test]
fn inherited_framing_geometry_and_tokenizer_refusals_propagate() {
    let document = golden();
    for (case, text, category) in [
        (
            "CRLF",
            document.replace('\n', "\r\n"),
            "canonical LF framing",
        ),
        (
            "executable",
            format!("{document}evalDeferred \"script\";\n"),
            "unknown body statement",
        ),
        (
            "escape",
            document.replacen("\"red\"", "\"bad\\q\"", 1),
            "unsupported string escape",
        ),
        (
            "geometry parent",
            document.replacen("-p \"plain\"", "-p \"missing\"", 1),
            "mesh owner",
        ),
        (
            "empty array row",
            document.replacen("\t-0.5 0 -0.5\n", "\t \n", 1),
            "array row framing",
        ),
        (
            "quoted command",
            document.replacen("createNode lambert", "\"createNode\" lambert", 1),
            "unknown body statement",
        ),
        (
            "quoted node type",
            document.replacen("createNode lambert", "createNode \"lambert\"", 1),
            "unknown body statement",
        ),
        (
            "unquoted color attribute",
            document.replacen("\".c\"", ".c", 1),
            "geometry after material tail",
        ),
    ] {
        rejects(case, &text, category);
    }
}
