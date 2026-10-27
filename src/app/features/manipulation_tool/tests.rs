use super::*;
use crate::app::features::move_tool::tests::{
    View, button, frame, key, oblique, open_file_menu, raw, scale_plane_point, viewport,
};
use azteccraft::{
    manipulator_handles::{DisabledKind, HandleFrame, RotateHandles},
    move_handles::ManipulatorSize,
    native_scene::{NativeError, ScaleUnsupported},
    scene::Primitive,
};
use eframe::egui::{self, Event, Key, Modifiers, Pos2, Shape};
use glam::Vec3;
use uuid::Uuid;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

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
    setup_with_camera(tool_key, scale, oblique())
}

fn setup_with_camera(
    tool_key: Key,
    scale: f32,
    camera: azteccraft::viewport::Camera,
) -> (Editor, egui::Context, View) {
    let mut editor = Editor {
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera,
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
fn rotate_hover_and_press_use_the_same_eight_pixel_distance() {
    // 7 px off the Y ring both highlights and begins the drag (red at 6 px); 9 px neither.
    for (offset, grabbed) in [(7.0_f32, true), (9.0, false)] {
        let (mut editor, context, view) = setup(Key::E, 1.0);
        let on_ring = view.ring(Vec3::Y, 0.0);
        let along = (view.ring(Vec3::Y, 1.0) - view.ring(Vec3::Y, -1.0)).normalized();
        let point = on_ring + egui::vec2(-along.y, along.x) * offset;
        let output = moved(&mut editor, &context, 1.0, point);
        let highlighted = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, Shape::LineSegment { stroke, .. }
                if stroke.color == egui::Color32::WHITE)
        });
        assert_eq!(highlighted, grabbed, "hover painter at {offset}");
        press(&mut editor, &context, 1.0, point);
        assert_eq!(
            editor.history.active_manipulation().is_some(),
            grabbed,
            "press at {offset}"
        );
    }
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
fn scale_hover_and_press_use_the_same_eight_pixel_distance() {
    // 7 px off the X shaft both names the handle and begins the drag (red at 6 px); 9 px neither.
    for (offset, grabbed) in [(7.0_f32, true), (9.0, false)] {
        let (mut editor, context, view) = setup(Key::R, 1.0);
        let origin = view.screen(Vec3::ZERO);
        let tip = view.screen(Vec3::X * view.radius());
        let along = (tip - origin).normalized();
        let normal = egui::vec2(-along.y, along.x);
        let point = origin + (tip - origin) * 0.6 + normal * offset;
        let help = editor.status.clone();
        moved(&mut editor, &context, 1.0, point);
        if grabbed {
            assert_eq!(editor.status, "World X Scale: drag to scale");
        } else {
            assert_eq!(editor.status, help, "hover at {offset}");
        }
        press(&mut editor, &context, 1.0, point);
        assert_eq!(
            editor.history.active_manipulation().is_some(),
            grabbed,
            "press at {offset}"
        );
    }
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
fn move_lists_maya_axis_orientations_with_only_world_enabled() {
    let (mut editor, context, _) = setup(Key::W, 1.0);
    assert_eq!(editor.active_tool, ActiveTool::Move);
    context.enable_accesskit();
    let output = idle(&mut editor, &context, 1.0);
    let combo = output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.role() == egui::accesskit::Role::ComboBox && node.value() == Some("World"))
                .then(|| node.bounds().unwrap())
        })
        .expect("the Move orientation selector");
    let at = Pos2::new(
        ((combo.x0 + combo.x1) / 2.0) as f32,
        ((combo.y0 + combo.y1) / 2.0) as f32,
    );
    let before = (
        editor.history.scene().clone(),
        editor.active_tool,
        editor.is_dirty(),
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
    for name in MOVE_ORIENTATIONS {
        let found: Vec<bool> = nodes
            .iter()
            .filter(|(_, node)| node.label() == Some(name))
            .map(|(_, node)| node.is_disabled())
            .collect();
        assert_eq!(found, [name != "World"], "{name}");
    }
    assert_eq!(
        (
            editor.history.scene().clone(),
            editor.active_tool,
            editor.is_dirty()
        ),
        before
    );
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
fn rotate_and_scale_target_reads_keep_exact_ownership_errors_and_mode_precedence() {
    let state = |editor: &Editor| {
        (
            (
                editor.history.scene().clone(),
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.remembered_vertex.clone(),
                editor.remembered_edge.clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.active_tool,
            ),
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.history.undo_label(),
                editor.history.redo_label(),
                editor.history.active_manipulation().is_none(),
            ),
        )
    };
    for (kind, key, tool) in [
        (ActiveTool::Rotate, Key::E, "Rotate"),
        (ActiveTool::Scale, Key::R, "Scale"),
    ] {
        let (mut editor, _, _) = setup(key, 1.0);
        let before = state(&editor);
        let begin = begin_operation(&editor, kind, ManipulationSpace::Object);
        editor.apply_operation(begin).unwrap();
        let id = editor.history.active_manipulation().unwrap().id;
        editor
            .apply_operation(Operation::FinishMove {
                id,
                finish: ManipulationFinish::Cancel,
            })
            .unwrap();
        assert_eq!(state(&editor), before);
        for (mode, noun) in [
            (SelectionMode::Object, None),
            (SelectionMode::Face, Some("Face")),
            (SelectionMode::Vertex, Some("Vertex")),
            (SelectionMode::Edge, Some("Edge")),
        ] {
            for condition in 0..3 {
                let (mut editor, _, _) = setup(key, 1.0);
                let original = editor.selected_move_target().unwrap();
                editor.create(Primitive::Cube);
                let foreign = editor.selected_move_target().unwrap();
                editor
                    .apply_operation(Operation::Select {
                        node: Some(original.node),
                        instance_path: Some(original.path),
                    })
                    .unwrap();
                let mut begin = begin_operation(&editor, kind, ManipulationSpace::Object);
                let (Operation::BeginRotate { target, .. } | Operation::BeginScale { target, .. }) =
                    &mut begin
                else {
                    panic!("expected rotate or scale begin")
                };
                match condition {
                    0 => *target = foreign,
                    1 => target.path.clear(),
                    _ => {
                        editor
                            .apply_operation(Operation::Select {
                                node: None,
                                instance_path: None,
                            })
                            .unwrap();
                    }
                }
                editor
                    .apply_operation(Operation::SetSelectionMode(mode))
                    .unwrap();
                let expected = if let Some(noun) = noun {
                    format!("{tool} is unavailable in {noun} selection; switch to Object selection")
                } else {
                    format!("{tool} target must be the exact selected transform instance")
                };
                let before = state(&editor);
                let error = editor.apply_operation(begin).unwrap_err();
                assert!(
                    matches!(error, OperationError::InvalidInput(ref message) if message == &expected),
                    "{tool} {mode:?} {condition}: {error:?}"
                );
                assert_eq!(state(&editor), before);
            }
        }
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

#[test]
fn tool_handle_defaults_are_x_and_uniform() {
    let editor = Editor::default();
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::X);
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::Uniform);
}

#[test]
fn set_tool_handle_reports_changed_then_unchanged_per_slot() {
    let mut editor = Editor::default();
    let rotate = ToolHandle::RotateAxis(RotateAxis::Y);
    assert!(matches!(
        editor.set_tool_handle(rotate),
        OperationReply::Changed
    ));
    assert!(matches!(
        editor.set_tool_handle(rotate),
        OperationReply::Unchanged
    ));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::Y);
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::Uniform);
    assert!(matches!(
        editor.set_tool_handle(ToolHandle::ScaleHandle(ScaleHandle::Z)),
        OperationReply::Changed
    ));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::Y);
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::Z);
}

#[test]
fn set_tool_handle_leaves_the_other_slot_untouched() {
    let mut editor = Editor::default();
    editor.set_tool_handle(ToolHandle::ScaleHandle(ScaleHandle::X));
    editor.set_tool_handle(ToolHandle::RotateAxis(RotateAxis::Z));
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::X);
    editor.set_tool_handle(ToolHandle::ScaleHandle(ScaleHandle::Y));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::Z);
}

#[test]
fn native_set_tool_handle_commits_a_pending_draft_first() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.channels[0] = "2".into();
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Z,
    )));
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        2.0
    );
    assert!(!editor.has_draft());
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::Z);
}

#[test]
fn tool_handle_selection_survives_a_tool_switch() {
    let mut editor = Editor::default();
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Z,
    )));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Scale));
    editor.native_operation(Operation::SetToolHandle(ToolHandle::ScaleHandle(
        ScaleHandle::X,
    )));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Rotate));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::Z);
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::X);
}

#[test]
fn numeric_rotate_entry_uses_the_selected_axis_through_the_shared_operation() {
    let (mut editor, _, _) = setup(Key::E, 1.0);
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Y,
    )));
    editor.manipulation.numeric = "30".into();
    editor.apply_numeric_manipulation();
    assert!(!editor.error, "{}", editor.status);
    let channels = channels(&editor);
    near(channels[4], 30.0, 1e-6);
    assert_eq!(channels[3], 0.0);
    assert_eq!(channels[5], 0.0);
}

#[test]
fn tool_handle_is_refused_during_a_manipulation_with_the_selection_unchanged() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    assert!(editor.history.active_manipulation().is_some());
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Z,
    )));
    assert_eq!(editor.status, OperationError::BusyManipulation.to_string());
    assert!(editor.error);
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::X);
    let token = editor.scene_token;
    let result = editor.remote_operation(
        crate::editor_operations::RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SetToolHandle(ToolHandle::ScaleHandle(ScaleHandle::X)),
        },
        &context,
    );
    assert!(matches!(result, Err(OperationError::BusyManipulation)));
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::Uniform);
    // The draft commit before native dispatch refuses with the same status text, so only the
    // operation's own result proves the move-dispatch guard holds.
    assert!(matches!(
        editor.apply_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
            RotateAxis::Z
        ))),
        Err(OperationError::BusyManipulation)
    ));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::X);
}

#[test]
fn tool_handle_is_refused_while_a_modal_prompt_is_open() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.request_close();
    assert!(editor.files.is_modal());
    // Native dispatch reaches the per-variant modal list; the remote path is refused earlier.
    editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(
        RotateAxis::Z,
    )));
    editor.native_operation(Operation::SetToolHandle(ToolHandle::ScaleHandle(
        ScaleHandle::X,
    )));
    assert_eq!(editor.manipulation.rotate_axis(), RotateAxis::X);
    assert_eq!(editor.manipulation.scale_handle(), ScaleHandle::Uniform);
    assert!(editor.error);
    assert_eq!(editor.status, OperationError::BusyEditing.to_string());
}

#[test]
fn inspection_reports_the_persistent_axis_and_handle_in_tool_argument_spelling() {
    let mut editor = Editor::default();
    let read = |editor: &Editor| {
        let inspected = editor.inspect_manipulation().unwrap();
        (inspected.rotate_axis, inspected.scale_handle)
    };
    assert_eq!(read(&editor), ("x", "uniform"));
    for (axis, text) in [
        (RotateAxis::Y, "y"),
        (RotateAxis::Z, "z"),
        (RotateAxis::X, "x"),
    ] {
        editor.native_operation(Operation::SetToolHandle(ToolHandle::RotateAxis(axis)));
        assert_eq!(read(&editor), (text, "uniform"));
    }
    for (handle, text) in [
        (ScaleHandle::X, "x"),
        (ScaleHandle::Y, "y"),
        (ScaleHandle::Z, "z"),
        (ScaleHandle::Uniform, "uniform"),
    ] {
        editor.native_operation(Operation::SetToolHandle(ToolHandle::ScaleHandle(handle)));
        assert_eq!(read(&editor), ("x", text));
    }
}

fn size(value: f64) -> ManipulatorSize {
    ManipulatorSize::new(value).unwrap()
}

#[test]
fn set_manipulator_size_reports_changed_then_unchanged_without_touching_the_scene() {
    let mut editor = Editor::default();
    assert!(!editor.is_dirty());
    let depth = undo_depth(&mut editor);
    assert!(matches!(
        editor.apply_operation(Operation::SetManipulatorSize(size(2.5))),
        Ok(OperationReply::Changed)
    ));
    assert!(matches!(
        editor.apply_operation(Operation::SetManipulatorSize(size(2.5))),
        Ok(OperationReply::Unchanged)
    ));
    assert_eq!(editor.inspect_manipulation().unwrap().manipulator_size, 2.5);
    assert!(!editor.is_dirty());
    assert_eq!(undo_depth(&mut editor), depth);
}

#[test]
fn manipulator_size_survives_a_new_scene() {
    let mut editor = Editor::default();
    editor.native_operation(Operation::SetManipulatorSize(size(3.0)));
    editor.native_operation(Operation::NewScene);
    assert_eq!(editor.manipulation.manipulator_size(), size(3.0));
}

#[test]
fn a_native_size_change_resizes_the_next_handles() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    editor.native_operation(Operation::SetManipulatorSize(size(2.0)));
    idle(&mut editor, &context, 1.0);
    // The default ring radius no longer matches the drawn ring.
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    assert!(editor.history.active_manipulation().is_none());
    press(
        &mut editor,
        &context,
        1.0,
        view.ring_sized(Vec3::Y, 0.0, 2.0),
    );
    assert!(editor.history.active_manipulation().is_some());
}

#[test]
fn manipulator_size_is_refused_during_a_manipulation_with_the_size_unchanged() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    assert!(editor.history.active_manipulation().is_some());
    editor.native_operation(Operation::SetManipulatorSize(size(2.0)));
    assert_eq!(editor.status, OperationError::BusyManipulation.to_string());
    assert!(editor.error);
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
    let token = editor.scene_token;
    let result = editor.remote_operation(
        crate::editor_operations::RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SetManipulatorSize(size(2.0)),
        },
        &context,
    );
    assert!(matches!(result, Err(OperationError::BusyManipulation)));
    // The draft commit before native dispatch refuses with the same status text, so only the
    // operation's own result proves the move-dispatch guard holds.
    assert!(matches!(
        editor.apply_operation(Operation::SetManipulatorSize(size(2.0))),
        Err(OperationError::BusyManipulation)
    ));
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
}

#[test]
fn manipulator_size_is_refused_while_a_modal_prompt_is_open() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.request_close();
    assert!(editor.files.is_modal());
    // Native dispatch reaches the per-variant modal list; the remote path is refused earlier.
    editor.native_operation(Operation::SetManipulatorSize(size(2.0)));
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
    assert!(editor.error);
    assert_eq!(editor.status, OperationError::BusyEditing.to_string());
}

#[test]
fn a_remote_size_change_is_refused_during_a_human_draft_and_native_commits_the_draft_first() {
    let (mut editor, context, _) = setup(Key::E, 1.0);
    editor.channels[0] = "2".into();
    let token = editor.scene_token;
    let result = editor.remote_operation(
        crate::editor_operations::RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SetManipulatorSize(size(2.0)),
        },
        &context,
    );
    assert!(matches!(result, Err(OperationError::BusyEditing)));
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
    assert!(editor.has_draft());
    editor.native_operation(Operation::SetManipulatorSize(size(2.0)));
    assert!(!editor.has_draft());
    assert_eq!(editor.manipulation.manipulator_size(), size(2.0));
}

fn size_key(key: Key, repeat: bool, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    }
}

fn press_size_key(tool: Key, event: Event) -> Editor {
    let (mut editor, context, _) = setup(tool, 1.0);
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![event]),
    );
    editor
}

#[test]
fn plus_with_or_without_shift_and_equals_grow_and_minus_shrinks() {
    let shift = Modifiers::SHIFT;
    for event in [
        size_key(Key::Plus, false, shift),
        size_key(Key::Plus, false, Modifiers::NONE),
        size_key(Key::Equals, false, Modifiers::NONE),
    ] {
        let editor = press_size_key(Key::W, event);
        assert_eq!(editor.manipulation.manipulator_size(), size(1.25));
    }
    let editor = press_size_key(Key::E, size_key(Key::Minus, false, Modifiers::NONE));
    assert_eq!(editor.manipulation.manipulator_size(), size(0.8));
    let editor = press_size_key(Key::R, size_key(Key::Plus, false, Modifiers::NONE));
    assert_eq!(editor.manipulation.manipulator_size(), size(1.25));
}

const TOP_STATUS: &str =
    "Manipulator size is at or above its largest key step (9.313); set_manipulator_size reaches 10";
const BOTTOM_STATUS: &str = "Manipulator size is at or below its smallest key step (0.107); set_manipulator_size reaches 0.1";

fn tap(editor: &mut Editor, context: &egui::Context, key: Key) {
    frame(
        editor,
        context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(key, false, Modifiers::NONE)],
        ),
    );
}

#[test]
fn holding_a_size_key_stops_at_the_grid_end_and_names_that_limit() {
    for (key, steps, end, status) in [
        (Key::Plus, 10, 1.25_f64.powi(10), TOP_STATUS),
        (Key::Minus, 10, 1.25_f64.powi(-10), BOTTOM_STATUS),
    ] {
        let (mut editor, context, _) = setup(Key::W, 1.0);
        for _ in 0..steps {
            tap(&mut editor, &context, key);
        }
        assert_eq!(editor.manipulation.manipulator_size().get(), end);
        assert!(editor.status.is_empty(), "{}", editor.status);
        tap(&mut editor, &context, key);
        assert_eq!(editor.status, status);
        assert_eq!(editor.manipulation.manipulator_size().get(), end);
    }
}

#[test]
fn an_mcp_size_above_the_key_grid_end_is_refused_by_plus_without_changing() {
    let (mut editor, context, _) = setup(Key::W, 1.0);
    editor.native_operation(Operation::SetManipulatorSize(size(9.5)));
    tap(&mut editor, &context, Key::Plus);
    assert_eq!(editor.status, TOP_STATUS);
    assert_eq!(editor.manipulation.manipulator_size().get(), 9.5);
}

#[test]
fn one_key_press_and_its_release_step_exactly_once() {
    let (mut editor, context, _) = setup(Key::W, 1.0);
    let release = Event::Key {
        key: Key::Plus,
        physical_key: None,
        pressed: false,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    for event in [size_key(Key::Plus, false, Modifiers::NONE), release] {
        frame(
            &mut editor,
            &context,
            raw(SIZE, Pos2::ZERO, 1.0, vec![event]),
        );
    }
    assert_eq!(editor.manipulation.manipulator_size(), size(1.25));
}

#[test]
fn other_modifiers_shift_minus_and_the_select_tool_do_not_resize() {
    for event in [
        size_key(Key::Plus, false, Modifiers::CTRL),
        size_key(Key::Minus, false, Modifiers::COMMAND),
        size_key(Key::Equals, false, Modifiers::ALT),
        size_key(Key::Minus, false, Modifiers::SHIFT),
    ] {
        let editor = press_size_key(Key::W, event);
        assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
    }
    let editor = press_size_key(Key::Q, size_key(Key::Plus, false, Modifiers::NONE));
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
}

#[test]
fn key_repeat_steps_and_the_bounds_report_a_status_without_an_operation() {
    let (mut editor, context, _) = setup(Key::W, 1.0);
    for _ in 0..2 {
        frame(
            &mut editor,
            &context,
            raw(
                SIZE,
                Pos2::ZERO,
                1.0,
                vec![size_key(Key::Plus, true, Modifiers::NONE)],
            ),
        );
    }
    assert_eq!(editor.manipulation.manipulator_size().get(), 1.5625);
    editor.native_operation(Operation::SetManipulatorSize(size(10.0)));
    frame(
        &mut editor,
        &context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(Key::Plus, false, Modifiers::NONE)],
        ),
    );
    assert_eq!(editor.status, TOP_STATUS);
    assert!(!editor.error);
    editor.native_operation(Operation::SetManipulatorSize(size(0.1)));
    frame(
        &mut editor,
        &context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(Key::Minus, false, Modifiers::NONE)],
        ),
    );
    assert_eq!(editor.status, BOTTOM_STATUS);
    assert_eq!(editor.manipulation.manipulator_size(), size(0.1));
    frame(
        &mut editor,
        &context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(Key::Plus, false, Modifiers::NONE)],
        ),
    );
    assert!(
        editor.status.is_empty() && !editor.error,
        "{}",
        editor.status
    );
}

#[test]
fn the_size_keys_are_refused_during_a_drag_and_ignored_under_a_modal() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    press(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    assert!(editor.history.active_manipulation().is_some());
    frame(
        &mut editor,
        &context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(Key::Plus, false, Modifiers::NONE)],
        ),
    );
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
    assert!(editor.history.active_manipulation().is_some());
    release(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    editor.request_close();
    assert!(editor.files.is_modal());
    frame(
        &mut editor,
        &context,
        raw(
            SIZE,
            Pos2::ZERO,
            1.0,
            vec![size_key(Key::Plus, false, Modifiers::NONE)],
        ),
    );
    assert_eq!(editor.manipulation.manipulator_size(), size(1.0));
}

#[test]
fn a_worst_case_manipulation_inspection_stays_within_the_reserved_budget() {
    let (mut editor, context, view) = setup(Key::R, 1.0);
    editor.native_operation(Operation::SetToolHandle(ToolHandle::ScaleHandle(
        ScaleHandle::Uniform,
    )));
    press(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::X * view.radius()),
    );
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(inspected.active.is_some() && inspected.viewport.is_some());
    let bytes = serde_json::to_vec(&inspected).unwrap().len();
    assert!(
        bytes <= crate::editor_operations::MOVE_INSPECTION_MAX_BYTES,
        "{bytes}"
    );
}

fn pointer_pick(editor: &Editor) -> (Option<&'static str>, Option<&'static str>) {
    let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
    (viewport.pointer_hit, viewport.pointer_disabled)
}

// A point `offset` px from `on` on the normal of the segment direction `along`.
fn beside(on: Pos2, along: egui::Vec2, offset: f32) -> Pos2 {
    let along = along.normalized();
    on + egui::vec2(-along.y, along.x) * offset
}

#[test]
fn inspection_reports_the_rotate_ring_under_the_pointer() {
    // Rotate must not report the (undrawn) Move arrow; 7 px picks the Y ring, 9 px nothing.
    for (offset, expected) in [(7.0_f32, Some("y")), (9.0, None)] {
        let (mut editor, context, view) = setup(Key::E, 1.0);
        let along = view.ring(Vec3::Y, 1.0) - view.ring(Vec3::Y, -1.0);
        moved(
            &mut editor,
            &context,
            1.0,
            beside(view.ring(Vec3::Y, 0.0), along, offset),
        );
        assert_eq!(pointer_pick(&editor), (expected, None), "{offset}");
    }
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let origin = view.screen(Vec3::ZERO);
    moved(
        &mut editor,
        &context,
        1.0,
        origin + egui::vec2(72.0 * 1.2, 0.0),
    );
    assert_eq!(pointer_pick(&editor), (None, Some("view_ring")));
    moved(&mut editor, &context, 1.0, origin);
    assert_eq!(pointer_pick(&editor), (None, None));
}

#[test]
fn inspection_reports_the_scale_handle_under_the_pointer() {
    for (offset, expected) in [(7.0_f32, Some("x")), (9.0, None)] {
        let (mut editor, context, view) = setup(Key::R, 1.0);
        let origin = view.screen(Vec3::ZERO);
        let tip = view.screen(Vec3::X * view.radius());
        let on = origin + (tip - origin) * 0.6;
        moved(&mut editor, &context, 1.0, beside(on, tip - origin, offset));
        assert_eq!(pointer_pick(&editor), (expected, None), "{offset}");
    }
    let (mut editor, context, view) = setup(Key::R, 1.0);
    moved(&mut editor, &context, 1.0, view.screen(Vec3::ZERO));
    assert_eq!(pointer_pick(&editor), (Some("uniform"), None));
    // A Scale plane outline at a small size, where no shaft is within the pick radius.
    editor
        .apply_operation(Operation::SetManipulatorSize(size(0.1)))
        .unwrap();
    idle(&mut editor, &context, 1.0);
    let plane = scale_plane_point(&editor, view.rect);
    moved(&mut editor, &context, 1.0, plane);
    assert_eq!(pointer_pick(&editor), (None, Some("scale_plane")));
}

#[test]
fn inspection_reports_no_handle_for_select_or_during_a_manipulation() {
    let (mut editor, context, view) = setup(Key::W, 1.0);
    let on_arrow = view.screen(Vec3::X * view.radius() * 0.5);
    moved(&mut editor, &context, 1.0, on_arrow);
    assert_eq!(pointer_pick(&editor), (Some("x"), None));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Select));
    assert_eq!(pointer_pick(&editor), (None, None));
    // An active manipulation suppresses the pick like native hover, while the pointer stays known.
    for (tool, point) in [
        (Key::W, on_arrow),
        (Key::E, view.ring(Vec3::Y, 0.0)),
        (Key::R, view.screen(Vec3::X * view.radius() * 0.6)),
    ] {
        let (mut editor, context, _) = setup(tool, 1.0);
        press(&mut editor, &context, 1.0, point);
        assert!(editor.history.active_manipulation().is_some(), "{tool:?}");
        let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
        assert_eq!(
            (viewport.pointer_hit, viewport.pointer_disabled),
            (None, None)
        );
        assert!(viewport.pointer_available, "{tool:?}");
    }
}

#[test]
fn a_plain_ascii_source_reports_the_rotate_ring_although_move_refuses() {
    let scene = azteccraft::native_scene::ascii::decode(
        concat!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; ",
            "currentUnit -l centimeter -a degree -t film;\n",
            "createNode transform -n \"a\";\n"
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
    let node = editor.history.scene().nodes()[0].id();
    editor.select(Some(node));
    editor.native_operation(Operation::SetActiveTool(ActiveTool::Rotate));
    let context = egui::Context::default();
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&idle(&mut editor, &context, 1.0)),
        scale: 1.0,
    };
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    let inspected = editor.inspect_manipulation().unwrap();
    assert!(
        !inspected.kinds[0].available,
        "Move must refuse this source"
    );
    let viewport = inspected.viewport.unwrap();
    // The Move frame's error is carried by the axes, while the Rotate pick still works.
    assert!(
        viewport
            .axes
            .iter()
            .all(|axis| axis.unavailable_reason.is_some())
    );
    assert_eq!(viewport.pointer_hit, Some("y"));
    // The refusal is the text native hover shows after the handle's label.
    let reason = "Maya ASCII scenes are read-only in this version";
    assert_eq!(viewport.pointer_refusal.as_deref(), Some(reason));
    assert_eq!(editor.status, format!("Object Y Rotate: {reason}"));
}

// An editor on a decoded Maya ASCII scene (read-only), with the node `select` selected and `tool`
// active, and the viewport rect taken from an idle frame.
fn ascii_editor(nodes: &str, select: &str, tool: ActiveTool) -> (Editor, egui::Context, View) {
    let source = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; \
         currentUnit -l centimeter -a degree -t film;\n{nodes}"
    );
    let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
    let mut editor = Editor {
        history: azteccraft::history::History::new(scene),
        renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
        camera: oblique(),
        ..Editor::default()
    };
    let node = editor
        .history
        .scene()
        .nodes()
        .iter()
        .find(|node| node.name() == select)
        .unwrap()
        .id();
    editor.select(Some(node));
    editor.native_operation(Operation::SetActiveTool(tool));
    let context = egui::Context::default();
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&idle(&mut editor, &context, 1.0)),
        scale: 1.0,
    };
    (editor, context, view)
}

fn segment_gap(point: glam::DVec2, segment: &azteccraft::move_handles::ScreenSegment) -> f64 {
    let along = segment.end - segment.start;
    let t = if along.length_squared() == 0.0 {
        0.0
    } else {
        ((point - segment.start).dot(along) / along.length_squared()).clamp(0.0, 1.0)
    };
    (point - (segment.start + along * t)).length()
}

// Scale handles for a frame the test builds from known numbers, never from the editor.
fn scale_handles_at(editor: &Editor, origin: DVec3, axes: [DVec3; 3]) -> ScaleHandles {
    let capture = editor.move_viewport.as_ref().unwrap().capture.clone();
    ScaleHandles::new(&capture, &HandleFrame::new(origin, axes).unwrap()).unwrap()
}

// The distance from `pixel` to the nearest drawn segment of the handles (axis geometry and the
// centre box).
fn clearance(handles: &ScaleHandles, pixel: glam::DVec2) -> f64 {
    handles
        .axes()
        .iter()
        .filter_map(|axis| axis.geometry())
        .flat_map(|geometry| geometry.segments())
        .chain(handles.center())
        .map(|segment| segment_gap(pixel, segment))
        .fold(f64::INFINITY, f64::min)
}

fn logical(rect: egui::Rect, pixel: glam::DVec2) -> Pos2 {
    rect.min + egui::vec2(pixel.x as f32, pixel.y as f32)
}

// The point on shaft `index` farthest from every wrong frame's handles.
fn shaft_probe(
    editor: &Editor,
    origin: DVec3,
    handles: &ScaleHandles,
    index: usize,
    wrong: &[(DVec3, [DVec3; 3])],
) -> glam::DVec2 {
    let shaft = handles.axes()[index]
        .geometry()
        .unwrap()
        .segments()
        .next()
        .unwrap();
    let wrong: Vec<_> = wrong
        .iter()
        .filter(|(other, _)| other.distance(origin) > 1e-9)
        .map(|(origin, axes)| scale_handles_at(editor, *origin, *axes))
        .collect();
    (1..20)
        .map(|step| shaft.start + (shaft.end - shaft.start) * (f64::from(step) / 20.0))
        .max_by(|a, b| {
            let worst = |p: &glam::DVec2| {
                wrong
                    .iter()
                    .map(|handles| clearance(handles, *p))
                    .fold(f64::INFINITY, f64::min)
            };
            worst(a).total_cmp(&worst(b))
        })
        .unwrap()
}

// Every frame the Scale tool could wrongly use is a different (origin, axes) pair; the probe
// pixel must be more than the pick radius from the handles of each wrong frame with another
// origin and must pick something else under each wrong frame.
fn assert_probe_discriminates(
    editor: &Editor,
    expected_origin: DVec3,
    pixel: glam::DVec2,
    expected: Pick<ScaleHandle>,
    wrong: &[(DVec3, [DVec3; 3])],
) {
    for (origin, axes) in wrong {
        let alternative = scale_handles_at(editor, *origin, *axes);
        // A frame at the same origin shares the centre box and, in Object space, the arm
        // directions, so it is told apart by the pick alone.
        if origin.distance(expected_origin) > 1e-9 {
            assert!(
                clearance(&alternative, pixel) > 8.0,
                "{origin:?} {axes:?} reaches the probe {pixel:?}"
            );
        }
        assert_ne!(alternative.pick(pixel), Some(expected));
    }
}

const WORLD_AXES: [DVec3; 3] = [DVec3::X, DVec3::Y, DVec3::Z];
// Rz(90): object X is world Y, object Y is world -X.
const ROTATED_AXES: [DVec3; 3] = [DVec3::Y, DVec3::NEG_X, DVec3::Z];
const ROTATE_ORIGIN: DVec3 = DVec3::new(1.0, 0.0, 0.0);
const SCALE_ORIGIN: DVec3 = DVec3::new(1.0, 2.0, 0.0);

// T=(1,0,0), Rz(90), scalePivotTranslate=(2,0,0): the Rotate origin is (1,0,0) and the Scale
// origin (1,2,0).
fn offset_pivot_scene() -> (Editor, egui::Context, View) {
    ascii_editor(
        concat!(
            "createNode transform -n \"a\";\n",
            "setAttr \".t\" -type \"double3\" 1 0 0;\n",
            "setAttr \".r\" -type \"double3\" 0 0 90;\n",
            "setAttr \".spt\" -type \"double3\" 2 0 0;\n"
        ),
        "a",
        ActiveTool::Scale,
    )
}

#[test]
fn scale_hover_and_inspection_use_the_scale_frame_not_the_rotate_frame() {
    for (space, axes, (index, handle, name, id), wrong) in [
        (
            ManipulationSpace::World,
            WORLD_AXES,
            (0, ScaleHandle::X, "X", "x"),
            [
                (ROTATE_ORIGIN, WORLD_AXES),
                (SCALE_ORIGIN, ROTATED_AXES),
                (ROTATE_ORIGIN, ROTATED_AXES),
            ],
        ),
        (
            ManipulationSpace::Object,
            ROTATED_AXES,
            (0, ScaleHandle::X, "X", "x"),
            [
                (ROTATE_ORIGIN, ROTATED_AXES),
                (SCALE_ORIGIN, WORLD_AXES),
                (ROTATE_ORIGIN, WORLD_AXES),
            ],
        ),
    ] {
        let (mut editor, context, view) = offset_pivot_scene();
        editor.set_tool_space(ActiveTool::Scale, space).unwrap();
        // Short handles keep the wrong frames' arms away from the probes.
        editor
            .apply_operation(Operation::SetManipulatorSize(size(0.5)))
            .unwrap();
        idle(&mut editor, &context, 1.0);
        // The fixture really has the frames the test names.
        assert!(
            editor
                .manipulation_frame(ActiveTool::Scale)
                .unwrap()
                .1
                .origin()
                .distance(SCALE_ORIGIN)
                < 1e-9
        );
        assert!(
            editor
                .manipulation_frame(ActiveTool::Rotate)
                .unwrap()
                .1
                .origin()
                .distance(ROTATE_ORIGIN)
                < 1e-9
        );
        let expected = scale_handles_at(&editor, SCALE_ORIGIN, axes);
        let space_name = if space == ManipulationSpace::World {
            "World"
        } else {
            "Object"
        };

        let shaft = shaft_probe(&editor, SCALE_ORIGIN, &expected, index, &wrong);
        assert_eq!(expected.pick(shaft), Some(Pick::Handle(handle)));
        assert_probe_discriminates(&editor, SCALE_ORIGIN, shaft, Pick::Handle(handle), &wrong);
        moved(&mut editor, &context, 1.0, logical(view.rect, shaft));
        assert_eq!(pointer_pick(&editor), (Some(id), None), "{space_name}");
        assert!(
            editor
                .status
                .starts_with(&format!("{space_name} {name} Scale: ")),
            "{}",
            editor.status
        );
        assert!(editor.error && !editor.status.ends_with("drag to scale"));

        // The centre box sits on the Scale origin; only the frames with another origin differ.
        let centre = scale_handles_at(&editor, SCALE_ORIGIN, axes);
        let origin_pixel = centre
            .center()
            .fold(glam::DVec2::ZERO, |sum, s| sum + (s.start + s.end) * 0.5)
            * 0.25;
        assert_eq!(
            centre.pick(origin_pixel),
            Some(Pick::Handle(ScaleHandle::Uniform))
        );
        let other_origin: Vec<_> = wrong
            .iter()
            .copied()
            .filter(|(origin, _)| *origin != SCALE_ORIGIN)
            .collect();
        assert_eq!(other_origin.len(), 2);
        assert_probe_discriminates(
            &editor,
            SCALE_ORIGIN,
            origin_pixel,
            Pick::Handle(ScaleHandle::Uniform),
            &other_origin,
        );
        moved(&mut editor, &context, 1.0, logical(view.rect, origin_pixel));
        assert_eq!(
            pointer_pick(&editor),
            (Some("uniform"), None),
            "{space_name}"
        );
        assert!(
            editor
                .status
                .starts_with(&format!("{space_name} Uniform Scale: ")),
            "{}",
            editor.status
        );
    }
}

#[test]
fn a_scale_press_uses_the_scale_frame_space_not_the_rotate_space() {
    // A cube turned 90 degrees about Z: Rotate defaults to Object axes, Scale to World axes.
    let (mut editor, context, view) = setup(Key::R, 1.0);
    let id = editor.selected.unwrap();
    editor
        .apply_operation(Operation::SetTransform {
            id,
            transform: crate::editor_operations::TransformInput {
                translation: [0.0; 3],
                rotation_degrees: [0.0, 0.0, 90.0],
                scale: [1.0; 3],
            },
        })
        .unwrap();
    idle(&mut editor, &context, 1.0);
    let expected = scale_handles_at(&editor, DVec3::ZERO, WORLD_AXES);
    let shaft = shaft_probe(
        &editor,
        DVec3::ZERO,
        &expected,
        0,
        &[(DVec3::ZERO, ROTATED_AXES)],
    );
    assert_probe_discriminates(
        &editor,
        DVec3::ZERO,
        shaft,
        Pick::Handle(ScaleHandle::X),
        &[(DVec3::ZERO, ROTATED_AXES)],
    );
    press(&mut editor, &context, 1.0, logical(view.rect, shaft));
    assert_eq!(
        editor.history.active_manipulation().unwrap().kind,
        ManipulationKindState::Scale {
            handle: ScaleHandle::X,
            space: azteccraft::native_scene::ScaleSpace::World
        }
    );
    assert_eq!(editor.status, "World X Scale");
}

// A writable cube turned `degrees` about Z at the origin with the Scale tool active.
fn rotated_cube_scale_editor(degrees: f64) -> (Editor, egui::Context, View) {
    let (mut editor, context, view) = setup(Key::R, 1.0);
    let id = editor.selected.unwrap();
    editor
        .apply_operation(Operation::SetTransform {
            id,
            transform: crate::editor_operations::TransformInput {
                translation: [0.0; 3],
                rotation_degrees: [0.0, 0.0, degrees],
                scale: [1.0; 3],
            },
        })
        .unwrap();
    idle(&mut editor, &context, 1.0);
    (editor, context, view)
}

#[test]
fn scale_hover_states_the_refusal_of_the_handle_under_the_pointer() {
    // World X and Y of a node turned 30 degrees about Z are not local axes; World Z is.
    let (mut editor, context, view) = rotated_cube_scale_editor(30.0);
    let handles = scale_handles_at(&editor, DVec3::ZERO, WORLD_AXES);
    let refusal = NativeError::UnsupportedScale(ScaleUnsupported::NotAxisAligned).to_string();
    for (index, handle, name, refused) in [
        (0, ScaleHandle::X, "X", true),
        (2, ScaleHandle::Z, "Z", false),
    ] {
        let pixel = shaft_probe(&editor, DVec3::ZERO, &handles, index, &[]);
        assert_eq!(handles.pick(pixel), Some(Pick::Handle(handle)));
        moved(&mut editor, &context, 1.0, logical(view.rect, pixel));
        let expected = if refused {
            format!("World {name} Scale: {refusal}")
        } else {
            format!("World {name} Scale: drag to scale")
        };
        assert_eq!(editor.status, expected);
        assert_eq!(editor.error, refused);
    }
}

fn pointer_refusal(editor: &Editor) -> Option<String> {
    editor
        .inspect_manipulation()
        .unwrap()
        .viewport
        .unwrap()
        .pointer_refusal
}

#[test]
fn pointer_refusal_is_per_handle_and_matches_the_native_hover_text() {
    // World X of a node turned 30 degrees about Z is refused, World Z and Uniform are not.
    let (mut editor, context, view) = rotated_cube_scale_editor(30.0);
    let handles = scale_handles_at(&editor, DVec3::ZERO, WORLD_AXES);
    let reason = NativeError::UnsupportedScale(ScaleUnsupported::NotAxisAligned).to_string();
    let centre = handles
        .center()
        .fold(glam::DVec2::ZERO, |sum, s| sum + (s.start + s.end) * 0.5)
        * 0.25;
    for (pixel, hit, expected) in [
        (
            shaft_probe(&editor, DVec3::ZERO, &handles, 0, &[]),
            "x",
            Some(reason.as_str()),
        ),
        (
            shaft_probe(&editor, DVec3::ZERO, &handles, 2, &[]),
            "z",
            None,
        ),
        (centre, "uniform", None),
    ] {
        moved(&mut editor, &context, 1.0, logical(view.rect, pixel));
        assert_eq!(pointer_pick(&editor), (Some(hit), None), "{hit}");
        assert_eq!(pointer_refusal(&editor).as_deref(), expected, "{hit}");
        if let Some(reason) = expected {
            assert!(editor.status.ends_with(reason), "{}", editor.status);
        }
    }
}

#[test]
fn pointer_refusal_is_cleared_during_a_manipulation_and_for_disabled_picks() {
    let (mut editor, context, view) = rotated_cube_scale_editor(30.0);
    let handles = scale_handles_at(&editor, DVec3::ZERO, WORLD_AXES);
    let refused = logical(
        view.rect,
        shaft_probe(&editor, DVec3::ZERO, &handles, 0, &[]),
    );
    moved(&mut editor, &context, 1.0, refused);
    assert!(pointer_refusal(&editor).is_some());
    // An accepted handle starts a drag; the pointer then rests over the refused one.
    let accepted = logical(
        view.rect,
        shaft_probe(&editor, DVec3::ZERO, &handles, 2, &[]),
    );
    press(&mut editor, &context, 1.0, accepted);
    assert!(editor.history.active_manipulation().is_some());
    moved(&mut editor, &context, 1.0, refused);
    let inspected = editor.inspect_manipulation().unwrap().viewport.unwrap();
    assert!(inspected.pointer_available);
    assert_eq!(
        (
            inspected.pointer_hit,
            inspected.pointer_disabled,
            inspected.pointer_refusal
        ),
        (None, None, None)
    );
    // A disabled pick in a read-only scene: the kind is the reason, so no refusal is added.
    let (mut editor, context, view) =
        ascii_editor("createNode transform -n \"a\";\n", "a", ActiveTool::Rotate);
    moved(
        &mut editor,
        &context,
        1.0,
        view.screen(Vec3::ZERO) + egui::vec2(72.0 * 1.2, 0.0),
    );
    assert_eq!(pointer_pick(&editor), (None, Some("view_ring")));
    assert_eq!(pointer_refusal(&editor), None);
}

#[test]
fn pointer_refusal_stays_null_for_move_even_when_its_frame_refuses() {
    let mirrored = concat!(
        "createNode transform -n \"p\"; setAttr \".s\" -type \"double3\" -1 1 1;\n",
        "createNode transform -n \"c\" -p \"p\";\n"
    );
    for (nodes, node) in [("createNode transform -n \"a\";\n", "a"), (mirrored, "c")] {
        let (mut editor, context, view) = ascii_editor(nodes, node, ActiveTool::Move);
        moved(&mut editor, &context, 1.0, view.screen(Vec3::ZERO));
        let inspected = editor.inspect_manipulation().unwrap();
        assert!(!inspected.kinds[0].available, "{node}");
        let viewport = inspected.viewport.unwrap();
        assert!(viewport.pointer_available);
        assert_eq!(viewport.pointer_refusal, None, "{node}");
    }
}

#[test]
fn a_mirrored_frame_is_reported_for_the_whole_tool_at_any_pixel() {
    let nodes = concat!(
        "createNode transform -n \"privateParent\"; setAttr \".s\" -type \"double3\" -1 1 1;\n",
        "createNode transform -n \"privateChild\" -p \"privateParent\";\n"
    );
    for (tool, name) in [(ActiveTool::Rotate, "Rotate"), (ActiveTool::Scale, "Scale")] {
        let (mut editor, context, view) = ascii_editor(nodes, "privateChild", tool);
        // World axes are never mirrored; Scale defaults to World.
        editor
            .set_tool_space(tool, ManipulationSpace::Object)
            .unwrap();
        idle(&mut editor, &context, 1.0);
        // No pointer yet: nothing to report.
        assert_eq!(pointer_refusal(&editor), None, "{name}");
        for at in [view.rect.center(), view.rect.min + egui::vec2(40.0, 40.0)] {
            moved(&mut editor, &context, 1.0, at);
            let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
            assert!(viewport.pointer_available);
            assert_eq!(viewport.pointer_hit, None);
            let reason = viewport.pointer_refusal.expect("the frame refusal");
            assert!(reason.contains("Handle axes are mirrored"), "{reason}");
            assert_eq!(
                editor.status,
                format!("{name} unavailable: {reason}"),
                "native shows the same suffix"
            );
            assert!(!reason.contains("private"), "{reason}");
        }
    }
}

fn ascii_chain(scale: &str) -> String {
    format!(
        concat!(
            "createNode transform -n \"a\"; setAttr \".s\" -type \"double3\" {s} {s} {s};\n",
            "createNode transform -n \"b\" -p \"a\"; setAttr \".s\" -type \"double3\" {s} {s} {s};\n",
            "createNode transform -n \"leaf\" -p \"b\";\n"
        ),
        s = scale
    )
}

// Two ancestors scaled 1e200 each have a finite matrix but no finite product, so the leaf's frame is
// refused as ParentTransform. A capture taken before the ancestors overflow survives later frames
// (a view build error does not clear `move_viewport`), so the test swaps the scene in directly after
// capturing the pointer. Reachable through the MCP set_transform tool on a writable scene (plausible,
// not exercised here); the scene here is read-only ASCII.
#[test]
fn a_scene_frame_refusal_is_reported_for_the_whole_tool_at_any_pixel() {
    let expected = NativeError::UnsupportedMove(
        azteccraft::native_scene::WorldMoveUnsupported::ParentTransform,
    )
    .to_string();
    for (tool, name) in [(ActiveTool::Rotate, "Rotate"), (ActiveTool::Scale, "Scale")] {
        for corner in [false, true] {
            let (mut editor, context, view) = ascii_editor(&ascii_chain("1"), "leaf", tool);
            let at = if corner {
                view.rect.min + egui::vec2(40.0, 40.0)
            } else {
                view.rect.center()
            };
            moved(&mut editor, &context, 1.0, at);
            let source = format!(
                "//Maya ASCII 2016 scene\nrequires maya \"2016\"; \
                 currentUnit -l centimeter -a degree -t film;\n{}",
                ascii_chain("1e200")
            );
            let scene =
                azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
            let leaf = scene
                .nodes()
                .iter()
                .find(|node| node.name() == "leaf")
                .unwrap()
                .id();
            editor.history = azteccraft::history::History::new(scene);
            editor.select(Some(leaf));
            let viewport = editor
                .inspect_manipulation()
                .expect("inspection survives a refused frame")
                .viewport
                .unwrap();
            assert_eq!(viewport.pointer_hit, None, "{name}");
            assert_eq!(
                viewport.pointer_refusal.as_deref(),
                Some(expected.as_str()),
                "{name}"
            );
        }
    }
}

// HandleFrame::new refuses an axis whose length overflows. Two ancestors scaled 1e100 give a finite
// 1e200 world scale the viewport still draws, so the capture is live and the whole tool is refused.
#[test]
fn an_unrepresentable_handle_frame_refuses_the_whole_tool_at_any_pixel() {
    for (tool, name) in [(ActiveTool::Rotate, "Rotate"), (ActiveTool::Scale, "Scale")] {
        let (mut editor, context, view) = ascii_editor(&ascii_chain("1e100"), "leaf", tool);
        editor
            .set_tool_space(tool, ManipulationSpace::Object)
            .unwrap();
        idle(&mut editor, &context, 1.0);
        let expected = format!("{name} handle projection cannot be represented");
        for at in [view.rect.center(), view.rect.min + egui::vec2(40.0, 40.0)] {
            moved(&mut editor, &context, 1.0, at);
            let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
            assert_eq!(viewport.pointer_hit, None, "{name}");
            assert_eq!(
                viewport.pointer_refusal.as_deref(),
                Some(expected.as_str()),
                "{name}"
            );
            assert_eq!(editor.status, format!("{name} unavailable: {expected}"));
        }
    }
}

#[test]
fn no_selection_has_no_frame_refusal() {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    editor.select(None);
    moved(&mut editor, &context, 1.0, view.rect.center());
    assert_eq!(pointer_refusal(&editor), None);
}

#[test]
fn pointer_refusals_carry_no_node_name_or_path() {
    let (mut editor, context, view) = ascii_editor(
        "createNode transform -n \"privateNodeName\";\n",
        "privateNodeName",
        ActiveTool::Rotate,
    );
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    let reason = pointer_refusal(&editor).expect("read-only refusal");
    for secret in ["privateNodeName", "/", "\\"] {
        assert!(!reason.contains(secret), "{reason}");
    }
}

#[test]
fn a_refused_hit_with_the_longest_reason_stays_within_the_reserved_budget() {
    let (mut editor, context, view) =
        ascii_editor("createNode transform -n \"a\";\n", "a", ActiveTool::Rotate);
    moved(&mut editor, &context, 1.0, view.ring(Vec3::Y, 0.0));
    let mut inspected = editor.inspect_manipulation().unwrap();
    let viewport = inspected.viewport.as_mut().unwrap();
    assert!(viewport.pointer_hit.is_some() && viewport.pointer_refusal.is_some());
    // The longest reason a handle refusal can carry.
    let longest = NativeError::AbsentChannelLeaf {
        channel: azteccraft::native_scene::ChannelLeaf::Translate,
        component: 0,
    }
    .to_string();
    assert!(longest.len() < 128, "{}", longest.len());
    viewport.pointer_refusal = Some(longest);
    let bytes = serde_json::to_vec(&inspected).unwrap().len();
    assert!(
        bytes <= crate::editor_operations::MOVE_INSPECTION_MAX_BYTES,
        "{bytes}"
    );
}

// The Y ring hovered on the Rotate tool: status "Object Y Rotate: drag to turn".
fn hovering_the_y_ring() -> (Editor, egui::Context, Pos2) {
    let (mut editor, context, view) = setup(Key::E, 1.0);
    let at = view.ring(Vec3::Y, 0.0);
    moved(&mut editor, &context, 1.0, at);
    assert_eq!(editor.status, "Object Y Rotate: drag to turn");
    assert_eq!(pointer_pick(&editor), (Some("y"), None));
    (editor, context, at)
}

// Pinned: an open egui popup suppresses the hover status text but not the pick (move_tool.rs hover gate).
#[test]
fn an_open_popup_suppresses_hover_status_but_inspection_still_picks() {
    let (mut editor, context, at) = hovering_the_y_ring();
    open_file_menu(&mut editor, &context, SIZE);
    moved(&mut editor, &context, 1.0, at);
    assert!(egui::Popup::is_any_open(&context));
    assert_eq!(editor.status, editor.tool_help());
    assert_eq!(pointer_pick(&editor), (Some("y"), None));
}

// Pinned: an open popup keeps a press on the ring from beginning a rotate (move_tool.rs press gate).
#[test]
fn an_open_popup_blocks_a_press_on_the_ring() {
    let (mut editor, context, at) = hovering_the_y_ring();
    open_file_menu(&mut editor, &context, SIZE);
    press(&mut editor, &context, 1.0, at);
    // A press outside the menu could have closed it before the gate read it, which would be vacuous.
    assert!(
        egui::Popup::is_any_open(&context),
        "popup survives the press"
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.status, editor.tool_help());
    assert!(!editor.error);
}

// Pinned: a popup opening mid-drag cancels the native drag (move_tool.rs drag-cancel gate). A key
// sequence cannot open a menu mid-drag without changing the drag, so the popup opens through egui memory.
#[test]
fn a_popup_opening_mid_drag_cancels_the_native_rotate() {
    let (mut editor, context, at) = hovering_the_y_ring();
    press(&mut editor, &context, 1.0, at);
    assert!(
        editor.history.active_manipulation().is_some(),
        "{}",
        editor.status
    );
    egui::Popup::open_id(&context, egui::Id::new("popup_admission_probe"));
    assert!(egui::Popup::is_any_open(&context));
    frame(&mut editor, &context, raw(SIZE, Pos2::ZERO, 1.0, vec![]));
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(
        editor.status,
        "Rotate cancelled: camera, viewport or native input changed"
    );
}

// Pinned, not gated: `pointer_hit` mirrors the painted highlight, which native paints whatever the
// focus, popup or modal state; only the status text is gated by the hover admission.
#[test]
fn inspection_reports_the_pick_while_native_hover_status_is_suppressed() {
    // The window loses focus: the status reverts, the pick stays.
    let (mut editor, context, at) = hovering_the_y_ring();
    let mut input = raw(SIZE, Pos2::ZERO, 1.0, vec![Event::PointerMoved(at)]);
    input.focused = false;
    frame(&mut editor, &context, input);
    assert_eq!(editor.status, editor.tool_help());
    assert_eq!(pointer_pick(&editor), (Some("y"), None));

    // A close modal is up: the pick stays.
    let (mut editor, context, at) = hovering_the_y_ring();
    editor.request_close();
    assert!(editor.files.is_modal());
    moved(&mut editor, &context, 1.0, at);
    assert_ne!(editor.status, "Object Y Rotate: drag to turn");
    assert_eq!(pointer_pick(&editor), (Some("y"), None));

    // The pointer leaves the window: nothing is available and nothing is picked.
    let (mut editor, context, _) = hovering_the_y_ring();
    frame(
        &mut editor,
        &context,
        raw(SIZE, Pos2::ZERO, 1.0, vec![Event::PointerGone]),
    );
    let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
    assert!(!viewport.pointer_available);
    assert_eq!(
        (
            viewport.pointer_hit,
            viewport.pointer_disabled,
            viewport.pointer_refusal
        ),
        (None, None, None)
    );

    // A hidden window has no viewport capture at all.
    let (mut editor, context, _) = hovering_the_y_ring();
    let mut input = raw(SIZE, Pos2::ZERO, 1.0, vec![]);
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
    assert!(editor.inspect_manipulation().unwrap().viewport.is_none());
}

fn camera_at(yaw: f32, pitch: f32) -> azteccraft::viewport::Camera {
    azteccraft::viewport::Camera::new(
        Vec3::ZERO,
        yaw,
        pitch,
        10.0,
        90_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap()
}

// Hover suffix, press status and inspection must all say `expected` at `at`.
fn assert_drag_refusal_everywhere(
    editor: &mut Editor,
    context: &egui::Context,
    at: Pos2,
    hit: &'static str,
    label: &str,
    expected: &str,
) {
    moved(editor, context, 1.0, at);
    let viewport = editor.inspect_manipulation().unwrap().viewport.unwrap();
    assert_eq!(viewport.pointer_hit, Some(hit), "precondition");
    assert_eq!(viewport.pointer_refusal.as_deref(), Some(expected));
    assert_eq!(editor.status, format!("{label}: {expected}"), "hover");
    assert!(editor.error);
    press(editor, context, 1.0, at);
    assert_eq!(editor.status, expected, "press");
    assert!(editor.error);
    assert!(editor.history.active_manipulation().is_none());
}

// Pitch 0 and yaw 0 put the camera in the Y ring's plane, so the ring is edge-on along the
// horizontal line through the pivot. The drag-begin refusal wins over the read-only scene refusal,
// as it does on press.
#[test]
fn an_edge_on_rotate_ring_reports_the_drag_refusal_before_the_scene_refusal() {
    let (mut editor, context, view) =
        ascii_editor("createNode transform -n \"a\";\n", "a", ActiveTool::Rotate);
    editor.camera = camera_at(0.0, 0.0);
    let view = View {
        camera: editor.camera.clone(),
        rect: viewport(&idle(&mut editor, &context, 1.0)),
        ..view
    };
    // Half the ring radius along the edge-on line.
    let at = view.ring(Vec3::Y, 30.0);
    assert_eq!(at.y, view.screen(Vec3::ZERO).y);
    assert_drag_refusal_everywhere(
        &mut editor,
        &context,
        at,
        "y",
        "Object Y Rotate",
        "Rotate ring is edge-on to the camera",
    );
}

// A Z shaft seen almost end-on, grabbed 8 px beside the pivot. The along-axis parameter of the
// press ray changes sign across a short run of pointers, and the grab is refused wherever it is under
// 1 px of world length, so the test scans the real drag-begin for that run and takes its middle (the
// zero crossing) instead of hard-coding a pixel.
#[test]
fn a_scale_grab_too_close_to_the_pivot_reports_the_drag_refusal() {
    let (mut editor, context, view) = setup_with_camera(Key::R, 1.0, camera_at(0.1, 0.0));
    let capture = editor.move_viewport.as_ref().unwrap().capture.clone();
    let (_, frame) = editor.manipulation_frame(ActiveTool::Scale).unwrap();
    let centre = view.screen(Vec3::ZERO);
    let refused: Vec<f32> = (-400..=400)
        .map(|step| step as f32 * 0.05)
        .filter(|dx| {
            let at = centre + egui::vec2(*dx, 8.0);
            azteccraft::manipulator_handles::ScaleDrag::begin(
                capture.clone(),
                &frame,
                ScaleHandle::Z,
                glam::Vec2::new(at.x, at.y),
            )
            .err()
                == Some(azteccraft::move_handles::HandleError::ZeroBeginParameter)
        })
        .collect();
    assert!(
        !refused.is_empty(),
        "no pointer 8 px beside the pivot is refused as too close; the camera no longer produces the case"
    );
    let dx = (refused[0] + refused[refused.len() - 1]) / 2.0;
    let at = centre + egui::vec2(dx, 8.0);
    assert_drag_refusal_everywhere(
        &mut editor,
        &context,
        at,
        "z",
        "World Z Scale",
        "Scale handle was grabbed too close to the pivot",
    );
}
