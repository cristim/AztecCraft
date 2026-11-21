//! Issue #458 S2c: the authored `.ma` camera blocks. Expected values are the writer's documented
//! form (`select -ne :perspShape;` then one `setAttr` per film back attribute, PROVISIONAL: Maya
//! acceptance of this form is unproved) and exact f64 bits.
use super::*;
use crate::native_scene::camera::FILM_ATTRIBUTES;
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

const PREFIX: &str = "//Maya ASCII 2026 scene\nrequires maya \"2026\";\ncurrentUnit -l centimeter -a degree -t film;\nfileInfo \"AztecCraft\" \"1\";\n";

fn complete_block(shape: &str) -> String {
    let mut scene = NativeScene::default();
    scene.state_camera_film_back(SceneCamera::Persp, FilmBack::DEFAULT);
    let text = String::from_utf8(emit_geometry_prefix(&scene).unwrap()).unwrap();
    text.replace("perspShape", shape)
}

#[test]
fn malformed_partial_and_invalid_camera_statements_are_refused() {
    let complete = complete_block("perspShape");
    assert!(decode(Arc::from(complete.clone().into_bytes())).is_ok());
    let with = |from: &str, to: &str| {
        let changed = complete.replace(from, to);
        assert_ne!(changed, complete, "{from}");
        changed
    };
    use ErrorKind::{Malformed, Unsupported};
    let cases = [
        (
            "duplicate fl",
            format!("{complete}\tsetAttr \".fl\" 40;\n"),
            Unsupported,
        ),
        (
            "duplicate hfa",
            format!("{complete}\tsetAttr \".hfa\" 2;\n"),
            Unsupported,
        ),
        ("fit 4", with("\".ff\" 0;", "\".ff\" 4;"), Unsupported),
        ("fit -1", with("\".ff\" 0;", "\".ff\" -1;"), Unsupported),
        ("fit float", with("\".ff\" 0;", "\".ff\" 1.5;"), Malformed),
        ("zero focal", with("\".fl\" 35;", "\".fl\" 0;"), Malformed),
        (
            "negative squeeze",
            with("\".lsr\" 1;", "\".lsr\" -1;"),
            Malformed,
        ),
        (
            "huge scale",
            with("\".psc\" 1;", "\".psc\" 1e300;"),
            Malformed,
        ),
        (
            "not a number",
            with("\".fl\" 35;", "\".fl\" nan;"),
            Malformed,
        ),
        ("word", with("\".fl\" 35;", "\".fl\" thirty;"), Malformed),
        (
            "gate word",
            with("\".dfg\" no;", "\".dfg\" maybe;"),
            Malformed,
        ),
        (
            "typed",
            with("\".fl\" 35;", "-type \"double\" \".fl\" 35;"),
            Unsupported,
        ),
        ("size", with("\".fl\" 35;", "-s 1 \".fl\" 35;"), Unsupported),
        (
            "unknown attribute",
            format!("{complete}\tsetAttr \".shakeEnabled\" 1;\n"),
            Unsupported,
        ),
        ("unknown shape", complete_block("unknownShape"), Unsupported),
    ];
    for (name, source, kind) in cases {
        let refused = decode(Arc::from(source.into_bytes())).expect_err(name);
        assert_eq!(refused.kind, kind, "{name}");
    }
}

#[test]
fn a_radian_file_refuses_the_plain_roll_unit() {
    let source = complete_block("perspShape").replace("-a degree", "-a radian");
    let refused = decode(Arc::from(source.into_bytes())).unwrap_err();
    assert_eq!(refused.kind, ErrorKind::Unsupported);
}

#[test]
fn an_unknown_roll_order_blocks_the_camera_and_the_source_keeps_it() {
    let source = complete_block("perspShape").replace("\".fro\" 0;", "\".fro\" 7;");
    let bytes: Arc<[u8]> = Arc::from(source.into_bytes());
    let scene = decode(bytes.clone()).unwrap();
    assert!(!scene.camera_projection_available(SceneCamera::Persp));
    assert!(scene.camera_projection_available(SceneCamera::Top));
    assert!(
        scene
            .camera_film_back(SceneCamera::Persp)
            .same_bits(&FilmBack::DEFAULT)
    );
    let kept = scene.source.as_ref().unwrap();
    assert!(
        kept.bytes
            .windows(b"\".fro\" 7;".len())
            .any(|w| w == b"\".fro\" 7;")
    );
    // A mesh-free file stays readable and read-only: the gate cannot re-emit a blocked camera.
    assert!(matches!(
        decode_document(bytes, true),
        Ok(AsciiDecoded::Retained(_))
    ));
}

#[test]
fn a_blocked_camera_refuses_edits_and_the_writer() {
    let mut scene = NativeScene::default();
    scene.block_camera(SceneCamera::Side);
    assert_eq!(
        scene.set_camera_film_back(SceneCamera::Side, film(0)),
        Err(NativeError::BlockedCamera)
    );
    assert_eq!(
        emit_geometry_prefix(&scene).unwrap_err(),
        AsciiWriteRefusal::CameraBlocked
    );
    scene
        .set_camera_film_back(SceneCamera::Top, film(1))
        .unwrap();
}

#[test]
fn a_mesh_file_with_an_unknown_roll_order_is_refused() {
    let (mut scene, _) = test_fixtures::cube();
    scene
        .set_camera_film_back(SceneCamera::Persp, film(0))
        .unwrap();
    let bytes = emit_geometry_prefix(&scene).unwrap();
    let changed = String::from_utf8(bytes)
        .unwrap()
        .replace("\".fro\" 0;", "\".fro\" 9;");
    // The gate cannot re-emit a blocked camera, and a mesh file is never retained read-only:
    // only a mesh-free file keeps its source bytes (see known-issues).
    assert!(matches!(
        decode_document(Arc::from(changed.into_bytes()), true),
        Err(AsciiDocumentError::Ascii(AsciiError {
            kind: ErrorKind::Unsupported,
            ..
        }))
    ));
}

#[test]
fn the_default_camera_form_of_a_maya_file_reads_a_complete_block_into_the_store() {
    // The Maya form: the shared camera transform and shape, then the attributes.
    let block = complete_block("perspShape");
    let attributes: String = block.lines().skip(5).collect::<Vec<_>>().join("\n");
    let source = format!(
        "{PREFIX}createNode transform -s -n \"persp\";\ncreateNode camera -s -n \"perspShape\" -p \"persp\";\n{attributes}\n"
    );
    match decode(Arc::from(source.into_bytes())) {
        Ok(scene) => assert!(
            scene
                .camera_film_back(SceneCamera::Persp)
                .same_bits(&FilmBack::DEFAULT)
        ),
        Err(error) => panic!("{error:?}"),
    }
}

#[test]
fn a_default_camera_that_states_only_its_focal_length_keeps_its_old_meaning() {
    let source = format!(
        "{PREFIX}createNode transform -s -n \"persp\";\ncreateNode camera -s -n \"perspShape\" -p \"persp\";\n\tsetAttr \".fl\" 34.999999999999993;\n"
    );
    let scene = decode(Arc::from(source.into_bytes())).unwrap();
    assert!(
        scene
            .camera_film_back(SceneCamera::Persp)
            .same_bits(&FilmBack::DEFAULT)
    );
    assert!(scene.camera_projection_available(SceneCamera::Persp));
}

fn without(block: &str, attribute: &str) -> String {
    block
        .lines()
        .filter(|line| !line.contains(&format!("\".{attribute}\"")))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn a_partial_block_blocks_the_camera_and_keeps_the_source_instead_of_refusing_the_file() {
    let complete = complete_block("perspShape");
    for attribute in FILM_ATTRIBUTES {
        let source = without(&complete, attribute);
        let bytes: Arc<[u8]> = Arc::from(source.clone().into_bytes());
        let scene = decode(bytes.clone()).unwrap_or_else(|e| panic!("{attribute}: {e:?}"));
        assert!(
            !scene.camera_projection_available(SceneCamera::Persp),
            "missing {attribute}"
        );
        assert!(scene.camera_projection_available(SceneCamera::Top));
        assert!(
            scene
                .camera_film_back(SceneCamera::Persp)
                .same_bits(&FilmBack::DEFAULT)
        );
        assert_eq!(&*scene.source.as_ref().unwrap().bytes, source.as_bytes());
        assert!(matches!(
            decode_document(bytes, true),
            Ok(AsciiDecoded::Retained(_))
        ));
    }
}

#[test]
fn one_real_maya_attribute_blocks_the_camera_and_is_not_an_error() {
    // Maya writes only what differs from the node defaults: a lone edited aperture.
    let source = format!(
        "{PREFIX}createNode transform -s -n \"persp\";\ncreateNode camera -s -n \"perspShape\" -p \"persp\";\n\tsetAttr \".fl\" 50;\n\tsetAttr \".vfa\" 1.2;\n"
    );
    let scene = decode(Arc::from(source.into_bytes())).unwrap();
    assert!(!scene.camera_projection_available(SceneCamera::Persp));
    assert!(scene.has_blocked_camera());
    assert_eq!(scene.blocked_camera_names(), ["persp"]);
    assert!(NativeScene::default().blocked_camera_names().is_empty());
    // A duplicate statement, a typed setAttr and an unknown attribute still refuse the file.
    for body in [
        "\tsetAttr \".vfa\" 1.2;\n\tsetAttr \".vfa\" 1.3;\n",
        "\tsetAttr -type \"double\" \".vfa\" 1.2;\n",
        "\tsetAttr \".vfa\" 1.2;\n\tsetAttr \".shakeEnabled\" 1;\n",
    ] {
        let source = format!(
            "{PREFIX}createNode transform -s -n \"persp\";\ncreateNode camera -s -n \"perspShape\" -p \"persp\";\n{body}"
        );
        assert!(decode(Arc::from(source.into_bytes())).is_err(), "{body}");
    }
}

#[test]
fn a_mesh_file_with_a_partial_camera_block_is_refused_like_any_unreemittable_statement() {
    let (scene, _) = test_fixtures::cube();
    let mut text = String::from_utf8(emit_geometry_prefix(&scene).unwrap()).unwrap();
    text.push_str("select -ne :perspShape;\n\tsetAttr \".hfa\" 1.2;\n");
    assert!(matches!(
        decode_document(Arc::from(text.into_bytes()), true),
        Err(AsciiDocumentError::Ascii(AsciiError {
            kind: ErrorKind::Unsupported,
            ..
        }))
    ));
}
