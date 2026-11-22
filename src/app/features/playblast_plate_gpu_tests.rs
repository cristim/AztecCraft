//! The image plane in playblasts on a real headless adapter (#460): the Editor runs
//! `Operation::Playblast` with the real `WgpuFrameRenderer`, real PNG plates and the real
//! decoder. Analytic checks use +-2 per channel (Lavapipe in CI, Metal locally); exact equality
//! is only asserted between two renders on the same adapter.
use super::super::playblast::tests::{decode, request, run_to_end};
use crate::app::Editor;
use crate::editor_operations::Operation;
use crate::viewport_renderer::ViewportRenderer;
use crate::viewport_renderer::tests::{Offscreen, creation_context};
use azteccraft::image_plane::model::{ImagePlaneEdit, PlateDisplayMode};
use azteccraft::native_scene::MaterialChoice;
use azteccraft::playblast::{JobProgress, PlayblastError};
use azteccraft::scene::Primitive;
use azteccraft::view_scene::ViewScene;
use azteccraft::viewport::film_gate_ndc;
use std::path::{Path, PathBuf};

/// Plate 1, 2, 3, 4 flat colours.
const COLOURS: [[u8; 3]; 4] = [[0, 200, 0], [0, 0, 200], [200, 200, 0], [200, 0, 200]];

fn write_png(path: &Path, texel: [u8; 4]) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 6, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&texel.repeat(24)).unwrap();
    writer.finish().unwrap();
}

/// plate.0001.png .. plate.0004.png, flat colours of `COLOURS` with alpha `alpha`.
fn plates(dir: &Path, alpha: u8) -> PathBuf {
    for (n, [r, g, b]) in COLOURS.iter().enumerate() {
        write_png(
            &dir.join(format!("plate.{:04}.png", n + 1)),
            [*r, *g, *b, alpha],
        );
    }
    dir.join("plate.0001.png")
}

/// A red cube at the origin, a plate sequence on the camera, the window's real renderer.
fn editor(creation: &eframe::CreationContext<'_>, first: &Path, edit: ImagePlaneEdit) -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let shape = editor.history.scene().shapes().first().unwrap().id();
    let paint = Operation::AssignSessionMaterial {
        shapes: vec![shape],
        material: MaterialChoice::New {
            color: [1.0, 0.0, 0.0],
        },
    };
    editor.apply_operation(paint).unwrap();
    let plane = ImagePlaneEdit {
        image_name: Some(first.to_path_buf()),
        use_frame_extension: Some(true),
        display_mode: Some(PlateDisplayMode::Rgb),
        ..edit
    };
    editor
        .apply_operation(Operation::SetImagePlane(plane))
        .unwrap();
    editor
        .apply_operation(Operation::SetCurrentTime { time: 1.0 })
        .unwrap();
    editor.playblast.factory = None;
    editor.renderer = Some(ViewportRenderer::initialize(creation).unwrap());
    editor
}

fn playblast(dir: &Path, name: &str, size: (u32, u32), range: (f64, f64), grid: bool) -> Operation {
    let mut request = request(dir);
    request.name = name.into();
    (request.start, request.end) = (Some(range.0), Some(range.1));
    (request.width, request.height) = (Some(size.0), Some(size.1));
    request.show_ornaments = grid;
    Operation::Playblast(request)
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
    pixels[((y * width + x) * 4) as usize..][..3]
        .try_into()
        .unwrap()
}

fn near(actual: [u8; 3], expected: [u8; 3], what: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 2),
        "{what}: {actual:?} vs {expected:?}"
    );
}

fn frame_pixels(dir: &Path, name: &str, frame: u32) -> Vec<u8> {
    decode(&dir.join(format!("{name}.{frame:04}.png"))).1
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_draws_each_frames_plate_and_geometry_occludes_it() {
    let creation = creation_context(eframe::egui::Context::default());
    let src = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    // The film back is 3:2, so 96 x 64 is exactly the gate: every pixel is plate.
    let mut editor = editor(
        &creation,
        &plates(src.path(), 255),
        ImagePlaneEdit::default(),
    );
    editor
        .apply_operation(playblast(out.path(), "seq", (96, 64), (1.0, 2.0), false))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 2 });
    for (n, colour) in [(1, COLOURS[0]), (2, COLOURS[1])] {
        let pixels = frame_pixels(out.path(), "seq", n);
        near(at(&pixels, 96, 2, 2), colour, &format!("frame {n} corner"));
        near(
            at(&pixels, 96, 93, 61),
            colour,
            &format!("frame {n} far corner"),
        );
        // The cube is 8 units away, the plate 100: the cube hides the plate.
        let centre = at(&pixels, 96, 48, 32);
        assert!(
            centre[0] > centre[1] + 20 && centre[0] > centre[2] + 20,
            "frame {n}: a red cube over the plate: {centre:?}"
        );
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_applies_the_frame_offset() {
    let creation = creation_context(eframe::egui::Context::default());
    let src = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let edit = ImagePlaneEdit {
        frame_offset: Some(1),
        ..ImagePlaneEdit::default()
    };
    let mut editor = editor(&creation, &plates(src.path(), 255), edit);
    editor
        .apply_operation(playblast(out.path(), "off", (96, 64), (1.0, 3.0), false))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 3 });
    for (n, colour) in [(1, COLOURS[1]), (2, COLOURS[2]), (3, COLOURS[3])] {
        near(
            at(&frame_pixels(out.path(), "off", n), 96, 2, 2),
            colour,
            &format!("frame {n}"),
        );
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_draws_the_plate_under_geometry_that_lies_behind_it() {
    let creation = creation_context(eframe::egui::Context::default());
    let src = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    // Plate 4 cm from the camera, in front of the cube (8 units), with alpha 128 under Rgba:
    // plate colour 200 * a + background 31 * (1 - a), a = 128 / 255 (50.2%). Drawn after the
    // geometry it would blend over the red cube instead; drawn first, the nearer plate's depth
    // hides the cube and the result is the plate over the clear colour.
    let edit = ImagePlaneEdit {
        depth: Some(4.0),
        display_mode: Some(PlateDisplayMode::Rgba),
        ..ImagePlaneEdit::default()
    };
    let mut editor = editor(&creation, &plates(src.path(), 128), edit);
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            display_mode: Some(PlateDisplayMode::Rgba),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    editor
        .apply_operation(playblast(out.path(), "back", (96, 64), (1.0, 1.0), false))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 1 });
    let pixels = frame_pixels(out.path(), "back", 1);
    let blended = [0, 200, 0].map(|c: u8| (f32::from(c) * 0.502 + 31.0 * 0.498).round() as u8);
    near(at(&pixels, 96, 2, 2), blended, "plate over background");
    near(
        at(&pixels, 96, 48, 32),
        blended,
        "plate hides the cube behind it",
    );
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_matches_the_viewports_plate_exactly_and_places_it_by_the_output_aspect() {
    let creation = creation_context(eframe::egui::Context::default());
    let src = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut editor = editor(
        &creation,
        &plates(src.path(), 255),
        ImagePlaneEdit::default(),
    );
    // The viewport path at frame 1 on a square viewport (the test harness is 128 x 128).
    let ctx = eframe::egui::Context::default();
    editor.aspect = 1.0;
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while editor.plate_draw().is_none() {
        editor.update_plate(&ctx);
        assert!(std::time::Instant::now() < until, "the plate never loaded");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    let draw = editor.plate_draw().unwrap();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let expected = Offscreen::from_context(&creation).image_with_plate(
        &view,
        &editor.camera,
        None,
        Some(draw),
    );
    editor
        .apply_operation(playblast(out.path(), "sq", (128, 128), (1.0, 1.0), true))
        .unwrap();
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 1 });
    let got = frame_pixels(out.path(), "sq", 1);
    assert_eq!(
        got, expected,
        "playblast frame equals the viewport's own render"
    );
    // Square output: the 3:2 film gate fills the height and overflows the width (its NDC width is
    // 3, as `film_gate_ndc` says), so the plate covers every pixel of the output.
    let film = match editor.camera.lens() {
        azteccraft::viewport::Lens::Film(film) => film,
        other => panic!("{other:?}"),
    };
    let gate = film_gate_ndc(&film, 1.0);
    assert!(gate[1].x > 1.4 && gate[2].y > 0.99, "{gate:?}");
    for (x, y) in [(2, 2), (125, 2), (2, 125), (125, 125)] {
        near(
            at(&got, 128, x, y),
            COLOURS[0],
            "corner covered by the plate",
        );
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_renders_a_missing_frame_without_a_plate_and_fails_on_a_corrupt_one() {
    let creation = creation_context(eframe::egui::Context::default());
    let src = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut editor = editor(
        &creation,
        &plates(src.path(), 255),
        ImagePlaneEdit::default(),
    );
    std::fs::write(src.path().join("plate.0006.png"), b"not a png").unwrap();
    editor
        .apply_operation(playblast(out.path(), "gap", (96, 64), (4.0, 5.0), false))
        .unwrap();
    // Frame 4 has a file; frame 5 has none (no plate, job continues).
    assert_eq!(run_to_end(&mut editor), JobProgress::Finished { total: 2 });
    near(
        at(&frame_pixels(out.path(), "gap", 4), 96, 2, 2),
        COLOURS[3],
        "frame 4",
    );
    near(
        at(&frame_pixels(out.path(), "gap", 5), 96, 2, 2),
        [31; 3],
        "frame 5 has no plate",
    );
    // Frame 6 has a corrupt file: the job stops with a typed error naming the frame.
    editor
        .apply_operation(playblast(out.path(), "bad", (96, 64), (5.0, 6.0), false))
        .unwrap();
    assert_eq!(
        run_to_end(&mut editor),
        JobProgress::Failed(PlayblastError::Plate { frame: 6 })
    );
}
