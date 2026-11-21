//! Issue #458 S2c: the authored `.ma` camera blocks. Expected values are the writer's documented
//! form (`select -ne :perspShape;` then one `setAttr` per film back attribute, PROVISIONAL: Maya
//! acceptance of this form is unproved) and exact f64 bits.
use super::*;
use crate::native_scene::test_fixtures;
use crate::viewport::{FilmFit, FilmRollOrder};

const CAMERAS: [SceneCamera; 4] = [
    SceneCamera::Persp,
    SceneCamera::Top,
    SceneCamera::Front,
    SceneCamera::Side,
];

fn film(index: usize) -> FilmBack {
    let mut value = FilmBack::DEFAULT;
    match index {
        0 => {
            value.focal_mm = 35.0_f64.next_down();
            value.film_translate = [-0.0, 0.0625];
            value.film_roll_degrees = -0.0;
        }
        1 => {
            value.aperture_in = [1.1, 0.7];
            value.film_roll_pivot = [0.2, -0.3];
            value.film_fit = FilmFit::Vertical;
        }
        2 => {
            value.overscan = 1.3;
            value.film_offset_in = [0.05, -0.05];
            value.film_roll_order = FilmRollOrder::TranslateRotate;
            value.film_fit = FilmFit::Overscan;
            value.film_fit_offset_in = 0.015625;
        }
        _ => {
            value.camera_scale = 2.0;
            value.pre_scale = 1.4;
            value.post_scale = 0.9;
            value.lens_squeeze = 1.5;
            value.film_roll_degrees = 33.3;
            value.display_film_gate = true;
        }
    }
    value
}

fn authored(decoded: AsciiDecoded) -> NativeScene {
    match decoded {
        AsciiDecoded::Authored(projection) => projection.into_scene(),
        other => panic!("expected an authored scene, got {other:?}"),
    }
}

#[test]
fn every_camera_round_trips_bit_for_bit_through_an_authored_document() {
    let (mut scene, _) = test_fixtures::cube();
    for (index, camera) in CAMERAS.into_iter().enumerate() {
        scene.set_camera_film_back(camera, film(index)).unwrap();
    }
    let bytes = emit_geometry_prefix(&scene).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let focal = format!(
        "select -ne :perspShape;\n\tsetAttr \".fl\" {};\n",
        film(0).focal_mm
    );
    assert!(text.contains(&focal));
    assert!(
        !focal.contains(" 35;"),
        "the focal length keeps its last bit: {focal}"
    );
    assert!(text.contains("\tsetAttr \".fth\" -0;\n"));
    // Enum ids, the plain degrees and the boolean are literal: they are the file's vocabulary.
    for statement in [
        "\tsetAttr \".ff\" 2;\n",
        "\tsetAttr \".ff\" 3;\n",
        "\tsetAttr \".fro\" 1;\n",
        "\tsetAttr \".fro\" 0;\n",
        "\tsetAttr \".frv\" 33.3;\n",
        "\tsetAttr \".dfg\" yes;\n",
        "\tsetAttr \".dfg\" no;\n",
    ] {
        assert!(text.contains(statement), "{statement}");
    }
    let decoded = authored(decode_document(Arc::from(bytes.clone()), true).unwrap());
    for (index, camera) in CAMERAS.into_iter().enumerate() {
        assert!(
            decoded.camera_film_back(camera).same_bits(&film(index)),
            "{camera:?}"
        );
    }
    assert_eq!(emit_geometry_prefix(&decoded).unwrap(), bytes);
}

#[test]
fn an_empty_scene_with_a_camera_is_an_authored_document() {
    let mut scene = NativeScene::default();
    scene
        .set_camera_film_back(SceneCamera::Front, film(2))
        .unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    let decoded = authored(decode_document(Arc::from(bytes), true).unwrap());
    assert!(
        decoded
            .camera_film_back(SceneCamera::Front)
            .same_bits(&film(2))
    );
    assert!(
        decoded
            .camera_film_back(SceneCamera::Persp)
            .same_bits(&FilmBack::DEFAULT)
    );
    assert_eq!(decoded, scene);
}

#[test]
fn a_scene_without_overrides_writes_no_camera_statement() {
    let (scene, _) = test_fixtures::cube();
    let text = String::from_utf8(emit_geometry_prefix(&scene).unwrap()).unwrap();
    assert!(!text.contains("select -ne"), "{text}");
    // An empty scene is exactly the header, as before the camera blocks existed.
    assert_eq!(
        emit_geometry_prefix(&NativeScene::default()).unwrap(),
        "//Maya ASCII 2026 scene\nrequires maya \"2026\";\ncurrentUnit -l centimeter -a degree -t film;\nfileInfo \"AztecCraft\" \"1\";\n"
            .as_bytes()
    );
}

#[test]
fn an_explicit_default_statement_is_kept_and_a_reset_removes_it() {
    let mut scene = NativeScene::default();
    scene.state_camera_film_back(SceneCamera::Top, FilmBack::DEFAULT);
    let bytes = emit_geometry_prefix(&scene).unwrap();
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains("select -ne :topShape;")
    );
    let decoded = authored(decode_document(Arc::from(bytes.clone()), true).unwrap());
    assert_eq!(decoded, scene);
    assert_ne!(decoded, NativeScene::default(), "stated is not unstated");
    assert_eq!(emit_geometry_prefix(&decoded).unwrap(), bytes);
    let mut reset = decoded;
    reset
        .set_camera_film_back(SceneCamera::Top, FilmBack::DEFAULT)
        .unwrap();
    assert_eq!(reset, NativeScene::default());
}
