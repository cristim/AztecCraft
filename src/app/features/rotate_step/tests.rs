use super::*;
use crate::app::tests::{click_at, frame_with, key};
use crate::editor_operations::{ActiveTool, ManipulationSpace, Operation, OperationReply};
use azteccraft::{
    native_scene::{RotateAxis, ScaleHandle},
    scene::Primitive,
};
use eframe::egui::accesskit;

const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

struct Rig {
    editor: Editor,
    ctx: egui::Context,
    output: egui::FullOutput,
}

impl Rig {
    fn new() -> Self {
        Self::with_tool(ActiveTool::Rotate)
    }

    fn with_tool(tool: ActiveTool) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        editor.create(Primitive::Cube);
        editor.native_operation(Operation::SetActiveTool(tool));
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

fn scale_rig() -> Rig {
    Rig::with_tool(ActiveTool::Scale)
}

fn scale_step_inspected(rig: &Rig) -> Option<f64> {
    rig.editor.inspect(false).unwrap().manipulation.scale_step
}

#[test]
fn each_tool_draws_only_its_own_step_row() {
    let mut rig = Rig::new();
    assert_eq!(rig.node(STEP_COMBO_LABEL).value(), Some("Off"));
    assert!(!has_node(&rig, SCALE_STEP_COMBO_LABEL));
    rig.editor
        .native_operation(Operation::SetActiveTool(ActiveTool::Scale));
    rig.settle();
    assert_eq!(rig.node(SCALE_STEP_COMBO_LABEL).value(), Some("Off"));
    assert!(!has_node(&rig, STEP_COMBO_LABEL));
}

fn has_node(rig: &Rig, label: &str) -> bool {
    rig.output
        .platform_output
        .accesskit_update
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .any(|(_, node)| node.label() == Some(label))
}

#[test]
fn the_scale_combo_dispatches_set_scale_step_and_matches_inspection() {
    let mut rig = scale_rig();
    assert_eq!(scale_step_inspected(&rig), None);
    rig.click(SCALE_STEP_COMBO_LABEL);
    rig.click("Relative");
    rig.settle();
    assert_eq!(rig.editor.manipulation.scale_step(), Some(0.25));
    assert_eq!(scale_step_inspected(&rig), Some(0.25));
    // The Rotate step is a different operation and stays untouched.
    assert_eq!(rig.editor.manipulation.rotate_step(), None);
    assert_eq!(rig.node(SCALE_STEP_COMBO_LABEL).value(), Some("Relative"));
    assert_eq!(rig.node(SCALE_STEP_FIELD_LABEL).value(), Some("0.25"));
    rig.click(SCALE_STEP_COMBO_LABEL);
    rig.click("Off");
    rig.settle();
    assert_eq!(scale_step_inspected(&rig), None);
}

#[test]
fn a_typed_scale_increment_sets_the_step_and_a_bad_one_keeps_it() {
    let mut rig = scale_rig();
    rig.editor
        .native_operation(Operation::SetScaleStep { step: Some(0.25) });
    rig.settle();
    rig.click(SCALE_STEP_FIELD_LABEL);
    rig.editor.manipulation.scale_step_text = "0.5".into();
    rig.frame(vec![key(egui::Key::Enter)]);
    rig.settle();
    assert_eq!(scale_step_inspected(&rig), Some(0.5));
    assert_eq!(rig.editor.manipulation.rotate_step(), None);
    for bad in ["0", "-1", "nan", "abc", "", "1e-7"] {
        rig.editor.error = false;
        rig.click(SCALE_STEP_FIELD_LABEL);
        rig.editor.manipulation.scale_step_text = bad.into();
        rig.frame(vec![key(egui::Key::Enter)]);
        rig.settle();
        assert_eq!(scale_step_inspected(&rig), Some(0.5), "{bad:?}");
        assert!(rig.editor.error, "{bad:?}");
    }
}

#[test]
fn the_scale_control_reflects_a_step_set_through_the_shared_operation() {
    let mut rig = scale_rig();
    rig.editor
        .apply_operation(Operation::SetScaleStep { step: Some(2.0) })
        .unwrap();
    rig.settle();
    assert_eq!(rig.node(SCALE_STEP_COMBO_LABEL).value(), Some("Relative"));
    assert_eq!(rig.node(SCALE_STEP_FIELD_LABEL).value(), Some("2"));
    rig.editor
        .apply_operation(Operation::SetScaleStep { step: None })
        .unwrap();
    rig.settle();
    assert_eq!(rig.node(SCALE_STEP_COMBO_LABEL).value(), Some("Off"));
    assert!(rig.node(SCALE_STEP_FIELD_LABEL).is_disabled());
    assert_eq!(
        rig.node(SCALE_STEP_FIELD_LABEL).description(),
        Some("Unavailable: Step Snap is Off")
    );
}

#[test]
fn the_scale_controls_are_disabled_with_a_reason_during_a_manipulation() {
    let mut rig = scale_rig();
    rig.editor
        .native_operation(Operation::SetScaleStep { step: Some(0.25) });
    rig.settle();
    assert!(!rig.node(SCALE_STEP_COMBO_LABEL).is_disabled());
    let target = rig.editor.selected_move_lead().unwrap();
    rig.editor
        .apply_operation(Operation::BeginScale {
            target,
            handle: ScaleHandle::Uniform,
            space: ManipulationSpace::World,
            expected_selection_generation: None,
        })
        .unwrap();
    rig.settle();
    for label in [SCALE_STEP_COMBO_LABEL, SCALE_STEP_FIELD_LABEL] {
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
fn the_scale_controls_are_disabled_with_a_reason_while_a_dialog_is_open() {
    let mut rig = scale_rig();
    rig.editor
        .native_operation(Operation::SetScaleStep { step: Some(0.25) });
    rig.editor.request_close();
    assert!(rig.editor.files.is_modal());
    rig.settle();
    for label in [SCALE_STEP_COMBO_LABEL, SCALE_STEP_FIELD_LABEL] {
        let node = rig.node(label);
        assert!(node.is_disabled(), "{label}");
        assert_eq!(node.description(), Some("Unavailable: a dialog is open"));
    }
}
