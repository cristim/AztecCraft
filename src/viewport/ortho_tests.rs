//! Issue #48 slice 1: orthographic Top, Front and Side views. Maya parity is unverified; the
//! zoom mapping is PROVISIONAL.
use super::*;

const ASPECT: f32 = 1.5;

fn ortho(view: OrthographicView) -> Camera {
    let mut camera = Camera::default();
    camera.show_orthographic(view).unwrap();
    camera
}

fn close(a: Vec3, b: Vec3) -> bool {
    // Top stops MAX_PITCH short of the pole, 1e-3 rad off the axis.
    (a - b).length() < 2e-3
}

#[test]
fn views_look_along_the_axis_with_maya_screen_orientation() {
    // (view, eye direction from the target, screen right, screen up)
    for (view, eye, right, up) in [
        (OrthographicView::Front, Vec3::Z, Vec3::X, Vec3::Y),
        (OrthographicView::Side, Vec3::X, -Vec3::Z, Vec3::Y),
        (OrthographicView::Top, Vec3::Y, Vec3::X, -Vec3::Z),
    ] {
        let camera = ortho(view);
        let direction = (camera.eye() - camera.target()).normalize();
        assert!(close(direction, eye), "{view:?} eye {direction:?}");
        let inverse = camera.view_matrix().inverse();
        assert!(
            close(inverse.transform_vector3(Vec3::X), right),
            "{view:?} right"
        );
        assert!(close(inverse.transform_vector3(Vec3::Y), up), "{view:?} up");
        assert_eq!(camera.orthographic_view(), Some(view));
    }
}

#[test]
fn projection_ignores_depth_and_scales_with_distance() {
    let camera = ortho(OrthographicView::Front);
    let half_height = camera.distance() * camera.lens().tan_half_y(ASPECT);
    let near = camera
        .project(Vec3::new(1.0, 0.5, 0.0), ASPECT)
        .unwrap()
        .unwrap();
    let far = camera
        .project(Vec3::new(1.0, 0.5, -50.0), ASPECT)
        .unwrap()
        .unwrap();
    assert!((near.x - far.x).abs() < 1e-6 && (near.y - far.y).abs() < 1e-6);
    assert!((near.y - 0.5 / half_height).abs() < 1e-5);
    assert!((near.x - 1.0 / (half_height * ASPECT)).abs() < 1e-5);
    assert!(far.z > near.z && (0.0..=1.0).contains(&far.z));
    let mut zoomed = camera.clone();
    zoomed.dolly(0.5).unwrap();
    let doubled = zoomed
        .project(Vec3::new(1.0, 0.5, 0.0), ASPECT)
        .unwrap()
        .unwrap();
    assert!((doubled.y - 2.0 * near.y).abs() < 1e-5);
}

#[test]
fn rays_are_parallel_and_start_over_the_picked_point() {
    let camera = ortho(OrthographicView::Top);
    let a = camera.ray(Vec2::new(0.5, 0.25), ASPECT).unwrap();
    let b = camera.ray(Vec2::new(-0.5, -0.25), ASPECT).unwrap();
    assert!(close(a.direction, -Vec3::Y) && close(b.direction, -Vec3::Y));
    // Top: screen right is +X, screen up is -Z.
    let half_height = camera.distance() * camera.lens().tan_half_y(ASPECT);
    assert!((a.origin.x - 0.5 * half_height * ASPECT).abs() < 1e-3);
    // The eye is 0.008 off the pole axis (distance * cos(MAX_PITCH)).
    assert!((a.origin.z + 0.25 * half_height).abs() < 2e-2);
}

#[test]
fn picking_through_an_orthographic_view_hits_the_object_under_the_cursor() {
    let mut scene = Scene::default();
    let id = scene.create(crate::scene::Primitive::Cube, "c").unwrap();
    let camera = ortho(OrthographicView::Front);
    assert_eq!(camera.pick(&scene, Vec2::ZERO, ASPECT).unwrap(), Some(id));
    assert_eq!(
        camera.pick(&scene, Vec2::new(0.95, 0.95), ASPECT).unwrap(),
        None
    );
}

#[test]
fn persp_restores_the_pose_left_by_the_first_orthographic_view() {
    let mut camera = Camera::default();
    camera.pan(Vec2::new(1.0, 2.0)).unwrap();
    let before = camera.clone();
    camera.show_orthographic(OrthographicView::Top).unwrap();
    camera.pan(Vec2::new(3.0, 0.0)).unwrap();
    camera.show_orthographic(OrthographicView::Side).unwrap();
    assert_eq!(camera.projection(), Projection::Orthographic);
    camera.show_perspective();
    assert_eq!(camera, before);
    let mut again = camera.clone();
    again.show_perspective();
    assert_eq!(again, camera);
}

#[test]
fn an_orthographic_camera_refuses_orbit_and_keeps_its_state() {
    let mut camera = ortho(OrthographicView::Front);
    let before = camera.clone();
    assert_eq!(camera.orbit(Vec2::new(0.1, 0.0)), Err(InvalidCameraInput));
    assert_eq!(camera, before);
    camera.set_pose(Vec3::ONE, 5.0, 0.3, 0.2).unwrap();
    assert_eq!(camera.projection(), Projection::Orthographic);
    assert_eq!(camera.orthographic_view(), None);
}

#[test]
fn framing_fits_the_bounds_in_an_orthographic_view() {
    let mut camera = ortho(OrthographicView::Front);
    let corners = [Vec3::new(-4.0, -1.0, -1.0), Vec3::new(4.0, 1.0, 1.0)];
    camera.frame_points(&corners, ASPECT).unwrap();
    for corner in corners {
        let ndc = camera.project(corner, ASPECT).unwrap().unwrap();
        assert!(ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0, "{ndc:?}");
    }
    // The bounding sphere (radius sqrt(18)) fits the limiting axis with the default margin.
    let fill = camera
        .project(Vec3::new(4.0, 1.0, 1.0), ASPECT)
        .unwrap()
        .unwrap();
    assert!(
        fill.x.abs() > 0.5,
        "framing left the view mostly empty: {fill:?}"
    );
}

#[test]
fn view_direction_is_parallel_in_orthographic_and_radial_in_perspective() {
    let ortho = ortho(OrthographicView::Side);
    let a = ortho.view_direction_at(Vec3::new(0.0, 5.0, 0.0));
    let b = ortho.view_direction_at(Vec3::new(0.0, -5.0, 3.0));
    assert!(close(a, b) && close(a, -Vec3::X));
    let persp = Camera::default();
    let toward = persp.view_direction_at(Vec3::new(0.0, 5.0, 0.0));
    assert!(close(
        toward,
        (Vec3::new(0.0, 5.0, 0.0) - persp.eye()).normalize()
    ));
}

#[test]
fn marker_bias_moves_toward_the_eye_along_the_view_direction() {
    let camera = ortho(OrthographicView::Front);
    let point = Vec3::new(2.0, 1.0, 0.0);
    let biased = camera.marker_biased(point);
    assert!((biased.x - point.x).abs() < 1e-6 && (biased.y - point.y).abs() < 1e-6);
    assert!(biased.z > point.z);
}

#[test]
fn same_capture_distinguishes_the_projection() {
    let persp = Camera::default();
    let mut ortho = persp.clone();
    ortho.show_orthographic(OrthographicView::Front).unwrap();
    ortho.show_perspective();
    assert!(persp.same_capture(&ortho));
    ortho.show_orthographic(OrthographicView::Front).unwrap();
    assert!(!persp.same_capture(&ortho));
}

#[test]
fn orthographic_depth_is_linear_between_near_and_far() {
    let camera = ortho(OrthographicView::Front);
    let depth = |distance_from_eye: f32| {
        let point = camera.eye() - Vec3::Z * distance_from_eye;
        camera.project(point, ASPECT).unwrap().map(|ndc| ndc.z)
    };
    assert!(
        depth(camera.near() * 0.5).is_none(),
        "closer than near is clipped"
    );
    assert!(depth(camera.far() * 1.1).is_none(), "beyond far is clipped");
    let (near, far) = (camera.near(), camera.far());
    let mid = (near + far) * 0.5;
    assert!(depth(near + 1e-3).unwrap() < 1e-3);
    assert!((depth(mid).unwrap() - 0.5).abs() < 1e-4);
    assert!((depth(far - 1e-2).unwrap() - 1.0).abs() < 1e-3);
}

#[test]
fn framing_distance_matches_the_orthographic_extent_exactly() {
    let mut camera = ortho(OrthographicView::Front);
    camera
        .frame_points(
            &[Vec3::new(-3.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0)],
            ASPECT,
        )
        .unwrap();
    // Radius 3, limiting axis half angle h: distance = radius / tan(h) * 1.05.
    let tan = camera
        .lens()
        .tan_half_y(ASPECT)
        .min(camera.lens().tan_half_y(ASPECT) * ASPECT);
    let expected = (3.0 / tan * 1.05).max((camera.near() + 3.0) * 1.05);
    assert!((camera.distance() - expected).abs() < 1e-3 * expected);
    let edge = camera
        .project(Vec3::new(3.0, 0.0, 0.0), ASPECT)
        .unwrap()
        .unwrap();
    // The limiting axis is vertical, so the sphere radius fills 1 / 1.05 of the half height.
    assert!((edge.x * ASPECT - 1.0 / 1.05).abs() < 1e-3, "{edge:?}");
}

#[test]
fn marker_bias_shift_is_depth_times_bias_along_the_view() {
    let camera = ortho(OrthographicView::Front);
    let point = Vec3::new(2.0, 1.0, 0.0);
    let depth = camera.distance();
    let expected = depth * camera.marker_bias(depth);
    assert!((camera.marker_biased(point).z - point.z - expected).abs() < 1e-6);
    let step = MARKER_DEPTH_ULPS * DEPTH32_ULP * (camera.far() - camera.near()) / depth;
    assert!(
        (camera.marker_bias(depth) - (FACE_CENTER_DEPTH_TOLERANCE + step).min(MARKER_BIAS_MAX))
            .abs()
            < 1e-9
    );
}
