use super::*;
use crate::viewport_renderer::tests::{Offscreen, creation_context, recorded_submission};
use eframe::wgpu;
use std::time::Duration;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_ascii_editor_open_select_frame_reaches_actual_curve_callback() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("curve.ma");
    let source = ascii_fixture::specimen();
    std::fs::write(&path, &source).unwrap();
    let context = egui::Context::default();
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor
        .apply_operation(Operation::OpenScene { path: path.clone() })
        .unwrap();
    let owner = editor.history.scene().nodes()[0].id();
    editor
        .apply_operation(Operation::Select {
            node: Some(owner),
            instance_path: None,
        })
        .unwrap();
    editor.apply_operation(Operation::FrameSelected).unwrap();
    let baseline = editor.history.scene().clone();
    let token = editor.scene_token;
    let output = context.run_ui(input(Vec::new(), egui::Vec2::splat(128.0)), |ui| {
        editor.viewport(ui, token, None, None, false, true)
    });
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| matches!(shape.shape, egui::Shape::Callback(_)))
    );
    let image = Offscreen::from_context(&creation).render_egui(context.clone(), output);
    assert!(image.as_chunks::<4>().0.contains(&[242, 163, 70, 255]));
    for size in [egui::vec2(640.0, 480.0), egui::vec2(1920.0, 1080.0)] {
        let mut output = context.run_ui(input(Vec::new(), size), |ui| editor.render(ui));
        output.textures_delta.clear();
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Callback(_)))
        );
        let submission = recorded_submission(editor.renderer.as_ref().unwrap());
        assert!(
            submission
                .vertices
                .iter()
                .any(|vertex| vertex[4..] == [242.0 / 255.0, 163.0 / 255.0, 70.0 / 255.0, 1.0])
        );
        assert!(editor.renderer.as_ref().unwrap().terminal_error().is_none());
        let inspection = editor.inspect(false).unwrap();
        assert!(
            inspection.curves[0].draw_available
                && inspection.depth_complete
                && inspection.clipping_complete
        );
    }
    assert_eq!(editor.history.scene(), &baseline);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.selected(), Some(owner));
    assert!(!editor.is_dirty() && !editor.history.can_undo());
    assert_eq!(std::fs::read(path).unwrap(), source);
}

fn input(events: Vec<egui::Event>, size: egui::Vec2) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
        events,
        ..Default::default()
    }
}

fn render(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    context.run_ui(input(events, egui::vec2(1440.0, 900.0)), |ui| {
        editor.render(ui)
    })
}

#[test]
fn default_editor_reports_uninitialized_without_claiming_depth_or_clipping() {
    let editor = Editor::default();
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(inspection.viewport_backend, "uninitialized");
    assert_eq!(
        inspection.viewport_error,
        Some(crate::viewport_renderer::ViewportError::Uninitialized)
    );
    assert!(!inspection.depth_complete && !inspection.clipping_complete);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_face_editor_typed_selection_reaches_actual_marker_callback() {
    let context = egui::Context::default();
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.create(Primitive::Cube);
    editor.camera = Camera::new(
        glam::Vec3::ZERO,
        0.0,
        0.0,
        5.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let target = editor
        .camera
        .pick_view(&view, glam::Vec2::ZERO, 1.0)
        .unwrap()
        .unwrap();
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    editor
        .apply_operation(Operation::SelectFace {
            target: Some(target),
        })
        .unwrap();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let output = context.run_ui(input(Vec::new(), egui::Vec2::splat(128.0)), |ui| {
        editor.viewport(ui, token, None, None, false, true)
    });
    let recorded = recorded_submission(editor.renderer.as_ref().unwrap());
    assert_eq!(recorded.markers.end - recorded.markers.start, 36);
    assert_eq!(
        recorded.vertices[..36]
            .iter()
            .filter(|vertex| vertex[4] > 0.35)
            .count(),
        6
    );
    let rect = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Callback(callback) => Some(callback.rect),
            _ => None,
        })
        .unwrap();
    let center = rect.center() * output.pixels_per_point;
    let offset = (center.y.floor() as usize * 128 + center.x.floor() as usize) * 4;
    let image = Offscreen::from_context(&creation).render_egui(context, output);
    assert_eq!(&image[offset..offset + 4], &[242, 163, 70, 255]);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_actual_editor_startup_installs_resources_used_by_its_own_callback() {
    let context = egui::Context::default();
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.create(Primitive::Cube);
    editor.camera = Camera::new(
        glam::Vec3::ZERO,
        0.0,
        0.0,
        5.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap();
    let token = editor.scene_token;
    let output = context.run_ui(input(Vec::new(), egui::Vec2::splat(128.0)), |ui| {
        editor.viewport(ui, token, None, None, false, true)
    });
    assert_eq!(
        output
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Callback(_)))
            .count(),
        1
    );
    let image = Offscreen::from_context(&creation).render_egui(context, output);
    assert!(image.as_chunks::<4>().0.contains(&[95, 110, 120, 255]));
    assert!(editor.renderer.as_ref().unwrap().terminal_error().is_none());
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(inspection.viewport_backend, "wgpu_geometry_preview");
    assert!(inspection.depth_complete && inspection.clipping_complete);
    assert_eq!(inspection.viewport_error, None);
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_real_device_failure_stays_visible_without_changing_authoring_state() {
    let context = egui::Context::default();
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    let state = creation.wgpu_render_state.as_ref().unwrap();
    let _invalid = state
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("private-driver-diagnostic-marker"),
            source: wgpu::ShaderSource::Wgsl("invalid shader syntax".into()),
        });
    state
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(
        inspection.viewport_error,
        Some(crate::viewport_renderer::ViewportError::BackendFailed)
    );
    assert!(!inspection.depth_complete && !inspection.clipping_complete);
    assert!(
        !serde_json::to_string(&inspection)
            .unwrap()
            .contains("private-driver-diagnostic-marker")
    );
    editor.status = "An unrelated operation completed".into();
    let mut output = render(&mut editor, &context, Vec::new());
    assert!(
        !output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Callback(_)))
    );
    assert!(output.shapes.iter().any(
        |s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.contains("backend_failed"))
    ));
    assert!(!output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.job.text.contains("private-driver-diagnostic-marker"))));
    output.textures_delta.clear();
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undo);
}

#[test]
fn replacement_frame_token_discards_navigation_and_click_for_old_document() {
    let context = egui::Context::default();
    let mut editor = Editor::default();
    let old = editor.scene_token;
    let mut warmup = context.run_ui(
        input(
            vec![egui::Event::PointerMoved(egui::pos2(64.0, 64.0))],
            egui::Vec2::splat(128.0),
        ),
        |ui| editor.viewport(ui, old, None, None, false, true),
    );
    warmup.textures_delta.clear();
    editor.new_document().unwrap();
    assert_ne!(editor.scene_token, old);
    let camera = editor.camera.clone();
    let mut output = context.run_ui(
        input(
            vec![
                egui::Event::PointerMoved(egui::pos2(64.0, 64.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Vec2::splat(128.0),
        ),
        |ui| editor.viewport(ui, old, None, None, false, true),
    );
    output.textures_delta.clear();
    assert_eq!(editor.camera, camera);
    assert!(editor.selected().is_none());
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_native_discard_resolves_before_fresh_callback_for_new_and_open() {
    for open in [false, true] {
        let context = egui::Context::default();
        let creation = creation_context(context.clone());
        let mut editor = Editor::new(&creation).unwrap();
        editor.create(Primitive::Cube);
        let old = editor.scene_token;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("replacement.aztec");
        let mut candidate = azteccraft::scene::Scene::default();
        candidate.create(Primitive::Plane, "replacement").unwrap();
        azteccraft::document::save_new(&path, &candidate).unwrap();
        if open {
            editor
                .apply_operation(Operation::OpenScene { path: path.clone() })
                .unwrap();
        } else {
            editor.new_document().unwrap();
        }
        assert!(editor.files.is_modal());
        let mut output = render(&mut editor, &context, Vec::new());
        let position = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.job.text == "Don't Save" => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        output.textures_delta.clear();
        for pressed in [true, false] {
            let mut output = render(
                &mut editor,
                &context,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            output.textures_delta.clear();
        }
        assert_ne!(editor.scene_token, old);
        assert!(!editor.files.is_modal());
        let submission = recorded_submission(editor.renderer.as_ref().unwrap());
        assert_eq!(submission.surfaces, if open { 0..6 } else { 0..0 });
        assert_eq!(editor.history.scene().nodes().len(), usize::from(open));
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_resize_and_keyboard_discard_open_fit_the_current_viewport() {
    for width in [1440.0, 960.0] {
        let context = egui::Context::default();
        let creation = creation_context(context.clone());
        let mut editor = Editor::new(&creation).unwrap();
        editor.create(Primitive::Cube);
        let mut first = render(&mut editor, &context, Vec::new());
        first.textures_delta.clear();
        let old = editor.scene_token;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wide-candidate.aztec");
        let mut candidate = azteccraft::scene::Scene::default();
        let id = candidate.create(Primitive::Cube, "wide").unwrap();
        candidate
            .set_transform(
                id,
                azteccraft::scene::Transform {
                    scale: glam::Vec3::new(20.0, 1.0, 1.0),
                    ..Default::default()
                },
            )
            .unwrap();
        azteccraft::document::save_new(&path, &candidate).unwrap();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let mut modal = render(&mut editor, &context, Vec::new());
        modal.textures_delta.clear();
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut focus = render(&mut editor, &context, vec![key(egui::Key::Tab)]);
        focus.textures_delta.clear();
        assert!(context.memory(|memory| memory.focused().is_some()));
        let mut changed = context.run_ui(
            input(
                vec![
                    key(egui::Key::Enter),
                    egui::Event::PointerMoved(egui::pos2(600.0, 350.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, 120.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::vec2(width, 900.0),
            ),
            |ui| editor.render(ui),
        );
        changed.textures_delta.clear();
        assert_ne!(
            editor.scene_token, old,
            "actual modal Don't Save must apply the staged candidate"
        );
        assert!(!editor.files.is_modal());
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        let mut expected = Camera::default();
        expected.frame_view(&view, None, editor.aspect).unwrap();
        assert_eq!(
            editor.camera, expected,
            "Don't Save must frame current aspect and discard old navigation"
        );
        let submission = recorded_submission(editor.renderer.as_ref().unwrap());
        for vertex in &submission.vertices[..submission.surfaces.end as usize] {
            assert!(
                (vertex[0] / vertex[3]).abs() <= 1.001 && (vertex[1] / vertex[3]).abs() <= 1.001
            );
        }
        assert_eq!(submission.surfaces, 0..36);
        assert!(editor.selected().is_none());
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_recoverable_clip_error_clears_after_typed_transform_repair() {
    let context = egui::Context::default();
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    editor.camera = Camera::new(
        glam::Vec3::ZERO,
        0.0,
        0.0,
        5.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap();
    let identity = TransformInput {
        translation: [0.0; 3],
        rotation_degrees: [0.0; 3],
        scale: [1.0; 3],
    };
    editor
        .apply_operation(Operation::SetTransform {
            id,
            transform: TransformInput {
                translation: [f64::from(f32::MAX), 0.0, 0.0],
                ..identity
            },
        })
        .unwrap();
    let large = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    let camera = editor.camera.clone();
    let mut failed = render(&mut editor, &context, Vec::new());
    assert!(
        !failed
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Callback(_)))
    );
    failed.textures_delta.clear();
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(
        inspection.viewport_error,
        Some(crate::viewport_renderer::ViewportError::NonFiniteClip)
    );
    assert!(inspection.depth_complete && inspection.clipping_complete);
    assert_eq!(editor.history.scene(), &large);
    assert_eq!(editor.history.can_undo(), undo);
    assert_eq!(editor.camera, camera);
    editor
        .apply_operation(Operation::SetTransform {
            id,
            transform: identity,
        })
        .unwrap();
    let repaired = editor.history.scene().clone();
    let mut success = render(&mut editor, &context, Vec::new());
    assert_eq!(
        success
            .shapes
            .iter()
            .filter(|s| matches!(s.shape, egui::Shape::Callback(_)))
            .count(),
        1
    );
    success.textures_delta.clear();
    let inspection = editor.inspect(false).unwrap();
    assert_eq!(inspection.viewport_backend, "wgpu_geometry_preview");
    assert_eq!(inspection.viewport_error, None);
    assert!(inspection.depth_complete && inspection.clipping_complete);
    assert_eq!(editor.history.scene(), &repaired);
    assert!(editor.history.can_undo());
    assert!(editor.renderer.as_ref().unwrap().terminal_error().is_none());
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_inspection_health_is_consistent_across_async_device_failure() {
    use crate::viewport_renderer::ViewportError;
    let context = egui::Context::default();
    let creation = creation_context(context);
    let mut editor = Editor::new(&creation).unwrap();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    let redo = editor.history.can_redo();
    let camera = editor.camera.clone();
    let initial = editor.inspect(false).unwrap();
    assert_eq!(
        (
            initial.viewport_error,
            initial.depth_complete,
            initial.clipping_complete
        ),
        (None, true, true)
    );
    let device = creation.wgpu_render_state.as_ref().unwrap().device.clone();
    let worker = std::thread::spawn(move || {
        let _invalid = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("concurrent viewport validation regression"),
            source: wgpu::ShaderSource::Wgsl("invalid shader syntax".into()),
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_secs(10)),
            })
            .unwrap();
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let inspection = editor.inspect(false).unwrap();
        let tuple = (
            inspection.viewport_error,
            inspection.depth_complete,
            inspection.clipping_complete,
        );
        assert!(
            tuple == (None, true, true)
                || tuple == (Some(ViewportError::BackendFailed), false, false)
        );
        if inspection.viewport_error == Some(ViewportError::BackendFailed) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "actual device failure was not observed"
        );
        std::thread::yield_now();
    }
    worker.join().unwrap();
    for _ in 0..10 {
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(
            (
                inspection.viewport_error,
                inspection.depth_complete,
                inspection.clipping_complete
            ),
            (Some(ViewportError::BackendFailed), false, false)
        );
    }
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undo);
    assert_eq!(editor.history.can_redo(), redo);
    assert_eq!(editor.camera, camera);
}
