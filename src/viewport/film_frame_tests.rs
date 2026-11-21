//! Issue #458 S1c: Frame All and Frame Selected with the whole film back (roll, scales, translate,
//! pivot, camera scale). Every framed point must land inside the view.
use super::*;

fn cube() -> Vec<Vec3> {
    [-1.0, 1.0]
        .into_iter()
        .flat_map(|x| {
            [-1.0, 1.0]
                .into_iter()
                .flat_map(move |y| [-1.0, 1.0].into_iter().map(move |z| Vec3::new(x, y, z)))
        })
        .collect()
}

fn max_ndc(camera: &Camera, points: &[Vec3], aspect: f32) -> f32 {
    points
        .iter()
        .map(|p| {
            camera
                .project(*p, aspect)
                .unwrap()
                .expect("point in front")
                .truncate()
                .abs()
                .max_element()
        })
        .fold(0.0, f32::max)
}

type Edit = fn(&mut FilmBack);

const FITS: [FilmFit; 4] = [
    FilmFit::Fill,
    FilmFit::Horizontal,
    FilmFit::Vertical,
    FilmFit::Overscan,
];

fn framed(fit: FilmFit, edit: Edit, aspect: f32) -> (Camera, Vec<Vec3>) {
    let mut film = FilmBack::DEFAULT;
    film.film_fit = fit;
    edit(&mut film);
    let mut camera = Camera {
        lens: Lens::Film(film),
        ..Camera::default()
    };
    let points = cube();
    camera.frame_points(&points, aspect).unwrap();
    (camera, points)
}

/// The cube is inscribed in a sphere of radius sqrt(3) framed with `DEFAULT_FRAME_MARGIN`, so on
/// the axis its nearest corner reaches at least 1 / (sqrt(3) * margin) of the view; the factor
/// 0.9 leaves room for the off-axis stretch of the projected sphere.
fn min_reach() -> f32 {
    0.9 / (3.0_f32.sqrt() * DEFAULT_FRAME_MARGIN)
}

#[test]
fn framing_keeps_every_point_inside_with_each_attribute_set() {
    let edits: [(&str, Edit); 8] = [
        ("shrink", |f| {
            f.pre_scale = 0.5;
            f.post_scale = 0.8;
            f.film_roll_degrees = 20.0;
        }),
        ("roll", |f| f.film_roll_degrees = 60.0),
        ("scales", |f| {
            f.pre_scale = 2.0;
            f.post_scale = 1.5;
        }),
        ("camera scale", |f| f.camera_scale = 1.7),
        ("translate", |f| f.film_translate = [0.8, -0.5]),
        ("pivot roll", |f| {
            f.film_roll_degrees = 40.0;
            f.film_roll_pivot = [0.7, 0.4];
            f.film_translate = [-0.3, 0.2];
        }),
        ("translate then rotate", |f| {
            f.film_roll_order = FilmRollOrder::TranslateRotate;
            f.film_roll_degrees = 75.0;
            f.film_translate = [0.4, 0.6];
        }),
        ("everything", |f| {
            f.film_roll_degrees = 60.0;
            f.film_offset_in = [0.12, -0.08];
            f.pre_scale = 2.0;
            f.post_scale = 1.5;
            f.overscan = 1.2;
            f.camera_scale = 1.3;
            f.film_translate = [0.8, -0.5];
            f.film_roll_pivot = [0.7, 0.4];
        }),
    ];
    for (name, edit) in edits {
        for fit in FITS {
            for aspect in [0.6_f32, 1.5, 2.4] {
                let (camera, points) = framed(fit, edit, aspect);
                let reach = max_ndc(&camera, &points, aspect);
                assert!(
                    (min_reach()..=1.00001).contains(&reach),
                    "{name} {fit:?} {aspect}: reach {reach}"
                );
            }
        }
    }
}

#[test]
fn the_framed_centre_lands_at_the_screen_centre_despite_translate_and_offset() {
    let (camera, _) = framed(
        FilmFit::Horizontal,
        |f| {
            f.film_translate = [0.6, -0.4];
            f.film_offset_in = [0.1, 0.05];
            f.film_roll_degrees = 30.0;
        },
        1.5,
    );
    let centre = camera.project(Vec3::ZERO, 1.5).unwrap().unwrap();
    assert!(centre.truncate().abs().max_element() < 1e-3, "{centre}");
}

#[test]
fn neutral_values_keep_the_frame_ratio_and_the_aim_bit_for_bit() {
    let window = BUSY.window(1.5);
    assert!((window.frame_ratio() - 1.0).abs() < 1e-12);
    let aim = window.tan_of_ndc(DVec2::ZERO);
    assert_eq!(aim.x.to_bits(), window.centre().x.to_bits());
    assert_eq!(aim.y.to_bits(), window.centre().y.to_bits());
}

const BUSY: FilmBack = FilmBack {
    film_offset_in: [0.1, -0.05],
    lens_squeeze: 1.5,
    film_fit: FilmFit::Horizontal,
    film_fit_offset_in: 0.02,
    ..FilmBack::DEFAULT
};

/// `(target bits, distance bits, far bits)` of `frame_points(cube)` computed by the code before
/// S1c (f1f6e2ff) for the default film, a neutral-map film with offset, squeeze and fit offset,
/// and an orthographic view, at aspects 0.6, 1, 1.5 and 2.4. These must not change.
#[allow(clippy::type_complexity)]
const BEFORE_S1C: [(&str, [([u32; 3], u32, u32); 4]); 3] = [
    (
        "default",
        [
            ([0, 0, 0], 1091594493, 1176256512),
            ([0, 0, 0], 1085501432, 1176256512),
            ([0, 0, 0], 1085501804, 1176256512),
            ([0, 0, 0], 1091231916, 1176256512),
        ],
    ),
    (
        "busy",
        [
            ([3197125468, 1031049149, 1042682424], 1077841786, 1176256512),
            ([3197125468, 1031049149, 1042682424], 1077841786, 1176256512),
            ([3200297777, 1034112886, 1046334286], 1082032159, 1176256512),
            ([3205495350, 1039406013, 1051049476], 1086205659, 1176256512),
        ],
    ),
    (
        "ortho",
        [
            ([0, 0, 0], 1091400374, 1176256512),
            ([0, 0, 0], 1084865756, 1176256512),
            ([0, 0, 0], 1084866147, 1176256512),
            ([0, 0, 0], 1091029890, 1176256512),
        ],
    ),
];

#[test]
fn framing_is_unchanged_for_neutral_map_films_and_orthographic_views() {
    let points = cube();
    for (name, expected) in BEFORE_S1C {
        let base = match name {
            "default" => Camera::default(),
            "busy" => Camera {
                lens: Lens::Film(BUSY),
                ..Camera::default()
            },
            _ => {
                let mut camera = Camera::default();
                camera.show_orthographic(OrthographicView::Front).unwrap();
                camera
            }
        };
        for (aspect, (target, distance, far)) in [0.6_f32, 1.0, 1.5, 2.4].into_iter().zip(expected)
        {
            let mut camera = base.clone();
            camera.frame_points(&points, aspect).unwrap();
            assert_eq!(
                camera.target().to_array().map(f32::to_bits),
                target,
                "{name} {aspect}"
            );
            assert_eq!(camera.distance().to_bits(), distance, "{name} {aspect}");
            assert_eq!(camera.far().to_bits(), far, "{name} {aspect}");
        }
    }
}

#[test]
fn an_overscan_below_one_now_widens_the_distance_to_the_real_visible_area() {
    // Before S1c overscan 0.8 framed exactly like overscan 1 (distance bits 1085501432 at
    // aspect 1); the widening projects through the real film, so the points stay inside.
    let (camera, points) = framed(FilmFit::Fill, |f| f.overscan = 0.8, 1.0);
    assert!(camera.distance() > f32::from_bits(1085501432));
    assert!(max_ndc(&camera, &points, 1.0) <= 1.00001);
}

#[test]
fn the_sphere_framing_alone_fits_a_scaled_film_on_axis() {
    // Widening off: the sphere half angle must already carry the 2D scale. Pre scale 2 halves the
    // tangent the sphere may fill: distance = sqrt(3) / sin(atan(ty / 2)) * margin, by hand.
    for (scale, ratio) in [(2.0, 0.5), (0.5, 2.0)] {
        let mut camera = Camera {
            lens: Lens::Film(FilmBack {
                pre_scale: scale,
                ..FilmBack::DEFAULT
            }),
            ..Camera::default()
        };
        let points = cube();
        camera
            .frame_iter_widening(points.iter().copied(), 1.0, 0)
            .unwrap();
        let tan = TY_DEFAULT * ratio;
        let expected = 3.0_f64.sqrt() / tan.atan().sin() * f64::from(DEFAULT_FRAME_MARGIN);
        assert!(
            (f64::from(camera.distance()) - expected).abs() < 1e-4 * expected,
            "scale {scale}: {} vs {expected}",
            camera.distance()
        );
        let reach = max_ndc(&camera, &points, 1.0);
        assert!(
            (min_reach()..=1.00001).contains(&reach),
            "scale {scale}: reach {reach}"
        );
    }
}

const TY_DEFAULT: f64 = 0.9449 * 25.4 / 70.0;

fn harsh(f: &mut FilmBack) {
    f.film_roll_degrees = 60.0;
    f.pre_scale = 2.0;
    f.post_scale = 1.5;
    f.film_translate = [0.8, -0.5];
    f.film_roll_pivot = [0.7, 0.4];
}

/// Off-axis enough that the sphere framing alone leaves points outside (reach 1.18 at aspect 0.6).
fn off_axis(f: &mut FilmBack) {
    f.film_fit = FilmFit::Horizontal;
    f.film_roll_order = FilmRollOrder::TranslateRotate;
    f.film_roll_degrees = 75.0;
    f.film_translate = [0.4, 0.6];
}

#[test]
fn a_widening_that_does_not_converge_keeps_the_sphere_distance_and_aim() {
    let make = |steps: usize| {
        let mut film = FilmBack::DEFAULT;
        off_axis(&mut film);
        let mut camera = Camera {
            lens: Lens::Film(film),
            ..Camera::default()
        };
        camera
            .frame_iter_widening(cube().into_iter(), 0.6, steps)
            .unwrap();
        camera
    };
    // Zero widening steps take the same path as a widening that never converges: the sphere
    // distance and aim stand. This roll keeps the disc ratio at 1 (pixel-space rotation), so the
    // sphere distance is sqrt(3) / sin(atan(tx)) * margin by hand, tx the horizontal half extent.
    let sphere = make(0);
    let tx: f64 = 1.4173 * 25.4 / 70.0;
    let expected = 3.0_f64.sqrt() / tx.atan().sin() * f64::from(DEFAULT_FRAME_MARGIN);
    assert!((f64::from(sphere.distance()) - expected).abs() < 1e-4 * expected);
    assert!(max_ndc(&sphere, &cube(), 0.6) > 1.1);
    let many = make(FRAME_REFINE_STEPS);
    assert!(many.distance() > sphere.distance());
    assert!(max_ndc(&many, &cube(), 0.6) <= 1.00001);
    // The sphere framing keeps the aim: the centre sits at the screen centre.
    let centre = sphere.project(Vec3::ZERO, 0.6).unwrap().unwrap();
    assert!(centre.truncate().abs().max_element() < 1e-3, "{centre}");
}

#[test]
fn extreme_and_degenerate_point_sets_still_frame() {
    let shifted =
        |z: f32| -> Vec<Vec3> { cube().iter().map(|p| *p + Vec3::new(0.0, 0.0, z)).collect() };
    for edit in [(|_: &mut FilmBack| {}) as Edit, harsh] {
        let lens = Lens::Film({
            let mut film = FilmBack::DEFAULT;
            edit(&mut film);
            film
        });
        for points in [
            shifted(1e15),
            vec![Vec3::new(3.0, 4.0, 5.0)],
            vec![Vec3::ZERO; 3],
        ] {
            let mut camera = Camera {
                lens,
                ..Camera::default()
            };
            camera.frame_points(&points, 1.5).unwrap();
        }
        // One point has zero radius: it frames at the minimum distance and is inside the view.
        let mut camera = Camera {
            lens,
            ..Camera::default()
        };
        let point = [Vec3::new(3.0, 4.0, 5.0)];
        camera.frame_points(&point, 1.5).unwrap();
        assert!(max_ndc(&camera, &point, 1.5) <= 1.00001);
    }
}

#[test]
fn a_widening_beyond_its_range_gives_up_and_keeps_the_sphere_framing() {
    let film = FilmBack {
        film_translate: [MAX_NDC_SHIFT, 0.0],
        ..FilmBack::DEFAULT
    };
    let camera = Camera {
        lens: Lens::Film(film),
        ..Camera::default()
    };
    let points = cube();
    let mut sphere = camera.clone();
    sphere
        .frame_iter_widening(points.iter().copied(), 1.0, 0)
        .unwrap();
    let aim = film.window(1.0).tan_of_ndc(DVec2::ZERO);
    let view = camera.view_matrix().inverse();
    let side = view.x_axis.truncate() * aim.x as f32 + view.y_axis.truncate() * aim.y as f32;
    let frame = |max_widening| FilmFrame {
        camera: &sphere,
        target: Vec3::ZERO,
        side,
        radius: 3.0_f32.sqrt(),
        allowed: 1.0,
        aspect: 1.0,
        max_widening,
    };
    let start = sphere.distance();
    assert!(
        frame(MAX_FRAME_WIDENING)
            .widened_distance(points.iter().copied(), start, FRAME_REFINE_STEPS)
            .is_some_and(|distance| distance > start)
    );
    for max_widening in [1.0, 2.0] {
        assert_eq!(
            frame(max_widening).widened_distance(points.iter().copied(), start, FRAME_REFINE_STEPS),
            None,
            "{max_widening}"
        );
    }
    // Non-finite points abort the widening instead of failing the framing.
    assert_eq!(
        frame(MAX_FRAME_WIDENING).widened_distance([Vec3::NAN].into_iter(), start, 3),
        None
    );
}
