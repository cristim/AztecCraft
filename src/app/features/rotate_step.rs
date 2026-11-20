//! The Rotate and Scale tools' Step Snap rows in the Tool Box: an Off/Relative combo and the
//! increment (degrees, or the scale factor). They dispatch `Operation::SetRotateStep` and
//! `Operation::SetScaleStep`, the operations `set_rotate_step` and `set_scale_step` use, so the
//! editor state is the one MCP reads and writes. Maya names: `manipRotateContext` and
//! `manipScaleContext` `-snap`, `-snapRelative` and `-snapValue`; the 15 degree and 0.25 factor
//! defaults are PROVISIONAL (not stated by the docs).

use crate::app::Editor;
use crate::app::features::primitive_options::describe_unavailable;
use crate::editor_operations::OperationError;
use eframe::egui;

pub(in crate::app) const STEP_COMBO_LABEL: &str = "Step Snap";
pub(in crate::app) const STEP_FIELD_LABEL: &str = "Step Snap increment (degrees)";
pub(in crate::app) const SCALE_STEP_COMBO_LABEL: &str = "Scale Step Snap";
pub(in crate::app) const SCALE_STEP_FIELD_LABEL: &str = "Scale Step Snap increment (factor)";
const OFF_REASON: &str = "Step Snap is Off";

pub(in crate::app) fn step_field_id() -> egui::Id {
    egui::Id::unique("rotate_step_increment")
}

pub(in crate::app) fn scale_step_field_id() -> egui::Id {
    egui::Id::unique("scale_step_increment")
}

/// Which tool's row is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum StepKind {
    Rotate,
    Scale,
}

impl StepKind {
    fn combo_label(self) -> &'static str {
        match self {
            Self::Rotate => STEP_COMBO_LABEL,
            Self::Scale => SCALE_STEP_COMBO_LABEL,
        }
    }

    fn field_label(self) -> &'static str {
        match self {
            Self::Rotate => STEP_FIELD_LABEL,
            Self::Scale => SCALE_STEP_FIELD_LABEL,
        }
    }

    fn mode_salt(self) -> &'static str {
        match self {
            Self::Rotate => "rotate_step_mode",
            Self::Scale => "scale_step_mode",
        }
    }

    fn default_increment(self) -> &'static str {
        match self {
            Self::Rotate => "15",
            Self::Scale => "0.25",
        }
    }

    fn field_id(self) -> egui::Id {
        match self {
            Self::Rotate => step_field_id(),
            Self::Scale => scale_step_field_id(),
        }
    }
}

impl Editor {
    /// Draws the row; returns the step the user asked for (`Some(None)` is Off) for the caller to
    /// dispatch, like the other Tool Box selectors.
    pub(in crate::app) fn step_row(
        &mut self,
        ui: &mut egui::Ui,
        kind: StepKind,
    ) -> Option<Option<f64>> {
        let unavailable = self.manipulation_unavailable();
        let step = match kind {
            StepKind::Rotate => self.manipulation.rotate_step(),
            StepKind::Scale => self.manipulation.scale_step(),
        };
        let typing = ui.ctx().memory(|memory| memory.has_focus(kind.field_id()));
        let text = self.manipulation.step_text_mut(kind);
        if !typing {
            if let Some(value) = step {
                *text = value.to_string();
            } else if text.is_empty() {
                *text = kind.default_increment().into();
            }
        }
        let mut turn_on = None;
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.label(kind.combo_label());
            let selected = if step.is_some() { "Relative" } else { "Off" };
            let combo = ui
                .add_enabled_ui(unavailable.is_none(), |ui| {
                    egui::ComboBox::from_id_salt(kind.mode_salt())
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(step.is_none(), "Off").clicked() {
                                turn_on = Some(false);
                            }
                            if ui.selectable_label(step.is_some(), "Relative").clicked() {
                                turn_on = Some(true);
                            }
                        })
                })
                .inner
                .response;
            name_control(&combo, egui::Role::ComboBox, kind.combo_label(), selected);
            if let Some(reason) = unavailable {
                describe_unavailable(ui.ctx(), combo.id, reason);
            }
            let field_reason = unavailable.or((step.is_none()).then_some(OFF_REASON));
            let response = ui
                .add_enabled_ui(field_reason.is_none(), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(self.manipulation.step_text_mut(kind))
                            .id(kind.field_id())
                            .desired_width(60.0),
                    )
                })
                .inner;
            let text = self.manipulation.step_text_mut(kind).clone();
            name_control(&response, egui::Role::TextInput, kind.field_label(), &text);
            if let Some(reason) = field_reason {
                describe_unavailable(ui.ctx(), response.id, reason);
            }
            commit = response.lost_focus();
        });
        match (turn_on, commit) {
            (Some(false), _) => Some(None),
            (Some(true), _) | (None, true) => {
                let text = std::mem::take(self.manipulation.step_text_mut(kind));
                match text.trim().parse::<f64>() {
                    Ok(value) => Some(Some(value)),
                    Err(_) => {
                        self.set_error_status(&OperationError::InvalidInput(
                            "Enter a number for the Step Snap increment".into(),
                        ));
                        self.error = true;
                        None
                    }
                }
            }
            (None, false) => None,
        }
    }
}

fn name_control(response: &egui::Response, role: egui::Role, label: &str, value: &str) {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(role, enabled, label));
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_value(value.to_owned());
    });
}

#[cfg(test)]
#[path = "rotate_step/tests.rs"]
mod tests;
