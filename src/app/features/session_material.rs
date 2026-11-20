//! Lighting/Shading > Assign New Material and Assign Existing Material on the selected shape
//! (#467). Assignment dispatches the same `Operation::AssignSessionMaterial` as MCP
//! `assign_session_material`, so the draft, modal, close and busy guards are shared.
//!
//! Maya assigns to every selected object (and to faces); this slice assigns to one object whose
//! transform has exactly one direct mesh shape, and says why otherwise (PROVISIONAL).
use crate::app::*;
use azteccraft::native_scene::{DEFAULT_LAMBERT_COLOR, MaterialChoice, SessionMaterialId};

const MENU_LABEL: &str = "Lighting/Shading";
const NO_SHAPE_REASON: &str = "The selected object needs exactly one direct mesh shape";
const COMPONENT_REASON: &str =
    "Use Object mode: assigning a material to selected faces is not implemented";
/// Maya's other Assign New Material types; only lambert exists in the session table.
const UNSUPPORTED_TYPES: [&str; 4] = ["Blinn", "Phong", "Phong E", "Standard Surface"];
const NOT_IMPLEMENTED_REASON: &str = "Not implemented: only lambert materials exist yet";

impl Editor {
    /// The shape an assignment targets, or why there is none.
    pub(in crate::app) fn session_material_target(&self) -> Result<NativeId, OperationError> {
        if self.selection_mode.is_component() {
            return Err(OperationError::InvalidInput(COMPONENT_REASON.into()));
        }
        let (node, _) = self.single_target()?;
        let transform = self
            .history
            .scene()
            .node(node)
            .map_err(OperationError::from_history)?
            .transform_id();
        let mut shapes = self.direct_shapes(transform);
        match (shapes.next(), shapes.next()) {
            (Some(shape), None) => Ok(shape),
            _ => Err(OperationError::InvalidInput(NO_SHAPE_REASON.into())),
        }
    }

    /// Why the assign rows are disabled, or `None` when an assignment may be attempted.
    pub(in crate::app) fn session_material_disabled_reason(&self) -> Option<String> {
        if let Some(refusal) = self.history.scene().session_material_edit_refusal() {
            return Some(refusal.to_string());
        }
        self.session_material_target().err().map(|e| e.to_string())
    }

    /// Assigns `choice` to the selected shape through the shared typed operation.
    pub(in crate::app) fn assign_material_to_selection(
        &mut self,
        choice: MaterialChoice,
    ) -> Result<OperationReply, OperationError> {
        let shape = match self.session_material_target() {
            Ok(shape) => shape,
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                return Err(error);
            }
        };
        self.native_operation_result(Operation::AssignSessionMaterial {
            shape,
            material: choice,
        })
    }

    /// Every session material in table order, with whether the target shape uses it.
    pub(in crate::app) fn session_material_choices(
        &self,
    ) -> Vec<(SessionMaterialId, String, bool)> {
        let scene = self.history.scene();
        let current = self
            .session_material_target()
            .ok()
            .and_then(|shape| scene.shape(shape).ok())
            .and_then(|shape| scene.session_material_id(shape));
        scene
            .session_materials()
            .iter()
            .enumerate()
            .map(|(index, lambert)| {
                let id = SessionMaterialId::from_index(index);
                (id, lambert.name().to_owned(), Some(id) == current)
            })
            .collect()
    }

    pub(in crate::app) fn lighting_shading_menu(&mut self, ui: &mut egui::Ui) {
        // Row order follows the Object-mode marking menu (Material Attributes, Assign New, Assign
        // Favorite, Assign Existing); its placement in this menu is PROVISIONAL.
        ui.menu_button(MENU_LABEL, |ui| {
            let reason = self.session_material_disabled_reason();
            let enabled = reason.is_none();
            let mut chosen = None;
            ui.add_enabled(false, egui::Button::new("Material Attributes"))
                .on_disabled_hover_text(
                    "Not implemented yet: use the Attribute Editor button in the Status Line",
                );
            ui.menu_button("Assign New Material", |ui| {
                let lambert = ui.add_enabled(enabled, egui::Button::new("Lambert"));
                let lambert = match &reason {
                    Some(reason) => lambert.on_disabled_hover_text(reason),
                    None => lambert,
                };
                if lambert.clicked() {
                    chosen = Some(MaterialChoice::New {
                        color: DEFAULT_LAMBERT_COLOR,
                    });
                    ui.close();
                }
                for label in UNSUPPORTED_TYPES {
                    ui.add_enabled(false, egui::Button::new(label))
                        .on_disabled_hover_text(NOT_IMPLEMENTED_REASON);
                }
            });
            ui.add_enabled(false, egui::Button::new("Assign Favorite Material"))
                .on_disabled_hover_text("Not implemented: there are no favourite materials");
            ui.menu_button("Assign Existing Material", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(320.0)
                    .show(ui, |ui| {
                        for (id, name, current) in self.session_material_choices() {
                            let row =
                                ui.add_enabled(enabled, egui::Button::selectable(current, name));
                            let row = match &reason {
                                Some(reason) => row.on_disabled_hover_text(reason),
                                None => row,
                            };
                            if row.clicked() {
                                chosen = Some(MaterialChoice::Existing(id));
                                ui.close();
                            }
                        }
                    });
            });
            if let Some(choice) = chosen {
                let _ = self.assign_material_to_selection(choice);
            }
        });
    }
}

#[cfg(test)]
mod tests;
