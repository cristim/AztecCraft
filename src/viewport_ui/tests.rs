use super::*;
use crate::viewport_renderer::tests::{RecordedSubmission, recorded_submission, renderer_handle};
use azteccraft::{
    native_scene::{NativeScene, NativeTransform},
    scene::Primitive,
};
use egui::{Event, Modifiers, MouseWheelUnit, Pos2, Shape};
use glam::Vec3;

mod maya_binary {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

fn camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap()
}

fn raw(time: f64, modifiers: Modifiers, mut events: Vec<Event>) -> egui::RawInput {
    events.insert(0, Event::ModifiersChanged(modifiers));
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 96.0))),
        time: Some(time),
        events,
        ..Default::default()
    }
}

fn capture_frame(
    context: &egui::Context,
    camera: &Camera,
    raw: egui::RawInput,
    clip: Rect,
) -> (ViewportInput, f32) {
    let mut captured = None;
    let mut scroll = 0.0;
    let mut output = context.run_ui(raw, |ui| {
        ui.set_clip_rect(clip);
        scroll = ui.input(|input| input.smooth_scroll_delta.y);
        captured = capture(ui, camera).unwrap();
    });
    output.textures_delta.clear();
    (captured.unwrap(), scroll)
}

#[test]
fn narrower_scissor_crops_without_reframing_click_or_mutating_camera() {
    let context = egui::Context::default();
    let camera = camera();
    let before = camera.clone();
    let clip = Rect::from_min_max(egui::pos2(16.0, 8.0), egui::pos2(112.0, 88.0));
    let point = egui::pos2(88.0, 28.0);
    let _ = capture_frame(
        &context,
        &camera,
        raw(0.0, Modifiers::NONE, vec![Event::PointerMoved(point)]),
        clip,
    );
    let press = Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    };
    let _ = capture_frame(
        &context,
        &camera,
        raw(
            1.0 / 60.0,
            Modifiers::NONE,
            vec![Event::PointerMoved(point), press],
        ),
        clip,
    );
    let release = Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    };
    let (mut input, _) = capture_frame(
        &context,
        &camera,
        raw(2.0 / 60.0, Modifiers::NONE, vec![release]),
        clip,
    );
    assert_eq!(
        input.rect,
        Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 96.0))
    );
    assert_eq!(input.aspect(), 4.0 / 3.0);
    assert!((input.click_ndc().unwrap() - Vec2::new(0.375, 5.0 / 12.0)).length() < 1e-6);
    assert!(input.take_navigation().is_empty());
    assert_eq!(camera, before);
}

#[test]
fn captured_typed_navigation_preserves_orbit_pan_and_both_dollies_and_drains_once() {
    let context = egui::Context::default();
    let mut camera = camera();
    camera
        .frame_points(&[Vec3::splat(-4.0), Vec3::splat(4.0)], 1.0)
        .unwrap();
    let before = camera.clone();
    let modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 96.0));
    let point = egui::pos2(40.0, 30.0);
    let _ = capture_frame(
        &context,
        &camera,
        raw(0.0, modifiers, vec![Event::PointerMoved(point)]),
        clip,
    );
    let mut events = vec![Event::PointerMoved(point)];
    for button in [
        PointerButton::Primary,
        PointerButton::Middle,
        PointerButton::Secondary,
    ] {
        events.push(Event::PointerButton {
            pos: point,
            button,
            pressed: true,
            modifiers,
        });
    }
    let _ = capture_frame(&context, &camera, raw(1.0 / 60.0, modifiers, events), clip);
    let (mut input, scroll) = capture_frame(
        &context,
        &camera,
        raw(
            2.0 / 60.0,
            modifiers,
            vec![
                Event::PointerMoved(egui::pos2(50.0, 50.0)),
                Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 20.0),
                    phase: egui::TouchPhase::Move,
                    modifiers,
                },
            ],
        ),
        clip,
    );
    assert_ne!(scroll, 0.0);
    let operations = input.take_navigation();
    let [
        Operation::Orbit { radians },
        Operation::Pan { world_units },
        Operation::Dolly { factor: secondary },
        Operation::Dolly { factor: wheel },
    ] = operations.as_slice()
    else {
        panic!("captured operations must keep the existing order")
    };
    assert_eq!(*radians, (Vec2::new(-10.0, 20.0) * 0.007).to_array());
    assert_eq!(
        *world_units,
        (Vec2::new(-10.0, 20.0) * camera.distance() * 0.002).to_array()
    );
    assert_eq!(*secondary, 0.2_f32.exp());
    assert_eq!(*wheel, (-scroll * 0.002).exp());
    assert!(input.take_navigation().is_empty());
    assert_eq!(camera, before);
}

fn painted_submission(
    view: &ViewScene,
    camera: &Camera,
    prefix: Option<&[NativeId]>,
) -> RecordedSubmission {
    let context = egui::Context::default();
    let producer = renderer_handle();
    let mut output = context.run_ui(raw(0.0, Modifiers::NONE, Vec::new()), |ui| {
        capture(ui, camera)
            .unwrap()
            .unwrap()
            .paint(Some(&producer), camera, view, prefix, true)
            .unwrap();
    });
    assert_eq!(
        output
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, Shape::Callback(_)))
            .count(),
        1
    );
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, Shape::Mesh(_)))
    );
    if view.instances().iter().any(|i| i.appearance_incomplete()) {
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, Shape::Text(t)
            if t.galley.job.text.contains("Appearance incomplete: geometry preview")))
        );
    }
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, Shape::Text(t)
        if t.galley.job.text.contains("Opaque geometry preview")))
    );
    output.textures_delta.clear();
    recorded_submission(&producer)
}

fn surface_color_count(submission: &RecordedSubmission, rgb: [u8; 3]) -> usize {
    let rgba = [
        f32::from(rgb[0]) / 255.0,
        f32::from(rgb[1]) / 255.0,
        f32::from(rgb[2]) / 255.0,
        1.0,
    ];
    submission.vertices[submission.surfaces.start as usize..submission.surfaces.end as usize]
        .iter()
        .filter(|vertex| vertex[4..] == rgba)
        .count()
}

#[test]
fn canonical_quad_instances_paint_only_selected_prefix_and_persistent_limitations() {
    let mut scene = NativeScene::default();
    for (name, x) in [("first", -1.0), ("second", 1.0)] {
        let id = scene.create(Primitive::Cube, name).unwrap();
        scene
            .set_transform(
                id,
                NativeTransform {
                    translation: [x, 0.0, 0.0],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let before = scene.clone();
    let view = ViewScene::from_native(&scene, 256 * 1024 * 1024).unwrap();
    let path = view.instances()[0].path();
    let submission = painted_submission(&view, &camera(), Some(&path[..path.len() - 1]));
    assert_eq!(submission.surfaces, 0..72);
    assert_eq!(submission.lines, 72..252);
    assert_eq!(surface_color_count(&submission, [95, 110, 120]), 36);
    assert_eq!(surface_color_count(&submission, [83, 83, 83]), 36);
    assert_eq!(scene, before);
}

#[test]
fn actual_binary_triangle_paints_source_corners_and_persistent_appearance_limit() {
    let mut scene =
        azteccraft::native_scene::binary::decode(maya_binary::specimen().into()).unwrap();
    let node = scene.nodes()[0].id();
    scene
        .set_channel(node, azteccraft::native_scene::Channel::TranslateX, 0.0)
        .unwrap();
    let before = scene.clone();
    let view = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    let instance = &view.instances()[0];
    assert_eq!(instance.faces()[0].corners().len(), 3);
    let path = instance.path();
    let mut camera = camera();
    camera.frame_view(&view, None, 4.0 / 3.0).unwrap();
    let submission = painted_submission(&view, &camera, Some(&path[..path.len() - 1]));
    assert_eq!(submission.surfaces, 0..3);
    assert_eq!(submission.lines, 3..93);
    assert_eq!(surface_color_count(&submission, [95, 110, 120]), 3);
    let matrix = camera.projection_matrix(4.0 / 3.0).unwrap() * camera.view_matrix();
    for (vertex, index) in submission.vertices[..3]
        .iter()
        .zip(instance.faces()[0].triangles().flatten())
    {
        assert_eq!(
            vertex[..4],
            (matrix * instance.world_vertices()[index].extend(1.0)).to_array()
        );
    }
    assert_eq!(scene, before);
}

#[test]
fn actual_binary_group_prefix_highlights_child_geometry_and_excludes_outside_geometry() {
    use maya_binary::{chunk, creation, group, mesh_payload, scene, triple};
    let children = [
        group(b"XFRM", &creation(b"assembly", None), 4),
        group(
            b"XFRM",
            &[
                creation(b"part", Some(b"assembly")),
                triple(b"t", [-1.0, 0.0, 0.0]),
            ]
            .concat(),
            4,
        ),
        group(
            b"DMSH",
            &[
                creation(b"partShape", Some(b"part")),
                chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
            ]
            .concat(),
            4,
        ),
        group(
            b"XFRM",
            &[creation(b"outside", None), triple(b"t", [1.0, 0.0, 0.0])].concat(),
            4,
        ),
        group(
            b"DMSH",
            &[
                creation(b"outsideShape", Some(b"outside")),
                chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
            ]
            .concat(),
            4,
        ),
    ]
    .concat();
    let scene = azteccraft::native_scene::binary::decode(scene(&children)).unwrap();
    let before = scene.clone();
    let prefix = [scene
        .nodes()
        .iter()
        .find(|n| n.name() == "assembly")
        .unwrap()
        .transform_id()];
    let view = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    assert_eq!(view.instances().len(), 2);
    let mut camera = camera();
    camera.frame_view(&view, None, 4.0 / 3.0).unwrap();
    let submission = painted_submission(&view, &camera, Some(&prefix));
    assert_eq!(submission.surfaces, 0..6);
    assert_eq!(submission.lines, 6..102);
    assert_eq!(surface_color_count(&submission, [95, 110, 120]), 3);
    assert_eq!(surface_color_count(&submission, [83, 83, 83]), 3);
    assert_eq!(scene, before);
}

#[test]
fn acute_native_face_tessellation_stays_near_its_bounded_projected_geometry() {
    let mut legacy = azteccraft::scene::Scene::default();
    legacy.create(Primitive::Plane, "acute").unwrap();
    let mut json = serde_json::to_value(legacy).unwrap();
    json["nodes"][0]["mesh"]["vertices"] = serde_json::json!([
        [-0.5, 0.0, 0.0],
        [0.5, 0.0, 0.0],
        [-0.4999, 0.0001, 0.0],
        [-0.5, 0.0001, 0.0]
    ]);
    let native = NativeScene::from_legacy(serde_json::from_value(json).unwrap()).unwrap();
    let view = ViewScene::from_native(&native, 1024 * 1024).unwrap();
    let path = view.instances()[0].path();
    let camera = camera();
    let submission = painted_submission(&view, &camera, Some(&path[..path.len() - 1]));
    assert_eq!(submission.surfaces, 0..6);
    assert_eq!(submission.lines, 6..98);
    for vertex in submission.vertices[..6]
        .iter()
        .chain(submission.vertices[90..].iter())
    {
        assert!(vertex.iter().all(|v| v.is_finite()));
        let x = vertex[0] / vertex[3];
        let y = vertex[1] / vertex[3];
        assert!(
            x.abs() < 0.3 && y.abs() < 0.001,
            "acute source generated distant geometry"
        );
    }
    assert_eq!(surface_color_count(&submission, [95, 110, 120]), 6);
}

#[test]
#[ignore = "requires an explicitly supplied private Maya binary fixture"]
fn private_native_open_face_tessellation_preserves_prepared_frustum_bounds() {
    let path = std::env::var_os("AZTECCRAFT_PRIVATE_MB").expect("private fixture required");
    let document = azteccraft::document::load_native(std::path::Path::new(&path)).unwrap();
    let view = ViewScene::from_native(&document.scene, 256 * 1024 * 1024).unwrap();
    let mut camera = camera();
    camera.frame_view(&view, None, 4.0 / 3.0).unwrap();
    let submission = painted_submission(&view, &camera, None);
    let surfaces = view
        .instances()
        .iter()
        .flat_map(|i| i.faces())
        .map(|f| f.triangles().count() * 3)
        .sum::<usize>();
    let boundaries = view
        .instances()
        .iter()
        .flat_map(|i| i.faces())
        .map(|f| f.corners().len() * 2)
        .sum::<usize>();
    assert_eq!(submission.surfaces, 0..surfaces as u32);
    assert_eq!(
        submission.lines,
        surfaces as u32..(surfaces + boundaries + 84) as u32
    );
    for vertex in submission.vertices[..surfaces]
        .iter()
        .chain(submission.vertices[surfaces + 84..].iter())
    {
        assert!(vertex.iter().all(|v| v.is_finite()));
        assert!((vertex[0] / vertex[3]).abs() <= 1.001);
        assert!((vertex[1] / vertex[3]).abs() <= 1.001);
        assert!((0.0..=1.0).contains(&(vertex[2] / vertex[3])));
    }
}

#[test]
fn missing_startup_state_fails_and_uninitialized_ui_never_paints_fake_geometry() {
    let context = egui::Context::default();
    let creation = eframe::CreationContext::_new_kittest(context.clone());
    assert!(matches!(
        crate::app::Editor::new(&creation),
        Err(RenderError::Backend(_))
    ));
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let view = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    let mut output = context.run_ui(raw(0.0, Modifiers::NONE, Vec::new()), |ui| {
        assert!(matches!(
            capture(ui, &camera())
                .unwrap()
                .unwrap()
                .paint(None, &camera(), &view, None, true),
            Err(RenderError::Uninitialized)
        ));
    });
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, Shape::Callback(_) | Shape::Mesh(_)))
    );
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape, Shape::Text(t) if t.galley.job.text.contains("uninitialized"))
    ));
    output.textures_delta.clear();
}

#[test]
fn capture_uses_rounded_screen_clamped_physical_rect_at_one_and_two_x() {
    for scale in [1.0, 2.0] {
        let context = egui::Context::default();
        let mut input = raw(0.0, Modifiers::NONE, Vec::new());
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(scale);
        let mut output = context.run_ui(input, |ui| {
            let rect = Rect::from_min_max(egui::pos2(16.3, 8.3), egui::pos2(112.3, 104.3));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            child.set_clip_rect(Rect::from_min_max(
                egui::pos2(24.0, 16.0),
                egui::pos2(100.0, 80.0),
            ));
            let captured = capture(&mut child, &camera()).unwrap().unwrap();
            let min = if scale == 1.0 {
                egui::pos2(16.0, 8.0)
            } else {
                egui::pos2(16.5, 8.5)
            };
            let max = if scale == 1.0 {
                egui::pos2(112.0, 96.0)
            } else {
                egui::pos2(112.5, 96.0)
            };
            assert_eq!(captured.rect, Rect::from_min_max(min, max));
        });
        output.textures_delta.clear();
    }
}
