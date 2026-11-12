use super::*;
use azteccraft::{
    native_scene::{NativeScene, NativeTransform},
    scene::Primitive,
};
use glam::Vec2;
use std::{sync::mpsc, time::Duration};

const VIEW_BUDGET: usize = 256 * 1024 * 1024;
const SIZE: u32 = 128;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
// Frozen CPU oracle from f11ec66, source blob fbd5cbb0dac65c0004a3b4fd26ac921cb145c152.
// Test-only duplication preserves executable fail-before pixels after GPU promotion.
#[path = "parent_cpu_reference.rs"]
mod parent_cpu_reference;

fn device_setup() -> (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("headless GPU adapter is required");
    eprintln!("headless GPU: {:?}", adapter.get_info());
    if std::env::var_os("AZTECCRAFT_SOFTWARE_GPU").is_some() {
        assert_eq!(adapter.get_info().backend, wgpu::Backend::Vulkan);
        assert_eq!(adapter.get_info().device_type, wgpu::DeviceType::Cpu);
    }
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("headless GPU device is required");
    (instance, adapter, device, queue)
}

pub(crate) fn creation_context(context: egui::Context) -> eframe::CreationContext<'static> {
    let (instance, adapter, device, queue) = device_setup();
    let renderer = egui_wgpu::Renderer::new(
        &device,
        FORMAT,
        egui_wgpu::RendererOptions {
            depth_stencil_format: Some(wgpu::TextureFormat::Depth32Float),
            ..egui_wgpu::RendererOptions::PREDICTABLE
        },
    );
    let mut context = eframe::CreationContext::_new_kittest(context);
    context.wgpu_render_state = Some(egui_wgpu::RenderState {
        instance,
        adapter,
        available_adapters: Vec::new(),
        device,
        queue,
        target_format: FORMAT,
        renderer: Arc::new(egui::epaint::mutex::RwLock::new(renderer)),
        surface_config: egui_wgpu::SurfaceConfig::LOW_LATENCY,
    });
    context
}

fn camera(near: f32) -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), near, 100.0).unwrap()
}

fn cube() -> NativeScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    scene
}

pub(crate) fn renderer_handle() -> ViewportRenderer {
    ViewportRenderer {
        errors: BackendErrors {
            first: Arc::default(),
            context: egui::Context::default(),
        },
        format: FORMAT,
        max_buffer_size: 1024 * 1024,
        submission: Arc::default(),
    }
}

pub(crate) struct RecordedSubmission {
    pub surfaces: Range<u32>,
    pub lines: Range<u32>,
    pub vertices: Vec<[f32; 8]>,
}

pub(crate) fn recorded_submission(renderer: &ViewportRenderer) -> RecordedSubmission {
    let recorded = renderer.submission.lock();
    let submission = recorded
        .as_ref()
        .expect("actual callback submission required");
    RecordedSubmission {
        surfaces: submission.surfaces.clone(),
        lines: submission.lines.clone(),
        vertices: submission
            .bytes
            .as_chunks::<VERTEX_BYTES>()
            .0
            .iter()
            .map(|vertex| {
                std::array::from_fn(|i| {
                    f32::from_ne_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
                })
            })
            .collect(),
    }
}

#[test]
fn actual_native_submission_checks_exact_payload_limit_and_preserves_crossing_corners() {
    let scene = cube();
    let before = scene.clone();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let submission =
        Submission::new(&view, &camera(4.75), 1.0, None, true, FORMAT, 1024 * 1024).unwrap();
    assert_eq!(submission.surfaces, 0..36);
    assert_eq!(submission.lines, 36..168);
    assert_eq!(submission.bytes.len(), 168 * VERTEX_BYTES);
    assert!(
        submission.bytes[..36 * VERTEX_BYTES]
            .as_chunks::<VERTEX_BYTES>()
            .0
            .iter()
            .any(|vertex| { f32::from_ne_bytes(vertex[8..12].try_into().unwrap()) < 0.0 })
    );
    assert!(
        Submission::new(
            &view,
            &camera(4.75),
            1.0,
            None,
            true,
            FORMAT,
            submission.bytes.len() as u64
        )
        .is_ok()
    );
    assert!(matches!(
        Submission::new(
            &view,
            &camera(4.75),
            1.0,
            None,
            true,
            FORMAT,
            submission.bytes.len() as u64 - 1
        ),
        Err(RenderError::ResourceLimit)
    ));
    assert_eq!(scene, before);
}

#[test]
fn aggregate_upload_budget_and_device_buffer_limit_have_independent_boundaries() {
    let maximum = MAX_UPLOAD_WORKING_BYTES / 4;
    assert_eq!(check_buffer_bytes(maximum, u64::MAX), Ok(()));
    assert_eq!(
        check_buffer_bytes(maximum + 1, u64::MAX),
        Err(RenderError::ResourceLimit)
    );
    assert_eq!(check_buffer_bytes(4096, 4096), Ok(()));
    assert_eq!(
        check_buffer_bytes(4096, 4095),
        Err(RenderError::ResourceLimit)
    );
    assert_eq!(
        check_buffer_bytes(u64::MAX, u64::MAX),
        Err(RenderError::ResourceLimit)
    );
}

#[test]
fn grid_visibility_omits_only_original_grid_bytes_and_its_allocation_charge() {
    let scene = cube();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let camera = Camera::default();
    for selected in [None, Some(view.instances()[0].path())] {
        let visible =
            Submission::new(&view, &camera, 1.0, selected, true, FORMAT, 1024 * 1024).unwrap();
        let hidden =
            Submission::new(&view, &camera, 1.0, selected, false, FORMAT, 1024 * 1024).unwrap();
        let restored =
            Submission::new(&view, &camera, 1.0, selected, true, FORMAT, 1024 * 1024).unwrap();
        assert_eq!(visible.bytes, restored.bytes);
        assert_eq!(visible.surfaces, hidden.surfaces);
        assert_eq!(visible.vertex_count() - hidden.vertex_count(), 84);
        assert_eq!(visible.lines.start, hidden.lines.start);
        assert_eq!(visible.lines.end - hidden.lines.end, 84);
        let surface_bytes = visible.surfaces.end as usize * VERTEX_BYTES;
        assert_eq!(
            visible.bytes[..surface_bytes],
            hidden.bytes[..surface_bytes]
        );
        assert_eq!(
            visible.bytes[surface_bytes + 84 * VERTEX_BYTES..],
            hidden.bytes[surface_bytes..]
        );
        assert!(
            Submission::new(
                &view,
                &camera,
                1.0,
                selected,
                false,
                FORMAT,
                hidden.bytes.len() as u64
            )
            .is_ok()
        );
        assert!(matches!(
            Submission::new(
                &view,
                &camera,
                1.0,
                selected,
                true,
                FORMAT,
                hidden.bytes.len() as u64
            ),
            Err(RenderError::ResourceLimit)
        ));
    }
    let empty = ViewScene::from_native(&NativeScene::default(), VIEW_BUDGET).unwrap();
    let hidden = Submission::new(&empty, &camera, 1.0, None, false, FORMAT, 0).unwrap();
    assert!(hidden.bytes.is_empty());
    assert_eq!(hidden.surfaces, 0..0);
    assert_eq!(hidden.lines, 0..0);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_grid_callback_clears_cold_and_warm_empty_frames_and_restores_pixels() {
    let creation = creation_context(egui::Context::default());
    let producer = ViewportRenderer::initialize(&creation).unwrap();
    let mut gpu = Offscreen::from_context(&creation);
    let empty = ViewScene::from_native(&NativeScene::default(), VIEW_BUDGET).unwrap();
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let camera = Camera::default();
    for pixels_per_point in [1.0, 2.0] {
        let mut images = Vec::new();
        for (scene, visible) in [
            (&empty, false),
            (&view, true),
            (&empty, false),
            (&empty, true),
            (&empty, false),
            (&view, false),
            (&view, true),
        ] {
            let frame = ui_frame(pixels_per_point, |ui| {
                crate::viewport_ui::capture(ui, &camera)
                    .unwrap()
                    .unwrap()
                    .paint(
                        Some(&producer),
                        &camera,
                        scene,
                        scene.instances().first().map(|instance| instance.path()),
                        visible,
                    )
                    .unwrap();
            });
            images.push(gpu.render(frame));
            assert!(producer.terminal_error().is_none());
        }
        assert_eq!(images[0], images[2]);
        assert_eq!(images[0], images[4]);
        assert_ne!(images[0], images[3]);
        assert_ne!(images[1], images[5]);
        assert_eq!(images[1], images[6]);
        assert_eq!(pixel(&images[0], 64, 40), &[31, 31, 31, 255]);
    }
}

#[test]
fn finite_native_world_that_overflows_clip_reports_a_render_limit() {
    let mut scene = cube();
    let node = scene.nodes()[0].id();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [f64::from(f32::MAX), 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    assert!(matches!(
        Submission::new(&view, &camera(0.01), 1.0, None, true, FORMAT, 1024 * 1024),
        Err(RenderError::NonFiniteClip)
    ));
}

#[test]
fn startup_without_wgpu_and_latched_errors_never_emit_a_callback() {
    let context = eframe::CreationContext::_new_kittest(egui::Context::default());
    assert!(matches!(
        ViewportRenderer::initialize(&context),
        Err(RenderError::Backend(_))
    ));
    let renderer = renderer_handle();
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    assert!(matches!(
        renderer.callback(&view, &camera(0.01), Rect::NOTHING, None, true),
        Err(RenderError::Camera(_))
    ));
    renderer.errors.record("first backend failure".into());
    renderer.errors.record("later failure".into());
    assert_eq!(
        renderer.terminal_error().as_deref(),
        Some("first backend failure")
    );
    assert!(matches!(
        renderer.callback(
            &view,
            &camera(0.01),
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(128.0, 128.0)),
            None,
            true,
        ),
        Err(RenderError::Backend(_))
    ));
}

#[test]
fn backend_error_repaint_can_read_the_latched_diagnostic() {
    let renderer = renderer_handle();
    let context = renderer.errors.context.clone();
    for _ in 0..2 {
        let _ = context.run_logic(&Default::default(), |_| {});
    }
    let first = renderer.errors.first.clone();
    let (send, receive) = mpsc::channel();
    context.set_request_repaint_callback(move |_| {
        send.send(first.lock().clone()).unwrap();
    });
    renderer.errors.record("device validation failed".into());
    assert_eq!(
        receive.recv_timeout(Duration::from_secs(1)).unwrap(),
        Some("device validation failed".into())
    );
    context.set_request_repaint_callback(|_| {});
}

pub(crate) struct Offscreen {
    device: wgpu::Device,
    queue: wgpu::Queue,
    resources: GpuResources,
    format: wgpu::TextureFormat,
    renderer: Option<Arc<egui::epaint::mutex::RwLock<egui_wgpu::Renderer>>>,
}

enum TestFrame {
    Geometry(Submission),
    Egui(egui::Context, Box<egui::FullOutput>),
}

impl Offscreen {
    pub(crate) fn new() -> Self {
        Self::with_format(FORMAT)
    }

    fn with_format(format: wgpu::TextureFormat) -> Self {
        let (_, _, device, queue) = device_setup();
        let resources = GpuResources::new(&device, format);
        Self {
            device,
            queue,
            resources,
            format,
            renderer: None,
        }
    }

    fn image(
        &mut self,
        view: &ViewScene,
        camera: &Camera,
        selected: Option<&[NativeId]>,
    ) -> Vec<u8> {
        let submission = Submission::new(
            view,
            camera,
            1.0,
            selected,
            true,
            self.format,
            self.device.limits().max_buffer_size,
        )
        .unwrap();
        self.render(TestFrame::Geometry(submission))
    }

    pub(crate) fn from_context(context: &eframe::CreationContext<'_>) -> Self {
        let state = context.wgpu_render_state.as_ref().unwrap();
        Self {
            device: state.device.clone(),
            queue: state.queue.clone(),
            format: state.target_format,
            resources: GpuResources::new(&state.device, state.target_format),
            renderer: Some(state.renderer.clone()),
        }
    }

    pub(crate) fn render_egui(
        &mut self,
        context: egui::Context,
        output: egui::FullOutput,
    ) -> Vec<u8> {
        self.render(TestFrame::Egui(context, Box::new(output)))
    }

    fn render(&mut self, frame: TestFrame) -> Vec<u8> {
        let extent = wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        };
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport regression color"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("viewport regression depth"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport regression pixels"),
            size: u64::from(SIZE * SIZE * 4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let color_view = color.create_view(&Default::default());
        let depth_view = depth.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut egui_frame = match frame {
            TestFrame::Geometry(ref submission) => {
                self.resources
                    .upload(&self.device, &self.queue, submission)
                    .unwrap();
                None
            }
            TestFrame::Egui(ref context, ref output) => {
                let renderer = self.renderer.clone().unwrap_or_else(|| {
                    let mut renderer = egui_wgpu::Renderer::new(
                        &self.device,
                        self.format,
                        egui_wgpu::RendererOptions {
                            depth_stencil_format: Some(wgpu::TextureFormat::Depth32Float),
                            ..egui_wgpu::RendererOptions::PREDICTABLE
                        },
                    );
                    renderer
                        .callback_resources
                        .insert(GpuResources::new(&self.device, self.format));
                    Arc::new(egui::epaint::mutex::RwLock::new(renderer))
                });
                let mut renderer_guard = renderer.write();
                for (id, deltas) in &output.textures_delta.set {
                    for delta in deltas {
                        renderer_guard.update_texture(&self.device, &self.queue, *id, delta);
                    }
                }
                let jobs = context.tessellate(output.shapes.clone(), output.pixels_per_point);
                let screen = egui_wgpu::ScreenDescriptor {
                    size_in_pixels: [SIZE; 2],
                    pixels_per_point: output.pixels_per_point,
                };
                let commands = renderer_guard.update_buffers(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    &jobs,
                    &screen,
                );
                self.queue.submit(commands);
                drop(renderer_guard);
                Some((renderer, jobs, screen))
            }
        };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("production viewport regression"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 31.0 / 255.0,
                            g: 31.0 / 255.0,
                            b: 31.0 / 255.0,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some((renderer, jobs, screen)) = &egui_frame {
                renderer
                    .read()
                    .render(&mut pass.forget_lifetime(), jobs, screen);
            } else if let TestFrame::Geometry(submission) = &frame {
                self.resources.draw(&mut pass, submission).unwrap();
            }
        }
        if let TestFrame::Egui(_, mut output) = frame {
            let mut renderer = egui_frame.as_mut().unwrap().0.write();
            for id in &output.textures_delta.free {
                renderer.free_texture(id);
            }
            output.textures_delta.clear();
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(SIZE * 4),
                    rows_per_image: Some(SIZE),
                },
            },
            extent,
        );
        let submitted = self.queue.submit([encoder.finish()]);
        let (send, receive) = mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
        receive
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let pixels = readback.slice(..).get_mapped_range().unwrap().to_vec();
        readback.unmap();
        pixels
    }
}

fn pixel(image: &[u8], x: u32, y: u32) -> &[u8] {
    let offset = ((y * SIZE + x) * 4) as usize;
    &image[offset..offset + 4]
}

fn ui_frame(pixels_per_point: f32, draw: impl FnMut(&mut egui::Ui)) -> TestFrame {
    let context = egui::Context::default();
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::Vec2::splat(SIZE as f32 / pixels_per_point),
        )),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(pixels_per_point);
    let output = context.run_ui(input, draw);
    TestFrame::Egui(context, Box::new(output))
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_actual_parent_painter_loses_the_crossing_side() {
    let mut gpu = Offscreen::new();
    let native = cube();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    let image = gpu.image(&view, &camera(4.75), Some(&path[..path.len() - 1]));
    assert_eq!(pixel(&image, 79, 57), &[95, 110, 120, 255]);
    let parent_camera = camera(4.75);
    let parent = ui_frame(1.0, |ui| {
        let mut captured = parent_cpu_reference::capture(ui, &parent_camera)
            .unwrap()
            .unwrap();
        assert_eq!(captured.click_ndc(), None);
        assert!(captured.take_navigation().is_empty());
        captured
            .paint(&parent_camera, &view, Some(&path[..path.len() - 1]))
            .unwrap();
    });
    let image = gpu.render(parent);
    assert_eq!(pixel(&image, 79, 57), &[52, 44, 35, 255]);
    assert_ne!(pixel(&image, 79, 57), &[95, 110, 120, 255]);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_actual_viewport_caller_preserves_crossing_side() {
    let mut gpu = Offscreen::new();
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    let frame = ui_frame(1.0, |ui| {
        let producer = renderer_handle();
        crate::viewport_ui::capture(ui, &camera(4.75))
            .unwrap()
            .unwrap()
            .paint(
                Some(&producer),
                &camera(4.75),
                &view,
                Some(&path[..path.len() - 1]),
                true,
            )
            .unwrap();
    });
    assert_eq!(pixel(&gpu.render(frame), 79, 57), &[95, 110, 120, 255]);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_actual_callback_scissor_and_following_egui_restore_at_one_and_two_x() {
    let mut gpu = Offscreen::new();
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    for pixels_per_point in [1.0, 2.0] {
        let frame = ui_frame(pixels_per_point, |ui| {
            ui.painter()
                .rect_filled(ui.max_rect(), 0.0, Color32::from_gray(31));
            let to_points = |physical| physical / pixels_per_point;
            let rect = Rect::from_min_max(
                egui::pos2(to_points(16.0), to_points(16.0)),
                egui::pos2(to_points(112.0), to_points(112.0)),
            );
            let clip = Rect::from_min_max(
                egui::pos2(to_points(32.0), to_points(32.0)),
                egui::pos2(to_points(96.0), to_points(96.0)),
            );
            let renderer = renderer_handle();
            ui.painter().with_clip_rect(clip).add(
                renderer
                    .callback(
                        &view,
                        &camera(0.01),
                        rect,
                        Some(&path[..path.len() - 1]),
                        true,
                    )
                    .unwrap(),
            );
            ui.painter().rect_filled(
                Rect::from_min_max(
                    egui::pos2(to_points(2.0), to_points(2.0)),
                    egui::pos2(to_points(6.0), to_points(6.0)),
                ),
                0.0,
                Color32::MAGENTA,
            );
        });
        let image = gpu.render(frame);
        assert_eq!(pixel(&image, 64, 58), &[95, 110, 120, 255]);
        assert_eq!(pixel(&image, 20, 64), &[31, 31, 31, 255]);
        assert_eq!(pixel(&image, 3, 3), &[255, 0, 255, 255]);
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_actual_validation_callback_latches_and_blocks_submission() {
    let gpu = Offscreen::new();
    let renderer = renderer_handle();
    renderer.errors.install(&gpu.device);
    let _invalid_shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("intentional viewport validation regression"),
            source: wgpu::ShaderSource::Wgsl("invalid shader syntax".into()),
        });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    let error = renderer.terminal_error().unwrap();
    assert!(error.contains("intentional viewport validation regression"));
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    assert!(matches!(
        renderer.callback(
            &view,
            &camera(0.01),
            Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(128.0)),
            None,
            true,
        ),
        Err(RenderError::Backend(_))
    ));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_intersecting_native_planes_change_front_surface_across_the_image() {
    let mut gpu = Offscreen::new();
    for selected_first in [false, true] {
        let mut scene = NativeScene::default();
        let first = scene.create(Primitive::Plane, "first").unwrap();
        let second = scene.create(Primitive::Plane, "second").unwrap();
        let (selected, other) = if selected_first {
            (first, second)
        } else {
            (second, first)
        };
        for (node, angle) in [(selected, 45.0), (other, -45.0)] {
            scene
                .set_transform(
                    node,
                    NativeTransform {
                        rotation: [90.0, angle, 0.0],
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
        let path = view
            .instances()
            .iter()
            .find(|instance| instance.transform() == selected)
            .unwrap()
            .path();
        let camera = camera(0.01);
        for (x, expected) in [(58.5, selected), (70.5, other)] {
            let ndc = Vec2::new(2.0 * x / SIZE as f32 - 1.0, 1.0 - 2.0 * 58.5 / SIZE as f32);
            assert_eq!(
                camera
                    .pick_view(&view, ndc, 1.0)
                    .unwrap()
                    .unwrap()
                    .transform,
                expected
            );
        }
        let image = gpu.image(&view, &camera, Some(&path[..path.len() - 1]));
        assert_eq!(pixel(&image, 58, 58), &[95, 110, 120, 255]);
        assert_eq!(pixel(&image, 70, 58), &[83, 83, 83, 255]);
        if !selected_first {
            let parent = ui_frame(1.0, |ui| {
                parent_cpu_reference::capture(ui, &camera)
                    .unwrap()
                    .unwrap()
                    .paint(&camera, &view, Some(&path[..path.len() - 1]))
                    .unwrap();
            });
            let parent = gpu.render(parent);
            assert_eq!(pixel(&parent, 70, 58), &[95, 110, 120, 255]);
            assert_ne!(pixel(&parent, 70, 58), pixel(&image, 70, 58));
        }
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_single_planes_remain_visible_from_back_and_reflected_and_across_far_clip() {
    let mut gpu = Offscreen::new();
    for (rotation, scale) in [
        ([90.0, 0.0, 0.0], [1.0; 3]),
        ([90.0, 180.0, 0.0], [1.0; 3]),
        ([90.0, 0.0, 0.0], [-1.0, 1.0, 1.0]),
    ] {
        let mut scene = NativeScene::default();
        let node = scene.create(Primitive::Plane, "plane").unwrap();
        scene
            .set_transform(
                node,
                NativeTransform {
                    rotation,
                    scale,
                    ..Default::default()
                },
            )
            .unwrap();
        let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
        let path = view.instances()[0].path();
        let image = gpu.image(&view, &camera(0.01), Some(&path[..path.len() - 1]));
        assert_eq!(pixel(&image, 64, 58), &[95, 110, 120, 255]);
    }
    let mut scene = NativeScene::default();
    let node = scene.create(Primitive::Plane, "far_crossing").unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [90.0, 45.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    let camera = Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 5.0).unwrap();
    for (x, visible) in [(58.5, true), (70.5, false)] {
        let ndc = Vec2::new(2.0 * x / SIZE as f32 - 1.0, 1.0 - 2.0 * 58.5 / SIZE as f32);
        assert_eq!(
            camera.pick_view(&view, ndc, 1.0).unwrap().is_some(),
            visible
        );
    }
    let image = gpu.image(&view, &camera, Some(&path[..path.len() - 1]));
    assert_eq!(pixel(&image, 58, 58), &[95, 110, 120, 255]);
    assert_eq!(pixel(&image, 70, 58), &[31, 31, 31, 255]);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_coplanar_first_path_matches_pick_and_colors_on_linear_and_srgb_targets() {
    let mut scene = NativeScene::default();
    let first = scene.create(Primitive::Cube, "first").unwrap();
    let second = scene.create(Primitive::Cube, "second").unwrap();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let camera = camera(0.01);
    assert_eq!(
        camera
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .unwrap()
            .transform,
        first
    );
    for format in [FORMAT, wgpu::TextureFormat::Rgba8UnormSrgb] {
        let mut gpu = Offscreen::with_format(format);
        for selected in [first, second] {
            let path = view
                .instances()
                .iter()
                .find(|instance| instance.transform() == selected)
                .unwrap()
                .path();
            let image = gpu.image(&view, &camera, Some(&path[..path.len() - 1]));
            let expected = if selected == first {
                [95_u8, 110, 120, 255]
            } else {
                [83, 83, 83, 255]
            };
            for (actual, expected) in pixel(&image, 64, 58).iter().zip(expected) {
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "{format:?}: {:?}",
                    pixel(&image, 64, 58)
                );
            }
        }
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_near_crossing_geometry_survives_and_occlusion_is_order_independent() {
    let mut gpu = Offscreen::new();
    let scene = cube();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    let clip_camera = camera(4.75);
    let ndc = Vec2::new(
        2.0 * 79.5 / SIZE as f32 - 1.0,
        1.0 - 2.0 * 57.5 / SIZE as f32,
    );
    assert!(clip_camera.pick_view(&view, ndc, 1.0).unwrap().is_some());
    let image = gpu.image(&view, &clip_camera, Some(&path[..path.len() - 1]));
    assert_eq!(pixel(&image, 79, 57), &[95, 110, 120, 255]);
    for front_first in [false, true] {
        let mut scene = NativeScene::default();
        let first = scene.create(Primitive::Cube, "first").unwrap();
        let second = scene.create(Primitive::Cube, "second").unwrap();
        let (front, back) = if front_first {
            (first, second)
        } else {
            (second, first)
        };
        scene
            .set_transform(
                front,
                NativeTransform {
                    translation: [0.0, 0.0, 1.0],
                    ..Default::default()
                },
            )
            .unwrap();
        let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
        let back = view
            .instances()
            .iter()
            .find(|instance| instance.transform() == back)
            .unwrap();
        let path = back.path();
        let image = gpu.image(&view, &camera(0.01), Some(&path[..path.len() - 1]));
        assert_eq!(pixel(&image, 64, 58), &[83, 83, 83, 255]);
    }
    for (translation, scale, expected) in [
        ([0.0, 0.0, -200.0], [1.0; 3], [31, 31, 31, 255]),
        ([0.0; 3], [-1.0, 2.0, 0.5], [83, 83, 83, 255]),
        ([0.0; 3], [0.0, 2.0, 0.5], [31, 31, 31, 255]),
    ] {
        let mut scene = cube();
        scene
            .set_transform(
                scene.nodes()[0].id(),
                NativeTransform {
                    translation,
                    scale,
                    ..Default::default()
                },
            )
            .unwrap();
        let before = scene.clone();
        let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
        let image = gpu.image(&view, &camera(0.01), None);
        assert_eq!(pixel(&image, 70, 58), &expected);
        assert_eq!(scene, before);
    }
}
