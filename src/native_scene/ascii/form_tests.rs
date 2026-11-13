//! Open closed (form 1) and periodic (form 2) nonrational curves from ASCII (#17).
use super::*;
use crate::native_scene::CurveForm;

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

// degree 3, 4 spans, 7 CVs (P0 P1 P2 P3 P0 P1 P2), uniform knots 0..8 (the documented shape).
const PERIODIC: &str = "3 4 2 no 3 9 0 1 2 3 4 5 6 7 8 7 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1 4 4 2";
const CLOSED: &str = "3 2 1 no 3 7 0 0 0 1 2 2 2 5 0 0 0 2 0 0 2 2 0 0 2 0 0 0 0";

#[test]
fn periodic_and_closed_curves_open_with_their_form_and_evaluate() {
    let scene = curve(PERIODIC).unwrap();
    let periodic = &scene.curves()[0];
    assert_eq!(periodic.form(), CurveForm::Periodic);
    assert_eq!(periodic.degree(), 3);
    assert_eq!(periodic.domain().unwrap(), [2.0, 6.0]);
    // uniform cubic B-spline closed form at the first knot: (P0 + 4 P1 + P2) / 6
    let start = periodic.evaluate(2.0).unwrap();
    let expected = (glam::DVec3::new(0.0, 0.0, 0.0)
        + glam::DVec3::new(4.0, 0.0, 1.0) * 4.0
        + glam::DVec3::new(4.0, 4.0, 2.0))
        / 6.0;
    assert!((start - expected).length() < 1e-9);
    assert!((start - periodic.evaluate(6.0).unwrap()).length() < 1e-9);
    let closed = curve(CLOSED).unwrap();
    assert_eq!(closed.curves()[0].form(), CurveForm::Closed);
}

#[test]
fn undocumented_forms_and_rational_curves_still_refuse_as_unsupported() {
    for cc in [
        // closed with a gap between first and last CV
        "3 2 1 no 3 7 0 0 0 1 2 2 2 5 0 0 0 2 0 0 2 2 0 0 2 0 0 0 5",
        // periodic with nonuniform knots
        "3 4 2 no 3 9 0 1 2 3 5 5 6 7 8 7 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1 4 4 2",
        // periodic with degree 2
        "2 4 2 no 3 7 0 1 2 3 4 5 6 6 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1",
        // rational stays refused whatever the form
        "3 4 2 yes 3 9 0 1 2 3 4 5 6 7 8 7 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1 4 4 2",
        // form outside 0..2
        "3 4 3 no 3 9 0 1 2 3 4 5 6 7 8 7 0 0 0 4 0 1 4 4 2 0 4 3 0 0 0 4 0 1 4 4 2",
    ] {
        assert_eq!(
            curve(cc).unwrap_err().kind(),
            ErrorKind::Unsupported,
            "{cc}"
        );
    }
}
