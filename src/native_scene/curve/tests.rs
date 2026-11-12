use super::*;
use crate::{native_scene::*, scene::Primitive};
use glam::DVec3;
use std::sync::Arc;

fn candidate() -> (NativeScene, crate::scene::NodeId) {
    let (mut scene, node) = crate::native_scene::test_fixtures::cube();
    let id = scene.shapes[0].id;
    scene.shapes.clear();
    let bytes: Arc<[u8]> = Arc::from(&b"setAttr geometry;"[..]);
    let end = bytes.len();
    scene.source = Some(SourceRetention {
        has_removals: false,
        sites: Vec::new(),
        records: std::iter::once(0..bytes.len()).collect(),
        bytes,
        format: SourceFormat::Ascii,
        leaves: Vec::new(),
        appearance: None,
    });
    scene.curves.push(NativeCurveShape {
        id,
        uuid: Some(Uuid::nil()),
        name: "curveShape".into(),
        degree: 3,
        control_vertices: vec![
            [0.0, -0.0, 0.0],
            [1.0, 2.0, 0.0],
            [2.0, -1.0, 1.0],
            [3.0, 0.0, 2.0],
        ],
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        source: CurveSource {
            owner: id,
            statement: 0..end,
        },
        view: GeometryViewFacts::authored(),
    });
    scene.validate().unwrap();
    (scene, node)
}

#[test]
fn canonical_curve_is_not_a_mesh_and_never_silently_projects_or_encodes() {
    let (scene, node) = candidate();
    let curve = &scene.curves()[0];
    assert_eq!(curve.degree(), 3);
    assert_eq!(curve.form(), CurveForm::Open);
    assert_eq!(curve.domain().unwrap(), [0.0, 1.0]);
    assert_eq!(curve.control_vertices().len(), 4);
    assert_eq!(curve.knots().len(), 6);
    assert_eq!(curve.name(), "curveShape");
    assert_eq!(curve.uuid(), Some(Uuid::nil()));
    assert!(scene.shape(curve.id()).is_err());
    assert!(scene.shapes().is_empty());
    assert!(matches!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView(_))
    ));
    assert_eq!(
        binary::encode(&scene).unwrap_err().kind(),
        binary::ErrorKind::Unsupported
    );
    assert!(
        scene
            .resolve_face_target(&FaceTarget {
                transform: node,
                shape: curve.id(),
                path: vec![scene.nodes[0].transform_id, curve.id()],
                face_index: 0,
            })
            .is_err()
    );
}

#[test]
fn malformed_curve_counts_knots_and_nonfinite_values_refuse() {
    let (original, _) = candidate();
    for knots in [
        vec![],
        vec![0.0; 6],
        vec![0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
        vec![0.0, 0.0, 0.0, -1.0, -1.0, -1.0],
        vec![
            -f64::MAX,
            -f64::MAX,
            -f64::MAX,
            f64::MAX,
            f64::MAX,
            f64::MAX,
        ],
    ] {
        let mut scene = original.clone();
        scene.curves[0].knots = knots;
        assert_eq!(scene.validate(), Err(NativeError::InvalidCurve));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut scene = original.clone();
        scene.curves[0].control_vertices[0][0] = value;
        assert_eq!(scene.validate(), Err(NativeError::NonFinite));
        let mut scene = original.clone();
        scene.curves[0].knots[0] = value;
        assert_eq!(scene.validate(), Err(NativeError::NonFinite));
    }
    let mut scene = original.clone();
    scene.curves[0].control_vertices.pop();
    assert_eq!(scene.validate(), Err(NativeError::InvalidCurve));
    let mut scene = original;
    scene.curves[0]
        .control_vertices
        .extend([[4.0; 3], [5.0; 3]]);
    scene.curves[0].knots = vec![0.0, 0.0, 0.0, 0.25, 0.75, 1.0, 1.0, 1.0];
    scene.validate().unwrap();
    scene.curves[0].knots[4] = 0.25;
    assert!(matches!(
        scene.validate(),
        Err(NativeError::UnsupportedView(_))
    ));
    scene.curves[0].knots[4] = 0.1;
    assert_eq!(scene.validate(), Err(NativeError::InvalidCurve));
}

#[test]
fn curve_ids_parents_and_source_owner_ranges_are_checked() {
    let (original, _) = candidate();
    for id in [
        NativeId(0),
        original.nodes[0].transform_id,
        NativeId(original.next_native_id),
    ] {
        let mut scene = original.clone();
        scene.curves[0].id = id;
        assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
    }
    let mut scene = original.clone();
    scene.curves.push(scene.curves[0].clone());
    assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
    for edge in [
        DagEdge {
            parent: None,
            child: original.curves[0].id,
        },
        DagEdge {
            parent: Some(original.curves[0].id),
            child: original.nodes[0].transform_id,
        },
        DagEdge {
            parent: Some(NativeId(999)),
            child: original.curves[0].id,
        },
        DagEdge {
            parent: Some(original.nodes[0].transform_id),
            child: original.nodes[0].transform_id,
        },
    ] {
        let mut scene = original.clone();
        scene.dag.push(edge);
        assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
    }
    let mut scene = original.clone();
    scene.dag.retain(|e| e.child != scene.curves[0].id);
    assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
    for range in [0..0, 1..4, 0..usize::MAX, Range { start: 5, end: 4 }] {
        let mut scene = original.clone();
        scene.curves[0].source.statement = range;
        assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    }
    let mut scene = original.clone();
    scene.curves[0].source.owner = scene.nodes[0].transform_id;
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    let mut scene = original.clone();
    scene.source = None;
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    let mut scene = original;
    scene.source.as_mut().unwrap().format = SourceFormat::Binary;
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
}

#[test]
fn all_ascii_authoring_refuses_and_preserves_exact_curve_source() {
    let (mut scene, node) = candidate();
    let before = scene.clone();
    let id = scene.curves[0].id;
    assert_eq!(
        scene.create(Primitive::Cube, "new"),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(scene.rename(node, "new"), Err(NativeError::ReadOnlyFormat));
    assert_eq!(scene.remove(node), Err(NativeError::ReadOnlyFormat));
    assert_eq!(
        scene.set_channel(node, Channel::TranslateX, 2.0),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.set_transform(node, NativeTransform::default()),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.edit_node(node, None, [None; 9]),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.extrude_face(id, 0, 1.0),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(scene, before);
    assert_eq!(
        (scene.next_id, scene.next_native_id),
        (before.next_id, before.next_native_id)
    );
    assert!(Arc::ptr_eq(
        &scene.source.as_ref().unwrap().bytes,
        &before.source.as_ref().unwrap().bytes
    ));
    let mut history = crate::history::History::new(scene);
    assert_eq!(
        history.execute(crate::history::Command::SetTransform {
            id: node,
            transform: NativeTransform::default(),
        }),
        Err(NativeError::ReadOnlyFormat)
    );
    assert!(!history.can_undo());
    assert!(!history.can_redo());
    assert_eq!(history.scene(), &before);
}

#[test]
fn signed_zero_curve_bits_and_retention_survive_authoring_restore() {
    let (mut scene, _) = candidate();
    let before = scene.clone();
    scene.curves[0].control_vertices[0][1] = 0.0;
    assert_ne!(scene, before);
    scene.restore_authoring(before.clone());
    assert_eq!(scene, before);
    scene.curves[0].knots[0] = -0.0;
    scene.validate().unwrap();
    assert_ne!(scene, before);
    scene.next_native_id = 100;
    scene.restore_authoring(before.clone());
    assert_eq!(scene, before);
    assert_eq!(scene.next_native_id, 100);
    assert!(Arc::ptr_eq(
        &scene.source.as_ref().unwrap().bytes,
        &before.source.as_ref().unwrap().bytes
    ));
}

fn spline(points: &[[f64; 3]], knots: &[f64]) -> NativeCurveShape {
    let (mut scene, _) = candidate();
    scene.curves[0].control_vertices = points.to_vec();
    scene.curves[0].knots = knots.to_vec();
    scene.validate().unwrap();
    scene.curves.remove(0)
}

fn expected(points: &[[f64; 3]], weights: &[f64], denominator: f64) -> DVec3 {
    points
        .iter()
        .zip(weights)
        .fold(DVec3::ZERO, |sum, (point, weight)| {
            sum + DVec3::from_array(*point) * *weight
        })
        / denominator
}

fn close(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).length() < 2e-12,
        "actual {actual:?}, expected {expected:?}"
    );
}

fn span_middle(curve: &NativeCurveShape, span: usize) -> DVec3 {
    let controls = curve
        .bezier_span(span)
        .unwrap()
        .map(|point| point.to_array());
    expected(&controls, &[1.0, 3.0, 3.0, 1.0], 8.0)
}

#[test]
fn uniform_cubic_matches_independent_rational_basis_examples() {
    let points = [
        [0.0, 1.0, 2.0],
        [1.0, 3.0, -2.0],
        [2.0, -4.0, 1.0],
        [4.0, 2.0, 3.0],
        [5.0, 7.0, -3.0],
        [8.0, -1.0, 2.0],
        [9.0, 4.0, 5.0],
        [12.0, -3.0, 1.0],
    ];
    let curve = spline(&points, &[0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 5.0, 5.0]);
    close(curve.evaluate(0.0).unwrap(), points[0].into());
    close(curve.evaluate(5.0).unwrap(), points[7].into());
    close(
        curve.evaluate(0.5).unwrap(),
        expected(&points, &[12.0, 57.0, 25.0, 2.0], 96.0),
    );
    close(
        curve.evaluate(2.5).unwrap(),
        expected(&points, &[0.0, 0.0, 1.0, 23.0, 23.0, 1.0], 48.0),
    );
    close(
        curve.evaluate(4.5).unwrap(),
        expected(&points, &[0.0, 0.0, 0.0, 0.0, 2.0, 25.0, 57.0, 12.0], 96.0),
    );
    close(
        span_middle(&curve, 0),
        expected(&points, &[12.0, 57.0, 25.0, 2.0], 96.0),
    );
    close(
        span_middle(&curve, 2),
        expected(&points, &[0.0, 0.0, 1.0, 23.0, 23.0, 1.0], 48.0),
    );
    close(
        span_middle(&curve, 4),
        expected(&points, &[0.0, 0.0, 0.0, 0.0, 2.0, 25.0, 57.0, 12.0], 96.0),
    );
}

#[test]
fn nonuniform_cubic_matches_independent_rational_basis_and_one_sided_derivatives() {
    let points = [
        [0.0, 1.0, 2.0],
        [1.0, 3.0, -2.0],
        [2.0, -4.0, 1.0],
        [4.0, 2.0, 3.0],
        [5.0, 7.0, -3.0],
    ];
    let curve = spline(&points, &[0.0, 0.0, 0.0, 1.0, 3.0, 3.0, 3.0]);
    close(curve.evaluate(0.0).unwrap(), points[0].into());
    close(curve.evaluate(3.0).unwrap(), points[4].into());
    close(
        curve.evaluate(0.5).unwrap(),
        expected(&points, &[9.0, 49.0, 13.0, 1.0], 72.0),
    );
    close(
        curve.evaluate(1.0).unwrap(),
        expected(&points, &[0.0, 4.0, 4.0, 1.0], 9.0),
    );
    close(
        curve.evaluate(2.0).unwrap(),
        expected(&points, &[0.0, 4.0, 22.0, 37.0, 9.0], 72.0),
    );
    close(
        span_middle(&curve, 0),
        expected(&points, &[9.0, 49.0, 13.0, 1.0], 72.0),
    );
    close(
        span_middle(&curve, 1),
        expected(&points, &[0.0, 4.0, 22.0, 37.0, 9.0], 72.0),
    );
    close(
        curve.de_boor(3, 0.0, true).unwrap(),
        (DVec3::from(points[1]) - DVec3::from(points[0])) * 3.0,
    );
    close(
        curve.de_boor(4, 3.0, true).unwrap(),
        (DVec3::from(points[4]) - DVec3::from(points[3])) * 1.5,
    );
    close(
        curve.de_boor(3, 1.0, true).unwrap(),
        curve.de_boor(4, 1.0, true).unwrap(),
    );
}

#[test]
fn exact_span_bezier_controls_reconstruct_analytic_cubics() {
    let points = [
        [0.0, 0.0, 1.0],
        [1.0, 3.0, -2.0],
        [2.0, -3.0, 2.0],
        [3.0, 0.0, -1.0],
    ];
    let curve = spline(&points, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
    let bezier = curve.bezier_span(0).unwrap();
    for (actual, point) in bezier.into_iter().zip(points) {
        close(actual, point.into());
    }
    for t in [0.0, 0.125, 0.25, 0.5, 0.75, 0.875, 1.0] {
        let s: f64 = 1.0 - t;
        let oracle = expected(
            &points,
            &[s.powi(3), 3.0 * s * s * t, 3.0 * s * t * t, t * t * t],
            1.0,
        );
        close(curve.evaluate(t).unwrap(), oracle);
    }
    assert!(curve.bezier_span(1).is_err());
    assert!(curve.bezier_span(usize::MAX).is_err());
    for t in [f64::NAN, f64::INFINITY, -1.0, 2.0] {
        assert!(curve.evaluate(t).is_err());
    }
}

#[test]
fn finite_canonical_words_refuse_nonfinite_evaluation_intermediates() {
    let curve = spline(
        &[
            [-f64::MAX, 0.0, 0.0],
            [f64::MAX, 1.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 3.0, 0.0],
        ],
        &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
    );
    assert!(matches!(
        curve.evaluate(0.5),
        Err(NativeError::UnsupportedView(_))
    ));
    assert!(matches!(
        curve.bezier_span(0),
        Err(NativeError::UnsupportedView(_))
    ));
}

fn spline_degree(degree: u32, points: &[[f64; 3]], knots: &[f64]) -> NativeCurveShape {
    let (mut scene, _) = candidate();
    scene.curves[0].degree = degree;
    scene.curves[0].control_vertices = points.to_vec();
    scene.curves[0].knots = knots.to_vec();
    scene.validate().unwrap();
    scene.curves.remove(0)
}

fn basis(full: &[f64], i: usize, d: usize, u: f64, last_span: usize) -> f64 {
    if d == 0 {
        let inside = full[i] <= u && u < full[i + 1];
        return f64::from(u8::from(
            inside || (u == full[full.len() - 1] && i == last_span),
        ));
    }
    let mut value = 0.0;
    if full[i + d] > full[i] {
        value += (u - full[i]) / (full[i + d] - full[i]) * basis(full, i, d - 1, u, last_span);
    }
    if full[i + d + 1] > full[i + 1] {
        value += (full[i + d + 1] - u) / (full[i + d + 1] - full[i + 1])
            * basis(full, i + 1, d - 1, u, last_span);
    }
    value
}

/// Independent Cox-de Boor oracle over the padded knot vector.
fn oracle(degree: usize, points: &[[f64; 3]], knots: &[f64], u: f64) -> DVec3 {
    let mut full = vec![knots[0]];
    full.extend_from_slice(knots);
    full.push(knots[knots.len() - 1]);
    let last_span = points.len() - 1;
    points.iter().enumerate().fold(DVec3::ZERO, |sum, (i, p)| {
        sum + DVec3::from_array(*p) * basis(&full, i, degree, u, last_span)
    })
}

fn cubic(controls: [DVec3; 4], t: f64) -> DVec3 {
    let s = 1.0 - t;
    controls[0] * (s * s * s)
        + controls[1] * (3.0 * s * s * t)
        + controls[2] * (3.0 * s * t * t)
        + controls[3] * (t * t * t)
}

fn assert_matches_oracle(degree: usize, points: &[[f64; 3]], knots: &[f64]) {
    let curve = spline_degree(u32::try_from(degree).unwrap(), points, knots);
    let [start, end] = curve.domain().unwrap();
    assert_eq!(
        [start, end],
        [knots[degree - 1], knots[knots.len() - degree]]
    );
    for step in 0..=20 {
        let u = start + (end - start) * f64::from(step) / 20.0;
        close(curve.evaluate(u).unwrap(), oracle(degree, points, knots, u));
    }
    for span in 0..points.len() - degree {
        let controls = curve.bezier_span(span).unwrap();
        let a = knots[degree + span - 1];
        let b = knots[degree + span];
        for step in 0..=8 {
            let t = f64::from(step) / 8.0;
            close(
                cubic(controls, t),
                oracle(degree, points, knots, a + (b - a) * t),
            );
        }
    }
    assert!(curve.bezier_span(points.len() - degree).is_err());
}

const POINTS: [[f64; 3]; 5] = [
    [0.0, 1.0, 2.0],
    [1.0, 3.0, -2.0],
    [2.0, -4.0, 1.0],
    [4.0, 2.0, 3.0],
    [5.0, 7.0, -3.0],
];

#[test]
fn degree_one_polyline_matches_the_cox_de_boor_oracle_and_segment_midpoints() {
    let knots = [0.0, 1.0, 2.5, 3.0, 7.0];
    assert_matches_oracle(1, &POINTS, &knots);
    let curve = spline_degree(1, &POINTS, &knots);
    close(
        curve.evaluate(1.75).unwrap(),
        (DVec3::from_array(POINTS[1]) + DVec3::from_array(POINTS[2])) / 2.0,
    );
}

#[test]
fn degree_two_uniform_and_nonuniform_match_the_cox_de_boor_oracle() {
    assert_matches_oracle(2, &POINTS, &[0.0, 0.0, 1.0, 2.0, 3.0, 3.0]);
    assert_matches_oracle(2, &POINTS, &[0.0, 0.0, 1.0, 3.5, 4.0, 4.0]);
}

#[test]
fn degree_three_oracle_agrees_with_the_existing_evaluator() {
    assert_matches_oracle(3, &POINTS, &[0.0, 0.0, 0.0, 1.0, 2.0, 2.0, 2.0]);
}

#[test]
fn clamp_and_count_rules_follow_the_degree() {
    let (mut scene, _) = candidate();
    let mut refused = |degree: u32, points: &[[f64; 3]], knots: &[f64]| {
        scene.curves[0].degree = degree;
        scene.curves[0].control_vertices = points.to_vec();
        scene.curves[0].knots = knots.to_vec();
        scene.validate().is_err()
    };
    // Degree 2 needs exactly two clamp knots on each side, not three.
    assert!(refused(2, &POINTS, &[0.0, 1.0, 2.0, 3.0, 4.0, 4.0]));
    assert!(refused(2, &POINTS, &[0.0, 0.0, 1.0, 2.0, 3.0, 4.0]));
    assert!(!refused(2, &POINTS, &[0.0, 0.0, 1.0, 2.0, 3.0, 3.0]));
    // Knot count is cv + degree - 1.
    assert!(refused(2, &POINTS, &[0.0, 0.0, 1.0, 2.0, 2.0]));
    assert!(refused(1, &POINTS, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]));
    // Too few CVs, degree 0 and degree 4.
    assert!(refused(2, &POINTS[..2], &[0.0, 0.0, 1.0]));
    assert!(refused(1, &POINTS[..1], &[0.0]));
    assert!(refused(0, &POINTS, &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]));
    assert!(refused(
        4,
        &POINTS,
        &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]
    ));
    // Repeated interior knots stay unsupported at every degree.
    assert!(refused(1, &POINTS, &[0.0, 1.0, 1.0, 2.0, 3.0]));
    assert!(refused(2, &POINTS, &[0.0, 0.0, 1.0, 1.0, 2.0, 2.0]));
}
