use super::*;
use glam::Vec3;

fn front_camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 10.0, 90_f32.to_radians(), 0.1, 100.0).unwrap()
}

fn capture(camera: &Camera, scale: f32) -> ViewportCapture {
    ViewportCapture::new(camera, [37, 53], [800, 600], scale).unwrap()
}

fn pointer(capture: &ViewportCapture, physical: DVec2) -> Vec2 {
    capture.logical_min + physical.as_vec2() / capture.pixels_per_point
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} within {tolerance}"
    );
}

#[test]
fn front_axes_share_exact_physical_drawing_and_hit_geometry() {
    let capture = capture(&front_camera(), 1.0);
    let handles = WorldHandles::new(&capture, DVec3::ZERO).unwrap();
    near(handles.origin.x, 400.0, 1e-5);
    near(handles.origin.y, 300.0, 1e-5);
    let x = handles.axes()[0].arrow().unwrap();
    let shaft = x.segments().next().unwrap();
    near(shaft.start.x, 400.0, 1e-5);
    near(shaft.end.x, 472.0, 1e-5);
    near(shaft.end.y, 300.0, 1e-5);
    let y = handles.axes()[1].arrow().unwrap();
    near(y.segments().next().unwrap().end.y, 228.0, 1e-5);
    assert_eq!(
        handles.axes()[2].unavailable_reason(),
        Some(HandleError::ParallelAxis)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(436.0, 300.0)),
        Some(WorldAxis::X)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(400.0, 264.0)),
        Some(WorldAxis::Y)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(436.0, 306.0)),
        Some(WorldAxis::X)
    );
    assert_eq!(handles.hit_test(DVec2::new(436.0, 306.001)), None);
    assert_eq!(handles.hit_test(DVec2::new(410.0, 300.0)), None);
    assert_eq!(
        handles.hit_test(DVec2::new(410.001, 300.0)),
        Some(WorldAxis::X)
    );
    assert_eq!(handles.hit_test(DVec2::new(400.0, 300.0)), None);
    assert_eq!(handles.hit_test(DVec2::new(f64::NAN, 300.0)), None);
    for segment in x.segments().skip(1) {
        assert_eq!(
            handles.hit_test((segment.start + segment.end) * 0.5),
            Some(WorldAxis::X)
        );
    }
}

#[test]
fn coincident_projected_axes_break_exact_hit_ties_in_xyz_order() {
    let camera = Camera::new(
        Vec3::ZERO,
        -std::f32::consts::FRAC_PI_4,
        0.0,
        10.0,
        90_f32.to_radians(),
        0.1,
        100.0,
    )
    .unwrap();
    let capture = capture(&camera, 1.0);
    let handles = WorldHandles::new(&capture, DVec3::ZERO).unwrap();
    assert!(handles.axes()[0].arrow().is_some());
    assert!(handles.axes()[2].arrow().is_some());
    assert_eq!(
        handles.hit_test(handles.origin + DVec2::new(36.0, 0.0)),
        Some(WorldAxis::X)
    );
}

#[test]
fn dpi_changes_logical_size_but_not_seventy_two_physical_pixel_arrows() {
    let camera = front_camera();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        let capture = capture(&camera, scale);
        assert_eq!(capture.aspect.to_bits(), (800_f32 / 600_f32).to_bits());
        let handles = WorldHandles::new(&capture, DVec3::ZERO).unwrap();
        let shaft = handles.axes()[0]
            .arrow()
            .unwrap()
            .segments()
            .next()
            .unwrap();
        near((shaft.end - shaft.start).length(), 72.0, 1e-10);
        let physical = DVec2::new(436.0, 300.0);
        let mapped = capture.pointer_pixel(pointer(&capture, physical)).unwrap();
        near(mapped.x, physical.x, 0.0001);
        near(mapped.y, physical.y, 0.0001);
        assert_eq!(handles.hit_test(mapped), Some(WorldAxis::X));
    }
}

#[test]
fn clipped_shafts_and_arrowheads_never_draw_or_hit_outside_the_viewport() {
    let capture = capture(&front_camera(), 1.0);
    let handles = WorldHandles::new(&capture, DVec3::new(12.666666666666666, 0.0, 0.0)).unwrap();
    let x = handles.axes()[0].arrow().unwrap();
    let shaft = x.segments().next().unwrap();
    near(shaft.start.x, 780.0, 0.001);
    near(shaft.end.x, 800.0, 1e-10);
    assert_eq!(
        handles.hit_test(DVec2::new(795.0, 300.0)),
        Some(WorldAxis::X)
    );
    assert_eq!(handles.hit_test(DVec2::new(800.001, 300.0)), None);
    for axis in handles.axes() {
        if let Some(arrow) = axis.arrow() {
            for segment in arrow.segments() {
                assert!(capture.contains(segment.start));
                assert!(capture.contains(segment.end));
            }
        }
    }
    for origin in [
        DVec3::new(100.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 9.95),
        DVec3::new(0.0, 0.0, -100.0),
    ] {
        assert!(matches!(
            WorldHandles::new(&capture, origin),
            Err(HandleError::OriginClipped)
        ));
    }
    assert!(matches!(
        WorldHandles::new(&capture, DVec3::splat(f64::INFINITY)),
        Err(HandleError::InvalidProjection)
    ));
}

#[test]
fn axis_drags_solve_independent_world_distances_in_both_directions() {
    for (axis, yaw, begin_pixel, positive_pixel, negative_pixel) in [
        (
            WorldAxis::X,
            0.0,
            [430.0, 300.0],
            [490.0, 300.0],
            [340.0, 300.0],
        ),
        (
            WorldAxis::Y,
            0.0,
            [400.0, 270.0],
            [400.0, 210.0],
            [400.0, 360.0],
        ),
        (
            WorldAxis::Z,
            std::f32::consts::FRAC_PI_2,
            [370.0, 300.0],
            [310.0, 300.0],
            [460.0, 300.0],
        ),
        (
            WorldAxis::X,
            std::f32::consts::PI,
            [370.0, 300.0],
            [310.0, 300.0],
            [460.0, 300.0],
        ),
    ] {
        let camera =
            Camera::new(Vec3::ZERO, yaw, 0.0, 10.0, 90_f32.to_radians(), 0.1, 100.0).unwrap();
        for scale in [1.0, 1.25, 2.0] {
            let capture = capture(&camera, scale);
            let drag = AxisDrag::begin(
                capture.clone(),
                DVec3::ZERO,
                axis,
                pointer(&capture, DVec2::from_array(begin_pixel)),
            )
            .unwrap();
            near(
                drag.distance_cm(
                    &capture,
                    pointer(&capture, DVec2::from_array(positive_pixel)),
                )
                .unwrap(),
                2.0,
                0.00002,
            );
            near(
                drag.distance_cm(
                    &capture,
                    pointer(&capture, DVec2::from_array(negative_pixel)),
                )
                .unwrap(),
                -3.0,
                0.00002,
            );
            near(
                drag.distance_cm(&capture, pointer(&capture, DVec2::from_array(begin_pixel)))
                    .unwrap(),
                0.0,
                1e-10,
            );
        }
    }
}

#[test]
fn off_center_depth_axis_uses_the_perspective_quotient_and_translated_line() {
    for scale in [1.0, 1.25, 2.0] {
        let capture = capture(&front_camera(), scale);
        let origin = DVec3::new(5.0, 1.0, 0.0);
        let handles = WorldHandles::new(&capture, origin).unwrap();
        near(handles.origin.x, 550.0, 0.001);
        near(handles.origin.y, 270.0, 0.001);
        let shaft = handles.axes()[2]
            .arrow()
            .unwrap()
            .segments()
            .next()
            .unwrap();
        near(shaft.end.x, 550.0 + 72.0 * 5.0 / 26_f64.sqrt(), 0.001);
        near(shaft.end.y, 270.0 - 72.0 / 26_f64.sqrt(), 0.001);
        let begin = DVec2::new(566.6666666666666, 266.6666666666667);
        assert_eq!(handles.hit_test(begin), Some(WorldAxis::Z));
        let drag = AxisDrag::begin(
            capture.clone(),
            origin,
            WorldAxis::Z,
            pointer(&capture, begin),
        )
        .unwrap();
        near(
            drag.distance_cm(
                &capture,
                pointer(&capture, DVec2::new(614.2857142857143, 257.14285714285717)),
            )
            .unwrap(),
            2.0,
            0.00005,
        );
        near(
            drag.distance_cm(&capture, pointer(&capture, DVec2::new(525.0, 275.0)))
                .unwrap(),
            -3.0,
            0.00005,
        );
    }
}

#[test]
fn capture_can_leave_the_viewport_without_clamping_or_changing_picking_rays() {
    let camera = front_camera();
    for ndc in [
        Vec2::ZERO,
        Vec2::new(-1.0, -1.0),
        Vec2::new(1.0, 1.0),
        Vec2::new(0.3, -0.7),
    ] {
        assert_eq!(
            camera.ray(ndc, 4.0 / 3.0),
            camera.captured_ray(ndc, 4.0 / 3.0)
        );
    }
    assert!(camera.ray(Vec2::new(2.0, 0.0), 4.0 / 3.0).is_err());
    assert!(camera.captured_ray(Vec2::new(2.0, 0.0), 4.0 / 3.0).is_ok());
    let capture = capture(&camera, 1.25);
    let drag = AxisDrag::begin(
        capture.clone(),
        DVec3::ZERO,
        WorldAxis::X,
        pointer(&capture, DVec2::new(430.0, 300.0)),
    )
    .unwrap();
    near(
        drag.distance_cm(&capture, pointer(&capture, DVec2::new(1000.0, 300.0)))
            .unwrap(),
        19.0,
        0.0001,
    );
    assert_eq!(
        drag.distance_cm(&capture, Vec2::splat(f32::NAN)),
        Err(HandleError::NonFinitePointer)
    );
    assert!(
        camera
            .captured_ray(Vec2::splat(f32::INFINITY), 4.0 / 3.0)
            .is_err()
    );
    assert!(
        camera
            .captured_ray(Vec2::splat(f32::MAX), 4.0 / 3.0)
            .is_err()
    );
}

#[test]
fn exact_camera_bits_cancel_capture_even_when_float_equality_matches() {
    let camera = front_camera();
    let capture = capture(&camera, 1.0);
    let stationary = pointer(&capture, DVec2::new(430.0, 300.0));
    let drag = AxisDrag::begin(capture.clone(), DVec3::ZERO, WorldAxis::X, stationary).unwrap();
    for changed in [
        Camera::new(Vec3::ZERO, -0.0, 0.0, 10.0, 90_f32.to_radians(), 0.1, 100.0).unwrap(),
        Camera::new(Vec3::ZERO, 0.0, -0.0, 10.0, 90_f32.to_radians(), 0.1, 100.0).unwrap(),
        Camera::new(
            Vec3::new(-0.0, 0.0, 0.0),
            0.0,
            0.0,
            10.0,
            90_f32.to_radians(),
            0.1,
            100.0,
        )
        .unwrap(),
    ] {
        assert_eq!(camera, changed);
        assert!(!camera.same_capture(&changed));
        let changed_capture = ViewportCapture::new(&changed, [37, 53], [800, 600], 1.0).unwrap();
        assert_eq!(
            drag.distance_cm(&changed_capture, stationary),
            Err(HandleError::CaptureChanged)
        );
    }
}

#[test]
fn wholly_clipped_axes_report_a_reason_and_tiny_visible_geometry_has_no_center_hit() {
    let capture = capture(&front_camera(), 1.0);
    let edge_x = 10.0 / capture.view_projection.x_axis.x;
    let handles = WorldHandles::new(&capture, DVec3::new(edge_x, 0.0, 0.0)).unwrap();
    assert_eq!(
        handles.axes()[0].unavailable_reason(),
        Some(HandleError::HandleClipped)
    );
    assert!(handles.axes()[1].arrow().is_some());
    let tiny = ViewportCapture::new(&front_camera(), [0, 0], [1, 1], 1.0).unwrap();
    let handles = WorldHandles::new(&tiny, DVec3::ZERO).unwrap();
    assert!(handles.axes()[0].arrow().is_some());
    for point in [DVec2::ZERO, DVec2::splat(0.5), DVec2::ONE] {
        assert_eq!(handles.hit_test(point), None);
    }
}

#[test]
fn backward_closest_ray_solution_is_refused_without_endpoint_fallback() {
    let forward = Ray {
        origin: Vec3::new(0.0, 2.0, 0.0),
        direction: Vec3::new(1.0, -1.0, 0.0),
    };
    near(
        axis_parameter(DVec3::ZERO, WorldAxis::X, forward).unwrap(),
        2.0,
        1e-10,
    );
    let backward = Ray {
        origin: forward.origin,
        direction: Vec3::new(1.0, 1.0, 0.0),
    };
    assert_eq!(
        axis_parameter(DVec3::ZERO, WorldAxis::X, backward),
        Err(HandleError::BehindRay)
    );
}

#[test]
fn mapping_comparison_catches_camera_projection_origin_size_and_fractional_scale_changes() {
    let camera = front_camera();
    let capture = capture(&camera, 1.25);
    let stationary = pointer(&capture, DVec2::new(430.0, 300.0));
    let drag = AxisDrag::begin(capture.clone(), DVec3::ZERO, WorldAxis::X, stationary).unwrap();
    let mut orbited = camera.clone();
    orbited.orbit(Vec2::new(0.01, 0.0)).unwrap();
    let projection =
        Camera::new(Vec3::ZERO, 0.0, 0.0, 10.0, 91_f32.to_radians(), 0.1, 100.0).unwrap();
    for changed in [
        ViewportCapture::new(&camera, [38, 53], [800, 600], 1.25).unwrap(),
        ViewportCapture::new(&camera, [37, 54], [800, 600], 1.25).unwrap(),
        ViewportCapture::new(&camera, [37, 53], [801, 600], 1.25).unwrap(),
        ViewportCapture::new(&camera, [37, 53], [800, 601], 1.25).unwrap(),
        ViewportCapture::new(&camera, [37, 53], [800, 600], 1.5).unwrap(),
        ViewportCapture::new(&orbited, [37, 53], [800, 600], 1.25).unwrap(),
        ViewportCapture::new(&projection, [37, 53], [800, 600], 1.25).unwrap(),
    ] {
        assert!(!capture.same_mapping(&changed));
        assert_eq!(
            drag.distance_cm(&changed, stationary),
            Err(HandleError::CaptureChanged)
        );
    }
    assert!(capture.same_mapping(&capture.clone()));
    near(drag.distance_cm(&capture, stationary).unwrap(), 0.0, 1e-10);
}

#[test]
fn invalid_or_empty_capture_is_refused_before_any_pointer_math() {
    let camera = front_camera();
    for (origin, size, scale) in [
        ([0, 0], [0, 600], 1.0),
        ([0, 0], [800, 0], 1.0),
        ([0, 0], [800, 600], 0.0),
        ([0, 0], [800, 600], f32::NAN),
        ([-1, 0], [800, 600], 1.0),
        ([i32::MAX, 0], [800, 600], 1.0),
        ([0, 0], [u32::MAX, 600], 1.0),
        ([0, 0], [800, 600], f32::MIN_POSITIVE),
    ] {
        assert!(matches!(
            ViewportCapture::new(&camera, origin, size, scale),
            Err(HandleError::InvalidCapture)
        ));
    }
    let capture = capture(&camera, 1.0);
    assert!(matches!(
        AxisDrag::begin(
            capture,
            DVec3::ZERO,
            WorldAxis::X,
            Vec2::new(-100.0, -100.0)
        ),
        Err(HandleError::OriginClipped)
    ));
}

#[test]
fn f64_ray_normalization_enforces_the_parallel_threshold_for_both_view_signs() {
    for sign in [-1.0, 1.0] {
        let below = Ray {
            origin: Vec3::new(0.0, 2.0, 0.0),
            direction: Vec3::new(sign, -0.000005, 0.0),
        };
        assert_eq!(
            axis_parameter(DVec3::ZERO, WorldAxis::X, below),
            Err(HandleError::ParallelAxis)
        );
        let above = Ray {
            origin: Vec3::new(0.0, 2.0, 0.0),
            direction: Vec3::new(sign, -0.00002, 0.0),
        };
        let parameter = axis_parameter(DVec3::ZERO, WorldAxis::X, above).unwrap();
        near(parameter, f64::from(sign) * 100_000.0, 0.1);
        let opposite = Ray {
            origin: Vec3::new(0.0, -2.0, 0.0),
            direction: -above.direction,
        };
        near(
            axis_parameter(DVec3::ZERO, WorldAxis::X, opposite).unwrap(),
            -parameter,
            0.1,
        );
    }
    for direction in [
        Vec3::ZERO,
        Vec3::splat(f32::NAN),
        Vec3::splat(f32::INFINITY),
    ] {
        assert_eq!(
            axis_parameter(
                DVec3::ZERO,
                WorldAxis::Y,
                Ray {
                    origin: Vec3::ZERO,
                    direction
                }
            ),
            Err(HandleError::InvalidSolve)
        );
    }
}
