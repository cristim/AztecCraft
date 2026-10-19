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
