//! The Rotate tool's Step Snap row in the Tool Box: an Off/Relative combo and the increment in
//! degrees. It dispatches `Operation::SetRotateStep`, the operation `set_rotate_step` uses, so
//! the editor state is the one MCP reads and writes. Maya names: `manipRotateContext -snap`,
//! `-snapRelative` and `-snapValue`; the 15 degree default is PROVISIONAL (not stated by the docs).

use crate::app::Editor;
use crate::app::features::primitive_options::describe_unavailable;
use crate::editor_operations::OperationError;
use eframe::egui;

const DEFAULT_INCREMENT: &str = "15";
pub(in crate::app) const STEP_COMBO_LABEL: &str = "Step Snap";
pub(in crate::app) const STEP_FIELD_LABEL: &str = "Step Snap increment (degrees)";
const OFF_REASON: &str = "Step Snap is Off";

pub(in crate::app) fn step_field_id() -> egui::Id {
    egui::Id::unique("rotate_step_increment")
}

impl Editor {
    /// Draws the row; returns the step the user asked for (`Some(None)` is Off) for the caller to
    /// dispatch, like the other Tool Box selectors.
    pub(in crate::app) fn rotate_step_row(&mut self, ui: &mut egui::Ui) -> Option<Option<f64>> {
        let unavailable = self.manipulation_unavailable();
        let step = self.manipulation.rotate_step();
        let typing = ui.ctx().memory(|memory| memory.has_focus(step_field_id()));
        if !typing {
            if let Some(degrees) = step {
                self.manipulation.rotate_step_text = degrees.to_string();
            } else if self.manipulation.rotate_step_text.is_empty() {
                self.manipulation.rotate_step_text = DEFAULT_INCREMENT.into();
            }
        }
        let mut turn_on = None;
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.label(STEP_COMBO_LABEL);
            let selected = if step.is_some() { "Relative" } else { "Off" };
            let combo = ui
                .add_enabled_ui(unavailable.is_none(), |ui| {
                    egui::ComboBox::from_id_salt("rotate_step_mode")
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
            name_control(&combo, egui::Role::ComboBox, STEP_COMBO_LABEL, selected);
            if let Some(reason) = unavailable {
                describe_unavailable(ui.ctx(), combo.id, reason);
            }
            let field_reason = unavailable.or((step.is_none()).then_some(OFF_REASON));
            let response = ui
                .add_enabled_ui(field_reason.is_none(), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.manipulation.rotate_step_text)
                            .id(step_field_id())
                            .desired_width(60.0),
                    )
                })
                .inner;
            let text = self.manipulation.rotate_step_text.clone();
            name_control(&response, egui::Role::TextInput, STEP_FIELD_LABEL, &text);
            if let Some(reason) = field_reason {
                describe_unavailable(ui.ctx(), response.id, reason);
            }
            commit = response.lost_focus();
        });
        match (turn_on, commit) {
            (Some(false), _) => Some(None),
            (Some(true), _) | (None, true) => {
                let text = std::mem::take(&mut self.manipulation.rotate_step_text);
                match text.trim().parse::<f64>() {
                    Ok(degrees) => Some(Some(degrees)),
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
