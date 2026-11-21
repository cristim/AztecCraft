//! #459 P7b: the plate through the real GpuResources on an offscreen target. Pixel values are
//! worked by hand from the shader's rules; the clear colour is (31, 31, 31).
use super::*;
use azteccraft::image_plane::{
    cache::FrameKey,
    decode::PlateFrame,
    fit::{PlateQuad, plate_quad},
    model::{Coverage, ImagePlane, ImagePlaneEdit, PlateDisplayMode, PlateFit},
};
use azteccraft::viewport::{FilmBack, FilmFit};

const BACKGROUND: [u8; 4] = [31, 31, 31, 255];

/// A quad whose edges run through the pixel centres 32 and 95 of the 128 pixel target, at the
/// given clip depth; uv covers the whole frame with v down.
fn quad(z_ndc: f32) -> PlateQuad {
    let x = |px: f32| (px + 0.5) / 64.0 - 1.0;
    let y = |py: f32| 1.0 - (py + 0.5) / 64.0;
    PlateQuad {
        ndc: [
            Vec2::new(x(32.0), y(95.0)),
            Vec2::new(x(95.0), y(95.0)),
            Vec2::new(x(95.0), y(32.0)),
            Vec2::new(x(32.0), y(32.0)),
        ],
        z_ndc,
        uv: [
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 0.0),
        ],
    }
}

fn frame(texel: [u8; 4]) -> Arc<PlateFrame> {
    Arc::new(PlateFrame {
        width: 2,
        height: 2,
        rgba: texel.repeat(4),
    })
}

fn plate(mode: PlateDisplayMode, texel: [u8; 4], alpha_gain: f32) -> PlateDraw {
    PlateDraw {
        key: FrameKey {
            generation: 1,
            frame: 1,
        },
        frame: frame(texel),
        quad: quad(0.5),
        mode,
        alpha_gain,
    }
}

fn empty_submission(plate: Option<PlateDraw>) -> Submission {
    Submission {
        plate,
        bytes: Vec::new(),
        surfaces: 0..0,
        lines: 0..0,
        markers: 0..0,
        centers_degraded: false,
    }
}

/// Renders `plate` alone and returns the image, asserting the device raised no validation error.
fn render(gpu: &mut Offscreen, plate: Option<PlateDraw>) -> Vec<u8> {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let image = gpu.render(TestFrame::Geometry(empty_submission(plate)));
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "validation error: {error:?}");
    image
}

fn close(actual: &[u8], expected: [u8; 3], tolerance: i32, what: &str) {
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (i32::from(*a) - i32::from(e)).abs() <= tolerance,
            "{what}: {actual:?} vs {expected:?}"
        );
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_outline_plate_with_no_geometry_draws_four_edges_without_a_validation_error() {
    let mut gpu = Offscreen::new();
    let image = render(
        &mut gpu,
        Some(plate(PlateDisplayMode::Outline, [0; 4], 1.0)),
    );
    for (x, y) in [(32, 64), (95, 64), (64, 32), (64, 95)] {
        assert_eq!(
            pixel(&image, x, y),
            &[255, 255, 255, 255],
            "edge ({x}, {y})"
        );
    }
    assert_eq!(pixel(&image, 64, 64), &BACKGROUND, "inside the outline");
    assert_eq!(pixel(&image, 10, 10), &BACKGROUND, "outside the outline");
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_every_display_mode_draws_by_hand_values_in_a_validation_scope() {
    let mut gpu = Offscreen::new();
    let texel = [200, 50, 20, 128];
    // Alpha 128 / 255 = 0.50196. RGBA blends over (31, 31, 31): 200 * a + 31 * (1 - a) = 115.8,
    // 50 -> 40.5, 20 -> 25.5. RGB ignores the alpha. Luminance is 0.2126 * 200 + 0.7152 * 50 +
    // 0.0722 * 20 = 79.72 on the encoded values, opaque. Alpha is the alpha as grey, 128.
    for (mode, expected) in [
        (PlateDisplayMode::Rgba, [116, 41, 26]),
        (PlateDisplayMode::Rgb, [200, 50, 20]),
        (PlateDisplayMode::Luminance, [80, 80, 80]),
        (PlateDisplayMode::Alpha, [128, 128, 128]),
        (PlateDisplayMode::None, [31, 31, 31]),
    ] {
        let image = render(&mut gpu, Some(plate(mode, texel, 1.0)));
        close(pixel(&image, 64, 64), expected, 2, &format!("{mode:?}"));
        assert_eq!(pixel(&image, 10, 10), &BACKGROUND, "{mode:?} outside");
    }
    // The gain multiplies alpha: 0.5 * 128 / 255 = 0.251, a grey of 64.
    let half = render(&mut gpu, Some(plate(PlateDisplayMode::Alpha, texel, 0.5)));
    close(pixel(&half, 64, 64), [64, 64, 64], 2, "gain 0.5");
    // Gain 4 clamps the alpha to 1 before it becomes a grey: 255.
    let clamped = render(&mut gpu, Some(plate(PlateDisplayMode::Alpha, texel, 4.0)));
    close(pixel(&clamped, 64, 64), [255, 255, 255], 1, "gain 4");
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_a_luminance_texel_uses_the_rec_709_weights_on_the_encoded_values() {
    let mut gpu = Offscreen::new();
    // (200, 50, 20): 0.2126 * 200 + 0.7152 * 50 + 0.0722 * 20 = 79.724, so 80. Swapped weights
    // (for example 0.0722 on red) would give 0.0722 * 200 + 0.7152 * 50 + 0.2126 * 20 = 54.5.
    let image = render(
        &mut gpu,
        Some(plate(PlateDisplayMode::Luminance, [200, 50, 20, 255], 1.0)),
    );
    close(pixel(&image, 64, 64), [80, 80, 80], 1, "luminance");
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_dropping_the_plate_releases_its_texture() {
    let mut gpu = Offscreen::new();
    render(
        &mut gpu,
        Some(plate(PlateDisplayMode::Rgba, [10, 20, 30, 255], 1.0)),
    );
    assert!(gpu.resources.plate.slot.is_some());
    let image = render(&mut gpu, None);
    assert!(!gpu.resources.plate.slot.is_some());
    assert_eq!(pixel(&image, 64, 64), &BACKGROUND);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_an_srgb_target_stores_the_texels_unchanged() {
    // The shader decodes sRGB to linear and the target encodes it again, so the stored bytes
    // equal the decoded frame's bytes; Luminance is still taken on the encoded values.
    let mut gpu = Offscreen::with_format(wgpu::TextureFormat::Rgba8UnormSrgb);
    let rgb = render(
        &mut gpu,
        Some(plate(PlateDisplayMode::Rgb, [200, 50, 20, 255], 1.0)),
    );
    close(pixel(&rgb, 64, 64), [200, 50, 20], 1, "rgb on srgb");
    let grey = render(
        &mut gpu,
        Some(plate(PlateDisplayMode::Luminance, [200, 50, 20, 255], 1.0)),
    );
    close(pixel(&grey, 64, 64), [80, 80, 80], 1, "luminance on srgb");
}

// P7c: occlusion, the RGBA gain, uv orientation and the plate's placement from `plate_quad`.

fn draw_of(frame: PlateFrame, quad: PlateQuad, mode: PlateDisplayMode, gain: f32) -> PlateDraw {
    PlateDraw {
        key: FrameKey {
            generation: 1,
            frame: 1,
        },
        frame: Arc::new(frame),
        quad,
        mode,
        alpha_gain: gain,
    }
}

/// One full-screen triangle at clip depth `z` in a flat colour.
fn backdrop(plate: Option<PlateDraw>, z: f32, colour: [u8; 3]) -> Submission {
    let mut submission = empty_submission(plate);
    for (x, y) in [(-1.0, -1.0), (3.0, -1.0), (-1.0, 3.0)] {
        submission
            .clip_vertex(
                glam::Vec4::new(x, y, z, 1.0),
                egui::Color32::from_rgb(colour[0], colour[1], colour[2]),
                FORMAT,
            )
            .unwrap();
    }
    submission.surfaces = 0..3;
    submission
}

fn render_submission(gpu: &mut Offscreen, submission: Submission) -> Vec<u8> {
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let image = gpu.render(TestFrame::Geometry(submission));
    let error = pollster::block_on(scope.pop());
    assert!(error.is_none(), "validation error: {error:?}");
    image
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_geometry_beyond_the_plate_depth_is_hidden_and_nearer_geometry_draws_over_it() {
    let mut gpu = Offscreen::new();
    let texel = [200, 50, 20, 255];
    let mine = || Some(plate(PlateDisplayMode::Rgb, texel, 1.0)); // plate at clip depth 0.5
    // Farther (z 0.7): hidden where the plate covers, drawn where it does not (pixel 10, 10).
    let far = render_submission(&mut gpu, backdrop(mine(), 0.7, [0, 255, 0]));
    assert_eq!(
        &pixel(&far, 64, 64)[..3],
        &[200, 50, 20],
        "far geometry behind the plate"
    );
    assert_eq!(
        &pixel(&far, 10, 10)[..3],
        &[0, 255, 0],
        "far geometry outside the plate"
    );
    // Nearer (z 0.3): over the plate everywhere.
    let near = render_submission(&mut gpu, backdrop(mine(), 0.3, [0, 255, 0]));
    assert_eq!(
        &pixel(&near, 64, 64)[..3],
        &[0, 255, 0],
        "near geometry over the plate"
    );
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_rgba_applies_the_alpha_gain_with_straight_alpha_blending() {
    let mut gpu = Offscreen::new();
    // Texel alpha 128 / 255 = 0.50196 over the clear colour 31: out = c * a + 31 * (1 - a).
    // Gain 0.5: a = 0.25098: (73.4, 35.8, 28.2). Gain 0: nothing (31). Gain 3: a clamps to 1.
    for (gain, expected) in [
        (0.5, [73, 36, 28]),
        (0.0, [31, 31, 31]),
        (3.0, [200, 50, 20]),
    ] {
        let image = render(
            &mut gpu,
            Some(plate(PlateDisplayMode::Rgba, [200, 50, 20, 128], gain)),
        );
        close(pixel(&image, 64, 64), expected, 2, &format!("gain {gain}"));
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_the_top_row_of_the_image_is_on_top_of_the_screen() {
    let mut gpu = Offscreen::new();
    // 1 x 2 image: top row red, bottom row green; v is down and sampled without a flip.
    let image = PlateFrame {
        width: 1,
        height: 2,
        rgba: [[255, 0, 0, 255], [0, 255, 0, 255]].concat(),
    };
    let drawn = render_submission(
        &mut gpu,
        empty_submission(Some(draw_of(image, quad(0.5), PlateDisplayMode::Rgb, 1.0))),
    );
    // Rows 32..95: row 40 is 0.13 down the plate (clamped to the red texel row), row 90 0.9.
    close(pixel(&drawn, 64, 40), [255, 0, 0], 3, "top");
    close(pixel(&drawn, 64, 90), [0, 255, 0], 3, "bottom");
}

/// A 4 x 2 image whose columns are red, green, blue and white, both rows alike.
fn columns() -> PlateFrame {
    let row = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ];
    PlateFrame {
        width: 4,
        height: 2,
        rgba: [row.concat(), row.concat()].concat(),
    }
}

/// The plate for `frame` placed by `plate_quad` at depth 100 on a square 128 pixel viewport.
fn placed(frame: PlateFrame, film: &FilmBack, edit: impl FnOnce(&mut ImagePlaneEdit)) -> PlateDraw {
    let size = (frame.width, frame.height);
    let mut e = ImagePlaneEdit {
        image_name: Some("p.png".into()),
        display_mode: Some(PlateDisplayMode::Rgb),
        ..ImagePlaneEdit::default()
    };
    edit(&mut e);
    let plane = ImagePlane::create(&e, size).unwrap();
    let camera = Camera::new(glam::Vec3::ZERO, 0.0, 0.0, 5.0, 1.0, 0.1, 10000.0).unwrap();
    let quad = plate_quad(&plane, size, film, 1.0, &camera).unwrap();
    draw_of(frame, quad, PlateDisplayMode::Rgb, 1.0)
}

fn square_film() -> FilmBack {
    FilmBack {
        aperture_in: [1.0, 1.0],
        ..FilmBack::DEFAULT
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_a_fill_plate_crops_the_overflowing_axis_and_a_coverage_shows_only_its_part() {
    let mut gpu = Offscreen::new();
    let film = square_film();
    // A square gate fills the view. A 4 x 2 image under Fill overflows in u and keeps half of
    // it, centred: u 0.25..0.75, the green and blue columns. Pixel 31 is u 0.373 (green, the
    // column centre is 0.375) and pixel 95 is u 0.623 (blue); stretched they would be mixes.
    let fill = placed(columns(), &film, |e| e.fit = Some(PlateFit::Fill));
    let image = render_submission(&mut gpu, empty_submission(Some(fill)));
    close(pixel(&image, 31, 64), [0, 255, 0], 4, "fill left");
    close(pixel(&image, 95, 64), [0, 0, 255], 4, "fill right");
    // The crop keeps the quad inside the gate: with overscan 2 (gate ndc +-0.5, pixels 32..96) an
    // uncropped Fill quad would spill to ndc +-1 and cover pixel 20.
    let mut small = square_film();
    small.overscan = 2.0;
    let held = placed(columns(), &small, |e| e.fit = Some(PlateFit::Fill));
    let image = render_submission(&mut gpu, empty_submission(Some(held)));
    assert_eq!(
        pixel(&image, 20, 64),
        &BACKGROUND,
        "fill stays inside the gate"
    );
    // Coverage of the right half (columns 2 and 3), stretched to the gate: u 0.5..1.
    let part = placed(columns(), &film, |e| {
        e.fit = Some(PlateFit::ToSize);
        e.coverage = Some(Some(Coverage {
            origin_x: 2,
            origin_y: 0,
            width: 2,
            height: 2,
        }));
    });
    let image = render_submission(&mut gpu, empty_submission(Some(part)));
    close(pixel(&image, 31, 64), [0, 0, 255], 4, "coverage left");
    close(pixel(&image, 95, 64), [255, 255, 255], 4, "coverage right");
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_the_plate_follows_overscan_and_the_film_fit_offset_like_the_gate() {
    let mut gpu = Offscreen::new();
    let solid = || PlateFrame {
        width: 1,
        height: 1,
        rgba: vec![200, 50, 20, 255],
    };
    // Overscan 2 halves the gate: ndc +-0.5, pixels 32..96.
    let mut film = square_film();
    film.overscan = 2.0;
    let image = render_submission(
        &mut gpu,
        empty_submission(Some(placed(solid(), &film, |e| {
            e.fit = Some(PlateFit::ToSize)
        }))),
    );
    close(
        pixel(&image, 40, 64),
        [200, 50, 20],
        1,
        "inside overscan gate",
    );
    assert_eq!(pixel(&image, 20, 64), &BACKGROUND, "outside overscan gate");
    assert_eq!(pixel(&image, 64, 20), &BACKGROUND, "above overscan gate");
    // Horizontal fit with Film Fit Offset 0.2 in moves the gate by -0.2 * 25.4 / 35 / ty in y
    // with ty = 1 * 25.4 / 70 (the film_tests formula): 0.4 down, so its top edge is ndc 0.6,
    // pixel row 25.6.
    let mut film = square_film();
    film.film_fit = FilmFit::Horizontal;
    film.film_fit_offset_in = 0.2;
    let image = render_submission(
        &mut gpu,
        empty_submission(Some(placed(solid(), &film, |e| {
            e.fit = Some(PlateFit::ToSize)
        }))),
    );
    assert_eq!(pixel(&image, 64, 20), &BACKGROUND, "above the shifted gate");
    close(
        pixel(&image, 64, 30),
        [200, 50, 20],
        1,
        "inside the shifted gate",
    );
}
