use super::*;
use crate::viewport::Camera;
use glam::Vec3;

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;

fn camera_at(yaw: f32, pitch: f32, distance: f32) -> Camera {
    Camera::new(
        Vec3::ZERO,
        yaw,
        pitch,
        distance,
        90_f32.to_radians(),
        0.1,
        100.0,
    )
    .unwrap()
}

fn front_camera() -> Camera {
    camera_at(0.0, 0.0, 10.0)
}

fn oblique_camera() -> Camera {
    camera_at(0.6, 0.4, 10.0)
}

fn capture(camera: &Camera) -> ViewportCapture {
    ViewportCapture::new(camera, [37, 53], [WIDTH, HEIGHT], 1.0).unwrap()
}

// Logical pointer for a physical pixel of the capture, which sits at (37, 53) at scale one.
fn pointer(physical: DVec2) -> Vec2 {
    Vec2::new(37.0, 53.0) + physical.as_vec2()
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} within {tolerance}"
    );
}

// Independent of the handle code: project through the camera's own f32 pipeline.
fn pixel_of(camera: &Camera, world: DVec3) -> DVec2 {
    let ndc = camera
        .project(world.as_vec3(), WIDTH as f32 / HEIGHT as f32)
        .unwrap()
        .unwrap();
    DVec2::new(
        (f64::from(ndc.x) + 1.0) * f64::from(WIDTH) * 0.5,
        (1.0 - f64::from(ndc.y)) * f64::from(HEIGHT) * 0.5,
    )
}

// World radius of a ring at the origin: 72 px at the origin depth of a 90 degree camera.
fn ring_radius(camera: &Camera) -> f64 {
    ARROW_LENGTH * f64::from(camera.distance()) / (f64::from(HEIGHT) * 0.5)
}

// The point of the ring around `axis` nearest the eye, and its opposite on the hidden half.
fn front_back(camera: &Camera, axis: DVec3) -> (DVec3, DVec3) {
    let eye = camera.eye().as_dvec3();
    let direction = (eye - axis * eye.dot(axis)).normalize();
    let radius = ring_radius(camera);
    (direction * radius, -direction * radius)
}

fn world_rings(camera: &Camera) -> RotateHandles {
    RotateHandles::new(&capture(camera), &HandleFrame::world(DVec3::ZERO).unwrap()).unwrap()
}

fn world_scale(camera: &Camera) -> ScaleHandles {
    ScaleHandles::new(&capture(camera), &HandleFrame::world(DVec3::ZERO).unwrap()).unwrap()
}

#[test]
fn ring_pick_selects_the_nearest_front_segment_within_the_threshold() {
    let camera = oblique_camera();
    let handles = world_rings(&camera);
    for (axis, direction) in [
        (RotateAxis::X, DVec3::X),
        (RotateAxis::Y, DVec3::Y),
        (RotateAxis::Z, DVec3::Z),
    ] {
        let (front, _) = front_back(&camera, direction);
        let pixel = pixel_of(&camera, front);
        assert_eq!(handles.hit_test(pixel), Some(axis), "{axis:?} at {pixel}");
    }
    for ring in handles.rings() {
        assert!(!ring.segments().is_empty());
        for segment in ring.segments() {
            let middle = (segment.start + segment.end) * 0.5;
            assert!(handles.hit_test(middle).is_some());
        }
    }
    assert_eq!(handles.hit_test(DVec2::new(10.0, 10.0)), None);
    assert_eq!(handles.hit_test(DVec2::new(f64::NAN, 300.0)), None);
    assert_eq!(handles.hit_test(DVec2::new(-1.0, 300.0)), None);
}

#[test]
fn ring_pick_threshold_is_the_pixel_distance_to_the_nearest_segment() {
    let handles = world_rings(&oblique_camera());
    let distance_to = |index: usize, pixel: DVec2| {
        handles.rings()[index]
            .segments()
            .iter()
            .map(|segment| segment_distance_squared(*segment, pixel).sqrt())
            .fold(f64::INFINITY, f64::min)
    };
    // A Y segment whose neighbourhood holds no other ring, probed perpendicular to itself.
    let (middle, normal) = handles.rings()[1]
        .segments()
        .iter()
        .find_map(|segment| {
            let middle = (segment.start + segment.end) * 0.5;
            let delta = segment.end - segment.start;
            let normal = DVec2::new(-delta.y, delta.x).normalize();
            (distance_to(0, middle) > 40.0 && distance_to(2, middle) > 40.0)
                .then_some((middle, normal))
        })
        .expect("a Y segment away from the other rings");
    for step in 0..40 {
        let pixel = middle + normal * (f64::from(step) * 0.25 + 0.01);
        assert_eq!(
            handles.hit_test(pixel),
            (distance_to(1, pixel) <= HIT_RADIUS).then_some(RotateAxis::Y),
            "{pixel}"
        );
    }
    assert_eq!(handles.hit_test(middle + normal * 5.9), Some(RotateAxis::Y));
    assert_eq!(handles.hit_test(middle + normal * 6.5), None);
}

#[test]
fn back_half_of_each_ring_is_neither_drawn_nor_picked() {
    let camera = oblique_camera();
    let handles = world_rings(&camera);
    for (index, direction) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
        let axis = [RotateAxis::X, RotateAxis::Y, RotateAxis::Z][index];
        let (front, back) = front_back(&camera, direction);
        let drawn = handles.rings()[index].segments();
        assert!(
            (RING_SEGMENTS / 2 - 2..=RING_SEGMENTS / 2 + 2).contains(&drawn.len()),
            "{axis:?} drew {} segments",
            drawn.len()
        );
        let on_ring = |pixel: DVec2| {
            drawn
                .iter()
                .any(|segment| segment_distance_squared(*segment, pixel) < 1.0)
        };
        assert!(on_ring(pixel_of(&camera, front)));
        let hidden = pixel_of(&camera, back);
        assert!(!on_ring(hidden), "{axis:?} back point is drawn");
        assert_ne!(handles.hit_test(hidden), Some(axis));
    }
}

#[test]
fn object_frame_rings_follow_the_supplied_axes() {
    let camera = oblique_camera();
    // Axes turned 90 degrees about Z (and one unnormalized): the ring around local X lies in the
    // world XZ plane.
    let frame = HandleFrame::new(DVec3::ZERO, [DVec3::Y, -DVec3::X, DVec3::Z * 3.0]).unwrap();
    let handles = RotateHandles::new(&capture(&camera), &frame).unwrap();
    let (front, _) = front_back(&camera, DVec3::Y);
    assert_eq!(
        handles.hit_test(pixel_of(&camera, front)),
        Some(RotateAxis::X)
    );
}

#[test]
fn invalid_frames_and_offscreen_pivots_are_refused() {
    let axes = [DVec3::X, DVec3::Y, DVec3::Z];
    assert!(HandleFrame::new(DVec3::NAN, axes).is_err());
    assert!(HandleFrame::new(DVec3::ZERO, [DVec3::X, DVec3::ZERO, DVec3::Z]).is_err());
    assert!(HandleFrame::new(DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::INFINITY]).is_err());
    let capture = capture(&front_camera());
    let off_screen = HandleFrame::world(DVec3::new(1e6, 0.0, 0.0)).unwrap();
    assert_eq!(
        RotateHandles::new(&capture, &off_screen).err(),
        Some(HandleError::OriginClipped)
    );
    assert_eq!(
        ScaleHandles::new(&capture, &off_screen).err(),
        Some(HandleError::OriginClipped)
    );
}

#[test]
fn ring_drag_angle_comes_from_the_ray_plane_hit_and_unwraps_past_half_a_turn() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    let mut drag =
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, at(72.0, 0.0)).unwrap();
    near(
        drag.angle_degrees(&capture, at(72.0, 0.0)).unwrap(),
        0.0,
        1e-9,
    );
    // Screen up is world +Y, so a counterclockwise quarter turn is +90 about +Z.
    near(
        drag.angle_degrees(&capture, at(0.0, -72.0)).unwrap(),
        90.0,
        1e-3,
    );
    near(
        drag.angle_degrees(&capture, at(-72.0, 0.0)).unwrap(),
        180.0,
        1e-3,
    );
    near(
        drag.angle_degrees(&capture, at(0.0, 72.0)).unwrap(),
        270.0,
        1e-3,
    );
    near(
        drag.angle_degrees(&capture, at(72.0, 0.0)).unwrap(),
        360.0,
        1e-3,
    );
    let mut backward =
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, at(72.0, 0.0)).unwrap();
    near(
        backward.angle_degrees(&capture, at(0.0, 72.0)).unwrap(),
        -90.0,
        1e-3,
    );
    // The angle does not depend on the pointer radius.
    near(
        backward.angle_degrees(&capture, at(0.0, 150.0)).unwrap(),
        -90.0,
        1e-3,
    );
}

#[test]
fn x_ring_angle_turns_from_y_toward_z() {
    // Looking down -X: world +Y is screen up and +Z is screen left.
    let camera = camera_at(std::f32::consts::FRAC_PI_2, 0.0, 10.0);
    let capture = capture(&camera);
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    let mut drag =
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::X, at(0.0, -72.0)).unwrap();
    near(
        drag.angle_degrees(&capture, at(-72.0, 0.0)).unwrap(),
        90.0,
        1e-3,
    );
}

#[test]
fn edge_on_ring_returns_degenerate_and_a_refused_sample_changes_nothing() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    // Looking along -Z, the X ring plane contains the view direction down the center column.
    assert_eq!(
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::X, at(0.0, -72.0)).err(),
        Some(HandleError::Degenerate)
    );
    let mut drag =
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, at(72.0, 0.0)).unwrap();
    near(
        drag.angle_degrees(&capture, at(0.0, -72.0)).unwrap(),
        90.0,
        1e-3,
    );
    // Exactly at the pivot the angle is undefined.
    assert_eq!(
        drag.angle_degrees(&capture, at(0.0, 0.0)).err(),
        Some(HandleError::Degenerate)
    );
    near(
        drag.angle_degrees(&capture, at(-72.0, 0.0)).unwrap(),
        180.0,
        1e-3,
    );
}

#[test]
fn ring_drag_refuses_bad_pointers_and_a_changed_capture() {
    let camera = front_camera();
    let capture = capture(&camera);
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let start = pointer(DVec2::new(472.0, 300.0));
    assert_eq!(
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, Vec2::NAN).err(),
        Some(HandleError::NonFinitePointer)
    );
    assert_eq!(
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, Vec2::new(-5.0, 0.0)).err(),
        Some(HandleError::OriginClipped)
    );
    let mut drag = RotateDrag::begin(capture.clone(), &frame, RotateAxis::Z, start).unwrap();
    let moved = ViewportCapture::new(&camera, [37, 53], [WIDTH + 1, HEIGHT], 1.0).unwrap();
    assert!(!drag.same_capture(&moved));
    assert_eq!(
        drag.angle_degrees(&moved, start).err(),
        Some(HandleError::CaptureChanged)
    );
    assert_eq!(
        drag.angle_degrees(&capture, Vec2::new(f32::INFINITY, 0.0))
            .err(),
        Some(HandleError::NonFinitePointer)
    );
}

#[test]
fn view_ring_and_free_rotate_are_drawn_but_only_reported_unsupported() {
    let handles = world_rings(&oblique_camera());
    let origin = handles.origin();
    assert_eq!(handles.disabled().len(), 2);
    assert_eq!(
        handles.hit_disabled(origin + DVec2::new(ARROW_LENGTH * VIEW_RING_FACTOR, 0.0)),
        Some(DisabledKind::ViewRing)
    );
    assert_eq!(handles.hit_test(origin + DVec2::new(86.0, 0.0)), None);
    let free = (0..360).any(|degrees| {
        let angle = f64::from(degrees).to_radians();
        let pixel = origin + DVec2::new(angle.cos(), angle.sin()) * ARROW_LENGTH;
        handles.hit_disabled(pixel) == Some(DisabledKind::FreeRotate)
    });
    assert!(free);
    assert_eq!(handles.hit_disabled(DVec2::NAN), None);
    // An enabled ring wins over a disabled handle at the same pixel.
    let ring_pixel = handles.rings()[0].segments()[0].start;
    assert_eq!(handles.hit_disabled(ring_pixel), None);
}

#[test]
fn scale_boxes_share_drawing_and_pick_geometry() {
    let handles = world_scale(&front_camera());
    assert_eq!(
        handles.axes()[2].unavailable_reason(),
        Some(HandleError::ParallelAxis)
    );
    // Shaft plus four box edges.
    assert_eq!(handles.axes()[0].geometry().unwrap().segments().count(), 5);
    assert_eq!(
        handles.hit_test(DVec2::new(472.0, 300.0)),
        Some(ScaleHandle::X)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(436.0, 304.0)),
        Some(ScaleHandle::X)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(400.0, 228.0)),
        Some(ScaleHandle::Y)
    );
    // The box edge sits five pixels past the tip, with the six pixel pick margin.
    assert_eq!(
        handles.hit_test(DVec2::new(483.0, 300.0)),
        Some(ScaleHandle::X)
    );
    assert_eq!(handles.hit_test(DVec2::new(483.5, 300.0)), None);
    assert_eq!(
        handles.hit_test(DVec2::new(407.0, 293.0)),
        Some(ScaleHandle::Uniform)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(400.0, 300.0)),
        Some(ScaleHandle::Uniform)
    );
    assert_eq!(handles.hit_test(DVec2::new(700.0, 500.0)), None);
    assert_eq!(handles.hit_test(DVec2::new(f64::NAN, 300.0)), None);
    assert_eq!(handles.center().count(), 4);
}

#[test]
fn scale_plane_handles_are_drawn_but_only_reported_unsupported() {
    let handles = world_scale(&front_camera());
    // Only the X-Y plane has two usable screen axes from the front.
    assert_eq!(handles.disabled().len(), 1);
    let corner = DVec2::new(400.0 + 0.3 * ARROW_LENGTH, 300.0 - 0.3 * ARROW_LENGTH);
    assert_eq!(handles.hit_test(corner), None);
    assert_eq!(handles.hit_disabled(corner), Some(DisabledKind::ScalePlane));
    assert_eq!(handles.hit_disabled(DVec2::new(472.0, 300.0)), None);
}

#[test]
fn scale_factor_is_the_parameter_ratio_along_the_axis() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    let drag = ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(72.0, 0.0)).unwrap();
    near(drag.factor(&capture, at(72.0, 0.0)).unwrap(), 1.0, 1e-9);
    near(drag.factor(&capture, at(144.0, 0.0)).unwrap(), 2.0, 1e-4);
    near(drag.factor(&capture, at(36.0, 0.0)).unwrap(), 0.5, 1e-4);
    // Across the pivot the ratio goes negative; the scale core refuses it, not the handle.
    near(drag.factor(&capture, at(-72.0, 0.0)).unwrap(), -1.0, 1e-4);
    // Grabbing the negative side keeps outward motion positive.
    let negative =
        ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(-72.0, 0.0)).unwrap();
    near(
        negative.factor(&capture, at(-144.0, 0.0)).unwrap(),
        2.0,
        1e-4,
    );
    let up = ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::Y, at(0.0, -72.0)).unwrap();
    near(up.factor(&capture, at(0.0, -108.0)).unwrap(), 1.5, 1e-4);
}

#[test]
fn scale_refuses_a_start_too_close_to_the_pivot() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    for x in [0.0, 0.4, -0.4, 0.99] {
        assert_eq!(
            ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(x, 0.0)).err(),
            Some(HandleError::ZeroBeginParameter),
            "{x}"
        );
    }
    assert!(ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(1.5, 0.0)).is_ok());
    // An axis pointing at the camera cannot be solved at all.
    assert_eq!(
        ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::Z, at(0.0, 0.0)).err(),
        Some(HandleError::ParallelAxis)
    );
}

#[test]
fn the_pivot_refusal_distance_scales_with_view_depth() {
    // At twice the distance one pixel is twice as long in the world, so 1.5 px still passes
    // and 0.5 px does not; the limit is pixels, not scene units.
    let capture = capture(&camera_at(0.0, 0.0, 20.0));
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64| pointer(DVec2::new(400.0 + x, 300.0));
    assert!(ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(1.5)).is_ok());
    assert_eq!(
        ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::X, at(0.5)).err(),
        Some(HandleError::ZeroBeginParameter)
    );
}

#[test]
fn uniform_scale_grows_right_and_up_from_a_reference_distance() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    let at = |x: f64, y: f64| pointer(DVec2::new(400.0 + x, 300.0 + y));
    let drag =
        ScaleDrag::begin(capture.clone(), &frame, ScaleHandle::Uniform, at(1.0, 1.0)).unwrap();
    near(drag.factor(&capture, at(1.0, 1.0)).unwrap(), 1.0, 1e-12);
    near(drag.factor(&capture, at(51.0, 1.0)).unwrap(), 1.5, 1e-6);
    near(drag.factor(&capture, at(1.0, -49.0)).unwrap(), 1.5, 1e-6);
    near(drag.factor(&capture, at(-49.0, 1.0)).unwrap(), 0.5, 1e-6);
    near(drag.factor(&capture, at(-99.0, 1.0)).unwrap(), 0.0, 1e-6);
    assert_eq!(
        drag.factor(&capture, Vec2::NAN).err(),
        Some(HandleError::NonFinitePointer)
    );
    let moved = ViewportCapture::new(&front_camera(), [38, 53], [WIDTH, HEIGHT], 1.0).unwrap();
    assert_eq!(
        drag.factor(&moved, at(1.0, 1.0)).err(),
        Some(HandleError::CaptureChanged)
    );
}

#[test]
fn scale_handles_follow_a_rotated_frame() {
    // Local X points up on screen, local Y points left, so the X box sits above the pivot.
    let frame = HandleFrame::new(DVec3::ZERO, [DVec3::Y, -DVec3::X, DVec3::Z]).unwrap();
    let handles = ScaleHandles::new(&capture(&front_camera()), &frame).unwrap();
    assert_eq!(
        handles.hit_test(DVec2::new(400.0, 228.0)),
        Some(ScaleHandle::X)
    );
    assert_eq!(
        handles.hit_test(DVec2::new(328.0, 300.0)),
        Some(ScaleHandle::Y)
    );
}

fn tie_segment() -> ScreenSegment {
    ScreenSegment {
        start: DVec2::new(100.0, 100.0),
        end: DVec2::new(200.0, 100.0),
    }
}

#[test]
fn identical_ring_segments_tie_to_x_then_y_then_z() {
    let mut handles = world_rings(&oblique_camera());
    for ring in &mut handles.rings {
        ring.segments = vec![tie_segment()];
    }
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(RotateAxis::X)
    );
    handles.rings[0].segments.clear();
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(RotateAxis::Y)
    );
    handles.rings[1].segments.clear();
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(RotateAxis::Z)
    );
}

#[test]
fn identical_scale_boxes_tie_to_x_then_y_then_z() {
    let mut handles = world_scale(&front_camera());
    for handle in &mut handles.axes {
        handle.geometry = Ok(ScaleGeometry {
            shaft: Some(tie_segment()),
            tip_box: [None; 4],
        });
    }
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(ScaleHandle::X)
    );
    handles.axes[0].geometry = Err(HandleError::HandleClipped);
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(ScaleHandle::Y)
    );
    handles.axes[1].geometry = Err(HandleError::HandleClipped);
    assert_eq!(
        handles.hit_test(DVec2::new(150.0, 100.0)),
        Some(ScaleHandle::Z)
    );
}

#[test]
fn edge_on_limit_sits_between_five_ten_thousandths_and_two_thousandths() {
    let capture = capture(&front_camera());
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    // On the center row the ray direction dotted with X is about dx / 300 for small dx.
    let at = |dx: f64| pointer(DVec2::new(400.0 + dx, 300.0 - 72.0));
    assert_eq!(
        RotateDrag::begin(capture.clone(), &frame, RotateAxis::X, at(0.15)).err(),
        Some(HandleError::Degenerate)
    );
    assert!(RotateDrag::begin(capture.clone(), &frame, RotateAxis::X, at(0.6)).is_ok());
}

#[test]
fn a_plane_behind_the_eye_returns_behind_ray() {
    // The eye is above the Y=0 plane, so rays that climb above the horizon meet it behind.
    let capture = capture(&camera_at(0.0, 0.4, 10.0));
    let frame = HandleFrame::world(DVec3::ZERO).unwrap();
    assert_eq!(
        RotateDrag::begin(
            capture.clone(),
            &frame,
            RotateAxis::Y,
            pointer(DVec2::new(400.0, 5.0))
        )
        .err(),
        Some(HandleError::BehindRay)
    );
}

#[test]
fn frames_with_a_non_finite_axis_length_are_refused() {
    // 1e200 is finite but its length overflows when squared.
    let huge = DVec3::new(1e200, 0.0, 0.0);
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, [huge, DVec3::Y, DVec3::Z]).err(),
        Some(HandleError::InvalidProjection)
    );
    assert_eq!(
        HandleFrame::new(
            DVec3::ZERO,
            [DVec3::X, DVec3::Y, DVec3::new(0.0, 0.0, f64::MAX)]
        )
        .err(),
        Some(HandleError::InvalidProjection)
    );
}

#[test]
fn mirrored_and_skewed_frames_are_refused() {
    // A negative-scale ancestor makes the axes left-handed, which would flip every ring drag.
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, [-DVec3::X, DVec3::Y, DVec3::Z]).err(),
        Some(HandleError::MirroredFrame)
    );
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, [DVec3::X, DVec3::Y, -DVec3::Z * 2.0]).err(),
        Some(HandleError::MirroredFrame)
    );
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, [DVec3::X, DVec3::X + DVec3::Y, DVec3::Z]).err(),
        Some(HandleError::SkewedFrame)
    );
    // Rotated, unevenly scaled but perpendicular right-handed axes remain valid.
    let tilt = glam::DMat3::from_rotation_z(0.7) * glam::DMat3::from_rotation_x(-0.3);
    assert!(
        HandleFrame::new(
            DVec3::ZERO,
            [tilt.x_axis * 2.0, tilt.y_axis * 0.5, tilt.z_axis * 3.0]
        )
        .is_ok()
    );
}

#[test]
fn pointers_outside_the_viewport_hit_no_handle_enabled_or_disabled() {
    let camera = front_camera();
    // A pivot 20 px from the left edge: the 72 px rings and the disabled circles cross the edge,
    // so their clipped segments end on it and sit within the pick radius of a pixel just outside.
    let origin = DVec3::new(-380.0 / 30.0, 0.0, 0.0);
    assert!(near_f(pixel_of(&camera, origin).x, 20.0));
    let frame = HandleFrame::world(origin).unwrap();
    let rotate = RotateHandles::new(&capture(&camera), &frame).unwrap();
    let scale = ScaleHandles::new(&capture(&camera), &frame).unwrap();
    // The view ring (86.4 px) leaves the left edge 84 px below the pivot, clear of every enabled
    // ring. Its clipped segment ends there, so a pixel on the edge is reported and one just
    // outside is not.
    let crossing = 300.0 + ((ARROW_LENGTH * VIEW_RING_FACTOR).powi(2) - 400.0).sqrt();
    let on_edge = DVec2::new(0.0, crossing);
    let outside = DVec2::new(-2.0, crossing);
    let beyond = DVec2::new(802.0, 300.0);
    assert_eq!(rotate.hit_test(on_edge), None);
    assert_eq!(rotate.hit_disabled(on_edge), Some(DisabledKind::ViewRing));
    for pixel in [
        outside,
        beyond,
        DVec2::new(400.0, -2.0),
        DVec2::new(400.0, 602.0),
    ] {
        assert_eq!(rotate.hit_test(pixel), None, "{pixel}");
        assert_eq!(rotate.hit_disabled(pixel), None, "{pixel}");
        assert_eq!(scale.hit_test(pixel), None, "{pixel}");
        assert_eq!(scale.hit_disabled(pixel), None, "{pixel}");
    }
}

fn near_f(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 0.5
}

fn scale_shaft_length(handles: &ScaleHandles, index: usize) -> f64 {
    let shaft = handles.axes()[index]
        .geometry()
        .unwrap()
        .segments()
        .next()
        .unwrap();
    (shaft.end - shaft.start).length()
}

#[test]
fn scale_shafts_and_boxes_foreshorten_with_the_view() {
    let front = world_scale(&camera_at(0.0, 0.0, 10.0));
    // Shaft stops at the box edge, five pixels short of the 72 px tip.
    near(scale_shaft_length(&front, 0), 72.0 - BOX_HALF, 1e-9);
    let oblique = world_scale(&camera_at(std::f32::consts::FRAC_PI_4, 0.0, 10.0));
    let tip = 72.0 * std::f64::consts::FRAC_1_SQRT_2;
    near(scale_shaft_length(&oblique, 0), tip - BOX_HALF, 0.01);
    near(scale_shaft_length(&oblique, 2), tip - BOX_HALF, 0.01);
    near(scale_shaft_length(&oblique, 1), 72.0 - BOX_HALF, 1e-9);
    let origin = DVec2::new(400.0, 300.0);
    let box_center = |handles: &ScaleHandles, index: usize| {
        let b = handles.axes()[index].geometry().unwrap().tip_box;
        let points: Vec<DVec2> = b.iter().flatten().map(|s| s.start).collect();
        points.iter().sum::<DVec2>() / points.len() as f64
    };
    near((box_center(&oblique, 0) - origin).length(), tip, 0.01);
}

#[test]
fn a_scale_shaft_shorter_than_its_box_never_reaches_back_past_the_origin() {
    // Z is seen at about 1.4 px, below BOX_HALF, so the unclamped shaft end would sit 3.6 px
    // behind the origin and draw a stub pointing away from the box.
    let handles = world_scale(&camera_at(0.02, 0.0, 10.0));
    let z = handles.axes()[2].geometry().unwrap();
    assert!(
        (z.tip_box.iter().flatten().map(|s| s.start).sum::<DVec2>() / 4.0
            - DVec2::new(400.0, 300.0))
        .length()
            < BOX_HALF
    );
    near(scale_shaft_length(&handles, 2), 0.0, 1e-9);
}

#[test]
fn an_end_on_scale_axis_collapses_into_the_center_box_and_steals_nothing() {
    let handles = world_scale(&camera_at(0.02, 0.0, 10.0));
    let z = handles.axes()[2].geometry().unwrap();
    for segment in z.segments() {
        assert!((segment.end - DVec2::new(400.0, 300.0)).length() < 12.0);
    }
    for x in [30.0, 60.0, 72.0] {
        assert_eq!(
            handles.hit_test(DVec2::new(400.0 + x, 300.0)),
            Some(ScaleHandle::X)
        );
    }
    assert_eq!(
        handles.hit_test(DVec2::new(400.0, 300.0 - 60.0)),
        Some(ScaleHandle::Y)
    );
}
