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
