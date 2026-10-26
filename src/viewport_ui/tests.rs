use super::*;
use crate::viewport_renderer::GRID_VERTICES;
use crate::viewport_renderer::tests::{RecordedSubmission, recorded_submission, renderer_handle};
use azteccraft::{
    native_scene::{NativeId, NativeScene, NativeTransform},
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
fn face_click_uses_release_event_modifiers_even_when_they_change_later() {
    for (released, later, click, unsupported) in [
        (Modifiers::SHIFT, Modifiers::NONE, true, true),
        (Modifiers::NONE, Modifiers::SHIFT, true, false),
        (Modifiers::ALT, Modifiers::NONE, false, false),
        (Modifiers::NONE, Modifiers::ALT, true, false),
    ] {
        let context = egui::Context::default();
        let camera = camera();
        let point = egui::pos2(64.0, 48.0);
        let clip = Rect::EVERYTHING;
        capture_frame(
            &context,
            &camera,
            raw(0.0, Modifiers::NONE, vec![Event::PointerMoved(point)]),
            clip,
        );
        capture_frame(
            &context,
            &camera,
            raw(
                0.1,
                released,
                vec![Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: released,
                }],
            ),
            clip,
        );
        let (input, _) = capture_frame(
            &context,
            &camera,
            raw(
                0.2,
                released,
                vec![
                    Event::PointerButton {
                        pos: point,
                        button: PointerButton::Primary,
                        pressed: false,
                        modifiers: released,
                    },
                    Event::ModifiersChanged(later),
                ],
            ),
            clip,
        );
        assert_eq!(input.click_ndc().is_some(), click);
        if click {
            assert_eq!(input.unsupported_selection().is_some(), unsupported);
        }
    }
}

#[test]
fn face_click_does_not_borrow_a_later_drag_release_modifiers() {
    for (later_drag, only_alt) in [(false, false), (true, false), (false, true), (true, true)] {
        let context = egui::Context::default();
        let camera = camera();
        let point = egui::pos2(64.0, 48.0);
        capture_frame(
            &context,
            &camera,
            raw(0.0, Modifiers::NONE, vec![Event::PointerMoved(point)]),
            Rect::EVERYTHING,
        );
        let first = if only_alt {
            Modifiers::ALT
        } else {
            Modifiers::SHIFT
        };
        let last = if only_alt {
            Modifiers::ALT
        } else {
            Modifiers::NONE
        };
        let mut events = vec![
            Event::ModifiersChanged(first),
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: first,
            },
            Event::PointerButton {
                pos: point,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: first,
            },
            Event::ModifiersChanged(last),
        ];
        if later_drag {
            let end = point + egui::vec2(36.0, 0.0);
            events.extend([
                Event::PointerButton {
                    pos: point,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: last,
                },
                Event::PointerMoved(end),
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: last,
                },
            ]);
        }
        let (input, _) = capture_frame(
            &context,
            &camera,
            raw(0.1, Modifiers::NONE, events),
            Rect::EVERYTHING,
        );
        let expected = if only_alt {
            None
        } else if later_drag {
            Some(SelectionRefusal::AmbiguousPrimaryInput)
        } else {
            Some(SelectionRefusal::ModifiedClick)
        };
        assert_eq!(input.unsupported_selection(), expected);
        assert_eq!(input.click_ndc().is_some(), !only_alt && !later_drag);
        assert_eq!(input.click_points.is_some(), !only_alt && !later_drag);
    }
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
    assert_eq!(*secondary, (-0.2_f32).exp());
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
            .paint(
                Some(&producer),
                camera,
                view,
                SelectionFeedback::Object(prefix),
                true,
                crate::editor_operations::ShadingMode::SmoothShadeAll,
            )
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

/// Counts surface vertices of the selection tint (blue-leaning) or of the grey fill; shading
/// scales both by a per-face factor, so exact colours depend on the view.
fn surface_color_count(submission: &RecordedSubmission, tinted: bool) -> usize {
    submission.vertices[submission.surfaces.start as usize..submission.surfaces.end as usize]
        .iter()
        .filter(|vertex| {
            let [red, green, blue] = [vertex[4], vertex[5], vertex[6]];
            if tinted {
                blue > red && green > red
            } else {
                red == green && green == blue
            }
        })
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
    assert_eq!(submission.lines, 72..72 + GRID_VERTICES as u32 + 96);
    assert_eq!(surface_color_count(&submission, true), 36);
    assert_eq!(surface_color_count(&submission, false), 36);
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
    assert_eq!(submission.lines, 3..3 + GRID_VERTICES as u32 + 6);
    assert_eq!(surface_color_count(&submission, true), 3);
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
    assert_eq!(submission.lines, 6..6 + GRID_VERTICES as u32 + 12);
    assert_eq!(surface_color_count(&submission, true), 3);
    assert_eq!(surface_color_count(&submission, false), 3);
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
    assert_eq!(submission.lines, 6..6 + GRID_VERTICES as u32 + 8);
    for vertex in submission.vertices[..6]
        .iter()
        .chain(submission.vertices[6 + GRID_VERTICES..].iter())
    {
        assert!(vertex.iter().all(|v| v.is_finite()));
        let x = vertex[0] / vertex[3];
        let y = vertex[1] / vertex[3];
        assert!(
            x.abs() < 0.3 && y.abs() < 0.001,
            "acute source generated distant geometry"
        );
    }
    assert_eq!(surface_color_count(&submission, true), 6);
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
        surfaces as u32..(surfaces + boundaries + GRID_VERTICES) as u32
    );
    for vertex in submission.vertices[..surfaces]
        .iter()
        .chain(submission.vertices[surfaces + GRID_VERTICES..].iter())
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
            capture(ui, &camera()).unwrap().unwrap().paint(
                None,
                &camera(),
                &view,
                SelectionFeedback::Object(None),
                true,
                crate::editor_operations::ShadingMode::SmoothShadeAll
            ),
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

#[test]
fn fractional_offset_viewport_uses_one_physical_projection_for_clicks_and_paint() {
    let mut native = NativeScene::default();
    let owner = native.create(Primitive::Cube, "offset").unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, 1024 * 1024).unwrap();
    let camera = camera();
    for scale in [1.3, 1.5, 2.25] {
        let world = if scale == 1.3 {
            Vec3::new(0.3, 0.2, 0.5)
        } else {
            Vec3::new(0.0, 0.0, 0.5)
        };
        let context = egui::Context::default();
        let producer = renderer_handle();
        let mut point = egui::pos2(64.0, 46.0);
        for phase in 0..3 {
            let events = if phase == 0 {
                vec![Event::PointerMoved(point)]
            } else {
                vec![
                    Event::PointerMoved(point),
                    Event::PointerButton {
                        pos: point,
                        button: PointerButton::Primary,
                        pressed: phase == 1,
                        modifiers: Modifiers::NONE,
                    },
                ]
            };
            let mut input = raw(phase as f64 * 0.1, Modifiers::NONE, events);
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            let mut output = context.run_ui(input, |ui| {
                let rect = Rect::from_min_max(egui::pos2(16.3, 8.3), egui::pos2(112.3, 84.3));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let captured = capture(&mut child, &camera).unwrap().unwrap();
                assert!(captured.rect.min.x > 0.0 && captured.rect.min.y > 0.0);
                assert_ne!(captured.physical_size[0], captured.physical_size[1]);
                let aspect = captured.physical_size[0] as f32 / captured.physical_size[1] as f32;
                assert_eq!(captured.aspect(), aspect);
                let ndc = camera.project(world, aspect).unwrap().unwrap();
                point = captured.rect.min
                    + egui::vec2((ndc.x + 1.0) * 0.5, (1.0 - ndc.y) * 0.5) * captured.rect.size();
                if phase == 2 {
                    let hit = camera
                        .pick_view(&view, captured.click_ndc().unwrap(), captured.aspect())
                        .unwrap()
                        .unwrap();
                    assert_eq!(hit.transform, owner);
                    assert_eq!(hit.face_index, 1);
                    native.resolve_face_target(&hit).unwrap();
                    let face = captured
                        .pick_face_center(
                            &camera,
                            &view,
                            Some((owner, &hit.path[..hit.path.len() - 1])),
                        )
                        .unwrap();
                    assert_eq!(face.is_some(), world.x == 0.0);
                    if let Some(face) = face {
                        assert_eq!(face, hit);
                        native.resolve_face_target(&face).unwrap();
                    }
                }
                captured
                    .paint(
                        Some(&producer),
                        &camera,
                        &view,
                        SelectionFeedback::Object(None),
                        true,
                        crate::editor_operations::ShadingMode::SmoothShadeAll,
                    )
                    .unwrap();
                let recorded = recorded_submission(&producer);
                let matrix = camera.projection_matrix(aspect).unwrap() * camera.view_matrix();
                let instance = &view.instances()[0];
                let corner = instance.faces().last().unwrap().triangles().next().unwrap()[0];
                assert_eq!(
                    recorded.vertices[0][..4],
                    (matrix * instance.world_vertices()[corner].extend(1.0)).to_array()
                );
            });
            output.textures_delta.clear();
        }
    }
    assert_eq!(native, before);
}

#[test]
fn fractional_capture_click_handle_and_submission_share_the_same_physical_aspect() {
    use azteccraft::{move_handles::WorldHandles, native_scene::WorldAxis};
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube").unwrap();
    let view = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    let camera = camera();
    for (scale, origin, size) in [(1.25, [20, 10], [119, 103]), (2.5, [41, 21], [237, 206])] {
        let context = egui::Context::default();
        let renderer = renderer_handle();
        let mut last = None;
        let mut pointer = Pos2::ZERO;
        for phase in 0..3 {
            let events = match phase {
                0 => vec![],
                1 => vec![
                    Event::PointerMoved(pointer),
                    Event::PointerButton {
                        pos: pointer,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::NONE,
                    },
                ],
                _ => vec![Event::PointerButton {
                    pos: pointer,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                }],
            };
            let mut input = raw(phase as f64 / 60.0, Modifiers::NONE, events);
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .native_pixels_per_point = Some(scale);
            let mut output = context.run_ui(input, |ui| {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_max(
                    egui::pos2(16.3, 8.3),
                    egui::pos2(111.1, 90.7),
                )));
                let captured = capture(&mut child, &camera).unwrap().unwrap();
                pointer = captured.rect.center() + egui::vec2(20.0 / scale, 0.0);
                captured
                    .paint(
                        Some(&renderer),
                        &camera,
                        &view,
                        SelectionFeedback::Object(None),
                        true,
                        crate::editor_operations::ShadingMode::SmoothShadeAll,
                    )
                    .unwrap();
                last = Some(captured);
            });
            output.textures_delta.clear();
        }
        let input = last.unwrap();
        assert_eq!(input.physical_origin, origin);
        assert_eq!(input.physical_size, size);
        let aspect = size[0] as f32 / size[1] as f32;
        assert_eq!(input.aspect().to_bits(), aspect.to_bits());
        let ndc = input.click_ndc().unwrap();
        assert!((ndc.x - 40.0 / size[0] as f32).abs() < 2e-6);
        assert!(ndc.y.abs() < 2e-6);
        let capture = input.move_capture(&camera).unwrap();
        let pixel = capture
            .pointer_pixel(Vec2::new(pointer.x, pointer.y))
            .unwrap();
        let handles = WorldHandles::new(&capture, glam::DVec3::ZERO).unwrap();
        assert_eq!(handles.hit_test(pixel), Some(WorldAxis::X));
        let actual_ray = camera.ray(ndc, input.aspect()).unwrap();
        let expected_ray = camera.ray(ndc, aspect).unwrap();
        assert_eq!(actual_ray, expected_ray);
        let submission = recorded_submission(&renderer);
        let matrix = camera.projection_matrix(aspect).unwrap() * camera.view_matrix();
        let instance = &view.instances()[0];
        for (vertex, index) in submission.vertices[..3]
            .iter()
            .zip(instance.faces().last().unwrap().triangles().flatten())
        {
            assert_eq!(
                vertex[..4],
                (matrix * instance.world_vertices()[index].extend(1.0)).to_array()
            );
        }
    }
}

#[test]
fn degraded_face_centres_reach_the_overlay_text() {
    let mut native = NativeScene::default();
    native.create(Primitive::Cube, "cube").unwrap();
    let shape = native.shapes()[0].id();
    for _ in 0..200 {
        native.extrude_face(shape, 1, 0.01).unwrap();
    }
    let view = ViewScene::from_native(&native, 256 * 1024 * 1024).unwrap();
    let instance = &view.instances()[0];
    let active = (
        instance.transform(),
        &instance.path()[..instance.path().len() - 1],
    );
    let camera = camera();
    let producer = renderer_handle();
    let mut degraded = None;
    let mut output = egui::Context::default().run_ui(raw(0.0, Modifiers::NONE, Vec::new()), |ui| {
        degraded = Some(
            capture(ui, &camera)
                .unwrap()
                .unwrap()
                .paint(
                    Some(&producer),
                    &camera,
                    &view,
                    SelectionFeedback::Face {
                        active: Some(active),
                        selected: None,
                    },
                    true,
                    crate::editor_operations::ShadingMode::Wireframe,
                )
                .unwrap(),
        );
    });
    output.textures_delta.clear();
    assert_eq!(degraded, Some(true));
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(&s.shape, Shape::Text(t)
        if t.galley.job.text.contains(super::FACE_CENTERS_HIDDEN)))
    );
}

#[test]
fn upward_right_drag_dollies_the_camera_back_and_downward_dollies_in() {
    for (dy, farther) in [(-20.0, true), (20.0, false)] {
        let context = egui::Context::default();
        let mut camera = camera();
        camera
            .frame_points(&[Vec3::splat(-4.0), Vec3::splat(4.0)], 1.0)
            .unwrap();
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 96.0));
        let point = egui::pos2(40.0, 50.0);
        let none = Modifiers::ALT;
        let _ = capture_frame(
            &context,
            &camera,
            raw(0.0, none, vec![Event::PointerMoved(point)]),
            clip,
        );
        let press = Event::PointerButton {
            pos: point,
            button: PointerButton::Secondary,
            pressed: true,
            modifiers: none,
        };
        let _ = capture_frame(&context, &camera, raw(1.0 / 60.0, none, vec![press]), clip);
        let (mut input, _) = capture_frame(
            &context,
            &camera,
            raw(
                2.0 / 60.0,
                none,
                vec![Event::PointerMoved(egui::pos2(40.0, 50.0 + dy))],
            ),
            clip,
        );
        let operations = input.take_navigation();
        let [Operation::Dolly { factor }] = operations.as_slice() else {
            panic!("a right-drag must produce exactly one dolly")
        };
        let before = camera.distance();
        camera.dolly(*factor).unwrap();
        assert_eq!(camera.distance() > before, farther, "dy {dy}");
    }
}

#[test]
fn plain_middle_and_right_drags_leave_the_camera_unbound_and_alt_still_navigates() {
    for (button, alt) in [
        (PointerButton::Middle, false),
        (PointerButton::Secondary, false),
        (PointerButton::Middle, true),
        (PointerButton::Secondary, true),
    ] {
        let context = egui::Context::default();
        let mut camera = camera();
        camera
            .frame_points(&[Vec3::splat(-4.0), Vec3::splat(4.0)], 1.0)
            .unwrap();
        let clip = Rect::from_min_size(Pos2::ZERO, egui::vec2(128.0, 96.0));
        let point = egui::pos2(40.0, 50.0);
        let modifiers = if alt { Modifiers::ALT } else { Modifiers::NONE };
        let _ = capture_frame(
            &context,
            &camera,
            raw(0.0, modifiers, vec![Event::PointerMoved(point)]),
            clip,
        );
        let press = Event::PointerButton {
            pos: point,
            button,
            pressed: true,
            modifiers,
        };
        let _ = capture_frame(
            &context,
            &camera,
            raw(1.0 / 60.0, modifiers, vec![press]),
            clip,
        );
        let (mut input, _) = capture_frame(
            &context,
            &camera,
            raw(
                2.0 / 60.0,
                modifiers,
                vec![Event::PointerMoved(egui::pos2(50.0, 70.0))],
            ),
            clip,
        );
        let operations = input.take_navigation();
        match (alt, button) {
            (false, _) => assert!(operations.is_empty(), "{button:?} without Alt"),
            (true, PointerButton::Middle) => {
                assert!(matches!(operations.as_slice(), [Operation::Pan { .. }]))
            }
            (true, _) => assert!(matches!(operations.as_slice(), [Operation::Dolly { .. }])),
        }
    }
}

fn primary(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

/// Press, drag past the threshold, then release; returns the release frame's input and
/// whether any frame of the drag emitted an Orbit.
fn drag_release(
    press: Modifiers,
    during: Modifiers,
    release: Modifiers,
    move_distance: f32,
) -> (ViewportInput, bool) {
    let context = egui::Context::default();
    let camera = camera();
    let start = egui::pos2(64.0, 48.0);
    let end = start + egui::vec2(move_distance, 0.0);
    let clip = Rect::EVERYTHING;
    capture_frame(
        &context,
        &camera,
        raw(0.0, press, vec![Event::PointerMoved(start)]),
        clip,
    );
    let mut orbited = false;
    let mut step = |time, modifiers, events| {
        let (mut input, _) = capture_frame(&context, &camera, raw(time, modifiers, events), clip);
        orbited |= input
            .take_navigation()
            .iter()
            .any(|operation| matches!(operation, Operation::Orbit { .. }));
        input
    };
    step(0.1, press, vec![primary(start, true, press)]);
    step(0.2, press, vec![Event::PointerMoved(end)]);
    step(
        0.25,
        during,
        vec![Event::PointerMoved(end + egui::vec2(4.0, 0.0))],
    );
    let input = step(0.3, release, vec![primary(end, false, release)]);
    (input, orbited)
}

#[test]
fn plain_drag_release_raises_marquee_but_a_quick_click_does_not() {
    let (input, orbited) = drag_release(Modifiers::NONE, Modifiers::NONE, Modifiers::NONE, 40.0);
    assert_eq!(
        input.unsupported_selection(),
        Some(SelectionRefusal::Marquee)
    );
    assert!(!orbited && input.click_ndc().is_none());
    let (input, _) = drag_release(Modifiers::NONE, Modifiers::NONE, Modifiers::NONE, 0.0);
    assert!(input.click_ndc().is_some());
    assert_eq!(input.unsupported_selection(), None);
}

#[test]
fn alt_drag_orbits_without_marquee_even_when_alt_is_released_before_the_button() {
    for release in [Modifiers::ALT, Modifiers::NONE] {
        let (input, orbited) = drag_release(Modifiers::ALT, Modifiers::ALT, release, 40.0);
        assert!(orbited);
        assert_eq!(input.unsupported_selection(), None, "{release:?}");
    }
}

#[test]
fn shift_click_keeps_modified_click() {
    let (input, _) = drag_release(Modifiers::SHIFT, Modifiers::SHIFT, Modifiers::SHIFT, 0.0);
    assert_eq!(
        input.unsupported_selection(),
        Some(SelectionRefusal::ModifiedClick)
    );
}

#[test]
fn marquee_follows_the_release_event_modifiers_not_the_frame_modifiers() {
    let (input, orbited) = drag_release(Modifiers::NONE, Modifiers::NONE, Modifiers::ALT, 40.0);
    assert!(!orbited);
    assert_eq!(input.unsupported_selection(), None);

    let context = egui::Context::default();
    let camera = camera();
    let start = egui::pos2(64.0, 48.0);
    let end = start + egui::vec2(40.0, 0.0);
    let clip = Rect::EVERYTHING;
    let none = Modifiers::NONE;
    for (time, events) in [
        (0.0, vec![Event::PointerMoved(start)]),
        (0.1, vec![primary(start, true, none)]),
        (0.2, vec![Event::PointerMoved(end)]),
    ] {
        capture_frame(&context, &camera, raw(time, none, events), clip);
    }
    let (input, _) = capture_frame(
        &context,
        &camera,
        raw(
            0.3,
            none,
            vec![
                primary(end, false, none),
                Event::ModifiersChanged(Modifiers::ALT),
            ],
        ),
        clip,
    );
    assert_eq!(
        input.unsupported_selection(),
        Some(SelectionRefusal::Marquee)
    );
}

#[test]
fn alt_pressed_mid_drag_orbits_without_marquee() {
    let (input, orbited) = drag_release(Modifiers::NONE, Modifiers::ALT, Modifiers::NONE, 40.0);
    assert!(orbited);
    assert_eq!(input.unsupported_selection(), None);
}
