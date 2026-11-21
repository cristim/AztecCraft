//! Issue #458 S1: camera scale, Film Translate, Film Roll Pivot and Film Roll Order. Expected
//! values are worked by hand (all PROVISIONAL: the docs state neither unit, direction nor order
//! semantics), never read back from `FilmBack`.
use super::*;

fn film(edit: impl FnOnce(&mut FilmBack)) -> FilmBack {
    let mut film = FilmBack::DEFAULT;
    edit(&mut film);
    film
}

fn camera(film: FilmBack) -> Camera {
    Camera {
        lens: Lens::Film(film),
        ..Camera::default()
    }
}

fn next_up(v: f64) -> f64 {
    f64::from_bits(v.to_bits() + 1)
}

fn next_down(v: f64) -> f64 {
    f64::from_bits(v.to_bits() - 1)
}

#[test]
fn the_new_fields_default_to_neutral() {
    let film = FilmBack::DEFAULT;
    assert_eq!(film.camera_scale, 1.0);
    assert_eq!(film.film_translate, [0.0, 0.0]);
    assert_eq!(film.film_roll_pivot, [0.0, 0.0]);
    assert_eq!(film.film_roll_order, FilmRollOrder::RotateTranslate);
}

#[test]
fn each_new_field_is_refused_by_name_and_accepted_at_its_boundary() {
    type Setter = fn(&mut FilmBack, f64);
    let shifts: [(&str, Setter); 4] = [
        ("film_translate_h", |f, v| f.film_translate[0] = v),
        ("film_translate_v", |f, v| f.film_translate[1] = v),
        ("horizontal_roll_pivot", |f, v| f.film_roll_pivot[0] = v),
        ("vertical_roll_pivot", |f, v| f.film_roll_pivot[1] = v),
    ];
    for (name, set) in shifts {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                film(|f| set(f, bad)).validate(),
                Err(InvalidFilmBack(name)),
                "{name} {bad}"
            );
        }
        for good in [0.0, -0.0, -1.0, MAX_NDC_SHIFT, -MAX_NDC_SHIFT] {
            assert_eq!(film(|f| set(f, good)).validate(), Ok(()), "{name} {good}");
        }
        for bad in [next_up(MAX_NDC_SHIFT), -next_up(MAX_NDC_SHIFT), 1e300] {
            assert_eq!(
                film(|f| set(f, bad)).validate(),
                Err(InvalidFilmBack(name)),
                "{name} {bad}"
            );
        }
    }
    for bad in [
        f64::NAN,
        f64::INFINITY,
        0.0,
        -1.0,
        next_down(MIN_FILM_SCALE),
        next_up(MAX_FILM_SCALE),
    ] {
        assert_eq!(
            film(|f| f.camera_scale = bad).validate(),
            Err(InvalidFilmBack("camera_scale")),
            "camera_scale {bad}"
        );
    }
    for good in [0.01, 1.0, 100.0] {
        assert_eq!(
            film(|f| f.camera_scale = good).validate(),
            Ok(()),
            "camera_scale {good}"
        );
    }
}

#[test]
fn camera_scale_counts_in_the_angle_of_view_bound() {
    // The scale range edges leave no usable angle of view at focal length 35 (PROVISIONAL).
    for edge in [MIN_FILM_SCALE, MAX_FILM_SCALE] {
        let extreme = film(|f| f.camera_scale = edge);
        assert_eq!(extreme.validate(), Err(InvalidFilmBack("angle_of_view")));
    }
}

#[test]
fn same_capture_flips_on_each_new_field_and_on_negative_zero() {
    type Edit = fn(&mut FilmBack);
    let base = Camera::default();
    let edits: [(&str, Edit); 12] = [
        ("cs", |f| f.camera_scale = 2.0),
        ("ftx", |f| f.film_translate[0] = 0.1),
        ("fty", |f| f.film_translate[1] = 0.1),
        ("rpx", |f| f.film_roll_pivot[0] = 0.1),
        ("rpy", |f| f.film_roll_pivot[1] = 0.1),
        ("order", |f| {
            f.film_roll_order = FilmRollOrder::TranslateRotate
        }),
        ("cs -ulp", |f| f.camera_scale = next_down(1.0)),
        ("ftx -0", |f| f.film_translate[0] = -0.0),
        ("fty -0", |f| f.film_translate[1] = -0.0),
        ("rpx -0", |f| f.film_roll_pivot[0] = -0.0),
        ("rpy -0", |f| f.film_roll_pivot[1] = -0.0),
        ("ftx ulp", |f| f.film_translate[0] = f64::from_bits(1)),
    ];
    for (name, edit) in edits {
        assert!(!base.same_capture(&camera(film(edit))), "{name}");
    }
}

#[test]
fn an_edit_sets_only_the_named_new_fields() {
    let edit = FilmBackEdit {
        camera_scale: Some(2.0),
        film_translate: [None, Some(0.25)],
        film_roll_pivot: [Some(-0.5), None],
        film_roll_order: Some(FilmRollOrder::TranslateRotate),
        ..FilmBackEdit::default()
    };
    assert!(!edit.is_empty());
    let applied = edit.applied_to(FilmBack::DEFAULT);
    assert_eq!(
        applied,
        FilmBack {
            camera_scale: 2.0,
            film_translate: [0.0, 0.25],
            film_roll_pivot: [-0.5, 0.0],
            film_roll_order: FilmRollOrder::TranslateRotate,
            ..FilmBack::DEFAULT
        }
    );
}
