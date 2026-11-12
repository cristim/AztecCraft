//! Open clamped nonrational curves of degree 1 and 2 (#17), beside the degree 3 specimen.
use super::*;

fn curve(cc: &str) -> Result<crate::native_scene::NativeScene, AsciiError> {
    decode(Arc::from(
        format!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; \
             createNode transform -n \"c1\"; createNode nurbsCurve -n \"cShape1\" -p \"c1\"; \
             setAttr \".cc\" -type \"nurbsCurve\" {cc};"
        )
        .into_bytes(),
    ))
}

const DEGREE_1: &str = "1 3 0 no 3 4 0 1 2 3 4 0 0 0 1 2 0 3 -1 1 4 0 2";
const DEGREE_2: &str = "2 2 0 no 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2";

#[test]
fn degree_one_and_two_curves_open_with_their_degree_and_exact_values() {
    let one = curve(DEGREE_1).unwrap();
    assert_eq!(one.curves()[0].degree(), 1);
    assert_eq!(one.curves()[0].knots(), [0.0, 1.0, 2.0, 3.0]);
    assert_eq!(one.curves()[0].control_vertices().len(), 4);
    assert_eq!(one.curves()[0].domain().unwrap(), [0.0, 3.0]);
    let two = curve(DEGREE_2).unwrap();
    assert_eq!(two.curves()[0].degree(), 2);
    assert_eq!(two.curves()[0].knots(), [0.0, 0.0, 1.0, 2.0, 2.0]);
    assert_eq!(two.curves()[0].domain().unwrap(), [0.0, 2.0]);
}

#[test]
fn unsupported_degrees_forms_and_rational_curves_refuse_as_unsupported() {
    for cc in [
        "0 3 0 no 3 4 0 1 2 3 4 0 0 0 1 2 0 3 -1 1 4 0 2",
        "4 1 0 no 3 8 0 0 0 0 1 1 1 1 5 0 0 0 1 2 0 3 -1 1 4 0 2 5 5 5",
        "2 2 1 no 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
        "2 2 2 no 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
        "2 2 0 yes 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
        "2 2 0 no 2 5 0 0 1 2 2 4 0 0 1 2 3 -1 4 0",
    ] {
        assert_eq!(
            curve(cc).unwrap_err().kind(),
            ErrorKind::Unsupported,
            "{cc}"
        );
    }
}

#[test]
fn count_and_clamp_mismatches_are_refused() {
    for (cc, kind) in [
        // spans must be cv - degree
        (
            "2 3 0 no 3 5 0 0 1 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            ErrorKind::Malformed,
        ),
        // knot count must be cv + degree - 1
        (
            "2 2 0 no 3 6 0 0 1 2 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            ErrorKind::Malformed,
        ),
        (
            "1 3 0 no 3 5 0 1 2 3 4 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            ErrorKind::Malformed,
        ),
        // a degree 2 curve needs two clamp knots at each end
        (
            "2 2 0 no 3 5 0 1 2 3 3 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            ErrorKind::Malformed,
        ),
        // too few CVs for the degree
        ("2 0 0 no 3 3 0 0 1 2 2 0 0 0 1 2", ErrorKind::Malformed),
        // repeated interior knots stay unsupported
        (
            "2 2 0 no 3 5 0 0 0 2 2 4 0 0 0 1 2 0 3 -1 1 4 0 2",
            ErrorKind::Unsupported,
        ),
    ] {
        assert_eq!(curve(cc).unwrap_err().kind(), kind, "{cc}");
    }
}
