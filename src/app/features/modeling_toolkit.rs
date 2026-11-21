use crate::app::{Editor, SelectionModeIntent};
use crate::editor_operations::{SelectionMode, SidebarPanel};
use eframe::egui;

impl Editor {
    pub(in crate::app) fn mesh_tools_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Mesh Tools", |ui| {
            let panel = SidebarPanel::ModelingToolkit;
            let visible = self.sidebar_panel_visible(panel);
            let label = if visible {
                "Hide Modeling Toolkit"
            } else {
                "Show Modeling Toolkit"
            };
            let refusal = self.display_switch_refusal();
            if ui
                .add_enabled(refusal.is_none(), egui::Button::new(label))
                .on_disabled_hover_text(refusal.unwrap_or_default())
                .clicked()
            {
                self.sidebar_panel_request = Some((panel, !visible));
                ui.close();
            }
            unsupported(ui, &["Multi-Cut", "Target Weld", "Connect", "Quad Draw"]);
        });
    }

    pub(in crate::app) fn modeling_toolkit(
        &mut self,
        ui: &mut egui::Ui,
        selection_intent: &mut Option<SelectionModeIntent>,
    ) {
        ui.heading("Modeling Toolkit");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label("Selection");
            ui.horizontal_wrapped(|ui| {
                for (label, mode, intent) in [
                    ("Object", SelectionMode::Object, SelectionModeIntent::Object),
                    ("Vertex", SelectionMode::Vertex, SelectionModeIntent::Vertex),
                    ("Edge", SelectionMode::Edge, SelectionModeIntent::Edge),
                    ("Face", SelectionMode::Face, SelectionModeIntent::Face),
                ] {
                    if ui
                        .selectable_label(self.selection_mode == mode, label)
                        .clicked()
                    {
                        selection_intent.get_or_insert(intent);
                    }
                }
            });
            unsupported(
                ui,
                &[
                    "Multi-Component",
                    "Convert Selection",
                    "Selection Constraints",
                ],
            );
            ui.separator();
            ui.label("Mesh");
            unsupported(ui, &["Combine", "Separate", "Smooth", "Boolean"]);
            ui.label("Components");
            let refusal = self.extrude_refusal();
            if ui
                .add_enabled(refusal.is_none(), egui::Button::new("Extrude Options"))
                .on_hover_text("Extrude one selected polygon face")
                .on_disabled_hover_text(refusal.unwrap_or_default())
                .clicked()
            {
                self.open_extrude_options();
            }
            unsupported(ui, &["Bevel", "Bridge", "Add Divisions"]);
            ui.label("Tools");
            unsupported(ui, &["Multi-Cut", "Target Weld", "Connect", "Quad Draw"]);
        });
    }
}

fn unsupported(ui: &mut egui::Ui, labels: &[&str]) {
    ui.horizontal_wrapped(|ui| {
        for label in labels {
            ui.add_enabled(false, egui::Button::new(*label))
                .on_disabled_hover_text("Not implemented in this panel");
        }
    });
}
