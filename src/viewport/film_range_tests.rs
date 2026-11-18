//! Issue #458 T6: the film back ranges (PROVISIONAL, development.md row 56). Each bound is
//! accepted exactly and refused one step beyond, with the offending field named.
use super::*;

fn film(edit: impl FnOnce(&mut FilmBack)) -> FilmBack {
    let mut film = FilmBack::DEFAULT;
    edit(&mut film);
    film
}

fn next_up(v: f64) -> f64 {
    f64::from_bits(v.to_bits() + 1)
}

#[test]
fn overscan_and_scales_stay_within_their_provisional_bounds() {
    type Setter = fn(&mut FilmBack, f64);
    let cases: [(&str, f64, Setter); 3] = [
        ("overscan", MAX_OVERSCAN, |f, v| f.overscan = v),
        ("pre_scale", MAX_FILM_SCALE, |f, v| f.pre_scale = v),
        ("post_scale", MAX_FILM_SCALE, |f, v| f.post_scale = v),
    ];
    for (name, max, set) in cases {
        assert_eq!(
            film(|f| set(f, max)).validate(),
            Ok(()),
            "{name} at the cap"
        );
        assert_eq!(
            film(|f| set(f, next_up(max))).validate(),
            Err(InvalidFilmBack(name)),
            "{name} one step above the cap"
        );
    }
    assert_eq!(MAX_OVERSCAN, 1e3);
    assert_eq!(MAX_FILM_SCALE, 1e6);
}

#[test]
fn scales_have_a_symmetric_lower_bound() {
    type Setter = fn(&mut FilmBack, f64);
    let cases: [(&str, Setter); 2] = [
        ("pre_scale", |f, v| f.pre_scale = v),
        ("post_scale", |f, v| f.post_scale = v),
    ];
    for (name, set) in cases {
        assert_eq!(
            film(|f| set(f, MIN_FILM_SCALE)).validate(),
            Ok(()),
            "{name}"
        );
        assert_eq!(
            film(|f| set(f, MIN_FILM_SCALE * 0.99)).validate(),
            Err(InvalidFilmBack(name)),
            "{name}"
        );
    }
    let both = film(|f| (f.pre_scale, f.post_scale) = (1e-10, 1e-10));
    assert_eq!(both.validate(), Err(InvalidFilmBack("pre_scale")));
    assert_eq!(MIN_FILM_SCALE, 1e-6);
}

#[test]
fn offsets_are_bounded_in_tangent_space_per_field() {
    // Focal 25.4 mm makes one inch of offset one tangent unit; squeeze 1 for the plain fields.
    let unit = |f: &mut FilmBack| f.focal_mm = 25.4;
    type Setter = fn(&mut FilmBack, f64);
    let cases: [(&str, Setter); 3] = [
        ("horizontal_film_offset", |f, v| f.film_offset_in[0] = v),
        ("vertical_film_offset", |f, v| f.film_offset_in[1] = v),
        ("film_fit_offset", |f, v| f.film_fit_offset_in = v),
    ];
    for (name, set) in cases {
        for sign in [1.0, -1.0] {
            let at = |v: f64| {
                film(|f| {
                    unit(f);
                    set(f, sign * v);
                })
            };
            assert_eq!(at(MAX_OFFSET_TAN).validate(), Ok(()), "{name}");
            assert_eq!(
                at(MAX_OFFSET_TAN * 1.0001).validate(),
                Err(InvalidFilmBack(name)),
                "{name}"
            );
            assert_eq!(at(1e35).validate(), Err(InvalidFilmBack(name)), "{name}");
        }
    }
    let squeezed = film(|f| {
        unit(f);
        f.lens_squeeze = 2.0;
        f.film_offset_in[0] = MAX_OFFSET_TAN * 0.6;
    });
    assert_eq!(
        squeezed.validate(),
        Err(InvalidFilmBack("horizontal_film_offset")),
        "the squeeze scales the horizontal offset"
    );
    let fit = film(|f| {
        unit(f);
        f.lens_squeeze = 2.0;
        f.film_fit_offset_in = MAX_OFFSET_TAN * 0.6;
    });
    assert_eq!(
        fit.validate(),
        Err(InvalidFilmBack("film_fit_offset")),
        "a wide squeeze scales the fit offset"
    );
    assert_eq!(MAX_OFFSET_TAN, 1e5);
}

#[test]
fn an_accepted_offset_still_builds_a_finite_projection_at_extreme_aspects() {
    for fit in [
        FilmFit::Fill,
        FilmFit::Horizontal,
        FilmFit::Vertical,
        FilmFit::Overscan,
    ] {
        let lens = film(|f| {
            f.film_fit = fit;
            f.film_offset_in = [MAX_OFFSET_TAN * 0.5, -MAX_OFFSET_TAN * 0.5];
            f.film_fit_offset_in = MAX_OFFSET_TAN * 0.5;
            f.focal_mm = 25.4;
        });
        assert_eq!(lens.validate(), Ok(()), "{fit:?}");
        let camera = Camera {
            lens: Lens::Film(lens),
            ..Camera::default()
        };
        for aspect in [1e-3_f32, 1e3] {
            let m = camera.projection_matrix(aspect).unwrap();
            assert!(m.is_finite(), "{fit:?} at aspect {aspect}");
        }
    }
}

#[test]
fn the_angle_of_view_is_bounded_by_the_half_extent_at_aspect_one() {
    // At aspect 1 Fill is Vertical, so the half extent is vfa * lsr-free ty = vfa * 25.4 / (2 fl).
    let at = |tan: f64| film(|f| f.focal_mm = f.aperture_in[1] * 25.4 / (2.0 * tan));
    assert_eq!(at(MIN_TAN_HALF * 1.0001).validate(), Ok(()));
    assert_eq!(
        at(MIN_TAN_HALF * 0.9999).validate(),
        Err(InvalidFilmBack("angle_of_view"))
    );
    assert_eq!(at(MAX_TAN_HALF * 0.9999).validate(), Ok(()));
    assert_eq!(
        at(MAX_TAN_HALF * 1.0001).validate(),
        Err(InvalidFilmBack("angle_of_view"))
    );
    assert_eq!((MIN_TAN_HALF, MAX_TAN_HALF), (1e-4, 1e4));
}

#[test]
fn a_huge_aperture_or_squeeze_is_refused_by_the_same_bound() {
    let wide = film(|f| f.aperture_in = [1e9, 1e9]);
    assert_eq!(wide.validate(), Err(InvalidFilmBack("angle_of_view")));
    let squeezed = film(|f| {
        f.film_fit = FilmFit::Horizontal;
        f.lens_squeeze = 1e9;
    });
    assert_eq!(squeezed.validate(), Err(InvalidFilmBack("angle_of_view")));
}
