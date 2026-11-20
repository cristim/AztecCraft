use super::*;
use crate::app::tests::{click_at, frame_with, key};
use crate::editor_operations::{ActiveTool, ManipulationSpace, Operation, OperationReply};
use azteccraft::{native_scene::RotateAxis, scene::Primitive};
use eframe::egui::accesskit;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

struct Rig {
    editor: Editor,
    ctx: egui::Context,
    output: egui::FullOutput,
}

impl Rig {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        editor.create(Primitive::Cube);
        editor.native_operation(Operation::SetActiveTool(ActiveTool::Rotate));
        let mut rig = Rig {
            editor,
            ctx,
            output: egui::FullOutput::default(),
        };
        rig.settle();
        rig
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.output = frame_with(&mut self.editor, &self.ctx, SIZE, 1.0, events);
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn node(&self, label: &str) -> accesskit::Node {
        let update = self
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap();
        update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(label))
            .unwrap_or_else(|| panic!("no node named {label:?}"))
            .1
            .clone()
    }

    fn click(&mut self, label: &str) {
        let bounds = self.node(label).bounds().unwrap();
        let at = egui::pos2(
            ((bounds.x0 + bounds.x1) / 2.0) as f32,
            ((bounds.y0 + bounds.y1) / 2.0) as f32,
        );
        self.output = click_at(&mut self.editor, &self.ctx, SIZE, at);
    }

    /// Types `text` into the increment field and presses Enter.
    fn enter_increment(&mut self, text: &str) {
        self.click(STEP_FIELD_LABEL);
        self.editor.manipulation.rotate_step_text = text.to_owned();
        self.frame(vec![key(egui::Key::Enter)]);
        self.settle();
    }
}

#[test]
fn the_combo_turns_the_step_on_with_the_default_and_off_again() {
    let mut rig = Rig::new();
    assert_eq!(rig.editor.manipulation.rotate_step(), None);
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Off"));
    rig.click(STEP_COMBO_LABEL);
    rig.click("Relative");
    rig.settle();
    assert_eq!(rig.editor.manipulation.rotate_step(), Some(15.0));
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Relative"));
    assert_eq!(rig.node(STEP_FIELD_LABEL).value(), Some("15"));
    rig.click(STEP_COMBO_LABEL);
    rig.click("Off");
    rig.settle();
    assert_eq!(rig.editor.manipulation.rotate_step(), None);
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Off"));
}

#[test]
fn a_typed_increment_sets_the_step_and_round_trips_through_inspection() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.settle();
    rig.enter_increment("7.5");
    assert_eq!(rig.editor.manipulation.rotate_step(), Some(7.5));
    assert!(!rig.editor.error);
    assert_eq!(
        rig.editor
            .inspect(false)
            .unwrap()
            .manipulation
            .rotate_step_degrees,
        Some(7.5)
    );
    rig.click(STEP_COMBO_LABEL);
    rig.click("Off");
    rig.settle();
    assert_eq!(
        rig.editor
            .inspect(false)
            .unwrap()
            .manipulation
            .rotate_step_degrees,
        None
    );
}

#[test]
fn an_invalid_increment_shows_an_error_and_keeps_the_old_step() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.settle();
    for bad in [
        "0",
        "-5",
        "nan",
        "NaN",
        "inf",
        "-inf",
        "1e-7",
        "0.0000001",
        "abc",
        "",
        "5e-324",
    ] {
        rig.editor.error = false;
        rig.enter_increment(bad);
        assert_eq!(
            rig.editor.manipulation.rotate_step(),
            Some(15.0),
            "increment {bad:?}"
        );
        assert!(rig.editor.error, "increment {bad:?} must show an error");
        assert_eq!(rig.node(STEP_FIELD_LABEL).value(), Some("15"), "{bad:?}");
    }
}

// The core accepts any finite step of at least 1e-6, so 1e308 is not refused: every turn then
// rounds to zero, which leaves the object untouched instead of overflowing.
#[test]
fn a_huge_finite_increment_is_accepted_like_the_mcp_tool() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.settle();
    rig.enter_increment("1e308");
    assert_eq!(rig.editor.manipulation.rotate_step(), Some(1e308));
}

#[test]
fn the_control_reflects_a_step_set_through_the_shared_operation() {
    let mut rig = Rig::new();
    let reply = rig
        .editor
        .apply_operation(Operation::SetRotateStep {
            degrees: Some(22.5),
        })
        .unwrap();
    assert!(matches!(reply, OperationReply::Changed));
    rig.settle();
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Relative"));
    assert_eq!(rig.node(STEP_FIELD_LABEL).value(), Some("22.5"));
    rig.editor
        .apply_operation(Operation::SetRotateStep { degrees: None })
        .unwrap();
    rig.settle();
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Off"));
    // Off keeps the last increment visible but disabled.
    assert_eq!(rig.node(STEP_FIELD_LABEL).value(), Some("22.5"));
    assert!(rig.node(STEP_FIELD_LABEL).is_disabled());
    assert_eq!(
        rig.node(STEP_FIELD_LABEL).description(),
        Some("Unavailable: Step Snap is Off")
    );
}

#[test]
fn the_controls_are_disabled_with_a_reason_during_a_manipulation() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.settle();
    assert!(!rig.node(STEP_COMBO_LABEL).is_disabled());
    assert!(!rig.node(STEP_FIELD_LABEL).is_disabled());
    let target = rig.editor.selected_move_target().unwrap();
    rig.editor
        .apply_operation(Operation::BeginRotate {
            target,
            axis: RotateAxis::Z,
            space: ManipulationSpace::Object,
        })
        .unwrap();
    rig.settle();
    for label in [STEP_COMBO_LABEL, STEP_FIELD_LABEL] {
        let node = rig.node(label);
        assert!(node.is_disabled(), "{label}");
        assert_eq!(
            node.description(),
            Some("Unavailable: a manipulation is in progress"),
            "{label}"
        );
    }
}

#[test]
fn the_controls_are_disabled_with_a_reason_while_a_dialog_is_open() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.editor.request_close();
    assert!(rig.editor.files.is_modal());
    rig.settle();
    for label in [STEP_COMBO_LABEL, STEP_FIELD_LABEL] {
        let node = rig.node(label);
        assert!(node.is_disabled(), "{label}");
        assert_eq!(node.description(), Some("Unavailable: a dialog is open"));
    }
}

// Keyboard path: Tab alone reaches the combo and then the increment field.
#[test]
fn tab_reaches_the_combo_and_the_increment_field() {
    let mut rig = Rig::new();
    rig.editor.native_operation(Operation::SetRotateStep {
        degrees: Some(15.0),
    });
    rig.settle();
    let mut reached = Vec::new();
    for _ in 0..150 {
        rig.frame(vec![key(egui::Key::Tab)]);
        let update = rig
            .output
            .platform_output
            .accesskit_update
            .as_ref()
            .unwrap();
        let label = update
            .nodes
            .iter()
            .find(|(id, _)| *id == update.focus)
            .and_then(|(_, node)| node.label().map(str::to_owned));
        if let Some(label) = label
            && [STEP_COMBO_LABEL, STEP_FIELD_LABEL].contains(&label.as_str())
            && !reached.contains(&label)
        {
            reached.push(label);
        }
    }
    assert_eq!(reached, [STEP_COMBO_LABEL, STEP_FIELD_LABEL]);
}
