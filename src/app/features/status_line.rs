use crate::app::{Editor, SelectionModeIntent};
use crate::editor_operations::{SelectionMode, SidebarPanel};
use crate::tool_icons::icon_button;
use azteccraft::history::ManipulationProvenance;
use azteccraft::ui_icons::UiIcon;
use eframe::egui::{self, Align, Layout};

const MENU_SETS: [&str; 5] = ["Modeling", "Rigging", "Animation", "FX", "Rendering"];
const ACTIVE_MENU_SET: &str = "Modeling";
const MENU_SET_LABEL: &str = "Menu set";
const MENU_SET_UNAVAILABLE: &str = "not implemented yet";
const SELECTION_MODE_MENU_LABEL: &str = "Selection mode menu";
const SELECTION_MODE_MENU_HOVER: &str = "Selection mode menu (presets): not implemented yet";

struct StripButton {
    label: &'static str,
    icon: UiIcon,
    hover: &'static str,
    /// `None` while the panel is unsupported.
    panel: Option<SidebarPanel>,
}

struct ModeButton {
    label: &'static str,
    icon: UiIcon,
    hover: &'static str,
    target: (SelectionModeIntent, SelectionMode),
}

const SELECTION_MODES: [ModeButton; 3] = [
    ModeButton {
        label: "Hierarchy selection",
        icon: UiIcon::SelectHierarchy,
        hover: "Hierarchy selection (Root only; Leaf and Template unsupported)",
        target: (SelectionModeIntent::Hierarchy, SelectionMode::Hierarchy),
    },
    ModeButton {
        label: "Object selection",
        icon: UiIcon::SelectObject,
        hover: "Object selection (F8 toggles)",
        target: (SelectionModeIntent::Object, SelectionMode::Object),
    },
    ModeButton {
        label: "Component selection",
        icon: UiIcon::SelectComponent,
        hover: "Component selection (keeps the current component mode; from Object or Hierarchy enters Face; pick Vertex, Edge or Face with the buttons to the right)",
        target: (SelectionModeIntent::Component, SelectionMode::Face),
    },
];

/// The order is provisional: no fetched Maya 2026 page names the Status Line component-type icons.
const COMPONENT_TYPES: [ModeButton; 3] = [
    ModeButton {
        label: "Vertex selection",
        icon: UiIcon::SelectVertex,
        hover: "Vertex selection  F9. Component modes are exclusive here, unlike Maya's per-type selection masks. UV is unsupported.",
        target: (SelectionModeIntent::Vertex, SelectionMode::Vertex),
    },
    ModeButton {
        label: "Edge selection",
        icon: UiIcon::SelectEdge,
        hover: "Edge selection  F10. Component modes are exclusive here, unlike Maya's per-type selection masks. UV is unsupported.",
        target: (SelectionModeIntent::Edge, SelectionMode::Edge),
    },
    ModeButton {
        label: "Face selection",
        icon: UiIcon::SelectFace,
        hover: "Face selection  F11. Component modes are exclusive here, unlike Maya's per-type selection masks. UV is unsupported.",
        target: (SelectionModeIntent::Face, SelectionMode::Face),
    },
];

/// Left to right as drawn at the right end of the strip.
const SIDEBAR_TOGGLES: [StripButton; 5] = [
    StripButton {
        label: "Modeling Toolkit",
        icon: UiIcon::SidebarModelingToolkit,
        hover: "Modeling Toolkit: not implemented yet",
        panel: None,
    },
    StripButton {
        label: "HumanIK",
        icon: UiIcon::SidebarHumanIk,
        hover: "HumanIK: not implemented yet",
        panel: None,
    },
    StripButton {
        label: "Attribute Editor",
        icon: UiIcon::SidebarAttributeEditor,
        hover: "Show or hide the Attribute Editor",
        panel: Some(SidebarPanel::AttributeEditor),
    },
    StripButton {
        label: "Tool Settings",
        icon: UiIcon::SidebarToolSettings,
        hover: "Tool Settings: not implemented yet",
        panel: None,
    },
    StripButton {
        label: "Channel Box / Layer Editor",
        icon: UiIcon::SidebarChannelBox,
        hover: "Show or hide the Channel Box (Layer Editor: not implemented yet)",
        panel: Some(SidebarPanel::ChannelBox),
    },
];

impl Editor {
    /// An MCP-owned manipulation, which a click cannot cancel.
    fn remote_manipulation_active(&self) -> bool {
        self.history
            .active_manipulation()
            .is_some_and(|active| active.provenance == ManipulationProvenance::Remote)
    }

    /// Enabled hover text, or the reason a live toggle is disabled.
    fn sidebar_toggle_hover(&self, button: &StripButton) -> String {
        let reason = if self.files.is_closing() {
            "the window is closing"
        } else if self.files.is_modal() {
            "a dialog is open"
        } else if self.history.active_manipulation().is_some() {
            "a Move is active"
        } else {
            return button.hover.into();
        };
        format!("{}: unavailable while {reason}", button.label)
    }

    pub(in crate::app) fn status_line(
        &mut self,
        ui: &mut egui::Ui,
        selection_intent: &mut Option<SelectionModeIntent>,
    ) {
        let admitted = !self.files.is_modal() && !self.files.is_closing();
        let mode_admitted = admitted && !self.remote_manipulation_active();
        ui.horizontal(|ui| {
            menu_set_selector(ui);
            ui.separator();
            for action in self.file_actions() {
                let response = icon_button(
                    ui,
                    action.label,
                    action.icon,
                    action.enabled,
                    false,
                    action.hover(),
                );
                if response.clicked() {
                    (action.run)(self);
                }
            }
            ui.separator();
            selection_mode_menu(ui);
            for button in &SELECTION_MODES {
                let (intent, mode) = button.target;
                // The Component button stays pressed in every component mode.
                let selected = if mode.is_component() {
                    self.selection_mode.is_component()
                } else {
                    mode == self.selection_mode
                };
                let response = icon_button(
                    ui,
                    button.label,
                    button.icon,
                    mode_admitted,
                    selected,
                    button.hover,
                );
                if response.clicked() {
                    selection_intent.get_or_insert(intent);
                }
            }
            ui.separator();
            for button in &COMPONENT_TYPES {
                let (intent, mode) = button.target;
                let response = icon_button(
                    ui,
                    button.label,
                    button.icon,
                    mode_admitted,
                    mode == self.selection_mode,
                    button.hover,
                );
                if response.clicked() {
                    selection_intent.get_or_insert(intent);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                for button in SIDEBAR_TOGGLES.iter().rev() {
                    match button.panel {
                        Some(panel) => {
                            let visible = self.sidebar_panel_visible(panel);
                            let enabled = admitted && self.history.active_manipulation().is_none();
                            let response = icon_button(
                                ui,
                                button.label,
                                button.icon,
                                enabled,
                                visible,
                                &self.sidebar_toggle_hover(button),
                            );
                            if response.clicked() {
                                self.sidebar_panel_request = Some((panel, !visible));
                            }
                        }
                        None => strip_button(ui, button),
                    }
                }
            });
        });
    }
}

fn strip_button(ui: &mut egui::Ui, button: &StripButton) {
    icon_button(ui, button.label, button.icon, false, false, button.hover);
}

fn menu_set_selector(ui: &mut egui::Ui) {
    let combo = egui::ComboBox::from_id_salt("menu_set")
        .selected_text(ACTIVE_MENU_SET)
        .show_ui(ui, |ui| {
            for set in MENU_SETS {
                let active = set == ACTIVE_MENU_SET;
                let entry = ui.add_enabled(active, egui::Button::selectable(active, set));
                if !active {
                    entry.on_disabled_hover_text(MENU_SET_UNAVAILABLE);
                }
            }
        });
    let enabled = combo.response.enabled();
    combo.response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, MENU_SET_LABEL)
    });
}

/// Presets need mask state (not implemented), so the control is disabled, stateless and empty.
fn selection_mode_menu(ui: &mut egui::Ui) {
    let combo = ui
        .add_enabled_ui(false, |ui| {
            egui::ComboBox::from_id_salt("selection_mode_menu")
                .selected_text("Presets")
                .show_ui(ui, |_| {})
        })
        .inner
        .response;
    let enabled = combo.enabled();
    combo.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::ComboBox,
            enabled,
            SELECTION_MODE_MENU_LABEL,
        )
    });
    combo.on_disabled_hover_text(SELECTION_MODE_MENU_HOVER);
}

#[cfg(test)]
#[path = "status_line/tests.rs"]
mod tests;
