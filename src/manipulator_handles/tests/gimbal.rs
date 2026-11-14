use super::*;
use glam::DQuat;

// Independent of the production axes: the xyz gimbal axes of rotate (x, y, z) in degrees, from
// quaternion products (X ring Rz Ry e_x, Y ring Rz e_y, Z ring e_z).
fn euler_axes(y_degrees: f64, z_degrees: f64) -> [DVec3; 3] {
    let rz = DQuat::from_rotation_z(z_degrees.to_radians());
    let rzy = rz * DQuat::from_rotation_y(y_degrees.to_radians());
    [rzy * DVec3::X, rz * DVec3::Y, DVec3::Z]
}

fn frame_at(y: f64, z: f64) -> GimbalFrame {
    GimbalFrame::new(DVec3::ZERO, euler_axes(y, z)).unwrap()
}

#[test]
fn every_ring_of_a_skewed_frame_lies_in_the_plane_perpendicular_to_its_own_axis() {
    let camera = oblique_camera();
    let axes = euler_axes(50.0, 70.0);
    let handles = RotateHandles::new_gimbal(&capture(&camera), &frame_at(50.0, 70.0)).unwrap();
    for (index, axis) in ROTATE_AXES.into_iter().enumerate() {
        let (front, _) = front_back(&camera, axes[index]);
        assert_eq!(
            handles.hit_test(pixel_of(&camera, front)),
            Some(axis),
            "{axis:?}"
        );
    }
}

#[test]
fn the_frame_under_test_is_skewed() {
    // The X ring leans toward Z by the pitch, so the old basis (the other two axes) would put the
    // X ring in the plane of Y and Z instead of the plane perpendicular to its own axis.
    let axes = euler_axes(50.0, 70.0);
    assert!(axes[0].dot(axes[2]).abs() > 0.1);
}

fn reference_angle(normal: DVec3, from: DVec3, to: DVec3) -> f64 {
    normal.dot(from.cross(to)).atan2(from.dot(to)).to_degrees()
}

#[test]
fn skewed_ring_drag_angle_matches_an_independent_atan2_reference() {
    let camera = oblique_camera();
    let capture = capture(&camera);
    let axes = euler_axes(50.0, 70.0);
    let frame = frame_at(50.0, 70.0);
    let radius = ring_radius(&camera);
    for (index, axis) in ROTATE_AXES.into_iter().enumerate() {
        let normal = axes[index];
        let seed = if normal.x.abs() < 0.9 {
            DVec3::X
        } else {
            DVec3::Y
        };
        let first = normal.cross(seed).normalize();
        let second = normal.cross(first);
        let point = |degrees: f64| {
            let angle = degrees.to_radians();
            (first * angle.cos() + second * angle.sin()) * radius
        };
        let mut drag = RotateDrag::begin_gimbal(
            capture.clone(),
            &frame,
            axis,
            pointer(pixel_of(&camera, point(0.0))),
        )
        .unwrap();
        for degrees in [25.0, 80.0, -60.0] {
            let turned = drag
                .angle_degrees(&capture, pointer(pixel_of(&camera, point(degrees))))
                .unwrap();
            near(
                turned,
                reference_angle(normal, point(0.0), point(degrees)),
                2e-2,
            );
        }
    }
}

#[test]
fn gimbal_frame_matches_the_world_frame_for_an_orthogonal_frame() {
    let camera = oblique_camera();
    let capture = capture(&camera);
    let world = RotateHandles::new(&capture, &HandleFrame::world(DVec3::ZERO).unwrap()).unwrap();
    let gimbal = RotateHandles::new_gimbal(
        &capture,
        &GimbalFrame::new(DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::Z]).unwrap(),
    )
    .unwrap();
    for pixel in [
        DVec2::new(400.0, 300.0),
        DVec2::new(430.0, 310.0),
        DVec2::new(380.0, 250.0),
    ] {
        assert_eq!(world.hit_test(pixel), gimbal.hit_test(pixel));
    }
    let (front, _) = front_back(&camera, DVec3::Z);
    assert_eq!(
        gimbal.hit_test(pixel_of(&camera, front)),
        Some(RotateAxis::Z)
    );
}

#[test]
fn exact_lock_and_near_lock_build_every_ring_and_drag_it() {
    let camera = oblique_camera();
    let capture = capture(&camera);
    // Exactly collinear axes too: Euler products only reach them up to rounding.
    let exact = [
        [-DVec3::Z, DVec3::Y, DVec3::Z],
        [DVec3::Z, DVec3::Y, DVec3::Z],
    ];
    let cases = [90.0, -90.0, 90.0 - 1e-6, 0.0, 0.0]
        .into_iter()
        .enumerate()
        .map(|(index, pitch)| match index {
            3 | 4 => (90.0, exact[index - 3]),
            _ => (pitch, euler_axes(pitch, 20.0)),
        });
    for (pitch, axes) in cases {
        let frame = GimbalFrame::new(DVec3::ZERO, axes).unwrap();
        let handles = RotateHandles::new_gimbal(&capture, &frame).unwrap();
        for (index, axis) in ROTATE_AXES.into_iter().enumerate() {
            // The Y ring is always perpendicular to the other two; X and Z coincide at lock, where
            // either ring may win the pick.
            let (front, _) = front_back(&camera, axes[index]);
            let hit = handles.hit_test(pixel_of(&camera, front));
            let coincide = index != 1 && pitch.abs() > 89.0;
            let ok = hit == Some(axis)
                || (coincide && matches!(hit, Some(RotateAxis::X | RotateAxis::Z)));
            assert!(ok, "{axis:?} at {pitch}: {hit:?}");
            let normal = axes[index];
            let first = normal.cross(DVec3::new(0.3, 0.5, 0.8)).normalize();
            let second = normal.cross(first);
            let radius = ring_radius(&camera);
            let point = |degrees: f64| {
                let angle = degrees.to_radians();
                (first * angle.cos() + second * angle.sin()) * radius
            };
            let mut drag = RotateDrag::begin_gimbal(
                capture.clone(),
                &frame,
                axis,
                pointer(pixel_of(&camera, point(0.0))),
            )
            .unwrap();
            let turned = drag
                .angle_degrees(&capture, pointer(pixel_of(&camera, point(40.0))))
                .unwrap();
            near(
                turned,
                reference_angle(normal, point(0.0), point(40.0)),
                2e-2,
            );
        }
    }
}

#[test]
fn a_frame_beyond_ninety_degrees_pitch_is_left_handed_but_accepted() {
    let axes = euler_axes(120.0, 20.0);
    let [x, y, z] = axes;
    assert!(x.cross(y).dot(z) < 0.0);
    assert!(GimbalFrame::new(DVec3::ZERO, axes).is_ok());
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, axes).err(),
        Some(HandleError::SkewedFrame)
    );
}

#[test]
fn skewed_frames_stay_refused_by_the_orthogonal_frame_and_bad_input_by_both() {
    let skewed = euler_axes(50.0, 70.0);
    assert_eq!(
        HandleFrame::new(DVec3::ZERO, skewed).err(),
        Some(HandleError::SkewedFrame)
    );
    assert!(GimbalFrame::new(DVec3::NAN, skewed).is_err());
    assert!(GimbalFrame::new(DVec3::ZERO, [DVec3::X, DVec3::ZERO, DVec3::Z]).is_err());
    assert!(GimbalFrame::new(DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::INFINITY]).is_err());
    assert!(GimbalFrame::new(DVec3::ZERO, [DVec3::X, DVec3::Y, DVec3::splat(f64::NAN)]).is_err());
    assert_eq!(frame_at(50.0, 70.0).origin(), DVec3::ZERO);
}
