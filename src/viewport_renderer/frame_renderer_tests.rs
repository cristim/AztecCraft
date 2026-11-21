use super::*;
use crate::viewport_renderer::tests::{Offscreen, creation_context, device_setup, renderer_handle};
use azteccraft::{
    native_scene::NativeScene, playblast::SceneAt, scene::Primitive, view_scene::ViewScene,
    viewport::InvalidCameraInput,
};
use glam::Vec3;

fn camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap()
}

fn cube_view() -> ViewScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    ViewScene::from_native(&scene, 256 * 1024 * 1024).unwrap()
}

fn pixel(image: &[u8], width: u32, x: u32, y: u32) -> &[u8] {
    let offset = ((y * width + x) * 4) as usize;
    &image[offset..offset + 4]
}

fn frame_renderer(grid: bool) -> WgpuFrameRenderer {
    let creation = creation_context(eframe::egui::Context::default());
    let viewport = ViewportRenderer::initialize(&creation).unwrap();
    let scene_at: SceneAt = Box::new(|_| Ok(cube_view()));
    viewport
        .frame_renderer(scene_at, camera(), grid, ShadingMode::SmoothShadeAll)
        .map_err(|error| error.to_string())
        .unwrap()
}

#[test]
fn render_errors_map_to_typed_playblast_errors() {
    for (error, expected) in [
        (RenderError::Uninitialized, PlayblastError::NoAdapter),
        (RenderError::ResourceLimit, PlayblastError::TooLarge),
        (
            RenderError::Camera(InvalidCameraInput),
            PlayblastError::Render("invalid viewport camera"),
        ),
        (
            RenderError::NonFiniteClip,
            PlayblastError::Render("viewport geometry is out of range"),
        ),
        (
            RenderError::Backend("device lost".into()),
            PlayblastError::Render(BACKEND_FAILED),
        ),
    ] {
        assert_eq!(playblast_error(error), expected);
    }
}

#[test]
fn a_latched_device_error_fails_the_frame_without_a_panic() {
    let viewport = renderer_handle();
    assert_eq!(backend_state(&viewport), Ok(()));
    viewport.errors.record("device lost (Unknown): gone".into());
    assert_eq!(
        backend_state(&viewport),
        Err(PlayblastError::Render(BACKEND_FAILED))
    );
}

#[test]
fn a_renderer_without_a_window_device_reports_no_adapter() {
    let scene_at: SceneAt = Box::new(|_| Ok(cube_view()));
    let result =
        renderer_handle().frame_renderer(scene_at, camera(), true, ShadingMode::SmoothShadeAll);
    assert_eq!(result.err(), Some(PlayblastError::NoAdapter));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_frame_equals_the_viewport_render_path() {
    let creation = creation_context(eframe::egui::Context::default());
    let viewport = ViewportRenderer::initialize(&creation).unwrap();
    let scene_at: SceneAt = Box::new(|_| Ok(cube_view()));
    let mut adapter = viewport
        .frame_renderer(scene_at, camera(), true, ShadingMode::SmoothShadeAll)
        .map_err(|error| error.to_string())
        .unwrap();
    let mut reference = Offscreen::from_context(&creation);
    let size = reference.size;
    let expected = reference.image(&cube_view(), &camera(), None);
    let frame = adapter.render(1, size, size).unwrap();
    assert_eq!(frame, expected);
    assert_eq!(
        adapter.render(2, size, size).unwrap(),
        frame,
        "deterministic"
    );
    assert!(
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] != [31, 31, 31])
    );
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_frame_has_the_requested_non_square_size() {
    let mut adapter = frame_renderer(false);
    for (width, height) in [(100, 60), (130, 70)] {
        let frame = adapter.render(1, width, height).unwrap();
        assert_eq!(frame.len(), (width * height * 4) as usize);
        // Grid off: a corner is the clear colour and the centre is the cube (+-2 per channel).
        assert_eq!(pixel(&frame, width, 0, 0), [31, 31, 31, 255]);
        let centre = pixel(&frame, width, width / 2, height / 2);
        assert!(
            centre[..3].iter().all(|&c| c.abs_diff(31) > 2),
            "{centre:?}"
        );
        assert_eq!(centre[3], 255);
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_frame_failures_are_typed() {
    let mut adapter = frame_renderer(true);
    assert_eq!(
        adapter.render(1, 0, 10),
        Err(PlayblastError::Render("invalid viewport camera"))
    );
    assert_eq!(adapter.render(1, 1 << 24, 4), Err(PlayblastError::TooLarge));
    adapter.scene_at = Box::new(|_| Err(PlayblastError::InvalidRange));
    assert_eq!(adapter.render(1, 8, 8), Err(PlayblastError::InvalidRange));
    adapter
        .viewport
        .errors
        .record("device lost (Unknown): gone".into());
    assert_eq!(
        adapter.render(1, 8, 8),
        Err(PlayblastError::Render(BACKEND_FAILED))
    );
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_keeps_rgba_channel_order_on_coloured_pixels() {
    let (_, _, device, queue) = device_setup();
    let view = cube_view();
    let selected = view.instances()[0].path()[..1].to_vec();
    let submission = Submission::new(
        &view,
        &camera(),
        [128; 2],
        SelectionFeedback::Object(Some(&selected)),
        false,
        ShadingMode::SmoothShadeAll,
        RENDER_FORMAT,
        device.limits().max_buffer_size,
    )
    .unwrap();
    let frame = OffscreenRenderer::new(device, queue)
        .render(&submission, [128; 2])
        .unwrap();
    // The selected edge colour is orange; a red/blue swap would make it (70, 163, 242).
    assert!(frame.as_chunks::<4>().0.contains(&[242, 163, 70, 255]));
}

#[test]
fn the_frame_renderer_can_move_to_the_job_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<WgpuFrameRenderer>();
    assert_send::<Box<dyn FrameRenderer>>();
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_refuses_oversize_without_latching_the_device_error() {
    let mut adapter = frame_renderer(false);
    let limits = adapter.offscreen.device.limits();
    // Within the texture edge limit, past the readback buffer limit when the device is small;
    // otherwise the edge limit refuses it. Either way it must be typed and must not latch.
    let edge = limits.max_texture_dimension_2d;
    for size in [(edge + 1, 4), (edge, edge.max(1) + 1)] {
        assert_eq!(
            adapter.render(1, size.0, size.1),
            Err(PlayblastError::TooLarge)
        );
        assert_eq!(adapter.viewport.terminal_error(), None);
    }
    let frame = adapter.render(1, 100, 60).unwrap();
    assert_eq!(frame.len(), 100 * 60 * 4);
}

/// Analytic oracle independent of the window path's shading: one flat triangle covering the
/// top-left half of the top-left quadrant. Cross-adapter policy: CI runs the lavapipe software
/// rasterizer, so analytic checks use +-2 per channel and only sample well inside or outside
/// the shape; bit-exact equality is kept for comparisons on one adapter only.
#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_playblast_flat_triangle_has_known_colour_and_orientation() {
    const COLOUR: [u8; 3] = [200, 50, 20];
    let (_, _, device, queue) = device_setup();
    let mut submission = Submission {
        bytes: Vec::new(),
        surfaces: 0..3,
        lines: 3..3,
        markers: 3..3,
        centers_degraded: false,
    };
    // Clip space: y = +1 is the top row. Corners: top-left, top-middle, middle-left.
    for clip in [[-1.0, 1.0], [0.0, 1.0], [-1.0, 0.0]] {
        submission
            .clip_vertex(
                glam::Vec4::new(clip[0], clip[1], 0.5, 1.0),
                eframe::egui::Color32::from_rgb(COLOUR[0], COLOUR[1], COLOUR[2]),
                RENDER_FORMAT,
            )
            .unwrap();
    }
    let (width, height) = (100, 60);
    let frame = OffscreenRenderer::new(device, queue)
        .render(&submission, [width, height])
        .unwrap();
    let near = |actual: &[u8], expected: [u8; 4]| {
        actual
            .iter()
            .zip(expected)
            .all(|(&a, e)| a.abs_diff(e) <= 2)
    };
    // Inside, near the top-left corner: the triangle colour with R and B in order.
    assert!(near(pixel(&frame, width, 4, 3), [200, 50, 20, 255]));
    // Outside: the clear colour right of the hypotenuse, and below it (a vertical flip would
    // put the triangle at the bottom-left instead).
    for (x, y) in [(80, 3), (4, 55), (90, 50), (4, 40)] {
        assert!(
            near(pixel(&frame, width, x, y), [31, 31, 31, 255]),
            "{x},{y}"
        );
    }
}
