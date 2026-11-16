//! Issue #458: film back storage, fits, squeeze, overscan, offset, roll, scales, rays, framing
//! and the film gate. Expected values are worked by hand from the formulas in the plan (all
//! PROVISIONAL: the Maya docs state none), never read back from `FilmBack`.
use super::*;

const TX_DEFAULT: f64 = 1.4173 * 25.4 / 70.0; // 0.5142774
const TY_DEFAULT: f64 = 0.9449 * 25.4 / 70.0; // 0.34286...

type Edit = fn(&mut FilmBack);
type Setter = fn(&mut FilmBack, f64);

fn film_camera(edit: impl FnOnce(&mut FilmBack)) -> Camera {
    let mut film = FilmBack::DEFAULT;
    edit(&mut film);
    Camera {
        lens: Lens::Film(film),
        ..Camera::default()
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs {expected}"
    );
}

/// 1/x_axis.x and 1/y_axis.y: the tangent half extents of an unrolled, unshifted projection.
fn tan_extents(camera: &Camera, aspect: f32) -> [f64; 2] {
    let m = camera.projection_matrix(aspect).unwrap();
    [1.0 / f64::from(m.x_axis.x), 1.0 / f64::from(m.y_axis.y)]
}

// T1a: storage

#[test]
fn default_film_back_is_the_create_camera_values_in_f64() {
    let film = FilmBack::default();
    assert_eq!(film.focal_mm, 35.0);
    assert_eq!(film.aperture_in, [1.4173, 0.9449]);
    assert_eq!(film.film_fit, FilmFit::Fill);
    assert_eq!(film.overscan, 1.0);
    assert_eq!(film.lens_squeeze, 1.0);
    assert_eq!([film.pre_scale, film.post_scale], [1.0, 1.0]);
    assert!(!film.display_film_gate);
    assert_eq!(Camera::default().lens(), Lens::Film(film));
}

#[test]
fn each_invalid_film_back_field_is_refused_by_name() {
    let cases: [(&str, Setter); 11] = [
        ("focal_mm", |f, v| f.focal_mm = v),
        ("horizontal_aperture", |f, v| f.aperture_in[0] = v),
        ("vertical_aperture", |f, v| f.aperture_in[1] = v),
        ("lens_squeeze", |f, v| f.lens_squeeze = v),
        ("pre_scale", |f, v| f.pre_scale = v),
        ("post_scale", |f, v| f.post_scale = v),
        ("overscan", |f, v| f.overscan = v),
        ("film_fit_offset", |f, v| f.film_fit_offset_in = v),
        ("horizontal_film_offset", |f, v| f.film_offset_in[0] = v),
        ("vertical_film_offset", |f, v| f.film_offset_in[1] = v),
        ("film_roll", |f, v| f.film_roll_degrees = v),
    ];
    for (index, (name, set)) in cases.iter().enumerate() {
        for bad in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
            let mut film = FilmBack::DEFAULT;
            set(&mut film, bad);
            // Offsets and the roll may be zero or negative; only non-finite values are bad.
            let must_fail = !bad.is_finite() || (index < 7 && bad <= 0.0);
            let result = film.validate();
            if must_fail {
                assert_eq!(result, Err(InvalidFilmBack(name)), "{name} {bad}");
                let camera = Camera {
                    lens: Lens::Film(film),
                    ..Camera::default()
                };
                assert!(camera.validate().is_err(), "{name} {bad}");
            } else {
                assert_eq!(result, Ok(()), "{name} {bad}");
            }
        }
    }
}

#[test]
fn same_capture_compares_every_film_back_field_by_bits() {
    let base = Camera::default();
    assert!(base.same_capture(&film_camera(|f| f.film_offset_in[0] = 0.0)));
    let edits: [(&str, Edit); 13] = [
        ("focal ulp", |f| {
            f.focal_mm = f64::from_bits(35.0_f64.to_bits() + 1)
        }),
        ("hfa", |f| f.aperture_in[0] = 1.5),
        ("vfa", |f| f.aperture_in[1] = 1.0),
        ("fit", |f| f.film_fit = FilmFit::Overscan),
        ("ffo", |f| f.film_fit_offset_in = 0.1),
        ("hfo", |f| f.film_offset_in[0] = 0.1),
        ("vfo", |f| f.film_offset_in[1] = 0.1),
        ("lsr", |f| f.lens_squeeze = 2.0),
        ("pre", |f| f.pre_scale = 2.0),
        ("post", |f| f.post_scale = 2.0),
        ("roll", |f| f.film_roll_degrees = 1.0),
        ("overscan", |f| f.overscan = 1.5),
        ("gate", |f| f.display_film_gate = true),
    ];
    for (name, edit) in edits {
        assert!(!base.same_capture(&film_camera(edit)), "{name}");
    }
    let minus_zero = film_camera(|f| f.film_offset_in[0] = -0.0);
    assert!(
        !base.same_capture(&minus_zero),
        "-0.0 is a different capture"
    );
}

#[test]
fn values_that_degenerate_the_f32_projection_are_refused() {
    let mut tiny_focal = FilmBack::DEFAULT;
    tiny_focal.focal_mm = 1e-300;
    assert_eq!(tiny_focal.validate(), Err(InvalidFilmBack("projection")));
    let mut tiny_scales = FilmBack::DEFAULT;
    (tiny_scales.pre_scale, tiny_scales.post_scale) = (1e-100, 1e-100);
    assert_eq!(tiny_scales.validate(), Err(InvalidFilmBack("projection")));
    assert_eq!(FilmBack::DEFAULT.validate(), Ok(()));
}

#[test]
fn fov_y_matches_the_drawn_view_with_overscan_scales_and_in_orthographic() {
    let mut camera = film_camera(|f| {
        f.overscan = 1.5;
        f.pre_scale = 2.0;
        f.post_scale = 1.25;
    });
    for aspect in [1.0_f32, 1.5, 2.4] {
        let drawn =
            2.0 * (1.0 / f64::from(camera.projection_matrix(aspect).unwrap().y_axis.y)).atan();
        assert_close(
            f64::from(camera.fov_y_for(aspect)),
            drawn,
            1e-5,
            "perspective",
        );
    }
    camera.show_orthographic(OrthographicView::Front).unwrap();
    let default_fov = Camera::default().fov_y_for(1.5);
    assert_eq!(camera.fov_y_for(1.5), default_fov);
}

// T1b-1: fits, squeeze, overscan

#[test]
fn default_film_matches_the_hand_computed_extents() {
    // Fill at A = 1.5 is Horizontal: tx = 1.4173 * 25.4 / 70 = 0.5142774.
    let camera = Camera::default();
    assert_close(TX_DEFAULT, 0.5142774, 1e-7, "hand value");
    let [tx, ty] = tan_extents(&camera, 1.5);
    assert_close(tx, 0.5142774, 1e-6, "tx");
    assert_close(ty, 0.5142774 / 1.5, 1e-6, "ty");
}

#[test]
fn each_fit_selects_its_axis_at_three_aspects() {
    for (fit, aspect, horizontal) in [
        (FilmFit::Horizontal, 1.0, true),
        (FilmFit::Horizontal, 2.0, true),
        (FilmFit::Vertical, 1.0, false),
        (FilmFit::Vertical, 2.0, false),
        // Fill: horizontal at or above 1.4173 / 0.9449, vertical below.
        (FilmFit::Fill, 1.0, false),
        (FilmFit::Fill, 2.0, true),
        // Overscan is the opposite.
        (FilmFit::Overscan, 1.0, true),
        (FilmFit::Overscan, 2.0, false),
    ] {
        let camera = film_camera(|f| f.film_fit = fit);
        let [tx, ty] = tan_extents(&camera, aspect as f32);
        let (expected_tx, expected_ty) = if horizontal {
            (TX_DEFAULT, TX_DEFAULT / aspect)
        } else {
            (TY_DEFAULT * aspect, TY_DEFAULT)
        };
        assert_close(tx, expected_tx, 1e-6, &format!("{fit:?} {aspect} tx"));
        assert_close(ty, expected_ty, 1e-6, &format!("{fit:?} {aspect} ty"));
    }
    assert_close(TY_DEFAULT, 0.34286, 1e-5, "hand value");
}

#[test]
fn squeeze_widens_the_horizontal_aperture_and_moves_the_fill_threshold() {
    // Horizontal fit: tx = hfa * lsr * 25.4 / (2 fl) doubles.
    let squeezed = film_camera(|f| {
        f.film_fit = FilmFit::Horizontal;
        f.lens_squeeze = 2.0;
    });
    assert_close(tan_extents(&squeezed, 1.0)[0], 2.0 * TX_DEFAULT, 2e-6, "tx");
    // Fill threshold is hfa * lsr / vfa = 3: A = 2 is Vertical now, Horizontal by default.
    let fill = film_camera(|f| f.lens_squeeze = 2.0);
    let [tx, ty] = tan_extents(&fill, 2.0);
    assert_close(ty, TY_DEFAULT, 1e-6, "squeezed Fill at 2 is vertical: ty");
    assert_close(
        tx,
        2.0 * TY_DEFAULT,
        1e-6,
        "squeezed Fill at 2 is vertical: tx",
    );
    assert_close(
        tan_extents(&Camera::default(), 2.0)[0],
        TX_DEFAULT,
        1e-6,
        "default",
    );
    // At A = 3 it is Horizontal again.
    assert_close(tan_extents(&fill, 3.0)[0], 2.0 * TX_DEFAULT, 2e-6, "A = 3");
}

#[test]
fn overscan_scales_both_extents() {
    let camera = film_camera(|f| f.overscan = 1.2);
    let [tx, ty] = tan_extents(&camera, 1.5);
    assert_close(tx, 1.2 * TX_DEFAULT, 1e-6, "tx");
    assert_close(ty, 1.2 * TX_DEFAULT / 1.5, 1e-6, "ty");
}

// T1b-2: offset, ffo, scales, roll

/// NDC of the view-space point (x, y, z) through the projection alone.
fn ndc(camera: &Camera, aspect: f32, view: [f32; 3]) -> [f64; 2] {
    let clip =
        camera.projection_matrix(aspect).unwrap() * Vec4::new(view[0], view[1], view[2], 1.0);
    [f64::from(clip.x / clip.w), f64::from(clip.y / clip.w)]
}

#[test]
fn film_offset_shifts_the_window_so_the_axis_moves_opposite() {
    let camera = film_camera(|f| f.film_offset_in = [0.1, -0.05]);
    let cx = 0.1 * 25.4 / 35.0;
    let cy = -0.05 * 25.4 / 35.0;
    // The axis point lands at -c / t; the point at tangent c lands at the window centre.
    let [tx, ty] = [TX_DEFAULT, TX_DEFAULT / 1.5];
    let axis = ndc(&camera, 1.5, [0.0, 0.0, -10.0]);
    assert_close(axis[0], -cx / tx, 1e-6, "axis x");
    assert_close(axis[1], -cy / ty, 1e-6, "axis y");
    let centre = ndc(
        &camera,
        1.5,
        [(cx * 10.0) as f32, (cy * 10.0) as f32, -10.0],
    );
    assert_close(centre[0], 0.0, 1e-6, "centre x");
    assert_close(centre[1], 0.0, 1e-6, "centre y");
}

#[test]
fn film_fit_offset_acts_on_the_free_axis_of_an_explicit_fit_only() {
    let ffo = 0.2 * 25.4 / 35.0;
    let horizontal = film_camera(|f| {
        f.film_fit = FilmFit::Horizontal;
        f.film_fit_offset_in = 0.2;
    });
    let at = ndc(&horizontal, 1.5, [0.0, 0.0, -10.0]);
    assert_close(at[0], 0.0, 1e-6, "horizontal fit keeps x");
    assert_close(
        at[1],
        -ffo / (TX_DEFAULT / 1.5),
        1e-5,
        "horizontal fit shifts y",
    );
    let vertical = film_camera(|f| {
        f.film_fit = FilmFit::Vertical;
        f.film_fit_offset_in = 0.2;
    });
    let at = ndc(&vertical, 1.5, [0.0, 0.0, -10.0]);
    assert_close(
        at[0],
        -ffo / (TY_DEFAULT * 1.5),
        1e-5,
        "vertical fit shifts x",
    );
    assert_close(at[1], 0.0, 1e-6, "vertical fit keeps y");
    for fit in [FilmFit::Fill, FilmFit::Overscan] {
        let camera = film_camera(|f| {
            f.film_fit = fit;
            f.film_fit_offset_in = 0.2;
        });
        assert_eq!(ndc(&camera, 1.5, [0.0, 0.0, -10.0]), [0.0, 0.0], "{fit:?}");
    }
}

#[test]
fn roll_ninety_degrees_rotates_in_pixel_space() {
    let camera = film_camera(|f| {
        f.film_fit = FilmFit::Horizontal;
        f.film_roll_degrees = 90.0;
    });
    // A = 1: (u, v) -> (-v, u). The point at u = 0.5, v = 0 goes to (0, 0.5).
    let at = ndc(&camera, 1.0, [(TX_DEFAULT * 5.0) as f32, 0.0, -10.0]);
    assert_close(at[0], 0.0, 1e-6, "A=1 x");
    assert_close(at[1], 0.5, 1e-6, "A=1 y");
    // A = 2: pixel space is (u * 2, v). (0, 0.5) -> p = (0, 0.5) -> (-0.5, 0) -> ndc (-0.25, 0).
    let ty = TX_DEFAULT / 2.0;
    let at = ndc(&camera, 2.0, [0.0, (ty * 5.0) as f32, -10.0]);
    assert_close(at[0], -0.25, 1e-6, "A=2 x");
    assert_close(at[1], 0.0, 1e-6, "A=2 y");
}

#[test]
fn pre_and_post_scale_multiply_around_a_thirty_degree_roll() {
    let camera = film_camera(|f| {
        f.pre_scale = 2.0;
        f.post_scale = 0.5;
        f.film_roll_degrees = 30.0;
    });
    // u = 0.5, v = 0.25 at A = 1.5: p = (0.75, 0.25), rotated by 30 degrees, x divided by A.
    let (s, c) = 30_f64.to_radians().sin_cos();
    let expected = [(0.75 * c - 0.25 * s) / 1.5, 0.75 * s + 0.25 * c];
    let tx = TX_DEFAULT;
    let ty = TX_DEFAULT / 1.5;
    let at = ndc(
        &camera,
        1.5,
        [(0.5 * tx * 10.0) as f32, (0.25 * ty * 10.0) as f32, -10.0],
    );
    assert_close(at[0], expected[0], 1e-6, "x");
    assert_close(at[1], expected[1], 1e-6, "y");
    let scaled = film_camera(|f| f.pre_scale = 2.0);
    let at = ndc(&scaled, 1.5, [(0.5 * tx * 10.0) as f32, 0.0, -10.0]);
    assert_close(at[0], 1.0, 1e-6, "pre scale doubles");
}

// T1c: rays, framing, ortho

fn world_point_at_depth(camera: &Camera, aspect: f32, ndc: Vec2, depth: f32) -> Vec3 {
    let view_projection = camera.projection_matrix(aspect).unwrap() * camera.view_matrix();
    let clip_z = camera.projection_matrix(aspect).unwrap() * Vec4::new(0.0, 0.0, -depth, 1.0);
    let w = depth;
    let world = view_projection.inverse() * Vec4::new(ndc.x * w, ndc.y * w, clip_z.z, w);
    world.truncate() / world.w
}

fn distance_to_ray(point: Vec3, ray: Ray) -> f32 {
    (point - ray.origin).cross(ray.direction).length()
}

#[test]
fn rays_invert_the_projection_under_offset_roll_and_squeeze() {
    let camera = film_camera(|f| {
        f.film_offset_in = [0.3, 0.0];
        f.film_roll_degrees = 10.0;
        f.lens_squeeze = 1.5;
    });
    let aspect = 1.6;
    for (x, y) in [
        (1.0, 1.0),
        (-1.0, 1.0),
        (1.0, -1.0),
        (-1.0, -1.0),
        (0.0, 0.0),
    ] {
        let target = Vec2::new(x, y);
        let point = world_point_at_depth(&camera, aspect, target, 10.0);
        let projected = camera.project(point, aspect).unwrap().unwrap();
        assert!(
            (projected.truncate() - target).length() < 1e-4,
            "{target:?} {projected:?}"
        );
        let ray = camera.captured_ray(target, aspect).unwrap();
        let error = distance_to_ray(point, ray) / 10.0;
        assert!(error < 1e-4, "{target:?}: ray misses the point by {error}");
    }
}

/// The pre-#458 formula, restated: ndc divided by the matrix diagonal.
fn old_default_ray_direction(camera: &Camera, ndc: Vec2, aspect: f32) -> Vec3 {
    let m = camera.projection_matrix(aspect).unwrap();
    let view = Vec3::new(ndc.x / m.x_axis.x, ndc.y / m.y_axis.y, -1.0).normalize();
    camera
        .view_matrix()
        .inverse()
        .transform_vector3(view)
        .normalize()
}

#[test]
fn the_default_camera_ray_stays_within_a_millionth_of_the_old_formula() {
    let camera = Camera::default();
    for aspect in [0.5, 1.0, 1.5, 16.0 / 9.0] {
        for ndc in [
            Vec2::ZERO,
            Vec2::new(0.9, -0.3),
            Vec2::new(-1.0, 1.0),
            Vec2::new(0.2, 0.77),
        ] {
            let new = camera.ray(ndc, aspect).unwrap().direction;
            let old = old_default_ray_direction(&camera, ndc, aspect);
            assert!(
                (new - old).abs().max_element() < 1e-6,
                "{aspect} {ndc:?}: {new:?} {old:?}"
            );
        }
    }
}

fn centre_ndc(camera: &Camera, aspect: f32, centre: Vec3) -> Vec2 {
    camera.project(centre, aspect).unwrap().unwrap().truncate()
}

#[test]
fn framing_centres_the_object_in_the_shifted_window() {
    let points = [Vec3::new(2.0, 1.0, 0.5), Vec3::new(4.0, 3.0, 1.5)];
    let centre = Vec3::new(3.0, 2.0, 1.0);
    for edit in [
        (|f: &mut FilmBack| f.film_offset_in = [0.3, 0.0]) as fn(&mut FilmBack),
        |f| {
            f.film_offset_in = [0.3, 0.1];
            f.film_fit = FilmFit::Horizontal;
            f.film_fit_offset_in = 0.2;
            f.lens_squeeze = 1.5;
        },
    ] {
        let mut camera = film_camera(edit);
        camera.frame_points(&points, 1.5).unwrap();
        let ndc = centre_ndc(&camera, 1.5, centre);
        assert!(ndc.abs().max_element() < 1e-3, "{ndc:?}");
    }
    let mut unshifted = Camera::default();
    unshifted.frame_points(&points, 1.5).unwrap();
    assert!((unshifted.target() - centre).length() < 1e-6);
}

#[test]
fn orthographic_ignores_the_film_back() {
    let mut plain = Camera::default();
    plain.show_orthographic(OrthographicView::Front).unwrap();
    let mut odd = film_camera(|f| {
        f.focal_mm = 80.0;
        f.film_offset_in = [0.4, 0.4];
        f.film_roll_degrees = 33.0;
        f.overscan = 2.0;
    });
    odd.show_orthographic(OrthographicView::Front).unwrap();
    for aspect in [0.7, 1.5, 2.4] {
        assert_eq!(
            plain.projection_matrix(aspect).unwrap(),
            odd.projection_matrix(aspect).unwrap()
        );
    }
    let points = [Vec3::new(-1.0, -1.0, -1.0), Vec3::new(2.0, 2.0, 2.0)];
    let mut a = plain.clone();
    let mut b = odd.clone();
    a.frame_points(&points, 1.5).unwrap();
    b.frame_points(&points, 1.5).unwrap();
    assert_eq!(a.target(), b.target());
    assert_eq!(a.distance(), b.distance());
}

#[test]
fn handle_pixel_scale_is_unchanged_by_offset_and_roll() {
    use crate::move_handles::ViewportCapture;
    let plain = ViewportCapture::new(&Camera::default(), [0, 0], [800, 600], 1.0).unwrap();
    let moved = film_camera(|f| {
        f.film_offset_in = [0.3, 0.1];
        f.film_roll_degrees = 30.0;
    });
    let moved = ViewportCapture::new(&moved, [0, 0], [800, 600], 1.0).unwrap();
    let (a, b) = (
        plain.world_length(72.0, 10.0),
        moved.world_length(72.0, 10.0),
    );
    assert!((a - b).abs() <= a * 1e-6, "{a} vs {b}");
}
