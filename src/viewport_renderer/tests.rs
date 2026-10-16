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

#[path = "tests/manipulator_handles.rs"]
mod manipulator_handles;

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

fn curve_scene(second: bool, hidden: bool, endpoint_z: f64) -> NativeScene {
    let mut source = String::from(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film;",
    );
    for (index, x) in if second {
        vec![(0, -0.8), (1, 0.8)]
    } else {
        vec![(0, 0.0)]
    } {
        source.push_str(&format!(" createNode transform -n \"curve{index}\"; setAttr \".t\" -type \"double3\" {x} 0 0; setAttr \".v\" {}; createNode nurbsCurve -n \"shape{index}\" -p \"curve{index}\"; setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 -0.4 0.5 {endpoint_z} -0.4 1.5 0 0.4 1.5 0 0.4 0.5 {endpoint_z};", if hidden { "no" } else { "yes" }));
    }
    azteccraft::native_scene::ascii::decode(std::sync::Arc::from(source.into_bytes())).unwrap()
}

fn region_has(image: &[u8], camera: &Camera, world: Vec3, color: [u8; 4]) -> bool {
    let ndc = camera.project(world, 1.0).unwrap().unwrap();
    let x = ((ndc.x + 1.0) * 0.5 * SIZE as f32).floor() as i32;
    let y = ((1.0 - ndc.y) * 0.5 * SIZE as f32).floor() as i32;
    (y - 2..=y + 2).any(|y| {
        (x - 2..=x + 2).any(|x| {
            x >= 0
                && y >= 0
                && x < SIZE as i32
                && y < SIZE as i32
                && pixel(image, x as u32, y as u32) == color
        })
    })
}

#[test]
fn face_mode_feedback_on_a_curves_only_scene_draws_lines_without_markers_or_highlight() {
    let native = curve_scene(true, false, 0.0);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert!(view.instances().is_empty());
    let curve = &view.curve_instances()[0];
    let vertices = 84
        + view
            .curve_instances()
            .iter()
            .map(|curve| 2 * curve.segments().len())
            .sum::<usize>();
    for selection in [
        SelectionFeedback::Face {
            active: None,
            selected: None,
        },
        SelectionFeedback::Face {
            active: Some((curve.transform(), &curve.path()[..curve.path().len() - 1])),
            selected: None,
        },
    ] {
        let submission = Submission::new(
            &view,
            &camera(0.01),
            [SIZE; 2],
            selection,
            true,
            FORMAT,
            u64::MAX,
        )
        .unwrap();
        assert_eq!(submission.surfaces, 0..0);
        assert_eq!(submission.lines, 0..vertices as u32);
        assert_eq!(submission.centers.start, submission.centers.end);
        assert_eq!(submission.bytes.len(), vertices * VERTEX_BYTES);
    }
}

#[test]
fn curve_submission_preflight_counts_actual_segments_and_refuses_short_buffer() {
    let native = curve_scene(true, false, 0.0);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let vertices = 84
        + view
            .curve_instances()
            .iter()
            .map(|curve| 2 * curve.segments().len())
            .sum::<usize>();
    let bytes = vertices * VERTEX_BYTES;
    let submission = Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        SelectionFeedback::Object(None),
        true,
        FORMAT,
        bytes as u64,
    )
    .unwrap();
    assert_eq!(submission.surfaces, 0..0);
    assert_eq!(submission.lines, 0..vertices as u32);
    assert_eq!(submission.bytes.len(), bytes);
    assert!(matches!(
        Submission::new(
            &view,
            &camera(0.01),
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true,
            FORMAT,
            bytes as u64 - 1
        ),
        Err(RenderError::ResourceLimit)
    ));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_curve_evaluation_differs_from_hull_and_colors_exact_instance() {
    let native = curve_scene(true, false, 0.0);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let camera = camera(0.01);
    let mut gpu = Offscreen::new();
    let first = &view.curve_instances()[0];
    let image = gpu.image(&view, &camera, Some(&first.path()[..1]));
    assert!(region_has(
        &image,
        &camera,
        Vec3::new(-0.8, 1.25, 0.0),
        [242, 163, 70, 255]
    ));
    assert!(region_has(
        &image,
        &camera,
        Vec3::new(0.8, 1.25, 0.0),
        [145, 145, 145, 255]
    ));
    assert!(!region_has(
        &image,
        &camera,
        Vec3::new(0.8, 1.25, 0.0),
        [242, 163, 70, 255]
    ));
    assert!(!region_has(
        &image,
        &camera,
        Vec3::new(-0.8, 1.5, 0.0),
        [242, 163, 70, 255]
    ));
    let second = &view.curve_instances()[1];
    let image = gpu.image(&view, &camera, Some(&second.path()[..1]));
    assert!(region_has(
        &image,
        &camera,
        Vec3::new(0.8, 1.25, 0.0),
        [242, 163, 70, 255]
    ));
    assert!(region_has(
        &image,
        &camera,
        Vec3::new(-0.8, 1.25, 0.0),
        [145, 145, 145, 255]
    ));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_curve_hardware_near_clip_and_ancestor_visibility_are_real() {
    let native = curve_scene(false, false, 4.8);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let mut gpu = Offscreen::new();
    let clipped = gpu.image(&view, &camera(0.5), None);
    assert!(clipped.as_chunks::<4>().0.contains(&[145, 145, 145, 255]));
    let outside = gpu.image(&view, &camera(6.0), None);
    assert!(!outside.as_chunks::<4>().0.contains(&[145, 145, 145, 255]));
    let hidden = curve_scene(false, true, 0.0);
    let hidden_view = ViewScene::from_native(&hidden, VIEW_BUDGET).unwrap();
    assert!(hidden_view.curve_instances().is_empty());
    let image = gpu.image(&hidden_view, &camera(0.01), None);
    assert!(!image.as_chunks::<4>().0.contains(&[145, 145, 145, 255]));
}

#[allow(dead_code)]
mod maya_binary {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

// A synthetic Maya binary scene whose single mesh face has twenty corners.
fn twenty_gon_scene() -> NativeScene {
    use maya_binary::{chunk, counted, creation, group, scene, triple};
    const N: u32 = 20;
    let mut payload = b"o\0\x20".to_vec();
    let positions: Vec<u32> = (0..N)
        .flat_map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / N as f32;
            [angle.cos().to_bits(), angle.sin().to_bits(), 0]
        })
        .collect();
    counted(&mut payload, &positions);
    let endpoints: Vec<u32> = (0..N)
        .flat_map(|i| [i | if i == 0 { 0x8000_0000 } else { 0 }, (i + 1) % N])
        .collect();
    counted(&mut payload, &endpoints);
    let corners: Vec<u32> = (0..N)
        .map(|i| i | if i == N - 1 { 0x6000_0000 } else { 0 })
        .collect();
    counted(&mut payload, &corners);
    counted(&mut payload, &[1e20f32.to_bits(), 0, 0]);
    for word in [0u32, 1, 0] {
        payload.extend(word.to_be_bytes());
    }
    payload.extend(b"uv\0");
    counted(&mut payload, &vec![0; 2 * N as usize]);
    counted(&mut payload, &(0..N).collect::<Vec<_>>());
    payload.extend([0; 20]);
    let bytes = scene(
        &[
            group(
                b"XFRM",
                &[creation(b"object", None), triple(b"t", [0.0; 3])].concat(),
                4,
            ),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &payload, 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    );
    azteccraft::native_scene::binary::decode(bytes).unwrap()
}

#[test]
fn twenty_gon_submission_counts_fan_triangles_and_every_outline_edge() {
    let view = ViewScene::from_native(&twenty_gon_scene(), VIEW_BUDGET).unwrap();
    let surface = 18 * 3;
    let outline = 20 * 2;
    let vertices = surface + outline;
    let bytes = vertices * VERTEX_BYTES;
    let submission = Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        SelectionFeedback::Object(None),
        false,
        FORMAT,
        bytes as u64,
    )
    .unwrap();
    assert_eq!(submission.surfaces, 0..surface as u32);
    assert_eq!(submission.lines, surface as u32..vertices as u32);
    assert_eq!(submission.bytes.len(), bytes);
    assert!(matches!(
        Submission::new(
            &view,
            &camera(0.01),
            [SIZE; 2],
            SelectionFeedback::Object(None),
            false,
            FORMAT,
            bytes as u64 - 1
        ),
        Err(RenderError::ResourceLimit)
    ));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_twenty_gon_fills_every_fan_triangle() {
    let view = ViewScene::from_native(&twenty_gon_scene(), VIEW_BUDGET).unwrap();
    let camera = camera(0.01);
    let submission = Submission::new(
        &view,
        &camera,
        [SIZE; 2],
        SelectionFeedback::Object(None),
        false,
        FORMAT,
        u64::MAX,
    )
    .unwrap();
    let image = Offscreen::new().render(TestFrame::Geometry(submission));
    let instance = &view.instances()[0];
    for triangle in instance.faces()[0].triangles() {
        let [a, b, c] = triangle.map(|index| instance.world_vertices()[index]);
        assert!(region_has(
            &image,
            &camera,
            (a + b + c) / 3.0,
            [83, 83, 83, 255]
        ));
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_curve_line_submission_obeys_actual_surface_depth() {
    let native = curve_scene(false, false, 0.0);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let camera = camera(0.01);
    let curve = Submission::new(
        &view,
        &camera,
        [SIZE; 2],
        SelectionFeedback::Object(None),
        true,
        FORMAT,
        u64::MAX,
    )
    .unwrap();
    let curve_lines = &curve.bytes[84 * VERTEX_BYTES..];
    let mut gpu = Offscreen::new();
    for (z, visible) in [(0.0, false), (-2.0, true)] {
        let mut mesh = NativeScene::default();
        let id = mesh.create(Primitive::Cube, "occluder").unwrap();
        mesh.set_transform(
            id,
            NativeTransform {
                translation: [0.0, 0.0, z],
                scale: [4.0, 4.0, 1.0],
                ..Default::default()
            },
        )
        .unwrap();
        let mesh_view = ViewScene::from_native(&mesh, VIEW_BUDGET).unwrap();
        let mut submission = Submission::new(
            &mesh_view,
            &camera,
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true,
            FORMAT,
            u64::MAX,
        )
        .unwrap();
        submission.bytes.extend_from_slice(curve_lines);
        submission.lines.end = submission.vertex_count();
        let image = gpu.render(TestFrame::Geometry(submission));
        assert_eq!(
            region_has(
                &image,
                &camera,
                Vec3::new(0.0, 1.25, 0.0),
                [145, 145, 145, 255]
            ),
            visible
        );
    }
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
    pub centers: Range<u32>,
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
        centers: submission.centers.clone(),
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
    let submission = Submission::new(
        &view,
        &camera(4.75),
        [SIZE; 2],
        SelectionFeedback::Object(None),
        true,
        FORMAT,
        1024 * 1024,
    )
    .unwrap();
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
            [SIZE; 2],
            SelectionFeedback::Object(None),
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
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true,
            FORMAT,
            submission.bytes.len() as u64 - 1
        ),
        Err(RenderError::ResourceLimit)
    ));
    assert_eq!(scene, before);
}

#[test]
fn face_submission_highlights_only_exact_identity_and_charges_physical_markers() {
    let scene = cube();
    let before = scene.clone();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    let path = &instance.path()[..instance.path().len() - 1];
    let mut target = FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 1,
    };
    for size in [128, 256] {
        let selection = SelectionFeedback::Face {
            active: Some((instance.transform(), path)),
            selected: Some(&target),
        };
        let submission = Submission::new(
            &view,
            &camera(0.01),
            [size; 2],
            selection,
            true,
            FORMAT,
            1024 * 1024,
        )
        .unwrap();
        assert_eq!(submission.surfaces, 0..36);
        assert_eq!(submission.centers.end - submission.centers.start, 36);
        assert_eq!(submission.bytes.len(), 204 * VERTEX_BYTES);
        let selected_vertices = submission.bytes[..36 * VERTEX_BYTES]
            .as_chunks::<VERTEX_BYTES>()
            .0
            .iter()
            .filter(|vertex| f32::from_ne_bytes(vertex[16..20].try_into().unwrap()) > 0.35)
            .count();
        assert_eq!(selected_vertices, 6);
        let start = submission.centers.start as usize * VERTEX_BYTES;
        let coordinates: Vec<_> = submission.bytes[start..start + 6 * VERTEX_BYTES]
            .as_chunks::<VERTEX_BYTES>()
            .0
            .iter()
            .map(|vertex| {
                let value = |index: usize| {
                    f32::from_ne_bytes(vertex[index * 4..index * 4 + 4].try_into().unwrap())
                };
                Vec2::new(value(0), value(1)) / value(3) * (size as f32 * 0.5)
            })
            .collect();
        let min = coordinates
            .iter()
            .copied()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min);
        let max = coordinates
            .iter()
            .copied()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
        // Literal pins the documented 4-physical-pixel policy against constant drift.
        assert!((max - min - Vec2::splat(4.0)).abs().max_element() < 0.0001);
        let bytes = submission.bytes.len() as u64;
        assert!(
            Submission::new(
                &view,
                &camera(0.01),
                [size; 2],
                selection,
                true,
                FORMAT,
                bytes
            )
            .is_ok()
        );
        assert!(matches!(
            Submission::new(
                &view,
                &camera(0.01),
                [size; 2],
                selection,
                true,
                FORMAT,
                bytes - 1
            ),
            Err(RenderError::ResourceLimit)
        ));
    }
    target.path[0] = instance.shape();
    let wrong = Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        SelectionFeedback::Face {
            active: None,
            selected: Some(&target),
        },
        true,
        FORMAT,
        1024 * 1024,
    )
    .unwrap();
    assert!(wrong.centers.is_empty());
    assert!(
        wrong.bytes[..36 * VERTEX_BYTES]
            .as_chunks::<VERTEX_BYTES>()
            .0
            .iter()
            .all(|vertex| f32::from_ne_bytes(vertex[16..20].try_into().unwrap()) < 0.35)
    );
    assert_eq!(scene, before);
}

fn center_quad_scene(points: [[f32; 3]; 4]) -> NativeScene {
    let mut legacy = azteccraft::scene::Scene::default();
    legacy.create(Primitive::Plane, "center").unwrap();
    let mut value = serde_json::to_value(legacy).unwrap();
    value["nodes"][0]["mesh"]["vertices"] = serde_json::json!(points);
    value["nodes"][0]["mesh"]["faces"] = serde_json::json!([[0, 1, 2, 3]]);
    NativeScene::from_legacy(serde_json::from_value(value).unwrap()).unwrap()
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_face_centers_match_picker_for_planar_nonplanar_and_degenerate_faces() {
    let mut gpu = Offscreen::new();
    for (points, visible) in [
        (
            [
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            true,
        ),
        (
            [
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 0.0],
            ],
            false,
        ),
        ([[0.0; 3]; 4], true),
    ] {
        let native = center_quad_scene(points);
        let before = native.clone();
        let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
        let instance = &view.instances()[0];
        let path = &instance.path()[..instance.path().len() - 1];
        let camera = camera(0.01);
        assert_eq!(
            camera
                .pick_face_center(
                    &view,
                    Some((instance.transform(), path)),
                    Vec2::splat(64.0),
                    [SIZE; 2],
                    1.0
                )
                .unwrap()
                .is_some(),
            visible
        );
        for scale in [1.0, 2.0] {
            let frame = ui_frame(scale, |ui| {
                let rect =
                    Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(SIZE as f32 / scale));
                ui.painter().add(
                    renderer_handle()
                        .callback(
                            &view,
                            &camera,
                            rect,
                            [SIZE; 2],
                            SelectionFeedback::Face {
                                active: Some((instance.transform(), path)),
                                selected: None,
                            },
                            true,
                        )
                        .unwrap(),
                );
            });
            let image = gpu.render(frame);
            let orange = (60..68)
                .flat_map(|y| (60..68).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(&image, x, y) == [242, 163, 70, 255])
                .count();
            assert_eq!(
                orange,
                if visible { 16 } else { 0 },
                "scale={scale} points={points:?}"
            );
        }
        assert_eq!(native, before);
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_face_feedback_matches_exact_face_and_instance_and_hides_occluded_centers() {
    let mut native = cube();
    let first = native.nodes()[0].id();
    let second = native.create(Primitive::Cube, "other").unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let camera = camera(0.01);
    let mut gpu = Offscreen::new();
    for (owner, index, expected) in [
        (first, 1, [95, 110, 120, 255]),
        (first, 0, [83, 83, 83, 255]),
        (second, 1, [83, 83, 83, 255]),
    ] {
        let instance = view
            .instances()
            .iter()
            .find(|instance| instance.transform() == owner)
            .unwrap();
        let path = &instance.path()[..instance.path().len() - 1];
        let target = FaceTarget {
            transform: owner,
            shape: instance.shape(),
            path: instance.path().to_vec(),
            face_index: index,
        };
        native.resolve_face_target(&target).unwrap();
        let submission = Submission::new(
            &view,
            &camera,
            [SIZE; 2],
            SelectionFeedback::Face {
                active: Some((owner, path)),
                selected: Some(&target),
            },
            true,
            FORMAT,
            1024 * 1024,
        )
        .unwrap();
        assert_eq!(
            pixel(&gpu.render(TestFrame::Geometry(submission)), 64, 58),
            expected
        );
    }
    assert_eq!(native, before);
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [0.0, 0.0, 1.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = view
        .instances()
        .iter()
        .find(|instance| instance.transform() == first)
        .unwrap();
    let path = &instance.path()[..instance.path().len() - 1];
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((first, path)),
                Vec2::splat(64.0),
                [SIZE; 2],
                1.0
            )
            .unwrap()
            .is_none()
    );
    let submission = Submission::new(
        &view,
        &camera,
        [SIZE; 2],
        SelectionFeedback::Face {
            active: Some((first, path)),
            selected: None,
        },
        true,
        FORMAT,
        1024 * 1024,
    )
    .unwrap();
    let image = gpu.render(TestFrame::Geometry(submission));
    assert!(!image.as_chunks::<4>().0.contains(&[242, 163, 70, 255]));
    native
        .set_transform(
            first,
            NativeTransform {
                translation: [0.0, 0.0, 6.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = view
        .instances()
        .iter()
        .find(|instance| instance.transform() == first)
        .unwrap();
    let path = &instance.path()[..instance.path().len() - 1];
    let submission = Submission::new(
        &view,
        &camera,
        [SIZE; 2],
        SelectionFeedback::Face {
            active: Some((first, path)),
            selected: None,
        },
        true,
        FORMAT,
        1024 * 1024,
    )
    .unwrap();
    assert!(submission.centers.is_empty());
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((first, path)),
                Vec2::splat(64.0),
                [SIZE; 2],
                1.0
            )
            .unwrap()
            .is_none()
    );
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
    let instance = &view.instances()[0];
    let target = FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 1,
    };
    for selection in [
        SelectionFeedback::Object(None),
        SelectionFeedback::Object(Some(instance.path())),
        SelectionFeedback::Face {
            active: Some((
                instance.transform(),
                &instance.path()[..instance.path().len() - 1],
            )),
            selected: Some(&target),
        },
    ] {
        let visible = Submission::new(
            &view,
            &camera,
            [SIZE; 2],
            selection,
            true,
            FORMAT,
            1024 * 1024,
        )
        .unwrap();
        let hidden = Submission::new(
            &view,
            &camera,
            [SIZE; 2],
            selection,
            false,
            FORMAT,
            1024 * 1024,
        )
        .unwrap();
        let restored = Submission::new(
            &view,
            &camera,
            [SIZE; 2],
            selection,
            true,
            FORMAT,
            1024 * 1024,
        )
        .unwrap();
        assert_eq!(visible.bytes, restored.bytes);
        assert_eq!(visible.surfaces, hidden.surfaces);
        assert_eq!(visible.vertex_count() - hidden.vertex_count(), 84);
        assert_eq!(visible.lines.start, hidden.lines.start);
        assert_eq!(visible.lines.end - hidden.lines.end, 84);
        assert_eq!(visible.centers.start - hidden.centers.start, 84);
        assert_eq!(visible.centers.end - hidden.centers.end, 84);
        if matches!(selection, SelectionFeedback::Face { .. }) {
            assert!(hidden.centers.end > hidden.centers.start);
            assert_eq!(
                visible.bytes[visible.centers.start as usize * VERTEX_BYTES..],
                hidden.bytes[hidden.centers.start as usize * VERTEX_BYTES..]
            );
        }
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
                [SIZE; 2],
                selection,
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
                [SIZE; 2],
                selection,
                true,
                FORMAT,
                hidden.bytes.len() as u64
            ),
            Err(RenderError::ResourceLimit)
        ));
    }
    let empty = ViewScene::from_native(&NativeScene::default(), VIEW_BUDGET).unwrap();
    let hidden = Submission::new(
        &empty,
        &camera,
        [SIZE; 2],
        SelectionFeedback::Object(None),
        false,
        FORMAT,
        0,
    )
    .unwrap();
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
                        SelectionFeedback::Object(
                            scene.instances().first().map(|instance| instance.path()),
                        ),
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
        Submission::new(
            &view,
            &camera(0.01),
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true,
            FORMAT,
            1024 * 1024
        ),
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
        renderer.callback(
            &view,
            &camera(0.01),
            Rect::NOTHING,
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true
        ),
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
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true
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
            [SIZE; 2],
            SelectionFeedback::Object(selected),
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
    assert_eq!(pixel(&image, 79, 57), &[38, 44, 48, 255]);
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
                SelectionFeedback::Object(Some(&path[..path.len() - 1])),
                true,
            )
            .unwrap();
    });
    assert_eq!(pixel(&gpu.render(frame), 79, 57), &[38, 44, 48, 255]);
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
                        [96; 2],
                        SelectionFeedback::Object(Some(&path[..path.len() - 1])),
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
            [SIZE; 2],
            SelectionFeedback::Object(None),
            true
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
        assert_eq!(pixel(&image, 58, 58), &[78, 91, 99, 255]);
        assert_eq!(pixel(&image, 70, 58), &[68, 68, 68, 255]);
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
    // Back faces get only the ambient floor (no two-sided lighting), so the side shows in the tint.
    for (rotation, scale, expected) in [
        ([90.0, 0.0, 0.0], [1.0; 3], [95, 110, 120, 255]),
        ([90.0, 180.0, 0.0], [1.0; 3], [38, 44, 48, 255]),
        ([90.0, 0.0, 0.0], [-1.0, 1.0, 1.0], [95, 110, 120, 255]),
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
        assert_eq!(pixel(&image, 64, 58), &expected);
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
    assert_eq!(pixel(&image, 58, 58), &[78, 91, 99, 255]);
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
    assert_eq!(pixel(&image, 79, 57), &[38, 44, 48, 255]);
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

fn surface_colors(submission: &Submission) -> Vec<[f32; 4]> {
    submission
        .surfaces
        .clone()
        .map(|vertex| {
            let start = vertex as usize * VERTEX_BYTES + 4 * size_of::<f32>();
            std::array::from_fn(|channel| {
                let at = start + channel * size_of::<f32>();
                f32::from_ne_bytes(submission.bytes[at..at + 4].try_into().unwrap())
            })
        })
        .collect()
}

#[test]
fn base_fill_is_83_and_a_face_looking_at_the_eye_keeps_it() {
    assert_eq!(SURFACE_BASE_RGB, [83, 83, 83]);
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let submission = Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        SelectionFeedback::Object(None),
        true,
        FORMAT,
        u64::MAX,
    )
    .unwrap();
    let base = Color32::from_gray(83).to_normalized_gamma_f32();
    let floor = Color32::from_gray(33).to_normalized_gamma_f32();
    let colors = surface_colors(&submission);
    assert_eq!(colors.len(), 36);
    // The +Z face looks straight at the eye; each other face is lit less or at the floor.
    assert_eq!(colors.iter().filter(|color| **color == base).count(), 6);
    assert!(
        colors
            .iter()
            .all(|color| color[0] >= floor[0] && color[0] <= base[0])
    );
    assert!(colors.contains(&floor));
}

#[test]
fn selection_tint_is_lit_by_the_same_factor() {
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let path = view.instances()[0].path();
    let submission = Submission::new(
        &view,
        &camera(0.01),
        [SIZE; 2],
        SelectionFeedback::Object(Some(&path[..path.len() - 1])),
        true,
        FORMAT,
        u64::MAX,
    )
    .unwrap();
    let tint = Color32::from_rgb(95, 110, 120).to_normalized_gamma_f32();
    let colors = surface_colors(&submission);
    assert_eq!(colors.iter().filter(|color| **color == tint).count(), 6);
    assert!(
        colors
            .iter()
            .all(|color| color[1] > color[0] && color[2] > color[1])
    );
}

fn rotated_cube(degrees_y: f64) -> NativeScene {
    let mut scene = NativeScene::default();
    let node = scene.create(Primitive::Cube, "cube").unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, degrees_y, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    scene
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_a_face_is_lit_differently_from_the_same_face_rotated() {
    let mut gpu = Offscreen::new();
    let straight = gpu.image(
        &ViewScene::from_native(&rotated_cube(0.0), VIEW_BUDGET).unwrap(),
        &camera(0.01),
        None,
    );
    let turned = gpu.image(
        &ViewScene::from_native(&rotated_cube(30.0), VIEW_BUDGET).unwrap(),
        &camera(0.01),
        None,
    );
    // Flat per-face colours may differ by one level between Metal and the software adapter.
    let near = |actual: &[u8], expected: u8| actual[..3].iter().all(|v| v.abs_diff(expected) <= 2);
    assert!(near(pixel(&straight, 64, 50), 83));
    let (left, right) = (pixel(&turned, 52, 50), pixel(&turned, 76, 50));
    assert!(left[0].abs_diff(83) > 4 || right[0].abs_diff(83) > 4);
    assert!(left[0].abs_diff(right[0]) > 8, "{left:?} {right:?}");
    assert!(left[0] >= 33 && right[0] >= 33 && left[0] <= 83 && right[0] <= 83);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_grid_behind_a_face_is_hidden_today_issue_111_oracle() {
    let mut gpu = Offscreen::new();
    let view = ViewScene::from_native(&cube(), VIEW_BUDGET).unwrap();
    let camera = Camera::new(Vec3::ZERO, 0.4, 0.6, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
    let render = |gpu: &mut Offscreen, grid: bool| {
        let submission = Submission::new(
            &view,
            &camera,
            [SIZE; 2],
            SelectionFeedback::Object(None),
            grid,
            FORMAT,
            u64::MAX,
        )
        .unwrap();
        gpu.render(TestFrame::Geometry(submission))
    };
    let with_grid = render(&mut gpu, true);
    let without_grid = render(&mut gpu, false);
    let mut differing = 0;
    for y in 58..70 {
        for x in 58..70 {
            differing += usize::from(pixel(&with_grid, x, y) != pixel(&without_grid, x, y));
        }
    }
    // Records current behaviour for #111: the centre of the cube, where grid lines pass
    // through the body, shows no grid pixel today (12x12 window well inside the silhouette).
    assert_eq!(differing, 0);
}
