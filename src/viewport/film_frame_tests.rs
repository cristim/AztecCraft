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
