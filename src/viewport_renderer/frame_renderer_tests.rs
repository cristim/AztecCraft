use super::*;
use crate::viewport_renderer::tests::{Offscreen, creation_context, device_setup, renderer_handle};
use azteccraft::{
    native_scene::NativeScene, scene::Primitive, view_scene::ViewScene,
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
