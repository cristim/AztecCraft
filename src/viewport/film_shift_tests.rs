//! Issue #458 S1: camera scale, Film Translate, Film Roll Pivot and Film Roll Order. Expected
//! values are worked by hand (all PROVISIONAL: the docs state neither unit, direction nor order
//! semantics), never read back from `FilmBack`.
use super::*;

const TX_DEFAULT: f64 = 1.4173 * 25.4 / 70.0;
const TY_DEFAULT: f64 = 0.9449 * 25.4 / 70.0;

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

/// NDC of the point at view-space tangent `tan`, ten units in front of the camera.
fn ndc_of(camera: &Camera, aspect: f32, tan: [f64; 2]) -> [f64; 2] {
    let depth = 10.0_f32;
    let view = Vec3::new(tan[0] as f32 * depth, tan[1] as f32 * depth, -depth);
    let world = camera.view_matrix().inverse().transform_point3(view);
    let ndc = camera.project(world, aspect).unwrap().unwrap();
    [f64::from(ndc.x), f64::from(ndc.y)]
}

fn close(actual: [f64; 2], expected: [f64; 2], what: &str) {
    for axis in 0..2 {
        assert!(
            (actual[axis] - expected[axis]).abs() <= 2e-5,
            "{what}: {actual:?} vs {expected:?}"
        );
    }
}

/// Bit patterns of `projection_matrix` and `film_gate_ndc` computed from the code before the
/// camera scale, translate and pivot existed (two films: the default and one with every older
/// attribute set). Neutral new fields must reproduce them exactly, including the -0.0 entries.
#[test]
fn neutral_new_fields_reproduce_the_pre_s1_matrices_and_gate_bit_for_bit() {
    let busy = |f: &mut FilmBack| {
        f.film_offset_in = [0.1, -0.05];
        f.film_roll_degrees = 30.0;
        f.lens_squeeze = 1.5;
        f.overscan = 1.2;
        f.pre_scale = 1.1;
        f.post_scale = 0.9;
        f.film_fit = FilmFit::Horizontal;
        f.film_fit_offset_in = 0.02;
    };
    let cameras = [Camera::default(), camera(film(busy))];
    let projections: [(usize, f32, [u32; 16]); 8] = [
        (
            0,
            0.5_f32,
            [
                1085974976, 0, 0, 0, 2147483648, 1077586368, 0, 0, 0, 0, 3212836948, 3212836864, 0,
                0, 3184315731, 0,
            ],
        ),
        (
            0,
            1_f32,
            [
                1077586368, 0, 0, 0, 2147483648, 1077586368, 0, 0, 0, 0, 3212836948, 3212836864, 0,
                0, 3184315731, 0,
            ],
        ),
        (
            0,
            1.5_f32,
            [
                1073276053, 0, 0, 0, 2147483648, 1077586800, 0, 0, 0, 0, 3212836948, 3212836864, 0,
                0, 3184315731, 0,
            ],
        ),
        (
            0,
            2_f32,
            [
                1073276053, 0, 0, 0, 2147483648, 1081664661, 0, 0, 0, 0, 3212836948, 3212836864, 0,
                0, 3184315731, 0,
            ],
        ),
        (
            1,
            0.5_f32,
            [
                1064114738, 1049158687, 0, 0, 3205030943, 1055726130, 0, 0, 1038504740, 1016845584,
                3212836948, 3212836864, 0, 0, 3184315731, 0,
            ],
        ),
        (
            1,
            1_f32,
            [
                1064114738, 1057547295, 0, 0, 3205030943, 1064114738, 0, 0, 1038504740, 1025234192,
                3212836948, 3212836864, 0, 0, 3184315731, 0,
            ],
        ),
        (
            1,
            1.5_f32,
            [
                1064114738, 1062032942, 0, 0, 3205030943, 1068618662, 0, 0, 1038504740, 1030340504,
                3212836948, 3212836864, 0, 0, 3184315731, 0,
            ],
        ),
        (
            1,
            2_f32,
            [
                1064114738, 1065935903, 0, 0, 3205030943, 1072503346, 0, 0, 1038504740, 1033622800,
                3212836948, 3212836864, 0, 0, 3184315731, 0,
            ],
        ),
    ];
    for (index, aspect, expected) in projections {
        let matrix = cameras[index].projection_matrix(aspect).unwrap();
        assert_eq!(
            matrix.to_cols_array().map(f32::to_bits),
            expected,
            "camera {index} aspect {aspect}"
        );
    }
    let gates: [(f32, [[u32; 2]; 4]); 4] = [
        (
            0.5_f32,
            [
                [3204840338, 3200144845],
                [1063768977, 1025960755],
                [1057617114, 1052210129],
                [3210992201, 3177052942],
            ],
        ),
        (
            1_f32,
            [
                [3204840338, 3208533453],
                [1063768977, 1034349363],
                [1057617114, 1060598737],
                [3210992201, 3185441550],
            ],
        ),
        (
            1.5_f32,
            [
                [3204840338, 3213803610],
                [1063768977, 1039818956],
                [1057617114, 1065981661],
                [3210992201, 3190193226],
            ],
        ),
        (
            2_f32,
            [
                [3204840338, 3216922061],
                [1063768977, 1042737971],
                [1057617114, 1068987345],
                [3210992201, 3193830158],
            ],
        ),
    ];
    let busy_film = film(busy);
    for (aspect, expected) in gates {
        let gate = film_gate_ndc(&busy_film, aspect).map(|v| [v.x.to_bits(), v.y.to_bits()]);
        assert_eq!(gate, expected, "gate aspect {aspect}");
    }
}

#[test]
fn each_new_field_is_refused_by_name_and_accepted_at_its_boundary() {
    assert_eq!(MAX_NDC_SHIFT, 1e5);
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
fn the_offset_bound_uses_the_effective_focal_length() {
    // Focal 25.4 mm: a 1e5 inch offset is exactly the bound at scale 1 and one step over at
    // scale 0.5 (effective focal 12.7 mm doubles the tangent), while scale 2 halves it.
    let at = |scale: f64, offset: f64| {
        film(|f| {
            f.focal_mm = 25.4;
            f.camera_scale = scale;
            f.film_offset_in = [0.0, offset];
        })
        .validate()
    };
    assert_eq!(at(1.0, 1e5), Ok(()));
    assert_eq!(at(0.5, 1e5), Err(InvalidFilmBack("vertical_film_offset")));
    assert_eq!(at(2.0, 2e5), Ok(()));
    assert_eq!(
        at(2.0, 2.0 * next_up(1e5)),
        Err(InvalidFilmBack("vertical_film_offset"))
    );
}

#[test]
fn accepted_extreme_shifts_still_build_a_finite_projection_at_extreme_aspects() {
    for fit in [
        FilmFit::Fill,
        FilmFit::Horizontal,
        FilmFit::Vertical,
        FilmFit::Overscan,
    ] {
        for order in [
            FilmRollOrder::RotateTranslate,
            FilmRollOrder::TranslateRotate,
        ] {
            let extreme = film(|f| {
                f.film_fit = fit;
                f.film_roll_order = order;
                f.film_translate = [MAX_NDC_SHIFT, -MAX_NDC_SHIFT];
                f.film_roll_pivot = [-MAX_NDC_SHIFT, MAX_NDC_SHIFT];
                f.film_roll_degrees = 33.0;
                f.post_scale = MAX_FILM_SCALE;
            });
            assert_eq!(extreme.validate(), Ok(()), "{fit:?} {order:?}");
            for aspect in [1e-3_f32, 1e3] {
                let matrix = camera(extreme).projection_matrix(aspect).unwrap();
                assert!(matrix.is_finite(), "{fit:?} {order:?} at aspect {aspect}");
            }
        }
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

#[test]
fn camera_scale_two_halves_the_extents_and_the_offset() {
    // fl * cs: default Fill at aspect 2 is horizontal, half extents (tx, tx / 2) halved.
    let scaled = camera(film(|f| f.camera_scale = 2.0));
    let extents = {
        let m = scaled.projection_matrix(2.0).unwrap();
        [1.0 / f64::from(m.x_axis.x), 1.0 / f64::from(m.y_axis.y)]
    };
    assert!((extents[0] - TX_DEFAULT / 2.0).abs() < 1e-6, "{extents:?}");
    assert!((extents[1] - TX_DEFAULT / 4.0).abs() < 1e-6, "{extents:?}");
    // The offset in inches keeps its fraction of the gate: tan offset 0.1 * 25.4 / (35 * 2).
    let offset = film(|f| {
        f.camera_scale = 2.0;
        f.film_offset_in = [0.1, -0.05];
    });
    let centre = offset.window(1.0).centre();
    assert!((centre.x - 0.1 * 25.4 / 70.0).abs() < 1e-12);
    assert!((centre.y + 0.05 * 25.4 / 70.0).abs() < 1e-12);
    // The reported focal length is the attribute, not the scaled one.
    assert_eq!(Lens::Film(offset).focal_length_mm(), Some(35.0));
}

#[test]
fn film_translate_moves_the_axis_point_by_its_ndc_value_at_any_aspect() {
    for aspect in [1.0_f32, 2.0, 0.5] {
        let moved = camera(film(|f| f.film_translate = [0.5, -0.25]));
        close(
            ndc_of(&moved, aspect, [0.0, 0.0]),
            [0.5, -0.25],
            "translate",
        );
        // The pre scale does not touch the translate; the post scale multiplies it.
        let pre = camera(film(|f| {
            f.film_translate = [0.5, 0.0];
            f.pre_scale = 2.0;
        }));
        close(ndc_of(&pre, aspect, [0.0, 0.0]), [0.5, 0.0], "pre scale");
        let post = camera(film(|f| {
            f.film_translate = [0.5, 0.0];
            f.post_scale = 3.0;
        }));
        close(ndc_of(&post, aspect, [0.0, 0.0]), [1.5, 0.0], "post scale");
    }
}

#[test]
fn roll_about_a_pivot_has_hand_computed_images() {
    let rolled = camera(film(|f| {
        f.film_roll_degrees = 90.0;
        f.film_roll_pivot = [0.5, 0.0];
    }));
    // Aspect 1: the pivot point is fixed and (0, 0) turns counter-clockwise about it.
    let ty = TY_DEFAULT;
    close(
        ndc_of(&rolled, 1.0, [0.5 * ty, 0.0]),
        [0.5, 0.0],
        "pivot fixed",
    );
    close(ndc_of(&rolled, 1.0, [0.0, 0.0]), [0.5, -0.5], "a1 origin");
    // Aspect 2 rotates in pixel space: (-0.5, 0) from the pivot is 1 pixel unit left, and its
    // image is 1 pixel unit below, i.e. ndc (0.5, -1) (x is scaled by the aspect).
    close(ndc_of(&rolled, 2.0, [0.0, 0.0]), [0.5, -1.0], "a2 origin");
    let tx = TX_DEFAULT;
    close(
        ndc_of(&rolled, 2.0, [0.5 * tx, 0.0]),
        [0.5, 0.0],
        "a2 pivot fixed",
    );
}

#[test]
fn film_roll_order_picks_rotate_then_translate_or_the_reverse() {
    let configure = |order| {
        camera(film(|f| {
            f.film_translate = [0.5, 0.0];
            f.film_roll_degrees = 90.0;
            f.pre_scale = 2.0;
            f.post_scale = 3.0;
            f.film_roll_order = order;
        }))
    };
    close(
        ndc_of(&configure(FilmRollOrder::RotateTranslate), 1.0, [0.0, 0.0]),
        [1.5, 0.0],
        "rotate then translate",
    );
    close(
        ndc_of(&configure(FilmRollOrder::TranslateRotate), 1.0, [0.0, 0.0]),
        [0.0, 1.5],
        "translate then rotate",
    );
}

#[test]
fn the_film_gate_moves_with_the_translate() {
    let neutral = film_gate_ndc(&FilmBack::DEFAULT, 1.5);
    let moved = film_gate_ndc(&film(|f| f.film_translate = [0.5, -0.25]), 1.5);
    for (a, b) in neutral.iter().zip(moved) {
        assert!((b.x - a.x - 0.5).abs() < 1e-6 && (b.y - a.y + 0.25).abs() < 1e-6);
    }
}

#[test]
fn rays_invert_the_projection_with_every_new_attribute_set() {
    // Round trip through f32 matrices: within 2e-4 in NDC, not exact.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    for case in 0..60 {
        let camera = camera(film(|f| {
            f.camera_scale = 0.5 + next();
            f.film_translate = [next() - 0.5, next() - 0.5];
            f.film_roll_pivot = [next() - 0.5, next() - 0.5];
            f.film_roll_degrees = next() * 360.0 - 180.0;
            f.film_roll_order = if case % 2 == 0 {
                FilmRollOrder::RotateTranslate
            } else {
                FilmRollOrder::TranslateRotate
            };
            f.pre_scale = 0.5 + next();
            f.post_scale = 0.5 + next();
        }));
        let aspect = 0.5 + 2.0 * next() as f32;
        let ndc = Vec2::new(next() as f32 * 1.6 - 0.8, next() as f32 * 1.6 - 0.8);
        let ray = camera.captured_ray(ndc, aspect).unwrap();
        let point = ray.origin + ray.direction * 20.0;
        let back = camera.project(point, aspect).unwrap().unwrap();
        assert!(
            (back.truncate() - ndc).abs().max_element() < 2e-4,
            "case {case}: {back:?} vs {ndc:?}"
        );
    }
}
