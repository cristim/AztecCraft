use super::*;
use crate::history::{Command, EditOutcome, History};
use crate::native_scene::{ascii, test_fixtures};
use crate::viewport::FilmFit;
use std::sync::Arc;

const CAMERAS: [SceneCamera; 4] = [
    SceneCamera::Persp,
    SceneCamera::Top,
    SceneCamera::Front,
    SceneCamera::Side,
];

fn film() -> FilmBack {
    FilmBack {
        focal_mm: 35.0_f64.next_down(),
        aperture_in: [1.25, 0.8125],
        film_fit: FilmFit::Horizontal,
        film_fit_offset_in: 0.015625,
        film_offset_in: [0.03125, -0.0625],
        lens_squeeze: 1.5,
        pre_scale: 1.1,
        post_scale: 0.9,
        film_roll_degrees: 33.3,
        overscan: 1.2,
        display_film_gate: true,
        camera_scale: 1.25,
        film_translate: [0.0625, -0.125],
        film_roll_pivot: [0.25, -0.375],
        film_roll_order: crate::viewport::FilmRollOrder::TranslateRotate,
    }
}

#[test]
fn no_override_reads_the_create_camera_defaults() {
    let scene = NativeScene::default();
    for camera in CAMERAS {
        assert!(scene.camera_film_back(camera).same_bits(&FilmBack::DEFAULT));
    }
    assert!(!scene.has_camera_overrides());
}

/// A film back that differs from the default in fields no other index shares.
fn distinct(index: usize) -> FilmBack {
    let mut value = FilmBack::DEFAULT;
    match index {
        0 => {
            value.focal_mm = 40.0;
            value.film_translate = [0.1, 0.0];
        }
        1 => {
            value.aperture_in = [1.1, 0.7];
            value.film_roll_pivot = [0.2, 0.3];
        }
        2 => {
            value.overscan = 1.3;
            value.film_offset_in = [0.05, -0.05];
            value.film_roll_order = crate::viewport::FilmRollOrder::TranslateRotate;
        }
        _ => {
            value.camera_scale = 2.0;
            value.pre_scale = 1.4;
            value.display_film_gate = true;
        }
    }
    value
}

#[test]
fn each_camera_keeps_its_own_exact_film_back() {
    let mut scene = NativeScene::default();
    for (index, camera) in CAMERAS.into_iter().enumerate() {
        scene.set_camera_film_back(camera, distinct(index)).unwrap();
    }
    for (index, camera) in CAMERAS.into_iter().enumerate() {
        assert!(
            scene.camera_film_back(camera).same_bits(&distinct(index)),
            "{camera:?}"
        );
    }
    assert!(scene.has_camera_overrides());
    // Clearing one camera leaves the other three alone.
    scene
        .set_camera_film_back(SceneCamera::Front, FilmBack::DEFAULT)
        .unwrap();
    for (index, camera) in CAMERAS.into_iter().enumerate() {
        let expected = if camera == SceneCamera::Front {
            FilmBack::DEFAULT
        } else {
            distinct(index)
        };
        assert!(
            scene.camera_film_back(camera).same_bits(&expected),
            "{camera:?}"
        );
    }
}

#[test]
fn an_invalid_film_back_is_refused_by_field_and_leaves_the_scene_untouched() {
    let mut scene = NativeScene::default();
    scene
        .set_camera_film_back(SceneCamera::Front, film())
        .unwrap();
    let before = scene.clone();
    type Bad = fn(&mut FilmBack);
    let cases: [(&str, Bad); 6] = [
        ("focal_mm", |f| f.focal_mm = f64::NAN),
        ("focal_mm", |f| f.focal_mm = 0.0),
        ("camera_scale", |f| f.camera_scale = -1.0),
        ("film_translate_h", |f| f.film_translate[0] = f64::INFINITY),
        ("horizontal_roll_pivot", |f| f.film_roll_pivot[0] = 1e300),
        ("angle_of_view", |f| f.focal_mm = 1e-300),
    ];
    for (field, edit) in cases {
        let mut bad = film();
        edit(&mut bad);
        assert_eq!(
            scene.set_camera_film_back(SceneCamera::Front, bad),
            Err(NativeError::InvalidFilmBack(field))
        );
        assert_eq!(scene, before, "{field}");
    }
}

#[test]
fn setting_the_default_bits_clears_the_override_and_a_negative_zero_keeps_it() {
    let mut scene = NativeScene::default();
    let original = scene.clone();
    scene
        .set_camera_film_back(SceneCamera::Persp, film())
        .unwrap();
    assert_ne!(scene, original);
    scene
        .set_camera_film_back(SceneCamera::Persp, FilmBack::DEFAULT)
        .unwrap();
    assert_eq!(scene, original);
    assert!(!scene.has_camera_overrides());
    let negative_zero = FilmBack {
        film_translate: [-0.0, 0.0],
        ..FilmBack::DEFAULT
    };
    scene
        .set_camera_film_back(SceneCamera::Persp, negative_zero)
        .unwrap();
    assert_ne!(scene, original, "-0.0 is an explicit statement");
    assert_eq!(
        scene.camera_film_back(SceneCamera::Persp).film_translate[0].to_bits(),
        (-0.0_f64).to_bits()
    );
}

#[test]
fn a_scene_opened_from_a_file_refuses_the_edit_and_stays_identical() {
    let source = b"//Maya ASCII 2026 scene\nrequires maya \"2026\"; currentUnit -l centimeter -a degree -t film;\n";
    let mut scene = ascii::decode(Arc::from(source.to_vec())).unwrap();
    assert!(scene.source.is_some());
    let before = scene.clone();
    assert_eq!(
        scene.set_camera_film_back(SceneCamera::Persp, film()),
        Err(NativeError::SourcedCameraEdit)
    );
    assert_eq!(scene, before);
}

#[test]
fn a_refusal_names_the_field_before_the_source_rule() {
    let source = b"//Maya ASCII 2026 scene\nrequires maya \"2026\"; currentUnit -l centimeter -a degree -t film;\n";
    let mut scene = ascii::decode(Arc::from(source.to_vec())).unwrap();
    let mut bad = film();
    bad.focal_mm = 0.0;
    assert_eq!(
        scene.set_camera_film_back(SceneCamera::Persp, bad),
        Err(NativeError::InvalidFilmBack("focal_mm"))
    );
}

#[test]
fn the_edit_is_undoable_and_redoable_and_a_failed_edit_records_nothing() {
    let (scene, _) = test_fixtures::cube();
    let original = scene.clone();
    let mut history = History::new(scene);
    let outcome = history
        .execute(Command::SetCameraFilmBack {
            camera: SceneCamera::Persp,
            film: film(),
        })
        .unwrap();
    assert_eq!(outcome, EditOutcome::Changed);
    assert_eq!(history.undo_label().as_deref(), Some("Camera"));
    assert!(
        history
            .scene()
            .camera_film_back(SceneCamera::Persp)
            .same_bits(&film())
    );
    assert!(history.undo());
    assert_eq!(history.scene(), &original);
    assert!(history.redo());
    assert!(
        history
            .scene()
            .camera_film_back(SceneCamera::Persp)
            .same_bits(&film())
    );
    // The same bits again change nothing and record nothing.
    let depth = history.revision();
    assert_eq!(
        history
            .execute(Command::SetCameraFilmBack {
                camera: SceneCamera::Persp,
                film: film(),
            })
            .unwrap(),
        EditOutcome::Unchanged
    );
    assert_eq!(history.revision(), depth);
    let mut bad = film();
    bad.camera_scale = 0.0;
    let before = history.scene().clone();
    assert_eq!(
        history.execute(Command::SetCameraFilmBack {
            camera: SceneCamera::Persp,
            film: bad,
        }),
        Err(NativeError::InvalidFilmBack("camera_scale"))
    );
    assert_eq!(history.scene(), &before);
    assert_eq!(history.revision(), depth);
    // Reverting to the default is one more undoable step that equals the original scene.
    history
        .execute(Command::SetCameraFilmBack {
            camera: SceneCamera::Persp,
            film: FilmBack::DEFAULT,
        })
        .unwrap();
    assert_eq!(history.scene(), &original);
}

#[test]
fn the_ascii_writer_saves_an_override_and_the_binary_writer_refuses_it_with_a_typed_reason() {
    let (mut scene, _) = test_fixtures::cube();
    let plain = ascii::write::emit_geometry_prefix(&scene).unwrap();
    #[cfg(unix)]
    assert!(crate::native_scene::binary::encode_authored(&scene).is_ok());
    scene
        .set_camera_film_back(SceneCamera::Top, film())
        .unwrap();
    let saved = ascii::write::emit_geometry_prefix(&scene).unwrap();
    assert!(saved.len() > plain.len());
    assert!(
        String::from_utf8(saved)
            .unwrap()
            .contains("select -ne :topShape;")
    );
    #[cfg(unix)]
    {
        use crate::native_scene::binary::AuthoredRefusalCode;
        let refusal = crate::native_scene::binary::encode_authored(&scene).unwrap_err();
        let code = AuthoredRefusalCode::from(&refusal);
        assert_eq!(code, AuthoredRefusalCode::CameraOverride);
        assert_eq!(code.as_str(), "camera_override");
    }
    scene
        .set_camera_film_back(SceneCamera::Top, FilmBack::DEFAULT)
        .unwrap();
    assert_eq!(ascii::write::emit_geometry_prefix(&scene).unwrap(), plain);
    #[cfg(unix)]
    assert!(crate::native_scene::binary::encode_authored(&scene).is_ok());
}

#[test]
fn scenes_that_differ_only_by_a_negative_zero_are_not_equal() {
    let mut positive = NativeScene::default();
    let mut negative = NativeScene::default();
    let base = FilmBack {
        focal_mm: 50.0,
        ..FilmBack::DEFAULT
    };
    positive
        .set_camera_film_back(SceneCamera::Side, base)
        .unwrap();
    negative
        .set_camera_film_back(
            SceneCamera::Side,
            FilmBack {
                film_offset_in: [-0.0, 0.0],
                ..base
            },
        )
        .unwrap();
    assert_ne!(positive, negative);
    assert_eq!(positive, positive.clone());
}
