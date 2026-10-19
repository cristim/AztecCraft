use super::*;
use crate::app::features::move_tool::tests::{button, frame, key, raw, viewport};
use azteccraft::{
    manipulator_handles::{DisabledKind, HandleFrame, RotateHandles},
    native_scene::{NativeError, ScaleUnsupported},
    scene::Primitive,
    viewport::Camera,
};
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Shape};
use glam::Vec3;
use uuid::Uuid;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

fn oblique() -> Camera {
    Camera::new(Vec3::ZERO, 0.6, 0.4, 10.0, 90_f32.to_radians(), 0.01, 100.0).unwrap()
}

fn key_up(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn moved(editor: &mut Editor, context: &egui::Context, scale: f32, to: Pos2) -> egui::FullOutput {
    frame(
        editor,
        context,
        raw(SIZE, Pos2::ZERO, scale, vec![Event::PointerMoved(to)]),
    )
}

fn idle(editor: &mut Editor, context: &egui::Context, scale: f32) -> egui::FullOutput {
    frame(editor, context, raw(SIZE, Pos2::ZERO, scale, vec![]))
}

// A cube at the origin seen obliquely, with the tool of `tool_key` active. The viewport rect is
// taken after the tool options row has appeared.
fn setup(tool_key: Key, scale: f32) -> (Editor, egui::Context, View) {
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: oblique(),
        ..Editor::default()
    };
    editor.create(Primitive::Cube);
    let context = egui::Context::default();
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, scale, vec![key(tool_key)]),
    );
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, scale, vec![key_up(tool_key)]),
    );
    let output = idle(&mut editor, &context, scale);
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&output),
        scale,
    };
    (editor, context, view)
}

#[derive(Clone)]
struct View {
    camera: Camera,
    aspect: f32,
    rect: Rect,
    scale: f32,
}

impl View {
    fn screen(&self, world: Vec3) -> Pos2 {
        let ndc = self.camera.project(world, self.aspect).unwrap().unwrap();
        Pos2::new(
            self.rect.left() + (ndc.x + 1.0) * self.rect.width() * 0.5,
            self.rect.top() + (1.0 - ndc.y) * self.rect.height() * 0.5,
        )
    }

    // World length of a 72 px handle at the origin: the 90 degree view spans 10 units up and down.
    fn radius(&self) -> f32 {
        72.0 / self.scale * 10.0 / (self.rect.height() * 0.5)
    }

    // The point of the ring about `axis` at `degrees` from the ring point nearest the eye.
    fn ring(&self, axis: Vec3, degrees: f32) -> Pos2 {
        let eye = self.camera.eye();
        let u = (eye - axis * eye.dot(axis)).normalize();
        let v = axis.cross(u);
        let angle = degrees.to_radians();
        self.screen((u * angle.cos() + v * angle.sin()) * self.radius())
    }
}

fn channels(editor: &Editor) -> [f64; 9] {
    let node = editor.selected.unwrap();
    editor
        .history
        .scene()
        .node(node)
        .unwrap()
        .transform()
        .values()
}

fn press(editor: &mut Editor, context: &egui::Context, scale: f32, at: Pos2) {
    frame(
        editor,
        context,
        raw(SIZE, Pos2::ZERO, scale, button(at, true)),
    );
}

fn release(editor: &mut Editor, context: &egui::Context, scale: f32, at: Pos2) {
    frame(
        editor,
        context,
        raw(SIZE, Pos2::ZERO, scale, button(at, false)),
    );
}

// Counts the undo entries by walking back and forward again.
fn undo_depth(editor: &mut Editor) -> usize {
    let mut depth = 0;
    while editor.history.undo() {
        depth += 1;
    }
    for _ in 0..depth {
        assert!(editor.history.redo());
    }
    depth
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected} within {tolerance}"
    );
}

#[test]
fn e_and_r_select_the_tools_and_the_tool_box_icons_are_live() {
    let (mut editor, context, _) = setup(Key::E, 1.0);
    assert_eq!(editor.active_tool, ActiveTool::Rotate);
    assert!(
        editor.status.starts_with("Rotate tool (Object)"),
        "{}",
        editor.status
    );
    assert!(!editor.error);
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::R)]),
    );
    assert_eq!(editor.active_tool, ActiveTool::Scale);
    assert!(
        editor.status.starts_with("Scale tool (World)"),
        "{}",
        editor.status
    );
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::W)]),
    );
    assert_eq!(editor.active_tool, ActiveTool::Move);
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::Q)]),
    );
    assert_eq!(editor.active_tool, ActiveTool::Select);
}

#[test]
fn a_ring_drag_turns_about_the_picked_axis_and_is_one_rotate_history_entry() {
    for scale in [1.0, 2.0] {
        let (mut editor, context, view) = setup(Key::E, scale);
        let before = editor.history.scene().clone();
        let y = Vec3::Y;
        let begin = view.ring(y, 0.0);
        press(&mut editor, &context, scale, begin);
        assert_eq!(
            editor.history.active_manipulation().unwrap().kind,
            ManipulationKindState::Rotate {
                axis: RotateAxis::Y,
                space: azteccraft::native_scene::RotateSpace::Object
            }
        );
        assert_eq!(editor.status, "Object Y Rotate");
        for degrees in [10.0_f32, 25.0, -20.0, 40.0] {
            moved(&mut editor, &context, scale, view.ring(y, degrees));
            near(channels(&editor)[4], f64::from(degrees), 0.05);
            assert_eq!(channels(&editor)[3], 0.0);
            assert!(
                editor.status.starts_with("Rotate preview"),
                "{}",
                editor.status
            );
            assert!(!editor.error);
        }
        assert!(!editor.history.can_undo() || editor.history.undo_label() == Some("Create"));
        release(&mut editor, &context, scale, view.ring(y, 40.0));
        assert!(editor.history.active_manipulation().is_none());
        assert!(editor.native_move.is_none());
        assert_eq!(editor.status, "Rotate committed");
        near(channels(&editor)[4], 40.0, 0.05);
        let after = editor.history.scene().clone();
        assert_eq!(editor.history.undo_label(), Some("Rotate"));
        editor.native_operation(Operation::Undo);
        assert_eq!(editor.status, "Undo Rotate");
        assert_eq!(channels(&editor)[4], 0.0);
        editor.native_operation(Operation::Redo);
        assert_eq!(editor.status, "Redo Rotate");
        assert_eq!(editor.history.scene(), &after);
        editor.native_operation(Operation::Undo);
        assert_eq!(editor.history.scene(), &before);
    }
}

#[test]
fn world_space_rings_turn_the_selected_object_about_the_world_axis() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    editor
        .set_tool_space(ActiveTool::Rotate, ManipulationSpace::World)
        .unwrap();
    let x = Vec3::X;
    press(&mut editor, &context, 1.0, view.ring(x, 0.0));
    assert_eq!(editor.status, "World X Rotate");
    moved(&mut editor, &context, 1.0, view.ring(x, 30.0));
    near(channels(&editor)[3], 30.0, 0.05);
    release(&mut editor, &context, 1.0, view.ring(x, 30.0));
    assert_eq!(editor.history.undo_label(), Some("Rotate"));
}

#[test]
fn an_axis_box_drag_scales_by_the_parameter_ratio_in_one_scale_history_entry() {
    let (mut editor, context, view) = setup(Key::R, 1.0);
    let radius = view.radius();
    let begin = view.screen(Vec3::X * radius);
    press(&mut editor, &context, 1.0, begin);
    assert_eq!(editor.status, "World X Scale");
    for factor in [1.5_f32, 2.0, 0.5] {
        moved(
            &mut editor,
            &context,
            1.0,
            view.screen(Vec3::X * radius * factor),
        );
        near(channels(&editor)[6], f64::from(factor), 1e-3);
        assert_eq!(channels(&editor)[7], 1.0);
        assert!(
            editor.status.starts_with("Scale preview"),
            "{}",
            editor.status
        );
    }
    release(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * radius * 0.5),
    );
    assert_eq!(editor.status, "Scale committed");
    assert_eq!(editor.history.undo_label(), Some("Scale"));
    editor.native_operation(Operation::Undo);
    assert_eq!(channels(&editor)[6], 1.0);
}

#[test]
fn a_drag_through_the_pivot_shows_the_scale_refusal_and_keeps_the_last_valid_preview() {
    let (mut editor, context, view) = setup(Key::R, 1.0);
    let radius = view.radius();
    press(&mut editor, &context, 1.0, view.screen(Vec3::X * radius));
    moved(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * radius * 2.0),
    );
    near(channels(&editor)[6], 2.0, 1e-3);
    // Zero and negative factors are refused by the core; the refusal is shown, not clamped.
    for factor in [-0.5_f32, -1.0, -3.0] {
        let output = moved(
            &mut editor,
            &context,
            1.0,
            view.screen(Vec3::X * radius * factor),
        );
        assert!(editor.error, "{factor}");
        assert!(
            editor
                .status
                .contains("scale factors must be finite and greater than zero"),
            "{}",
            editor.status
        );
        assert!(
            editor.status.ends_with("; showing the last valid preview"),
            "{}",
            editor.status
        );
        near(channels(&editor)[6], 2.0, 1e-3);
        // The refused sample does not blank the viewport.
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, Shape::Callback(_)))
        );
    }
    assert!(editor.history.active_manipulation().is_some());
    moved(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * radius * 3.0),
    );
    assert!(!editor.error);
    near(channels(&editor)[6], 3.0, 1e-3);
    moved(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * radius * -2.0),
    );
    release(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * radius * -2.0),
    );
    assert!(editor.history.active_manipulation().is_none());
    near(channels(&editor)[6], 3.0, 1e-3);
    assert!(editor.error);
    assert!(
        editor.status.ends_with("last valid preview committed"),
        "{}",
        editor.status
    );
    assert_eq!(editor.history.undo_label(), Some("Scale"));
}

#[test]
fn uniform_scale_from_the_center_box_edits_all_three_channels() {
    let (mut editor, context, view) = setup(Key::R, 1.0);
    let origin = view.screen(Vec3::ZERO);
    press(&mut editor, &context, 1.0, origin);
    assert_eq!(editor.status, "Uniform Scale");
    moved(&mut editor, &context, 1.0, origin + egui::vec2(100.0, 0.0));
    let values = channels(&editor);
    near(values[6], 2.0, 1e-6);
    assert_eq!(values[6..], [values[6]; 3]);
    release(&mut editor, &context, 1.0, origin + egui::vec2(100.0, 0.0));
    assert_eq!(editor.history.undo_label(), Some("Scale"));
}

#[test]
fn hover_status_names_pickable_refused_and_unsupported_handles_and_restores_the_help() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let help = editor.status.clone();
    let on_ring = view.ring(Vec3::Y, 0.0);
    moved(&mut editor, &context, 1.0, on_ring);
    assert_eq!(editor.status, "Object Y Rotate: drag to turn");
    assert!(!editor.error);
    moved(
        &mut editor,
        &context,
        1.0,
        view.rect.right_bottom() - egui::vec2(2.0, 2.0),
    );
    assert_eq!(editor.status, help);
    // The view ring lies outside the axis rings and is drawn but unsupported.
    let origin = view.screen(Vec3::ZERO);
    moved(
        &mut editor,
        &context,
        1.0,
        origin + egui::vec2(72.0 * 1.2, 0.0),
    );
    assert_eq!(editor.status, DisabledKind::ViewRing.to_string());
    assert!(editor.error);
    // Unrelated status text is not overwritten when the pointer leaves.
    editor.status = "sentinel".into();
    moved(
        &mut editor,
        &context,
        1.0,
        view.rect.right_bottom() - egui::vec2(2.0, 2.0),
    );
    assert_eq!(editor.status, "sentinel");
}

#[test]
fn pressing_an_unsupported_handle_is_refused_and_consumed_without_selecting() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let origin = view.screen(Vec3::ZERO);
    let before = editor.history.scene().clone();
    let on_view_ring = origin + egui::vec2(72.0 * 1.2, 0.0);
    press(&mut editor, &context, 1.0, on_view_ring);
    assert_eq!(editor.status, DisabledKind::ViewRing.to_string());
    assert!(editor.error);
    assert!(editor.history.active_manipulation().is_none());
    release(&mut editor, &context, 1.0, on_view_ring);
    assert!(editor.selected.is_some(), "the click must not deselect");
    assert_eq!(editor.history.scene(), &before);
}

#[test]
fn escape_cancels_a_native_rotate_and_restores_the_exact_baseline() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let before = editor.history.scene().clone();
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 30.0));
    assert_ne!(editor.history.scene(), &before);
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::Escape)]),
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    assert_eq!(editor.status, "Edit cancelled");
}

#[test]
fn the_channel_box_follows_the_live_preview() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 30.0));
    let shown: f64 = editor.channels[4].parse().unwrap();
    near(shown, 30.0, 0.05);
    assert!(!editor.has_draft());
}

#[test]
fn remote_operations_are_busy_during_a_native_rotate() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    let id = editor.history.active_manipulation().unwrap().id;
    let token = editor.scene_token;
    for operation in [
        Operation::UpdateManipulation {
            id,
            value: ManipulationValue::Degrees(5.0),
        },
        Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        },
    ] {
        let result = editor.remote_operation(
            crate::editor_operations::RemoteRequest {
                expected_scene_token: Some(token),
                operation,
            },
            &context,
        );
        assert!(
            matches!(result, Err(OperationError::BusyManipulation)),
            "native-owned manipulation must stay busy"
        );
    }
}

#[test]
fn a_value_of_the_wrong_kind_or_not_finite_is_refused_without_changing_the_preview() {
    let (mut editor, _, _) = setup(Key::E, 1.0);
    let target = editor.selected_move_target().unwrap();
    let scene = editor.history.scene().clone();
    editor
        .apply_operation(Operation::BeginRotate {
            target,
            axis: RotateAxis::Z,
            space: ManipulationSpace::Object,
        })
        .unwrap();
    let id = editor.history.active_manipulation().unwrap().id;
    assert!(matches!(
        editor.apply_operation(Operation::UpdateManipulation {
            id,
            value: ManipulationValue::Factor(2.0)
        }),
        Err(OperationError::KindMismatch {
            parameter: "factor",
            active: "rotate"
        })
    ));
    assert_eq!(editor.history.scene(), &scene);
    for value in [
        ManipulationValue::Degrees(f64::NAN),
        ManipulationValue::Degrees(f64::INFINITY),
    ] {
        assert!(matches!(
            editor.apply_operation(Operation::UpdateManipulation { id, value }),
            Err(OperationError::InvalidInput(_))
        ));
        assert_eq!(editor.history.scene(), &scene);
    }
    assert!(matches!(
        editor.apply_operation(Operation::UpdateMove {
            id,
            distance: crate::editor_operations::MoveDistance::Centimeters(1.0)
        }),
        Err(OperationError::KindMismatch {
            parameter: "distance",
            active: "rotate"
        })
    ));
    editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Cancel,
        })
        .unwrap();
    assert_eq!(editor.history.scene(), &scene);
}

#[test]
fn numeric_entry_applies_one_labelled_edit_and_shows_core_refusals() {
    let (mut editor, _, _) = setup(Key::E, 1.0);
    editor.manipulation.rotate_axis = RotateAxis::Z;
    editor.manipulation.numeric = "45".into();
    editor.apply_numeric_manipulation();
    assert!(!editor.error, "{}", editor.status);
    assert_eq!(editor.status, "Rotate applied");
    assert!(editor.manipulation.numeric.is_empty());
    near(channels(&editor)[5], 45.0, 1e-9);
    assert_eq!(editor.history.undo_label(), Some("Rotate"));
    editor.native_operation(Operation::Undo);
    assert_eq!(channels(&editor)[5], 0.0);

    editor.native_operation(Operation::SetActiveTool(ActiveTool::Scale));
    editor.manipulation.scale_handle = ScaleHandle::Uniform;
    editor.manipulation.numeric = "2".into();
    editor.apply_numeric_manipulation();
    assert_eq!(editor.status, "Scale applied");
    assert_eq!(channels(&editor)[6..], [2.0; 3]);
    assert_eq!(editor.history.undo_label(), Some("Scale"));
    let scene = editor.history.scene().clone();
    let entries = undo_depth(&mut editor);
    for refused in ["0", "-1"] {
        editor.manipulation.numeric = refused.into();
        editor.apply_numeric_manipulation();
        assert!(editor.error);
        assert!(
            editor
                .status
                .contains("scale factors must be finite and greater than zero"),
            "{}",
            editor.status
        );
        assert_eq!(editor.manipulation.numeric, refused, "kept for correction");
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(undo_depth(&mut editor), entries, "{refused} left an entry");
    }
    for garbage in ["", "abc", "inf", "NaN"] {
        editor.manipulation.numeric = garbage.into();
        editor.apply_numeric_manipulation();
        assert!(editor.error, "{garbage}");
        assert_eq!(editor.history.scene(), &scene);
    }
}

#[test]
fn numeric_entry_through_the_text_field_commits_on_enter_and_blocks_remote_edits_while_focused() {
    let (mut editor, context, _) = setup(Key::E, 1.0);
    editor.manipulation.rotate_axis = RotateAxis::X;
    context.memory_mut(|memory| memory.request_focus(manipulation_value_id()));
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![Event::Text("30".into())]),
    );
    assert_eq!(editor.manipulation.numeric, "30");
    assert!(crate::app::text_edit_focused(&context));
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![key(Key::Enter)]),
    );
    near(channels(&editor)[3], 30.0, 1e-9);
    assert_eq!(editor.history.undo_label(), Some("Rotate"));
}

#[test]
fn unsupported_spaces_are_listed_disabled_with_the_reason() {
    let (mut editor, context, _) = setup(Key::E, 1.0);
    context.enable_accesskit();
    // Open the selector's popup and read the entries.
    let output = idle(&mut editor, &context, 1.0);
    let combo = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::accesskit::Role::ComboBox && node.value() == Some("Object"))
                .then(|| node.bounds().unwrap())
        })
        .expect("the space selector");
    let at = Pos2::new(
        ((combo.x0 + combo.x1) / 2.0) as f32,
        ((combo.y0 + combo.y1) / 2.0) as f32,
    );
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, button(at, true)),
    );
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, button(at, false)),
    );
    let output = idle(&mut editor, &context, 1.0);
    let nodes = &output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes;
    for name in UNSUPPORTED_SPACES {
        let disabled = nodes
            .iter()
            .any(|(_, node)| node.label() == Some(name) && node.is_disabled());
        assert!(disabled, "{name} must be listed and disabled");
    }
    for name in ["Object", "World"] {
        assert!(
            nodes
                .iter()
                .any(|(_, node)| node.label() == Some(name) && !node.is_disabled())
        );
    }
}

#[test]
fn face_vertex_and_edge_selection_refuse_rotate_and_scale_handles() {
    for (mode, noun) in [
        (SelectionMode::Face, "Face"),
        (SelectionMode::Vertex, "Vertex"),
        (SelectionMode::Edge, "Edge"),
    ] {
        for (tool_key, name) in [(Key::E, "Rotate"), (Key::R, "Scale")] {
            let (mut editor, context, view) = setup(tool_key, 1.0);
            editor.selection_mode = mode;
            let before = editor.history.scene().clone();
            press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
            assert!(editor.history.active_manipulation().is_none());
            assert_eq!(
                editor.status,
                format!("{name} is unavailable in {noun} selection; switch to Object selection")
            );
            assert_eq!(editor.history.scene(), &before);
        }
    }
}

#[test]
fn inspection_names_each_kind_in_its_selection_mode_refusal() {
    let (mut editor, _, _) = setup(Key::E, 1.0);
    editor.selection_mode = SelectionMode::Face;
    let inspected = editor.inspect_manipulation().unwrap();
    for (kind, name) in [("move", "Move"), ("rotate", "Rotate"), ("scale", "Scale")] {
        let entry = inspected.kinds.iter().find(|k| k.kind == kind).unwrap();
        assert!(!entry.available);
        assert_eq!(
            entry.unavailable_reason.as_deref(),
            Some(
                format!("{name} is unavailable in Face selection; switch to Object selection")
                    .as_str()
            )
        );
    }
}

#[test]
fn a_tool_switch_mid_drag_cancels_with_the_tool_name() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Select));
    assert!(editor.history.active_manipulation().is_none());
    assert!(editor.native_move.is_none());
    assert_eq!(channels(&editor)[4], 0.0);
}

#[test]
fn a_negative_scale_parent_refuses_object_space_handles_with_a_visible_reason() {
    let scene = azteccraft::native_scene::ascii::decode(
        concat!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; ",
            "currentUnit -l centimeter -a degree -t film;\n",
            "createNode transform -n \"a\"; setAttr \".s\" -type \"double3\" -1 1 1;\n",
            "createNode transform -n \"b\" -p \"a\";\n"
        )
        .as_bytes()
        .to_vec()
        .into(),
    )
    .unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: oblique(),
        ..Editor::default()
    };
    let child = editor
        .history
        .scene()
        .nodes()
        .iter()
        .find(|node| node.name() == "b")
        .unwrap()
        .id();
    editor.select(Some(child));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Rotate));
    let context = egui::Context::default();
    let output = idle(&mut editor, &context, 1.0);
    let rect = viewport(&output);
    let shown = output.shapes.iter().any(|shape| {
        matches!(&shape.shape,
        Shape::Text(text) if text.galley.job.text.contains("Rotate unavailable: Handle axes are mirrored"))
    });
    assert!(shown, "the overlay names the mirrored frame");
    press(&mut editor, &context, 1.0, rect.center());
    assert!(
        editor.status.contains("Handle axes are mirrored"),
        "{}",
        editor.status
    );
    assert!(editor.error);
    // World space has no mirrored axes, so the handles return (the core then refuses the edit
    // because the scene is read-only).
    editor
        .set_tool_space(ActiveTool::Rotate, ManipulationSpace::World)
        .unwrap();
    let frame_ok = editor.manipulation_frame(ActiveTool::Rotate).is_ok();
    assert!(frame_ok);
}

#[test]
fn handle_texts_for_rotate_and_scale_do_not_say_move() {
    for error in [
        HandleError::InvalidCapture,
        HandleError::CaptureChanged,
        HandleError::NonFinitePointer,
        HandleError::InvalidProjection,
        HandleError::OriginClipped,
        HandleError::ProjectedAxisDegenerate,
        HandleError::HandleClipped,
        HandleError::ParallelAxis,
        HandleError::BehindRay,
        HandleError::InvalidSolve,
        HandleError::Degenerate,
        HandleError::ZeroBeginParameter,
        HandleError::MirroredFrame,
        HandleError::SkewedFrame,
    ] {
        for tool in ["Rotate", "Scale"] {
            let text = error.describe_for(tool);
            assert!(!text.contains("Move"), "{text}");
        }
        assert_eq!(error.describe_for("Move"), error.to_string());
    }
}

#[test]
fn the_scale_refusal_text_is_the_cores() {
    assert_eq!(
        NativeError::UnsupportedScale(ScaleUnsupported::InvalidFactor).to_string(),
        "Scale unsupported: scale factors must be finite and greater than zero"
    );
    let _ = (HandleFrame::world, RotateHandles::new);
}

#[test]
fn a_committed_message_survives_the_pointer_resting_on_the_handle() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let begin = view.ring(Vec3::Y, 0.0);
    press(&mut editor, &context, 1.0, begin);
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 20.0));
    release(&mut editor, &context, 1.0, view.ring(Vec3::Y, 20.0));
    for _ in 0..3 {
        idle(&mut editor, &context, 1.0);
    }
    assert_eq!(editor.status, "Rotate committed");
}

fn begin_operation(editor: &Editor, kind: ActiveTool, space: ManipulationSpace) -> Operation {
    let target = editor.selected_move_target().unwrap();
    match kind {
        ActiveTool::Rotate => Operation::BeginRotate {
            target,
            axis: RotateAxis::Z,
            space,
        },
        _ => Operation::BeginScale {
            target,
            handle: ScaleHandle::Uniform,
            space,
        },
    }
}

fn update_operation(kind: ActiveTool, id: ManipulationId) -> Operation {
    Operation::UpdateManipulation {
        id,
        value: match kind {
            ActiveTool::Rotate => ManipulationValue::Angle(15.0),
            _ => ManipulationValue::Factor(1.5),
        },
    }
}

#[test]
fn editor_rotate_and_scale_guards_preserve_drafts_modal_close_tokens_and_failed_open_baseline() {
    use crate::editor_operations::{FileDecision, RemoteRequest};
    for kind in [ActiveTool::Rotate, ActiveTool::Scale] {
        for condition in 0..6 {
            for operation in 0..4 {
                let mut editor = Editor::default();
                editor.create(Primitive::Cube);
                let context = egui::Context::default();
                let token = editor.scene_token;
                let before = editor.history.scene().clone();
                let begin = begin_operation(&editor, kind, ManipulationSpace::Object);
                match condition {
                    0 => editor.channels[0] = "human".into(),
                    1 => context
                        .memory_mut(|memory| memory.request_focus(crate::app::name_edit_id())),
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
                let id = ManipulationId::from_decimal("1").unwrap();
                let request = match operation {
                    0 => Operation::SetActiveTool(kind),
                    1 => begin,
                    2 => update_operation(kind, id),
                    _ => Operation::FinishMove {
                        id,
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
                assert!(
                    matches!(
                        reply,
                        Err(OperationError::BusyEditing | OperationError::StaleScene)
                    ),
                    "{kind:?} {condition} {operation}"
                );
                assert_eq!(editor.history.scene(), &before);
                assert!(editor.history.active_manipulation().is_none());
                assert_eq!(editor.active_tool, ActiveTool::Select);
            }
        }
    }
}

#[test]
fn rotate_and_scale_owner_busy_cancel_and_close_follow_move() {
    for kind in [ActiveTool::Rotate, ActiveTool::Scale] {
        let (mut editor, context, _) = setup(
            if kind == ActiveTool::Rotate {
                Key::E
            } else {
                Key::R
            },
            1.0,
        );
        let before = editor.history.scene().clone();
        let bytes = azteccraft::native_scene::binary::encode(&before);
        let token = editor.scene_token;
        let camera = editor.camera.clone();

        // A native manipulation refuses every remote step, including its own update and finish.
        let begin = begin_operation(&editor, kind, ManipulationSpace::Object);
        editor.apply_operation(begin).unwrap();
        let id = editor.history.active_manipulation().unwrap().id;
        for operation in [
            update_operation(kind, id),
            Operation::FinishMove {
                id,
                finish: ManipulationFinish::Commit,
            },
            Operation::Undo,
        ] {
            let reply = editor.remote_operation(
                crate::editor_operations::RemoteRequest {
                    expected_scene_token: Some(token),
                    operation,
                },
                &context,
            );
            assert!(matches!(reply, Err(OperationError::BusyManipulation)));
        }
        editor
            .apply_operation(Operation::FinishMove {
                id,
                finish: ManipulationFinish::Cancel,
            })
            .unwrap();
        assert_eq!(editor.history.scene(), &before);

        // A remote manipulation rejects a native step as another owner's, and a second begin.
        let begin = begin_operation(&editor, kind, ManipulationSpace::Object);
        editor
            .apply_operation_guarded(ManipulationProvenance::Remote, begin, &|| true)
            .unwrap();
        let id = editor.history.active_manipulation().unwrap().id;
        assert!(matches!(
            editor.apply_operation(update_operation(kind, id)),
            Err(OperationError::Move(ManipulationError::WrongOwner))
        ));
        assert!(matches!(
            editor.apply_operation_guarded(
                ManipulationProvenance::Remote,
                begin_operation(&editor, kind, ManipulationSpace::World),
                &|| true
            ),
            Err(OperationError::BusyManipulation)
        ));
        editor
            .apply_operation_guarded(
                ManipulationProvenance::Remote,
                update_operation(kind, id),
                &|| true,
            )
            .unwrap();
        assert_ne!(editor.history.scene(), &before);

        // A failed Open cancels the remote manipulation and restores its exact baseline.
        let missing = tempfile::tempdir().unwrap();
        assert!(
            editor
                .apply_operation(Operation::OpenScene {
                    path: missing.path().join("missing-rotate-scale.mb")
                })
                .is_err()
        );
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.camera, camera);
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(
            azteccraft::native_scene::binary::encode(editor.history.scene()),
            bytes
        );
        assert_eq!(editor.history.undo_label(), Some("Create"));
        assert!(!editor.history.can_redo());
    }
}

#[test]
fn scale_handles_sit_at_the_scale_fixed_point_and_rotate_handles_at_the_pivot() {
    let scene = azteccraft::native_scene::ascii::decode(
        concat!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; ",
            "currentUnit -l centimeter -a degree -t film;\n",
            "createNode transform -n \"a\"; setAttr \".t\" -type \"double3\" 1 0 0; ",
            "setAttr \".r\" -type \"double3\" 0 0 90; ",
            "setAttr \".rp\" -type \"double3\" 1 0 0; ",
            "setAttr \".rpt\" -type \"double3\" 0 3 0; ",
            "setAttr \".spt\" -type \"double3\" 2 0 0;\n"
        )
        .as_bytes()
        .to_vec()
        .into(),
    )
    .unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        ..Editor::default()
    };
    let node = editor.history.scene().nodes()[0].id();
    editor.select(Some(node));
    let target = editor.selected_move_target().unwrap();
    let core = editor.history.scene().manipulator_frame(&target).unwrap();
    assert!((core.scale_origin_cm - core.origin_cm).length() > 1.0);
    let (_, rotate) = editor.manipulation_frame(ActiveTool::Rotate).unwrap();
    let (_, scale) = editor.manipulation_frame(ActiveTool::Scale).unwrap();
    assert_eq!(rotate.origin(), core.origin_cm);
    assert_eq!(scale.origin(), core.scale_origin_cm);
}

// Maya 2026 Scale Tool settings: Axis Orientation "World (default)". Rotate Tool settings:
// "Object (default)". Maya keeps these per tool.
#[test]
fn fresh_tools_start_in_their_documented_spaces() {
    let editor = Editor::default();
    assert_eq!(
        editor.manipulation.space(ActiveTool::Scale),
        ManipulationSpace::World
    );
    assert_eq!(
        editor.manipulation.space(ActiveTool::Rotate),
        ManipulationSpace::Object
    );
}

#[test]
fn changing_one_tools_space_does_not_leak_into_the_other() {
    let mut editor = Editor::default();
    editor
        .set_tool_space(ActiveTool::Scale, ManipulationSpace::Object)
        .unwrap();
    assert_eq!(
        editor.manipulation.space(ActiveTool::Rotate),
        ManipulationSpace::Object
    );
    editor
        .set_tool_space(ActiveTool::Rotate, ManipulationSpace::World)
        .unwrap();
    assert_eq!(
        editor.manipulation.space(ActiveTool::Scale),
        ManipulationSpace::Object
    );
    assert_eq!(
        editor.manipulation.space(ActiveTool::Rotate),
        ManipulationSpace::World
    );
}

#[test]
fn numeric_scale_of_a_fresh_editor_applies_in_world_space() {
    let (mut editor, _, _) = setup(Key::E, 1.0);
    editor.manipulation.rotate_axis = RotateAxis::Z;
    editor.manipulation.numeric = "90".into();
    editor.apply_numeric_manipulation();
    assert!(!editor.error, "{}", editor.status);
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Scale));
    editor.manipulation.scale_handle = ScaleHandle::X;
    editor.manipulation.numeric = "2".into();
    editor.apply_numeric_manipulation();
    assert!(!editor.error, "{}", editor.status);
    // World X of an object turned 90 degrees about Z is its local Y.
    let scale = channels(&editor)[6..].to_vec();
    near(scale[0], 1.0, 1e-9);
    near(scale[1], 2.0, 1e-9);
    near(scale[2], 1.0, 1e-9);
}
