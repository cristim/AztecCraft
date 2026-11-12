use super::*;
use crate::native_scene::{NativeScene, ascii};
use std::fmt::Write;
use std::sync::Arc;

fn scene(points: [[f64; 3]; 4]) -> NativeScene {
    let values = points
        .into_iter()
        .flatten()
        .map(|x| x.to_string())
        .collect::<Vec<_>>()
        .join(" ");
    ascii::decode(Arc::from(format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"curve\"; createNode nurbsCurve -n \"shape\" -p \"curve\"; setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 {values};"
    ).into_bytes())).unwrap()
}

fn prepared(points: [[f64; 3]; 4], world: DMat4) -> PreparedCurve {
    prepare(
        &scene(points).curves()[0],
        &world,
        16 * 1024 * 1024,
        &mut CurveWork::default(),
    )
    .unwrap()
}

fn output_distance(p: DVec3, result: &PreparedCurve) -> f64 {
    result
        .segments
        .iter()
        .map(|[a, b]| {
            let a = result.origin + a.as_dvec3();
            let v = result.origin + b.as_dvec3() - a;
            let t = if v.length_squared() == 0.0 {
                0.0
            } else {
                ((p - a).dot(v) / v.length_squared()).clamp(0.0, 1.0)
            };
            (p - (a + v * t)).length()
        })
        .fold(f64::INFINITY, f64::min)
}

fn assert_bound(points: [[f64; 3]; 4], world: DMat4, result: &PreparedCurve) {
    let points = points.map(DVec3::from_array);
    let transformed = points.map(|p| world.transform_point3(p));
    let low = transformed
        .into_iter()
        .fold(DVec3::splat(f64::INFINITY), DVec3::min);
    let high = transformed
        .into_iter()
        .fold(DVec3::splat(f64::NEG_INFINITY), DVec3::max);
    let tolerance = ((high - low).length() * 1e-4).max(1e-6);
    assert_eq!(result.bounds, [low, high]);
    for step in 0..=4096 {
        let t = step as f64 / 4096.0;
        let s = 1.0 - t;
        let p = points[0] * (s * s * s)
            + points[1] * (3.0 * s * s * t)
            + points[2] * (3.0 * s * t * t)
            + points[3] * (t * t * t);
        assert!(output_distance(world.transform_point3(p), result) <= tolerance * (1.0 + 1e-8));
    }
    let end_error =
        |segment: Vec3, expected: DVec3| (result.origin + segment.as_dvec3() - expected).length();
    assert!(end_error(result.segments.first().unwrap()[0], transformed[0]) <= tolerance);
    assert!(end_error(result.segments.last().unwrap()[1], transformed[3]) <= tolerance);
    assert!(
        result
            .segments
            .windows(2)
            .all(|pair| pair[0][1] == pair[1][0])
    );
}

#[test]
fn actual_output_bounds_s_curve_zero_chord_and_collinear_backtracking() {
    for points in [
        [
            [0.0, 0.0, 0.0],
            [1.0, 3.0, 0.0],
            [2.0, -3.0, 0.0],
            [3.0, 0.0, 0.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [2.0, 3.0, 0.0],
            [-2.0, 3.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [-4.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
    ] {
        let result = prepared(points, DMat4::IDENTITY);
        assert!(result.segments.len() > 1);
        assert_bound(points, DMat4::IDENTITY, &result);
    }
    let points = [[2.0, -1.0, 3.0]; 4];
    let result = prepared(points, DMat4::IDENTITY);
    assert_eq!(result.origin, DVec3::new(2.0, -1.0, 3.0));
    assert_eq!(result.segments, [[Vec3::ZERO; 2]]);
}

#[test]
fn world_space_bound_survives_reflection_large_and_small_scale() {
    let points = [
        [0.0, 0.0, 0.0],
        [1.0, 3.0, 2.0],
        [2.0, -3.0, -1.0],
        [3.0, 0.0, 0.0],
    ];
    for world in [
        DMat4::from_scale(DVec3::new(-2.0, 3.0, 0.5)),
        DMat4::from_scale(DVec3::splat(1e6)),
        DMat4::from_scale(DVec3::splat(1e-8)),
    ] {
        let result = prepared(points, world);
        assert_bound(points, world, &result);
    }
}

#[test]
fn actual_f32_endpoint_quantization_cannot_spend_extra_error() {
    let points = [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ];
    let source = scene(points);
    let world = DMat4::from_translation(DVec3::new(1e8, 0.0, 0.0));
    assert_bound(points, world, &prepared(points, world));
    let world = DMat4::from_translation(DVec3::new(1e15, 0.0, 0.0));
    assert!(matches!(
        prepare(
            &source.curves()[0],
            &world,
            usize::MAX,
            &mut CurveWork::default()
        ),
        Err(ViewError::NonFiniteWorld)
    ));
    let world = DMat4::from_translation(DVec3::new(128.0, 0.0, 0.0));
    assert_bound(points, world, &prepared(points, world));
    let tiny = [
        [0.0, 0.0, 0.0],
        [1e-9, 0.0, 0.0],
        [2e-9, 0.0, 0.0],
        [3e-9, 0.0, 0.0],
    ];
    let world = DMat4::from_translation(DVec3::X);
    let result = prepared(tiny, world);
    assert_eq!(result.segments.len(), 1);
    // Offsets from the f64 origin keep the 3e-9 span that f32 world coordinates at x = 1 lost.
    assert!((result.segments[0][1] - result.segments[0][0]).length() > 2e-9);
    assert_bound(tiny, world, &result);
}

#[test]
fn exact_capacity_budget_and_shared_work_refuse_without_partial_result() {
    let source = scene([
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
    ]);
    let required = size_of::<PreparedCurve>()
        + size_of::<[Option<Piece>; MAX_DEPTH + 1]>()
        + size_of::<[Vec3; 2]>();
    let curve = &source.curves()[0];
    assert_eq!(
        prepare(curve, &DMat4::IDENTITY, required, &mut CurveWork::default())
            .unwrap()
            .segments
            .len(),
        1
    );
    assert!(matches!(
        prepare(
            curve,
            &DMat4::IDENTITY,
            required - 1,
            &mut CurveWork::default()
        ),
        Err(ViewError::ResourceLimit)
    ));
    let mut work = CurveWork(MAX_WORK - 8);
    prepare(curve, &DMat4::IDENTITY, required, &mut work).unwrap();
    assert_eq!(work.0, MAX_WORK);
    assert!(matches!(
        prepare(curve, &DMat4::IDENTITY, required, &mut work),
        Err(ViewError::ResourceLimit)
    ));
}

#[test]
fn finite_intermediate_overflow_and_invalid_projection_refuse() {
    let source = scene([
        [0.0, 0.0, 0.0],
        [1.0, 2.0, 0.0],
        [2.0, -1.0, 0.0],
        [3.0, 0.0, 0.0],
    ]);
    let world = DMat4::from_scale(DVec3::splat(f64::MAX));
    assert!(matches!(
        prepare(
            &source.curves()[0],
            &world,
            usize::MAX,
            &mut CurveWork::default()
        ),
        Err(ViewError::NonFiniteWorld)
    ));
    assert!(matches!(
        distance_squared(DVec3::ZERO, DVec3::ZERO, DVec3::splat(1e-200)),
        Err(ViewError::NonFiniteWorld)
    ));
    assert_eq!(
        distance_squared(DVec3::new(2.0, 0.0, 0.0), DVec3::ZERO, DVec3::X).unwrap(),
        1.0
    );
    assert_eq!(
        distance_squared(DVec3::Y, DVec3::ZERO, DVec3::ZERO).unwrap(),
        1.0
    );
}

#[test]
fn real_many_span_curve_accepts_65536_segments_and_refuses_65537() {
    for spans in [65_536usize, 65_537] {
        let cvs = spans + 3;
        let knots = cvs + 2;
        let mut source = format!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"curve\"; createNode nurbsCurve -n \"shape\" -p \"curve\"; setAttr \".cc\" -type \"nurbsCurve\" 3 {spans} 0 no 3 {knots} 0 0 0"
        );
        for knot in 1..spans {
            write!(source, " {knot}").unwrap();
        }
        write!(source, " {spans} {spans} {spans} {cvs}").unwrap();
        source.push_str(&" 1 2 3".repeat(cvs));
        source.push(';');
        let scene = ascii::decode(Arc::from(source.into_bytes())).unwrap();
        let mut work = CurveWork::default();
        let result = prepare(
            &scene.curves()[0],
            &DMat4::IDENTITY,
            16 * 1024 * 1024,
            &mut work,
        );
        if spans == 65_536 {
            let result = result.unwrap();
            assert_eq!(result.segments.len(), 65_536);
            assert!(
                result
                    .segments
                    .iter()
                    .all(|segment| *segment == [Vec3::ZERO; 2])
            );
        } else {
            assert!(matches!(result, Err(ViewError::ResourceLimit)));
        }
        assert!(work.0 < MAX_WORK);
    }
}

#[test]
fn small_curve_far_from_origin_is_drawn_until_f64_grid_exceeds_one_percent_of_curve() {
    // ~3.6 cm specimen (CV hull diagonal ~8.8 cm). With f32 world segments it was drawn at
    // x = 2e6 and refused at x = 3e6; offsets from an f64 origin keep it exact.
    let points = [
        [0.0, 0.0, 0.0],
        [1.0, 2.0, 3.0],
        [4.0, -3.0, 2.0],
        [6.0, 0.0, -1.0],
    ];
    let source = scene(points);
    let run = |x: f64| {
        let world = DMat4::from_translation(DVec3::new(x, 0.0, 0.0));
        prepare(
            &source.curves()[0],
            &world,
            usize::MAX,
            &mut CurveWork::default(),
        )
    };
    for x in [
        2000.3,
        1e6,
        3e6,
        3e6 + 0.37,
        1e7 + 0.37,
        1e8 + 0.37,
        -1e9 + 0.37,
    ] {
        let world = DMat4::from_translation(DVec3::new(x, 0.0, 0.0));
        let result = run(x).unwrap();
        assert!(result.segments.len() > 1, "{x}");
        assert_bound(points, world, &result);
        for [a, b] in &result.segments {
            assert!(
                a.abs().max_element() < 10.0 && b.abs().max_element() < 10.0,
                "{x}"
            );
        }
    }
    // The f64 grid at 1e15 cm is 0.125 cm: coarser than 1% of the 8.8 cm curve.
    for x in [1e15, 1e15 + 0.37] {
        assert!(matches!(run(x), Err(ViewError::NonFiniteWorld)), "{x}");
    }
    let non_finite = DMat4::from_translation(DVec3::new(f64::NAN, 0.0, 0.0));
    assert!(matches!(
        prepare(
            &source.curves()[0],
            &non_finite,
            usize::MAX,
            &mut CurveWork::default()
        ),
        Err(ViewError::NonFiniteWorld)
    ));
}
