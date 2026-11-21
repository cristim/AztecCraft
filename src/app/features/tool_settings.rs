//! The Tool Settings panel (#491): the active tool's settings in the right-hand dock, opened from
//! the Status Line sidebar button, a double-click on a Tool Box tool or Windows > Editors > Tool
//! Settings. The Maya 2027 Tool Settings page (Maya-Basics GUID-1DB77C1E-6176-4987-B997-ED9CE55EF1CD)
//! names the menu path Windows > General Editors; the 2027 Windows menu page groups it under
//! Editors, which `editors_menu` follows.
//!
//! It draws the controls the row above the viewport draws (`manipulation_options`) and dispatches
//! the same typed operations (`SetMoveSpace`, `SetToolSpace`, `SetToolHandle`, the numeric
//! Begin/Update/Finish path, `SetRotateStep`, `SetScaleStep`), so native and MCP share one state.
//! The settings Maya lists that AztecCraft lacks are named as not implemented rather than drawn as
//! dead controls. The Settings section's name and the order of Axis Orientation, Step Snap and the
//! missing settings follow the Maya 2027 Move, Rotate and Scale Tool pages
//! (GUID-9BCE8B23-6DA6-4C7F-B897-7E25D6E8A865, GUID-C2D0F80C-812B-465D-A634-DB6ECFABFCB3,
//! GUID-487B6A23-E135-45E2-94F1-B0D59D6B6F33). Numeric Entry is an AztecCraft convenience with no
//! Maya Tool Settings equivalent and is labelled as such. The section is always open: a
//! collapsed body would stop drawing a field whose draft commits on focus loss. The title row,
//! the label column and Reset Tool's placement are PROVISIONAL: the pages' screenshots were not
//! compared.

use crate::app::Editor;
use crate::app::features::manipulation_tool::{SettingsChoice, tool_name};
use crate::app::features::primitive_options::describe_unavailable;
use crate::editor_operations::ActiveTool;
use eframe::egui::{self, RichText};

pub(in crate::app) const PANEL_TITLE: &str = "Tool Settings";
pub(in crate::app) const AXIS_ORIENTATION_LABEL: &str = "Axis Orientation";
pub(in crate::app) const NUMERIC_FIELD_LABEL: &str = "Numeric entry value";
pub(in crate::app) const NUMERIC_HEADING: &str = "Numeric Entry (AztecCraft, not a Maya setting)";
pub(in crate::app) const RESET_TOOL_LABEL: &str = "Reset Tool";
const RESET_TOOL_REASON: &str = "Reset Tool is not implemented yet";

/// The settings of each tool's first section that AztecCraft does not implement, in the order of
/// the Maya 2027 tool pages.
fn missing_settings(tool: ActiveTool) -> &'static str {
    match tool {
        ActiveTool::Move => {
            "Pivot, Transform Constraint, Step Snap, Preserve Children, Preserve UVs, Tweak Mode"
        }
        ActiveTool::Rotate => {
            "Pivot, Rotate Center, Modify Translation, Transform Constraint, Preserve Children, \
             Preserve UVs, Free Rotate (#492)"
        }
        ActiveTool::Scale => {
            "Pivot, Scale Center, Transform Constraint, Preserve Children, Preserve UVs, \
             Prevent Negative Scale"
        }
        ActiveTool::Select => "",
    }
}

/// The sections after the first that AztecCraft does not implement.
fn missing_sections(tool: ActiveTool) -> &'static str {
    match tool {
        ActiveTool::Move => {
            "Joint Orient Settings, Move Snap Settings, Common Selection Options, Soft Selection, \
             Symmetry Settings, Smart Duplicate Settings"
        }
        ActiveTool::Rotate | ActiveTool::Scale => {
            "Common Selection Options, Soft Selection, Symmetry Settings, Smart Duplicate Settings"
        }
        ActiveTool::Select => "Common Selection Options, Soft Selection, Symmetry Settings",
    }
}

fn name_control(response: &egui::Response, role: egui::Role, label: &str, value: &str) {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(role, enabled, label));
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_value(value.to_owned());
    });
}

impl Editor {
    pub(in crate::app) fn tool_settings(&mut self, ui: &mut egui::Ui) {
        let tool = self.active_tool;
        ui.label(RichText::new(PANEL_TITLE).strong());
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{} Tool", tool_name(tool))).heading());
            let reset = ui
                .add_enabled(false, egui::Button::new(RESET_TOOL_LABEL))
                .on_disabled_hover_text(RESET_TOOL_REASON);
            describe_unavailable(ui.ctx(), reset.id, RESET_TOOL_REASON);
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if tool == ActiveTool::Select {
                    ui.weak(format!(
                        "Not implemented yet: {}",
                        missing_sections(ActiveTool::Select)
                    ));
                    return;
                }
                ui.label(RichText::new(format!("{} Settings", tool_name(tool))).strong());
                self.transform_settings(ui, tool);
                ui.separator();
                ui.weak(format!(
                    "Other sections not implemented yet: {}",
                    missing_sections(tool)
                ));
            });
    }

    // Move, Rotate or Scale Settings: Axis Orientation and, for Rotate and Scale, Step Snap, then
    // the missing Maya settings; Rotate and Scale end with the AztecCraft numeric entry.
    fn transform_settings(&mut self, ui: &mut egui::Ui, tool: ActiveTool) {
        let unavailable = self.manipulation_unavailable();
        let mut choice = SettingsChoice::default();
        ui.horizontal(|ui| {
            ui.label(AXIS_ORIENTATION_LABEL);
            let combo = ui
                .add_enabled_ui(unavailable.is_none(), |ui| {
                    self.orientation_combo(ui, tool, &mut choice)
                })
                .inner;
            name_control(
                &combo,
                egui::Role::ComboBox,
                AXIS_ORIENTATION_LABEL,
                self.orientation_name(tool),
            );
            if let Some(reason) = unavailable {
                describe_unavailable(ui.ctx(), combo.id, reason);
            }
        });
        self.step_settings(ui, tool);
        ui.weak(format!("Not implemented yet: {}", missing_settings(tool)));
        if tool == ActiveTool::Move {
            self.apply_settings_choice(tool, choice);
            return;
        }
        ui.separator();
        ui.label(RichText::new(NUMERIC_HEADING).strong());
        let (label, handle_label) = if tool == ActiveTool::Rotate {
            ("Axis", "Numeric entry axis")
        } else {
            ("Handle", "Numeric entry handle")
        };
        ui.horizontal(|ui| {
            ui.label(label);
            let combo = ui
                .add_enabled_ui(unavailable.is_none(), |ui| {
                    self.handle_combo(ui, tool, &mut choice)
                })
                .inner;
            name_control(
                &combo,
                egui::Role::ComboBox,
                handle_label,
                self.handle_name(tool),
            );
            if let Some(reason) = unavailable {
                describe_unavailable(ui.ctx(), combo.id, reason);
            }
        });
        let field = ui
            .add_enabled_ui(unavailable.is_none(), |ui| {
                ui.horizontal(|ui| self.numeric_entry(ui, tool, &mut choice))
                    .inner
            })
            .inner;
        let text = self.numeric_text().to_owned();
        name_control(&field, egui::Role::TextInput, NUMERIC_FIELD_LABEL, &text);
        if let Some(reason) = unavailable {
            describe_unavailable(ui.ctx(), field.id, reason);
        }
        self.apply_settings_choice(tool, choice);
    }
}

#[cfg(test)]
#[path = "tool_settings/tests.rs"]
mod tests;
