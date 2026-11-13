//! Closed (form 1) and periodic (form 2) curves (#17), checked against closed forms and a
//! test-local Cox-de Boor evaluator over the textbook extended knot vector.
use super::tests::candidate;
use super::*;
use glam::DVec3;

fn build(
    form: CurveForm,
    degree: u32,
    points: &[[f64; 3]],
    knots: &[f64],
) -> crate::native_scene::NativeScene {
    let (mut scene, _) = candidate();
    scene.curves[0].form = form;
    scene.curves[0].degree = degree;
    scene.curves[0].control_vertices = points.to_vec();
    scene.curves[0].knots = knots.to_vec();
    scene
}

fn admit(form: CurveForm, degree: u32, points: &[[f64; 3]], knots: &[f64]) -> NativeCurveShape {
    let mut scene = build(form, degree, points, knots);
    scene.validate().unwrap();
    scene.curves.remove(0)
}

fn refusal(form: CurveForm, degree: u32, points: &[[f64; 3]], knots: &[f64]) -> NativeError {
    build(form, degree, points, knots).validate().unwrap_err()
}

fn basis(full: &[f64], i: usize, d: usize, u: f64, last: usize) -> f64 {
    if d == 0 {
        return f64::from(u8::from(
            (full[i] <= u && u < full[i + 1]) || (u == full[full.len() - 1] && i == last),
        ));
    }
    let mut value = 0.0;
    if full[i + d] > full[i] {
        value += (u - full[i]) / (full[i + d] - full[i]) * basis(full, i, d - 1, u, last);
    }
    if full[i + d + 1] > full[i + 1] {
        value += (full[i + d + 1] - u) / (full[i + d + 1] - full[i + 1])
            * basis(full, i + 1, d - 1, u, last);
    }
    value
}

/// Uniform periodic oracle: the two omitted outer knots are extrapolated by the step.
fn periodic_oracle(points: &[[f64; 3]], knots: &[f64], u: f64) -> DVec3 {
    let step = knots[1] - knots[0];
    let mut full = vec![knots[0] - step];
    full.extend_from_slice(knots);
    full.push(knots[knots.len() - 1] + step);
    let last = points.len() - 1;
    let mut sum = DVec3::ZERO;
    for (i, p) in points.iter().enumerate() {
        sum += DVec3::from_array(*p) * basis(&full, i, 3, u, last);
    }
    sum
}

fn close(a: DVec3, b: DVec3) {
    assert!((a - b).length() < 1e-9, "{a:?} != {b:?}");
}

const LOOP: [[f64; 3]; 4] = [
    [0.0, 0.0, 0.0],
    [4.0, 0.0, 1.0],
    [4.0, 4.0, 2.0],
    [0.0, 4.0, 3.0],
];

fn loop_cvs(distinct: &[[f64; 3]], count: usize) -> Vec<[f64; 3]> {
    (0..count).map(|i| distinct[i % distinct.len()]).collect()
}

fn ints(range: std::ops::Range<i32>) -> Vec<f64> {
    range.map(f64::from).collect()
}

#[test]
fn periodic_cubic_matches_closed_form_at_knots_and_cox_de_boor_between() {
    let points = loop_cvs(&LOOP, 7);
    let knots = ints(0..9);
    let curve = admit(CurveForm::Periodic, 3, &points, &knots);
    assert_eq!(curve.form(), CurveForm::Periodic);
    assert_eq!(curve.domain().unwrap(), [2.0, 6.0]);
    for j in 0..4usize {
        let at = |k: usize| DVec3::from_array(LOOP[k % 4]);
        let expected = (at(j) + at(j + 1) * 4.0 + at(j + 2)) / 6.0;
        close(curve.evaluate(2.0 + j as f64).unwrap(), expected);
    }
    for step in 0..=40 {
        let u = 2.0 + 4.0 * f64::from(step) / 40.0;
        close(
            curve.evaluate(u).unwrap(),
            periodic_oracle(&points, &knots, u),
        );
    }
    close(curve.evaluate(2.0).unwrap(), curve.evaluate(6.0).unwrap());
    for span in 0..4 {
        let c = curve.bezier_span(span).unwrap();
        for t in [0.0, 0.25, 0.5, 1.0] {
            let s = 1.0 - t;
            let h = c[0] * (s * s * s)
                + c[1] * (3.0 * s * s * t)
                + c[2] * (3.0 * s * t * t)
                + c[3] * (t * t * t);
            close(h, periodic_oracle(&points, &knots, 2.0 + span as f64 + t));
        }
    }
}

#[test]
fn maya_shaped_periodic_circle_has_negative_knot_start_and_a_smooth_seam() {
    let ring: Vec<[f64; 3]> = (0..8)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 8.0;
            [a.cos(), 0.0, a.sin()]
        })
        .collect();
    let points = loop_cvs(&ring, 11);
    let curve = admit(CurveForm::Periodic, 3, &points, &ints(-2..11));
    assert_eq!(curve.domain().unwrap(), [0.0, 8.0]);
    close(curve.evaluate(0.0).unwrap(), curve.evaluate(8.0).unwrap());
    let first = curve.bezier_span(0).unwrap();
    let last = curve.bezier_span(7).unwrap();
    close(first[1] - first[0], last[3] - last[2]);
}

#[test]
fn closed_curves_are_clamped_with_coincident_end_cvs() {
    let cubic = [
        [0.0; 3],
        [2.0, 0.0, 0.0],
        [2.0, 2.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0; 3],
    ];
    let knots = [0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0];
    let curve = admit(CurveForm::Closed, 3, &cubic, &knots);
    assert_eq!(curve.form(), CurveForm::Closed);
    close(curve.evaluate(0.0).unwrap(), curve.evaluate(2.0).unwrap());
    // Clamped quadratic Bezier pair: at the shared knot the point is the middle CV (4,4)/2 pair.
    let quad = [[0.0; 3], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0; 3]];
    let curve = admit(CurveForm::Closed, 2, &quad, &[0.0, 0.0, 1.0, 2.0, 2.0]);
    // u=1: basis weights (1/2, 1/2) on P1,P2 (clamped end spans are not uniform).
    close(curve.evaluate(1.0).unwrap(), DVec3::new(4.0, 2.0, 0.0));
    let line = [[0.0; 3], [1.0, 1.0, 0.0], [0.0; 3]];
    let curve = admit(CurveForm::Closed, 1, &line, &[0.0, 1.0, 2.0]);
    close(curve.evaluate(1.0).unwrap(), DVec3::new(1.0, 1.0, 0.0));
}

#[test]
fn form_is_part_of_curve_equality() {
    let a = admit(
        CurveForm::Open,
        3,
        &[[0.0; 3], [1.0; 3], [2.0; 3], [3.0; 3]],
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    );
    let mut b = a.clone();
    b.form = CurveForm::Closed;
    assert_ne!(a, b);
}

type Case = (CurveForm, u32, Vec<[f64; 3]>, Vec<f64>);

#[test]
fn undocumented_closed_and_periodic_shapes_refuse_as_unsupported_view() {
    let open_pts = [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 1.0, 0.0], [3.0, 0.0, 0.0]];
    let clamped = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let loop7 = loop_cvs(&LOOP, 7);
    let mut nonuniform = ints(0..9);
    nonuniform[4] = 4.5;
    let mut gap = loop7.clone();
    gap[6] = [9.0, 9.0, 9.0];
    let cases: Vec<Case> = vec![
        (CurveForm::Closed, 3, open_pts.to_vec(), clamped.to_vec()),
        (CurveForm::Periodic, 3, loop7.clone(), nonuniform),
        (CurveForm::Periodic, 3, gap, ints(0..9)),
        (CurveForm::Periodic, 3, open_pts.to_vec(), clamped.to_vec()),
        (CurveForm::Periodic, 2, loop_cvs(&LOOP, 6), ints(0..7)),
        (CurveForm::Periodic, 1, loop_cvs(&LOOP, 5), ints(0..5)),
        // spans < 3 with the overlap satisfied, so only the span rule refuses
        (CurveForm::Periodic, 3, loop_cvs(&LOOP[..2], 5), ints(0..7)),
        // unclamped closed curve
        (CurveForm::Closed, 3, vec![[1.0; 3]; 5], ints(0..7)),
        // clamped knots under form 2 with the overlap satisfied
        (
            CurveForm::Periodic,
            3,
            loop_cvs(&LOOP, 7),
            vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 4.0, 4.0],
        ),
    ];
    for (form, degree, points, knots) in cases {
        assert!(
            matches!(
                refusal(form, degree, &points, &knots),
                NativeError::UnsupportedView(_)
            ),
            "{form:?} {degree} {points:?} {knots:?}"
        );
    }
}

#[test]
fn periodic_overlap_compares_values_and_structure_errors_stay_distinct() {
    let mut points = loop_cvs(&LOOP, 7);
    points[0][0] = -0.0;
    points[4][0] = 0.0;
    admit(CurveForm::Periodic, 3, &points, &ints(0..9));
    let loop7 = loop_cvs(&LOOP, 7);
    let mut nan = ints(0..9);
    nan[4] = f64::NAN;
    assert!(matches!(
        refusal(CurveForm::Periodic, 3, &loop7, &nan),
        NativeError::NonFinite
    ));
    let mut down = ints(0..9);
    down[5] = 3.0;
    assert!(matches!(
        refusal(CurveForm::Periodic, 3, &loop7, &down),
        NativeError::InvalidCurve
    ));
    assert!(matches!(
        refusal(CurveForm::Periodic, 3, &loop7, &[0.0; 9]),
        NativeError::InvalidCurve
    ));
    assert!(matches!(
        refusal(CurveForm::Periodic, 3, &loop7, &ints(0..8)),
        NativeError::InvalidCurve
    ));
}
