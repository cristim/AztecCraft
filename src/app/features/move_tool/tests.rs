use super::*;
use crate::editor_operations::{SelectionMode, SidebarPanel};
use azteccraft::{
    manipulator_handles::{DisabledKind, Pick, ScaleHandles},
    native_scene::NativeError,
    viewport::Camera,
};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, Shape};
use glam::Vec3;

const FACE_MODE_REFUSAL: &str = "Move is unavailable in Face selection; switch to Object selection";

pub(in crate::app) fn raw(
    size: egui::Vec2,
    origin: Pos2,
    scale: f32,
    events: Vec<Event>,
) -> egui::RawInput {
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(origin, size)),
        events,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.native_pixels_per_point = Some(scale);
    viewport.minimized = Some(false);
    viewport.occluded = Some(false);
    input
}

pub(in crate::app) fn frame(
    editor: &mut Editor,
    context: &egui::Context,
    mut input: egui::RawInput,
) -> egui::FullOutput {
    eframe::App::raw_input_hook(editor, context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(input, |ui| {
        eframe::App::logic(editor, ui.ctx(), &mut native_frame);
        eframe::App::ui(editor, ui, &mut native_frame);
    });
    output.textures_delta.clear();
    output
}

pub(in crate::app) fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

pub(in crate::app) fn button(position: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(position),
        Event::PointerButton {
            pos: position,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        },
    ]
}

pub(in crate::app) fn viewport(output: &egui::FullOutput) -> Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            Shape::Callback(callback) => Some(callback.rect),
            _ => None,
        })
        .expect("actual viewport callback")
}

pub(in crate::app) fn oblique() -> Camera {
    Camera::new(Vec3::ZERO, 0.6, 0.4, 10.0, 90_f32.to_radians(), 0.01, 100.0).unwrap()
}

#[derive(Clone)]
pub(in crate::app) struct View {
    pub(in crate::app) camera: Camera,
    pub(in crate::app) aspect: f32,
    pub(in crate::app) rect: Rect,
    pub(in crate::app) scale: f32,
}

impl View {
    pub(in crate::app) fn screen(&self, world: Vec3) -> Pos2 {
        let ndc = self.camera.project(world, self.aspect).unwrap().unwrap();
        Pos2::new(
            self.rect.left() + (ndc.x + 1.0) * self.rect.width() * 0.5,
            self.rect.top() + (1.0 - ndc.y) * self.rect.height() * 0.5,
        )
    }

    // World length of a 72 px handle at the origin: the 90 degree view spans 10 units up and down.
    pub(in crate::app) fn radius(&self) -> f32 {
        72.0 / self.scale * 10.0 / (self.rect.height() * 0.5)
    }

    // The point of the ring about `axis` at `degrees` from the ring point nearest the eye.
    pub(in crate::app) fn ring(&self, axis: Vec3, degrees: f32) -> Pos2 {
        self.ring_sized(axis, degrees, 1.0)
    }

    // The same point on a ring whose radius is `factor` times the default one.
    pub(in crate::app) fn ring_sized(&self, axis: Vec3, degrees: f32, factor: f32) -> Pos2 {
        let eye = self.camera.eye();
        let u = (eye - axis * eye.dot(axis)).normalize();
        let v = axis.cross(u);
        let angle = degrees.to_radians();
        self.screen((u * angle.cos() + v * angle.sin()) * self.radius() * factor)
    }
}

// The midpoint of a Scale plane outline that no shaft outranks, as a logical pointer for a
// viewport at `rect.min` and scale one. Needs the Scale tool at a small manipulator size.
pub(in crate::app) fn scale_plane_point(editor: &Editor, rect: Rect) -> Pos2 {
    let capture = editor.move_viewport.as_ref().unwrap().capture.clone();
    let (_, frame) = editor.manipulation_frame(ActiveTool::Scale).unwrap();
    let handles = ScaleHandles::new(&capture, &frame).unwrap();
    let pixel = handles
        .disabled()
        .iter()
        .flat_map(|handle| handle.segments())
        .map(|segment| (segment.start + segment.end) * 0.5)
        .find(|pixel| handles.pick(*pixel) == Some(Pick::Disabled(DisabledKind::ScalePlane)))
        .expect("a plane outline pixel that no shaft outranks");
    rect.min + egui::vec2(pixel.x as f32, pixel.y as f32)
}

pub(in crate::app) fn setup(scale: f32) -> (Editor, egui::Context, Rect) {
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: azteccraft::viewport::Camera::new(
            glam::Vec3::ZERO,
            0.0,
            0.0,
            10.0,
            90_f32.to_radians(),
            0.01,
            100.0,
        )
        .unwrap(),
        ..Editor::default()
    };
    editor.create(azteccraft::scene::Primitive::Cube);
    let context = egui::Context::default();
    let output = frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            scale,
            vec![key(Key::W)],
        ),
    );
    assert_eq!(editor.active_tool, ActiveTool::Move);
    let rect = viewport(&output);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            scale,
            vec![Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        ),
    );
    (editor, context, rect)
}

pub(in crate::app) fn press_x(
    editor: &mut Editor,
    context: &egui::Context,
    rect: Rect,
    scale: f32,
) -> Pos2 {
    let position = rect.center() + egui::vec2(40.0 / scale, 0.0);
    frame(
        editor,
        context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            scale,
            button(position, true),
        ),
    );
    assert!(editor.native_move.is_some(), "{}", editor.status);
    assert_eq!(
        editor.history.active_manipulation().unwrap().provenance,
        ManipulationProvenance::Native
    );
    position
}

#[test]
fn real_w_q_and_vector_buttons_activate_tools_and_leave_unsupported_tools_disabled() {
    let (mut editor, context, _) = setup(1.0);
    context.enable_accesskit();
    let output = frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![key(Key::Q)],
        ),
    );
    assert_eq!(editor.active_tool, ActiveTool::Select);
    let label = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            let bounds = node.bounds()?;
            (node.label() == Some("Move")
                && node.role() == egui::accesskit::Role::Button
                && bounds.x1 < 100.0)
                .then(|| {
                    Pos2::new(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    )
                })
        })
        .unwrap();
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(label, true),
        ),
    );
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(label, false),
        ),
    );
    assert_eq!(editor.active_tool, ActiveTool::Move);
    context.memory_mut(|memory| memory.request_focus(crate::app::name_edit_id()));
    frame(
        &mut editor,
        &context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
    );
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![key(Key::Q)],
        ),
    );
    assert_eq!(editor.active_tool, ActiveTool::Move);
    assert!(editor.history.active_manipulation().is_none());
}

#[test]
fn real_axis_press_absolute_motion_release_makes_one_undo_at_fractional_dpi() {
    for scale in [1.0, 1.25, 2.0] {
        let (mut editor, context, rect) = setup(scale);
        let node = editor.selected.unwrap();
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let selected_path = editor.selected_path.clone();
        let begin = press_x(&mut editor, &context, rect, scale);
        for distance in [0.5, 1.0, -1.0, 2.0] {
            let position = begin + egui::vec2(rect.height() * distance / 20.0, 0.0);
            frame(
                &mut editor,
                &context,
                raw(
                    egui::vec2(1440.0, 900.0),
                    Pos2::ZERO,
                    scale,
                    vec![Event::PointerMoved(position)],
                ),
            );
            let translation = editor
                .history
                .scene()
                .node(node)
                .unwrap()
                .transform()
                .translation;
            assert!(
                (translation[0] - f64::from(distance)).abs() < 2e-5,
                "{translation:?}"
            );
            assert_eq!(translation[1].to_bits(), 0.0f64.to_bits());
            assert_eq!(translation[2].to_bits(), 0.0f64.to_bits());
            assert!(!editor.has_draft());
        }
        let end = begin + egui::vec2(rect.height() / 10.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                scale,
                button(end, false),
            ),
        );
        assert!(editor.history.active_manipulation().is_none());
        assert!(editor.native_move.is_none());
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.selected_path, selected_path);
        let after = editor.history.scene().clone();
        editor.history_step(false);
        assert_eq!(editor.history.scene(), &before);
        editor.history_step(false);
        assert!(editor.history.scene().nodes().is_empty());
        editor.history_step(true);
        assert_eq!(editor.history.scene(), &before);
        editor.history_step(true);
        assert_eq!(editor.history.scene(), &after);
    }
}

#[test]
fn stationary_release_in_changed_window_origin_size_or_dpi_cancels_without_fallthrough() {
    for (size, origin, scale) in [
        (egui::vec2(1500.0, 900.0), Pos2::ZERO, 1.0),
        (egui::vec2(1440.0, 900.0), egui::pos2(1.3, 0.7), 1.0),
        (egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.25),
    ] {
        let (mut editor, context, rect) = setup(1.0);
        let node = editor.selected.unwrap();
        editor
            .apply_operation(Operation::Rename {
                id: node,
                name: "renamed".into(),
            })
            .unwrap();
        editor.history_step(false);
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let path = editor.selected_path.clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let position = begin + egui::vec2(rect.height() / 10.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(position)],
            ),
        );
        assert_ne!(editor.history.scene(), &before);
        frame(
            &mut editor,
            &context,
            raw(size, origin, scale, button(position, false)),
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.selected, Some(node));
        assert_eq!(editor.selected_path, path);
        assert!(!editor.has_draft());
        assert!(editor.history.can_undo());
        assert!(editor.history.can_redo());
        editor.history_step(true);
        assert_eq!(editor.history.scene().node(node).unwrap().name(), "renamed");
        editor.history_step(false);
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn escape_modifiers_focus_and_pointer_loss_cancel_real_captured_gestures() {
    for interruption in 0..4 {
        let (mut editor, context, rect) = setup(1.0);
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let position = begin + egui::vec2(rect.height() / 10.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(position)],
            ),
        );
        let mut input = raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            match interruption {
                0 => vec![key(Key::Escape)],
                1 => vec![Event::ModifiersChanged(Modifiers::SHIFT)],
                2 => vec![Event::WindowFocused(false)],
                _ => vec![Event::PointerGone],
            },
        );
        if interruption == 2 {
            input.focused = false;
        }
        frame(&mut editor, &context, input);
        assert!(
            editor.history.active_manipulation().is_none(),
            "{interruption}: {}",
            editor.status
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(position, false),
            ),
        );
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.native_move.is_none());
    }
}

#[test]
fn real_q_cancels_preview_before_tool_switch_and_remote_cannot_finish_native_move() {
    let (mut editor, context, rect) = setup(1.0);
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let id = editor.history.active_manipulation().unwrap().id;
    let position = begin + egui::vec2(rect.height() / 10.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(position)],
        ),
    );
    let preview = editor.history.scene().clone();
    let reply = editor.remote_operation(
        crate::editor_operations::RemoteRequest {
            expected_scene_token: Some(editor.scene_token),
            operation: Operation::FinishMove {
                id,
                finish: ManipulationFinish::Commit,
            },
        },
        &context,
    );
    assert!(matches!(reply, Err(OperationError::BusyManipulation)));
    assert_eq!(editor.history.scene(), &preview);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![key(Key::Q)],
        ),
    );
    assert_eq!(editor.active_tool, ActiveTool::Select);
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(position, false),
        ),
    );
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn real_xyz_drag_samples_reach_independent_world_positions_and_can_leave_the_viewport() {
    for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
        let (mut editor, context, _) = setup(1.0);
        editor.camera = azteccraft::viewport::Camera::new(
            glam::Vec3::ZERO,
            0.6,
            0.4,
            10.0,
            90_f32.to_radians(),
            0.01,
            100.0,
        )
        .unwrap();
        let output = frame(
            &mut editor,
            &context,
            raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
        );
        let rect = viewport(&output);
        let aspect = editor.aspect;
        let screen = |world: glam::Vec3| {
            let ndc = editor.camera.project(world, aspect).unwrap().unwrap();
            egui::pos2(
                rect.left() + (ndc.x + 1.0) * rect.width() * 0.5,
                rect.top() + (1.0 - ndc.y) * rect.height() * 0.5,
            )
        };
        let direction = axis.direction().as_vec3();
        let begin = screen(direction * 0.8);
        let end = screen(direction * 2.8);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(begin, true),
            ),
        );
        assert_eq!(
            editor
                .history
                .active_manipulation()
                .unwrap()
                .move_axis()
                .unwrap(),
            axis
        );
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(end)],
            ),
        );
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(end, false),
            ),
        );
        let actual =
            glam::DVec3::from_array(editor.history.scene().nodes()[0].transform().translation);
        assert!(
            (actual - axis.direction() * 2.0).length() < 5e-5,
            "{axis:?}: {actual:?}"
        );
        assert!(editor.history.active_manipulation().is_none());
    }
    let (mut editor, context, rect) = setup(1.0);
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let outside = egui::pos2(rect.right() + 50.0, begin.y);
    let expected = f64::from((outside.x - begin.x) * 20.0 / rect.height());
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(outside)],
        ),
    );
    assert!((editor.history.scene().nodes()[0].transform().translation[0] - expected).abs() < 5e-5);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(outside, false),
        ),
    );
    assert!(editor.history.active_manipulation().is_none());
    assert!((editor.history.scene().nodes()[0].transform().translation[0] - expected).abs() < 5e-5);
}

#[test]
fn projection_replacement_and_missing_viewport_cancel_before_release_commit() {
    for missing in [false, true] {
        let (mut editor, context, rect) = setup(1.0);
        let before = editor.history.scene().clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let end = begin + egui::vec2(rect.height() / 10.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(end)],
            ),
        );
        let size = if missing {
            egui::vec2(1.0, 1.0)
        } else {
            egui::vec2(1440.0, 900.0)
        };
        if !missing {
            editor.camera = azteccraft::viewport::Camera::new(
                glam::Vec3::ZERO,
                0.0,
                0.0,
                10.0,
                75_f32.to_radians(),
                0.01,
                100.0,
            )
            .unwrap();
        }
        let camera = editor.camera.clone();
        frame(
            &mut editor,
            &context,
            raw(size, Pos2::ZERO, 1.0, button(end, false)),
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
    }
}

#[test]
fn editor_move_guards_preserve_drafts_modal_close_tokens_and_failed_open_baseline() {
    use crate::editor_operations::{FileDecision, RemoteRequest};
    for condition in 0..6 {
        for operation in 0..4 {
            let mut editor = Editor::default();
            editor.create(azteccraft::scene::Primitive::Cube);
            let context = egui::Context::default();
            let target = editor.selected_move_target().unwrap();
            let token = editor.scene_token;
            let before = editor.history.scene().clone();
            match condition {
                0 => editor.channels[0] = "human".into(),
                1 => context.memory_mut(|memory| memory.request_focus(crate::app::name_edit_id())),
                2 => editor.native_input_pending = true,
                3 => {
                    editor.apply_operation(Operation::NewScene).unwrap();
                }
                4 => {
                    let OperationReply::PendingConfirmation {
                        pending_action_id, ..
                    } = editor.apply_operation(Operation::RequestClose).unwrap()
                    else {
                        panic!()
                    };
                    editor
                        .apply_operation(Operation::ResolveFileAction {
                            pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                            decision: FileDecision::Discard,
                        })
                        .unwrap();
                }
                _ => (),
            }
            let request = match operation {
                0 => Operation::SetActiveTool(ActiveTool::Move),
                1 => Operation::BeginMove {
                    target,
                    axis: WorldAxis::X,
                },
                2 => Operation::UpdateMove {
                    id: ManipulationId::from_decimal("1").unwrap(),
                    distance: MoveDistance::Declared(1.0),
                },
                _ => Operation::FinishMove {
                    id: ManipulationId::from_decimal("1").unwrap(),
                    finish: ManipulationFinish::Commit,
                },
            };
            let reply = editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(if condition == 5 {
                        Uuid::new_v4()
                    } else {
                        token
                    }),
                    operation: request,
                },
                &context,
            );
            assert!(matches!(
                reply,
                Err(OperationError::BusyEditing | OperationError::StaleScene)
            ));
            assert_eq!(editor.history.scene(), &before);
            assert!(editor.history.active_manipulation().is_none());
            assert_eq!(editor.active_tool, ActiveTool::Select);
        }
    }
    let mut editor = Editor::default();
    editor.create(azteccraft::scene::Primitive::Cube);
    editor
        .apply_operation(Operation::SetActiveTool(ActiveTool::Move))
        .unwrap();
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let camera = editor.camera.clone();
    let target = editor.selected_move_target().unwrap();
    let reply = editor
        .apply_operation_guarded(
            ManipulationProvenance::Remote,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
            &|| true,
        )
        .unwrap();
    assert!(matches!(reply, OperationReply::ManipulationStarted { .. }));
    let id = editor.history.active_manipulation().unwrap().id;
    editor
        .apply_operation_guarded(
            ManipulationProvenance::Remote,
            Operation::UpdateMove {
                id,
                distance: MoveDistance::Declared(2.0),
            },
            &|| true,
        )
        .unwrap();
    let missing_directory = tempfile::tempdir().unwrap();
    assert!(
        editor
            .apply_operation(Operation::OpenScene {
                path: missing_directory.path().join("missing-move-fixture.mb")
            })
            .is_err()
    );
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.active_tool, ActiveTool::Move);
    assert!(editor.history.active_manipulation().is_none());
    let pending = editor.apply_operation(Operation::NewScene).unwrap();
    let OperationReply::PendingConfirmation {
        pending_action_id, ..
    } = pending
    else {
        panic!()
    };
    editor
        .apply_operation(Operation::ResolveFileAction {
            pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
            decision: FileDecision::Discard,
        })
        .unwrap();
    assert_ne!(editor.scene_token, token);
    assert_eq!(editor.active_tool, ActiveTool::Select);
    assert!(editor.history.active_manipulation().is_none());
    assert!(editor.history.scene().nodes().is_empty());
}

#[test]
fn finite_parallel_or_backward_release_commits_exact_last_valid_preview_with_honest_status() {
    for backward in [false, true] {
        let (mut editor, context, mut rect) = setup(1.0);
        let (begin, end) = if backward {
            editor.camera = azteccraft::viewport::Camera::new(
                glam::Vec3::ZERO,
                0.6,
                0.4,
                10.0,
                90_f32.to_radians(),
                0.01,
                100.0,
            )
            .unwrap();
            let output = frame(
                &mut editor,
                &context,
                raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
            );
            rect = viewport(&output);
            let project = |x| {
                let point = editor
                    .camera
                    .project(glam::Vec3::new(x, 0.0, 0.0), editor.aspect)
                    .unwrap()
                    .unwrap();
                egui::pos2(
                    rect.left() + (point.x + 1.0) * rect.width() * 0.5,
                    rect.top() + (1.0 - point.y) * rect.height() * 0.5,
                )
            };
            (project(0.8), project(2.8))
        } else {
            let begin = rect.center() + egui::vec2(40.0, 0.0);
            (begin, begin + egui::vec2(rect.height() / 10.0, 0.0))
        };
        let before = editor.history.scene().clone();
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(begin, true),
            ),
        );
        assert_eq!(
            editor
                .history
                .active_manipulation()
                .unwrap()
                .move_axis()
                .unwrap(),
            WorldAxis::X
        );
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(end)],
            ),
        );
        let last_valid = editor.history.scene().clone();
        assert_ne!(last_valid, before);
        let rejected = if backward {
            egui::pos2(rect.center().x, rect.top() - rect.height() * 10.0)
        } else {
            egui::pos2(100_000_000.0, begin.y)
        };
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(rejected, false),
            ),
        );
        assert!(
            editor.history.active_manipulation().is_none(),
            "{}",
            editor.status
        );
        assert_eq!(editor.history.scene(), &last_valid);
        assert!(
            editor.status.contains("last valid position committed"),
            "{}",
            editor.status
        );
        assert!(
            editor
                .status
                .contains(if backward { "behind" } else { "parallel" }),
            "{}",
            editor.status
        );
        editor.history_step(false);
        assert_eq!(editor.history.scene(), &before);
        editor.history_step(false);
        assert!(editor.history.scene().nodes().is_empty());
    }
}

#[test]
fn tiny_visible_geometry_inspection_reports_pointer_exclusion_without_global_ready_claim() {
    let (mut editor, context, _) = setup(1.0);
    let mut output = context.run_ui(
        raw(
            egui::vec2(16.0, 16.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(egui::pos2(2.0, 2.0))],
        ),
        |ui| {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(Rect::from_min_size(Pos2::ZERO, egui::vec2(4.0, 4.0))),
            );
            let input = crate::viewport_ui::capture(&mut child, &editor.camera)
                .unwrap()
                .unwrap();
            assert!(
                !editor
                    .handle_move_input(&input, editor.scene_token)
                    .unwrap()
            );
            editor.paint_move(&input);
        },
    );
    output.textures_delta.clear();
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(inspected.kinds[0].available);
    let viewport = inspected.viewport.unwrap();
    assert_eq!(viewport.provenance, "last_native_viewport_capture");
    assert!(viewport.pointer_available);
    assert_eq!(viewport.pointer_hit, None);
    assert!(
        viewport
            .axes
            .iter()
            .find(|axis| axis.axis == "x")
            .unwrap()
            .visible
    );
    assert!(
        viewport
            .axes
            .iter()
            .find(|axis| axis.axis == "y")
            .unwrap()
            .visible
    );
    assert!(
        !viewport
            .axes
            .iter()
            .find(|axis| axis.axis == "z")
            .unwrap()
            .visible
    );
}

mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[test]
fn native_source_bound_cancel_restores_prior_authored_signed_zero_bytes_and_redo() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("native-move-original.mb");
    let original = wire::specimen();
    std::fs::write(&source, &original).unwrap();
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    editor
        .apply_operation(Operation::OpenScene {
            path: source.clone(),
        })
        .unwrap();
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    let node = target.node;
    editor
        .apply_operation(Operation::Select {
            node: Some(node),
            instance_path: Some(target.path),
        })
        .unwrap();
    editor
        .apply_operation(Operation::EditNode {
            id: node,
            name: None,
            translation: [Some(0.25), Some(-0.0), None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    editor
        .apply_operation(Operation::EditNode {
            id: node,
            name: None,
            translation: [Some(0.5), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    editor.history_step(false);
    editor.camera = azteccraft::viewport::Camera::new(
        glam::Vec3::new(0.25, 0.0, 3.0),
        0.0,
        0.0,
        10.0,
        90_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap();
    let before = editor.history.scene().clone();
    let encoded = azteccraft::native_scene::binary::encode(&before).unwrap();
    let token = editor.scene_token;
    let dirty = editor.is_dirty();
    let context = egui::Context::default();
    let output = frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![key(Key::W)],
        ),
    );
    let rect = viewport(&output);
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let end = begin + egui::vec2(rect.height() / 10.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(end)],
        ),
    );
    assert_ne!(editor.history.scene(), &before);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.25,
            button(end, false),
        ),
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(
        editor
            .history
            .scene()
            .node(node)
            .unwrap()
            .transform()
            .translation[1]
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        azteccraft::native_scene::binary::encode(editor.history.scene()).unwrap(),
        encoded
    );
    editor.history_step(true);
    assert_eq!(
        editor
            .history
            .scene()
            .node(node)
            .unwrap()
            .transform()
            .translation[0],
        0.5
    );
    editor.history_step(false);
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(std::fs::read(&source).unwrap(), original);
}

#[test]
fn coalesced_modified_release_or_modifier_transition_cancels_before_final_modifier_up() {
    for transition_only in [false, true] {
        let (mut editor, context, rect) = setup(1.0);
        let before = editor.history.scene().clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let end = begin + egui::vec2(rect.height() / 10.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(end)],
            ),
        );
        assert_ne!(editor.history.scene(), &before);
        let events = if transition_only {
            vec![
                Event::ModifiersChanged(Modifiers::SHIFT),
                Event::ModifiersChanged(Modifiers::NONE),
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::NONE,
                },
            ]
        } else {
            vec![
                Event::PointerButton {
                    pos: end,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::SHIFT,
                },
                Event::ModifiersChanged(Modifiers::NONE),
            ]
        };
        frame(
            &mut editor,
            &context,
            raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, events),
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn coalesced_modified_tool_key_is_not_unmodified_by_later_modifier_up() {
    for tool_key in [Key::Q, Key::W] {
        let (mut editor, context, _) = setup(1.0);
        if tool_key == Key::W {
            frame(
                &mut editor,
                &context,
                raw(
                    egui::vec2(1440.0, 900.0),
                    Pos2::ZERO,
                    1.0,
                    vec![key(Key::Q)],
                ),
            );
        }
        let before = editor.active_tool;
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![
                    Event::Key {
                        key: tool_key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::SHIFT,
                    },
                    Event::ModifiersChanged(Modifiers::NONE),
                ],
            ),
        );
        assert_eq!(editor.active_tool, before);
        assert!(editor.history.active_manipulation().is_none());
    }
}

#[test]
fn idle_escape_or_tool_activation_consumes_same_frame_axis_press_and_terminal_release() {
    for intent in [Key::Escape, Key::W] {
        let (mut editor, context, rect) = setup(1.0);
        if intent == Key::W {
            frame(
                &mut editor,
                &context,
                raw(
                    egui::vec2(1440.0, 900.0),
                    Pos2::ZERO,
                    1.0,
                    vec![key(Key::Q)],
                ),
            );
        }
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let point = rect.center() + egui::vec2(40.0, 0.0);
        let mut events = vec![key(intent)];
        events.extend(button(point, true));
        frame(
            &mut editor,
            &context,
            raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, events),
        );
        assert_eq!(editor.active_tool, ActiveTool::Move);
        assert!(editor.history.active_manipulation().is_none());
        assert!(editor.native_move.is_none());
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(point, false),
            ),
        );
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
    }
}

#[test]
fn idle_cancels_from_select_new_scene_and_tool_changes_do_not_swallow_the_next_press() {
    let (mut editor, context, rect) = setup(1.0);
    let id = editor.selected.unwrap();
    assert!(!editor.cancel_move().unwrap());
    assert!(!editor.move_consumed_frame && !editor.move_suppress_primary);
    editor.native_operation(Operation::Select {
        node: Some(id),
        instance_path: Some(editor.selected_path[..editor.selected_path.len() - 1].to_vec()),
    });
    assert!(!editor.move_consumed_frame && !editor.move_suppress_primary);
    press_x(&mut editor, &context, rect, 1.0);
    assert!(editor.cancel_move().unwrap());
    assert!(editor.move_consumed_frame && editor.move_suppress_primary);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(rect.center(), false),
        ),
    );
    assert!(!editor.move_suppress_primary);
    editor.native_operation(Operation::NewScene);
    assert!(!editor.move_consumed_frame && !editor.move_suppress_primary);
}

#[test]
fn release_with_a_refused_final_sample_commits_the_last_valid_preview() {
    let (mut editor, context, rect) = setup(1.0);
    let node = editor.selected.unwrap();
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let valid = begin + egui::vec2(rect.height() / 20.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(valid)],
        ),
    );
    let preview = editor.history.scene().clone();
    assert_ne!(preview, before);
    let id = editor.native_move.as_ref().unwrap().id;
    editor
        .finish_native_sample(
            id,
            ActiveTool::Move,
            true,
            Err(OperationError::InvalidInput("final sample refused".into())),
        )
        .unwrap();
    assert!(editor.history.active_manipulation().is_none());
    assert!(editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &preview, "{}", editor.status);
    assert!(
        editor.status == "final sample refused; last valid preview committed",
        "{}",
        editor.status
    );
    assert!(editor.error);
    editor.history_step(false);
    assert_eq!(
        editor
            .history
            .scene()
            .node(node)
            .unwrap()
            .transform()
            .translation,
        [0.0; 3]
    );
}

fn tap(editor: &mut Editor, context: &egui::Context, point: Pos2, mut events: Vec<Event>) {
    events.extend(button(point, true));
    frame(
        editor,
        context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, events),
    );
    frame(
        editor,
        context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(point, false),
        ),
    );
}

#[test]
fn face_mode_refuses_move_handles_native_and_numeric_and_keeps_face_click_working() {
    let (mut editor, context, rect) = setup(1.0);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let before = editor.history.scene().clone();
    let axis = rect.center() + egui::vec2(40.0, 0.0);
    tap(&mut editor, &context, axis, vec![]);
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.status, FACE_MODE_REFUSAL);
    let target = editor.selected_move_target().unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X
        }),
        Err(OperationError::InvalidInput(message)) if message == FACE_MODE_REFUSAL
    ));
    assert!(editor.history.active_manipulation().is_none());
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(!inspected.kinds[0].available);
    assert_eq!(
        inspected.kinds[0].unavailable_reason.as_deref(),
        Some(FACE_MODE_REFUSAL)
    );
    assert!(inspected.viewport.is_none());
    tap(&mut editor, &context, rect.center(), vec![]);
    assert!(editor.remembered_face.is_some(), "{}", editor.status);
}

#[test]
fn vertex_and_edge_modes_refuse_move_everywhere_face_does() {
    for (mode, noun) in [
        (SelectionMode::Vertex, "Vertex"),
        (SelectionMode::Edge, "Edge"),
    ] {
        let refusal =
            format!("Move is unavailable in {noun} selection; switch to Object selection");
        let (mut editor, context, rect) = setup(1.0);
        editor.selection_mode = mode;
        let before = editor.history.scene().clone();
        tap(
            &mut editor,
            &context,
            rect.center() + egui::vec2(40.0, 0.0),
            vec![],
        );
        assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.status, refusal);
        let target = editor.selected_move_target().unwrap();
        assert!(matches!(
            editor.apply_operation(Operation::BeginMove {
                target,
                axis: WorldAxis::X
            }),
            Err(OperationError::InvalidInput(message)) if message == refusal
        ));
        let inspected = editor.inspect_manipulation().unwrap();
        assert!(!inspected.kinds[0].available);
        assert_eq!(
            inspected.kinds[0].unavailable_reason.as_deref(),
            Some(refusal.as_str())
        );
        assert!(inspected.viewport.is_none());
    }
}

#[test]
fn a_consumed_frame_does_not_select_a_face_from_the_same_click() {
    let (mut editor, context, rect) = setup(1.0);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    tap(&mut editor, &context, rect.center(), vec![key(Key::Escape)]);
    assert!(editor.remembered_face.is_none());
    tap(&mut editor, &context, rect.center(), vec![]);
    assert!(editor.remembered_face.is_some(), "{}", editor.status);
}

#[test]
fn native_face_mode_switch_cancels_an_active_native_move_and_remote_move_blocks_it() {
    for via_key in [false, true] {
        let (mut editor, context, rect) = setup(1.0);
        let before = editor.history.scene().clone();
        let begin = press_x(&mut editor, &context, rect, 1.0);
        let moved = begin + egui::vec2(rect.height() / 20.0, 0.0);
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(moved)],
            ),
        );
        assert_ne!(editor.history.scene(), &before);
        if via_key {
            frame(
                &mut editor,
                &context,
                raw(
                    egui::vec2(1440.0, 900.0),
                    Pos2::ZERO,
                    1.0,
                    vec![key(Key::F11)],
                ),
            );
        } else {
            editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
        }
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.history.can_undo() || editor.history.scene() == &before);
    }
    let (mut editor, _, _) = setup(1.0);
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation_guarded(
            ManipulationProvenance::Remote,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
            &|| true,
        )
        .unwrap();
    for operation in [
        Operation::SetSelectionMode(SelectionMode::Face),
        Operation::SelectFace { target: None },
        Operation::SetGridVisible { visible: false },
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::ChannelBox,
            visible: false,
        },
    ] {
        assert!(matches!(
            editor.apply_operation_guarded(ManipulationProvenance::Remote, operation, &|| true),
            Err(OperationError::BusyManipulation)
        ));
    }
    assert_eq!(editor.selection_mode, SelectionMode::Object);
    assert!(editor.grid_visible);
    editor.native_operation(Operation::SetGridVisible { visible: false });
    assert!(
        editor.grid_visible,
        "native Grid toggle is refused during a Move"
    );
    assert!(editor.sidebar_panel_visible(SidebarPanel::ChannelBox));
    editor.status.clear();
    editor.native_operation(Operation::SetSidebarPanelVisible {
        panel: SidebarPanel::ChannelBox,
        visible: false,
    });
    assert!(
        editor.sidebar_panel_visible(SidebarPanel::ChannelBox),
        "native sidebar toggle is refused during a Move"
    );
    assert_eq!(editor.status, OperationError::BusyManipulation.to_string());
}

#[test]
fn native_tool_switch_reports_no_scene_edit_status() {
    let (mut editor, _, _) = setup(1.0);
    editor.status = "sentinel".into();
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Select));
    assert_eq!(editor.active_tool, ActiveTool::Select);
    assert_eq!(editor.status, "sentinel");
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Select));
    assert_eq!(editor.status, "sentinel");
}

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[test]
fn read_only_ascii_curve_scene_refuses_move_natively_and_numerically_with_the_core_reason() {
    let scene = azteccraft::native_scene::ascii::decode(ascii_fixture::specimen().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        ..Editor::default()
    };
    let node = editor.history.scene().nodes()[0].id();
    editor.select(Some(node));
    let reason = editor
        .history
        .scene()
        .authoring_read_only_reason()
        .unwrap()
        .to_string();
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Move));
    assert_eq!(editor.active_tool, ActiveTool::Move);
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(!inspected.kinds[0].available);
    assert_eq!(
        inspected.kinds[0].unavailable_reason.as_deref(),
        Some(reason.as_str())
    );
    let before = editor.history.scene().clone();
    let target = editor.selected_move_target().unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X
        }),
        Err(OperationError::Scene(NativeError::ReadOnlyFormat))
    ));
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
    let context = egui::Context::default();
    let output = frame(
        &mut editor,
        &context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
    );
    let rect = viewport(&output);
    let shown = output.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        Shape::Text(text) if text.galley.job.text.contains(&format!("Move unavailable: {reason}")))
    });
    assert!(shown, "Move unavailable text with the read-only reason");
    tap(
        &mut editor,
        &context,
        rect.center() + egui::vec2(40.0, 0.0),
        vec![],
    );
    assert!(editor.history.active_manipulation().is_none() && editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn hover_and_active_painter_states_use_the_same_actual_axis_hit() {
    let (mut editor, context, rect) = setup(1.0);
    let shaft_color = |output: &egui::FullOutput| {
        output
            .shapes
            .iter()
            .find_map(|shape| {
                if let Shape::LineSegment { points, stroke } = &shape.shape
                    && points[0].distance(rect.center()) < 0.01
                    && points[1].x > rect.center().x + 30.0
                    && (points[1].y - rect.center().y).abs() < 0.01
                {
                    Some(stroke.color)
                } else {
                    None
                }
            })
            .unwrap()
    };
    let idle = frame(
        &mut editor,
        &context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
    );
    assert_eq!(shaft_color(&idle), egui::Color32::from_rgb(235, 75, 75));
    let point = rect.center() + egui::vec2(40.0, 0.0);
    let hover = frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(point)],
        ),
    );
    assert_eq!(shaft_color(&hover), egui::Color32::WHITE);
    assert!(editor.history.active_manipulation().is_none());
    let active = frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            button(point, true),
        ),
    );
    assert_eq!(shaft_color(&active), egui::Color32::from_rgb(255, 225, 80));
    assert_eq!(
        editor
            .history
            .active_manipulation()
            .unwrap()
            .move_axis()
            .unwrap(),
        WorldAxis::X
    );
}

#[test]
fn hover_and_press_use_the_same_eight_pixel_distance() {
    // 7 px off the X shaft both highlight and grab (red at 6 px); 9 px does neither.
    for (offset, grabbed) in [(7.0, true), (9.0, false)] {
        let (mut editor, context, rect) = setup(1.0);
        let point = rect.center() + egui::vec2(40.0, offset);
        let shaft_is_white = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, Shape::LineSegment { points, stroke }
                    if points[0].distance(rect.center()) < 0.01
                        && points[1].x > rect.center().x + 30.0
                        && (points[1].y - rect.center().y).abs() < 0.01
                        && stroke.color == egui::Color32::WHITE)
            })
        };
        let hover = frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                vec![Event::PointerMoved(point)],
            ),
        );
        assert_eq!(shaft_is_white(&hover), grabbed, "hover at {offset}");
        let inspected = editor.inspect_manipulation().unwrap();
        let pointer_hit = inspected.viewport.unwrap().pointer_hit;
        assert_eq!(
            pointer_hit,
            grabbed.then_some("x"),
            "inspection at {offset}"
        );
        frame(
            &mut editor,
            &context,
            raw(
                egui::vec2(1440.0, 900.0),
                Pos2::ZERO,
                1.0,
                button(point, true),
            ),
        );
        assert_eq!(
            editor.history.active_manipulation().is_some(),
            grabbed,
            "press at {offset}"
        );
    }
}

#[test]
fn actual_hidden_logic_cancels_native_pointer_capture() {
    let (mut editor, context, rect) = setup(1.0);
    let before = editor.history.scene().clone();
    let begin = press_x(&mut editor, &context, rect, 1.0);
    let end = begin + egui::vec2(rect.height() / 10.0, 0.0);
    frame(
        &mut editor,
        &context,
        raw(
            egui::vec2(1440.0, 900.0),
            Pos2::ZERO,
            1.0,
            vec![Event::PointerMoved(end)],
        ),
    );
    let mut input = raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]);
    input.focused = false;
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .minimized = Some(true);
    eframe::App::raw_input_hook(&mut editor, &context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let _ = context.run_logic(&input, |ctx| {
        eframe::App::logic(&mut editor, ctx, &mut native_frame)
    });
    assert!(editor.history.active_manipulation().is_none());
    assert!(editor.native_move.is_none());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.inspect_manipulation().unwrap().viewport.is_none());
}

#[test]
fn same_frame_name_or_channel_focus_blocks_qw_before_move_dispatch() {
    for field in [crate::app::name_edit_id(), crate::app::channel_edit_id(0)] {
        for shortcut in [Key::Q, Key::W] {
            let (mut editor, context, _) = setup(1.25);
            let (mut baseline, baseline_context, _) = setup(1.25);
            if shortcut == Key::W {
                for (editor, context) in
                    [(&mut editor, &context), (&mut baseline, &baseline_context)]
                {
                    frame(
                        editor,
                        context,
                        raw(
                            egui::vec2(1440.0, 900.0),
                            Pos2::ZERO,
                            1.25,
                            vec![key(Key::Q)],
                        ),
                    );
                    frame(
                        editor,
                        context,
                        raw(
                            egui::vec2(1440.0, 900.0),
                            Pos2::ZERO,
                            1.25,
                            vec![Event::Key {
                                key: Key::Q,
                                physical_key: None,
                                pressed: false,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            }],
                        ),
                    );
                }
            }
            let tool = editor.active_tool;
            assert_eq!(tool, baseline.active_tool);
            assert_eq!(context.memory(|memory| memory.focused()), None);
            assert_eq!(baseline_context.memory(|memory| memory.focused()), None);
            let position = context.read_response(field).unwrap().rect.center();
            assert_eq!(
                position,
                baseline_context.read_response(field).unwrap().rect.center()
            );
            let before = editor.history.scene().clone();
            let baseline_before = baseline.history.scene().clone();
            let token = editor.scene_token;
            let mut events = button(position, true);
            events.extend(button(position, false));
            let mut shortcut_events = events.clone();
            shortcut_events.push(key(shortcut));
            let text = if shortcut == Key::Q { "q" } else { "w" };
            events.push(Event::Text(text.into()));
            shortcut_events.push(Event::Text(text.into()));
            frame(
                &mut baseline,
                &baseline_context,
                raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.25, events),
            );
            frame(
                &mut editor,
                &context,
                raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.25, shortcut_events),
            );
            assert_eq!(context.memory(|memory| memory.focused()), Some(field));
            assert_eq!(
                baseline_context.memory(|memory| memory.focused()),
                Some(field)
            );
            assert_eq!(editor.active_tool, tool);
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(baseline.history.scene(), &baseline_before);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.name, baseline.name);
            assert_eq!(editor.channels, baseline.channels);
            assert_eq!(editor.name_intent, baseline.name_intent);
            assert_eq!(editor.channel_intents, baseline.channel_intents);
            assert_eq!(editor.has_draft(), baseline.has_draft());
            assert!(editor.history.active_manipulation().is_none());
        }
    }
}

#[test]
fn a_scene_refusal_mid_move_drag_keeps_the_last_valid_preview_and_other_errors_propagate() {
    let (mut editor, _, _) = setup(1.0);
    let node = editor.selected.unwrap();
    editor
        .apply_operation(Operation::EditNode {
            id: node,
            name: None,
            translation: [Some(1e8), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        })
        .unwrap();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginMove {
            target,
            axis: WorldAxis::X,
        })
        .unwrap();
    let id = editor.history.active_manipulation().unwrap().id;
    let update = |editor: &mut Editor, distance: f64| {
        editor.apply_operation(Operation::UpdateMove {
            id,
            distance: MoveDistance::Centimeters(distance),
        })
    };
    let updated = update(&mut editor, 1.0);
    assert!(updated.is_ok(), "{updated:?}");
    let valid = editor.history.scene().clone();
    // A step below the precision of the large translation is refused by the scene.
    let updated = update(&mut editor, 1e-3);
    assert!(matches!(
        updated,
        Err(OperationError::Move(ManipulationError::Native(_)))
    ));
    editor
        .finish_native_sample(id, ActiveTool::Move, false, updated)
        .unwrap();
    assert!(editor.error);
    assert!(
        editor.status.ends_with("; showing the last valid preview"),
        "{}",
        editor.status
    );
    assert_eq!(editor.history.scene(), &valid);
    assert!(editor.history.active_manipulation().is_some());
    // Anything but a scene refusal stops the drag handling.
    for error in [
        OperationError::Move(ManipulationError::StaleManipulation),
        OperationError::InvalidInput("x".into()),
    ] {
        assert!(
            editor
                .finish_native_sample(id, ActiveTool::Move, false, Err(error))
                .is_err()
        );
    }
    assert_eq!(editor.history.scene(), &valid);
}

fn focused_label(output: &egui::FullOutput) -> Option<String> {
    let update = output.platform_output.accesskit_update.as_ref()?;
    update
        .nodes
        .iter()
        .find(|(id, _)| *id == update.focus)
        .and_then(|(_, node)| node.label().map(str::to_owned))
}

#[test]
fn escape_closing_an_open_menu_leaves_the_move_in_flight_and_a_second_escape_cancels_it() {
    // A native pointer Move is already cancelled by an open popup, so the surviving case is a
    // remote-owned Move that a human Escape would otherwise cancel from behind the menu.
    let (mut editor, context, _) = setup(1.0);
    context.enable_accesskit();
    let before = editor.history.scene().clone();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation_guarded(
            ManipulationProvenance::Remote,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
            &|| true,
        )
        .unwrap();
    let size = egui::vec2(1440.0, 900.0);
    let step = |editor: &mut Editor, key_press: Key| {
        frame(
            editor,
            &context,
            raw(size, Pos2::ZERO, 1.0, vec![key(key_press)]),
        )
    };
    let mut reached = false;
    for _ in 0..60 {
        let output = step(&mut editor, Key::Tab);
        if focused_label(&output).as_deref() == Some("File") {
            reached = true;
            break;
        }
    }
    assert!(reached, "Tab reaches the File menu button");
    step(&mut editor, Key::Space);
    assert!(egui::Popup::is_any_open(&context), "File menu opens");
    assert!(editor.history.active_manipulation().is_some());
    let status = editor.status.clone();
    step(&mut editor, Key::Escape);
    assert!(
        !egui::Popup::is_any_open(&context),
        "Escape closes the menu"
    );
    assert!(
        editor.history.active_manipulation().is_some(),
        "Move survives menu Escape"
    );
    assert_eq!(editor.status, status);
    step(&mut editor, Key::Escape);
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.status, "Edit cancelled");
}

#[test]
fn editor_move_begins_on_a_sited_identity_node() {
    // The specimen with Maya-style zero padding and its keys shifted (t to r, r to s): the
    // transform has no translation leaf but the file records where one may go.
    let mut bytes = wire::specimen();
    let tree = azteccraft::maya_binary::container::inspect(bytes.clone().into()).unwrap();
    for chunk in tree.chunks() {
        bytes[chunk.padding_range()].fill(0);
    }
    for (from, to) in [(b't', b'r'), (b'r', b's')] {
        let start = tree
            .chunks()
            .iter()
            .find(|c| c.tag() == *b"DBL3" && tree.source()[c.payload_range()][0] == from)
            .unwrap()
            .payload_range()
            .start;
        bytes[start] = to;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sited.mb");
    std::fs::write(&path, &bytes).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let target = editor.history.scene().transform_paths().unwrap().remove(0);
    editor
        .apply_operation(Operation::Select {
            node: Some(target.node),
            instance_path: Some(target.path.clone()),
        })
        .unwrap();
    assert!(
        editor.history.scene().nodes()[0].transform().translation == [0.0; 3],
        "the translation leaf is absent"
    );
    let kinds = editor.inspect_manipulation().unwrap().kinds;
    let named = |name: &str| kinds.iter().find(|k| k.kind == name).unwrap();
    assert!(named("move").available, "{:?}", named("move"));
    assert_eq!(named("move").unavailable_reason, None);
    let started = editor
        .begin_editor_move(&target, WorldAxis::X, ManipulationProvenance::Native)
        .unwrap();
    assert!(matches!(
        started,
        OperationReply::ManipulationStarted { .. }
    ));
    assert!(editor.history.active_manipulation().is_some());
}

const MARQUEE_STATUS: &str = "Marquee (drag) selection is not implemented yet; click to select";

fn drag(editor: &mut Editor, context: &egui::Context, from: Pos2, to: Pos2) {
    let size = egui::vec2(1440.0, 900.0);
    frame(
        editor,
        context,
        raw(size, Pos2::ZERO, 1.0, button(from, true)),
    );
    frame(
        editor,
        context,
        raw(size, Pos2::ZERO, 1.0, vec![Event::PointerMoved(to)]),
    );
    frame(
        editor,
        context,
        raw(size, Pos2::ZERO, 1.0, button(to, false)),
    );
}

#[test]
fn drag_off_a_handle_shows_the_marquee_refusal_and_on_a_handle_does_not() {
    let (mut editor, context, rect) = setup(1.0);
    let from = rect.center() + egui::vec2(-300.0, 200.0);
    drag(&mut editor, &context, from, from + egui::vec2(60.0, 30.0));
    assert_eq!(editor.status, MARQUEE_STATUS);

    let (mut editor, context, rect) = setup(1.0);
    let node = editor.selected.unwrap();
    let from = rect.center() + egui::vec2(40.0, 0.0);
    drag(&mut editor, &context, from, from + egui::vec2(60.0, 0.0));
    assert_ne!(editor.status, MARQUEE_STATUS);
    assert!(editor.history.active_manipulation().is_none());
    let translation = editor
        .history
        .scene()
        .node(node)
        .unwrap()
        .transform()
        .translation;
    assert!(translation[0] > 0.0, "{translation:?}");
}

#[test]
fn face_mode_refusal_wins_over_marquee_for_a_manipulator_drag() {
    let (mut editor, context, rect) = setup(1.0);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    let from = rect.center() + egui::vec2(40.0, 0.0);
    drag(&mut editor, &context, from, from + egui::vec2(60.0, 0.0));
    assert_eq!(editor.status, FACE_MODE_REFUSAL);
}

fn drag_with(
    editor: &mut Editor,
    context: &egui::Context,
    from: Pos2,
    to: Pos2,
    modifiers: Modifiers,
) {
    let size = egui::vec2(1440.0, 900.0);
    let with = |mut events: Vec<Event>| {
        events.insert(0, Event::ModifiersChanged(modifiers));
        raw(size, Pos2::ZERO, 1.0, events)
    };
    let press = |position: Pos2, pressed: bool| Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    };
    frame(
        editor,
        context,
        with(vec![Event::PointerMoved(from), press(from, true)]),
    );
    frame(editor, context, with(vec![Event::PointerMoved(to)]));
    frame(
        editor,
        context,
        with(vec![Event::PointerMoved(to), press(to, false)]),
    );
}

fn select_tool_editor() -> (Editor, egui::Context, Rect) {
    let (mut editor, context, rect) = setup(1.0);
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Select));
    frame(
        &mut editor,
        &context,
        raw(egui::vec2(1440.0, 900.0), Pos2::ZERO, 1.0, vec![]),
    );
    (editor, context, rect)
}

#[test]
fn a_plain_drag_after_an_orbit_drag_still_shows_the_marquee_refusal() {
    let (mut editor, context, rect) = select_tool_editor();
    let from = rect.center() + egui::vec2(-300.0, 200.0);
    let to = from + egui::vec2(60.0, 30.0);
    drag_with(&mut editor, &context, from, to, Modifiers::ALT);
    assert_ne!(editor.status, MARQUEE_STATUS);
    drag_with(&mut editor, &context, from, to, Modifiers::NONE);
    assert_eq!(editor.status, MARQUEE_STATUS);
    assert!(!editor.error);
}

#[test]
fn a_marquee_drag_over_an_object_leaves_selection_and_history_untouched() {
    let (mut editor, context, rect) = select_tool_editor();
    let selected = editor.selected;
    assert!(selected.is_some());
    let dirty = editor.is_dirty();
    let undo = editor.history.undo_label();
    let redo = editor.history.can_redo();
    let from = rect.center();
    drag_with(
        &mut editor,
        &context,
        from,
        from + egui::vec2(60.0, 30.0),
        Modifiers::NONE,
    );
    assert_eq!(editor.status, MARQUEE_STATUS);
    assert_eq!(editor.selected, selected);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.undo_label(), undo);
    assert_eq!(editor.history.can_redo(), redo);
}
