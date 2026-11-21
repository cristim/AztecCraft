//! #459 P7b: the plate through the real GpuResources on an offscreen target. Pixel values are
//! worked by hand from the shader's rules; the clear colour is (31, 31, 31).
use super::*;
use azteccraft::image_plane::{
    cache::FrameKey, decode::PlateFrame, fit::PlateQuad, model::PlateDisplayMode,
};

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
