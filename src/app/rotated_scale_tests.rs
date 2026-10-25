//! Issue #253: a scale edit on a rotated node must apply on every axis.
use super::tests::remote;
use super::*;
use crate::editor_operations::{ManipulationSpace, ManipulationValue};
use azteccraft::history::{ManipulationFinish, ManipulationId};
use azteccraft::native_scene::{ScaleHandle, TransformTarget};

fn rotated_cube(rotation: [f64; 3]) -> (Editor, NodeId) {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    editor.channels[3] = rotation[0].to_string();
    editor.channels[4] = rotation[1].to_string();
    editor.channels[5] = rotation[2].to_string();
    assert!(editor.commit_pending());
    assert!(!editor.error, "rotate failed: {}", editor.status);
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .rotation,
        rotation
    );
    (editor, id)
}

fn scale_of(editor: &Editor, id: NodeId) -> [f64; 3] {
    editor.history.scene().node(id).unwrap().transform().scale
}

#[test]
fn channel_box_scale_applies_on_every_axis_after_rotation() {
    for (rotation, axis) in [
        ([45.0, 0.0, 0.0], 1),
        ([45.0, 0.0, 0.0], 2),
        ([0.0, 45.0, 0.0], 0),
        ([0.0, 45.0, 0.0], 2),
        ([0.0, 0.0, 45.0], 0),
        ([0.0, 0.0, 45.0], 1),
    ] {
        let (mut editor, id) = rotated_cube(rotation);
        let before = editor.history.scene().clone();
        editor.channels[6 + axis] = "2".into();
        let committed = editor.commit_pending();
        let mut expected = [1.0; 3];
        expected[axis] = 2.0;
        assert_eq!(
            scale_of(&editor, id),
            expected,
            "rotation {rotation:?} axis {axis}: committed={committed} status={:?} error={}",
            editor.status,
            editor.error
        );
        assert_ne!(editor.history.scene(), &before);
    }
}

#[test]
fn mcp_edit_node_scale_applies_on_every_axis_after_rotation() {
    let (mut editor, id) = rotated_cube([0.0, 0.0, 45.0]);
    remote(
        &mut editor,
        Operation::EditNode {
            id,
            name: None,
            translation: [None; 3],
            rotation_degrees: [None; 3],
            scale: [Some(2.0), None, None],
        },
    )
    .unwrap();
    assert_eq!(scale_of(&editor, id), [2.0, 1.0, 1.0]);
}

#[test]
fn mcp_set_transform_scale_applies_after_rotation() {
    let (mut editor, id) = rotated_cube([0.0, 0.0, 45.0]);
    remote(
        &mut editor,
        Operation::SetTransform {
            id,
            transform: TransformInput {
                translation: [0.0; 3],
                rotation_degrees: [0.0, 0.0, 45.0],
                scale: [2.0, 1.0, 1.0],
            },
        },
    )
    .unwrap();
    assert_eq!(scale_of(&editor, id), [2.0, 1.0, 1.0]);
}

fn press(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn click_type_enter(editor: &mut Editor, ctx: &egui::Context, index: usize, text: &str) {
    use super::tests::frame_with;
    let size = egui::vec2(1440.0, 900.0);
    let f = |e: &mut Editor, ev: Vec<egui::Event>| {
        frame_with(e, ctx, size, 1.0, ev);
    };
    let center = ctx
        .read_response(channel_edit_id(index))
        .unwrap()
        .rect
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    f(
        editor,
        vec![egui::Event::PointerMoved(center), button(true)],
    );
    f(editor, vec![button(false)]);
    f(editor, vec![press(egui::Key::A, egui::Modifiers::COMMAND)]);
    f(editor, vec![egui::Event::Text(text.into())]);
    f(editor, vec![press(egui::Key::Enter, egui::Modifiers::NONE)]);
    f(editor, vec![]);
}

#[test]
fn click_type_enter_rotate_then_scale() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let ctx = egui::Context::default();
    super::tests::frame_with(&mut editor, &ctx, egui::vec2(1440.0, 900.0), 1.0, vec![]);
    super::tests::frame_with(&mut editor, &ctx, egui::vec2(1440.0, 900.0), 1.0, vec![]);
    click_type_enter(&mut editor, &ctx, 5, "45");
    assert_eq!(
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .rotation,
        [0.0, 0.0, 45.0],
        "status {:?}",
        editor.status
    );
    click_type_enter(&mut editor, &ctx, 6, "2");
    assert_eq!(
        scale_of(&editor, id),
        [2.0, 1.0, 1.0],
        "status={:?} error={} channels={:?}",
        editor.status,
        editor.error,
        editor.channels
    );
}

#[test]
fn chained_scale_edits_after_rotation() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.single_target().unwrap().0;
    let ctx = egui::Context::default();
    let size = egui::vec2(1440.0, 900.0);
    super::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    super::tests::frame_with(&mut editor, &ctx, size, 1.0, vec![]);
    click_type_enter(&mut editor, &ctx, 5, "45");
    click_type_enter(&mut editor, &ctx, 6, "2");
    click_type_enter(&mut editor, &ctx, 7, "3");
    click_type_enter(&mut editor, &ctx, 8, "4");
    assert_eq!(
        scale_of(&editor, id),
        [2.0, 3.0, 4.0],
        "{:?} {}",
        editor.status,
        editor.error
    );
}

fn scale_tool_x(
    space: ManipulationSpace,
) -> (Editor, NodeId, Result<OperationReply, OperationError>) {
    let (mut editor, id) = rotated_cube([0.0, 0.0, 45.0]);
    let path = editor.single_target().unwrap().1.to_vec();
    let started = remote(
        &mut editor,
        Operation::BeginScale {
            target: TransformTarget { node: id, path },
            handle: ScaleHandle::X,
            space,
        },
    );
    let result = match started {
        Ok(OperationReply::ManipulationStarted { manipulation_id }) => {
            let mid = ManipulationId::from_decimal(&manipulation_id).unwrap();
            remote(
                &mut editor,
                Operation::UpdateManipulation {
                    id: mid,
                    value: ManipulationValue::Factor(2.0),
                },
            )
            .and_then(|_| {
                remote(
                    &mut editor,
                    Operation::FinishMove {
                        id: mid,
                        finish: ManipulationFinish::Commit,
                    },
                )
            })
        }
        other => other,
    };
    (editor, id, result)
}

#[test]
fn scale_tool_object_space_scales_a_rotated_node() {
    let (editor, id, result) = scale_tool_x(ManipulationSpace::Object);
    result.unwrap();
    assert_eq!(scale_of(&editor, id), [2.0, 1.0, 1.0]);
}

#[test]
fn inspection_scale_availability_follows_the_scale_space() {
    let (mut editor, _) = rotated_cube([0.0, 0.0, 45.0]);
    let scale = |editor: &Editor| {
        editor
            .inspect_manipulation()
            .unwrap()
            .kinds
            .into_iter()
            .find(|kind| kind.kind == "scale")
            .unwrap()
    };
    let set_space = |editor: &mut Editor, space| {
        remote(
            editor,
            Operation::SetToolSpace {
                tool: ActiveTool::Scale,
                space,
            },
        )
        .unwrap();
    };

    set_space(&mut editor, ManipulationSpace::World);
    let world = scale(&editor);
    assert!(world.available, "{world:?}");
    let reason = world.unavailable_reason.expect("per-axis refusal");
    assert!(
        reason.contains("Scale unsupported: that world axis is not aligned with a local axis"),
        "{reason}"
    );

    set_space(&mut editor, ManipulationSpace::Object);
    let object = scale(&editor);
    assert!(object.available, "{object:?}");
    assert_eq!(object.unavailable_reason, None);
}

#[test]
fn inspection_rotate_availability_follows_the_rotate_space() {
    let (mut editor, id) = rotated_cube([0.0, 0.0, 45.0]);
    editor.channels[6] = "-1".into();
    assert!(editor.commit_pending(), "{}", editor.status);
    assert_eq!(scale_of(&editor, id)[0], -1.0);
    let rotate = |editor: &Editor| {
        editor
            .inspect_manipulation()
            .unwrap()
            .kinds
            .into_iter()
            .find(|kind| kind.kind == "rotate")
            .unwrap()
    };
    let set_space = |editor: &mut Editor, space| {
        remote(
            editor,
            Operation::SetToolSpace {
                tool: ActiveTool::Rotate,
                space,
            },
        )
        .unwrap();
    };
    set_space(&mut editor, ManipulationSpace::Object);
    let object = rotate(&editor);
    assert!(object.available, "{object:?}");
    assert_eq!(object.unavailable_reason, None);
    set_space(&mut editor, ManipulationSpace::World);
    let world = rotate(&editor);
    assert!(!world.available, "{world:?}");
    assert!(
        world
            .unavailable_reason
            .unwrap()
            .contains("world rotation needs positive scale on the selected node")
    );
}
