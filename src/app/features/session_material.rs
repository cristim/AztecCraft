//! Lighting/Shading > Assign New Material and Assign Existing Material on the selected shape
//! (#467). Assignment dispatches the same `Operation::AssignSessionMaterial` as MCP
//! `assign_session_material`, so the draft, modal, close and busy guards are shared.
//!
//! Maya assigns to every selected object (and to faces); this slice assigns to every selected
//! object whose transform has exactly one direct mesh shape, refusing the whole selection and
//! saying why otherwise (PROVISIONAL), and not to faces.
use crate::app::*;
use azteccraft::native_scene::{
    DEFAULT_LAMBERT_COLOR, LambertAttribute, MAX_ASSIGN_SHAPES, MaterialChoice, SessionMaterialId,
};

const MENU_LABEL: &str = "Lighting/Shading";
const NO_SHAPE_REASON: &str = "The selected object needs exactly one direct mesh shape";
const MULTI_SHAPE_REASON: &str = "Every selected object needs exactly one direct mesh shape to assign a material to the selection";
const TOO_MANY_REASON: &str = "Too many selected shapes for one material assignment";
const NO_MATERIAL_REASON: &str = "The selected shape has no material to show";
const COMPONENT_REASON: &str =
    "Use Object mode: assigning a material to selected faces is not implemented";
/// Maya's other Assign New Material types; only lambert exists in the session table.
const UNSUPPORTED_TYPES: [&str; 4] = ["Blinn", "Phong", "Phong E", "Standard Surface"];
/// Shown under the attribute rows: they are stored and saved but the viewport ignores them.
const NO_VIEWPORT_EFFECT: &str =
    "Transparency, Ambient Color and Incandescence do not yet affect the viewport";
const NOT_IMPLEMENTED_REASON: &str = "Not implemented: only lambert materials exist yet";

impl Editor {
    /// The shapes an assignment targets, one per selected object in selection order (an object
    /// selected through two instances of one shape counts once), or why there are none.
    pub(in crate::app) fn session_material_targets(&self) -> Result<Vec<NativeId>, OperationError> {
        if self.selection_mode.is_component() {
            return Err(OperationError::InvalidInput(COMPONENT_REASON.into()));
        }
        if self.selection.len() <= 1 {
            return self.single_direct_shape().map(|shape| vec![shape]);
        }
        let scene = self.history.scene();
        let mut shapes = Vec::new();
        for member in self.selection.members() {
            let transform = scene
                .node(member.node)
                .map_err(OperationError::from_history)?
                .transform_id();
            let mut direct = self.direct_shapes(transform);
            let (Some(shape), None) = (direct.next(), direct.next()) else {
                return Err(OperationError::InvalidInput(MULTI_SHAPE_REASON.into()));
            };
            if !shapes.contains(&shape) {
                if shapes.len() == MAX_ASSIGN_SHAPES {
                    return Err(OperationError::InvalidInput(TOO_MANY_REASON.into()));
                }
                shapes.push(shape);
            }
        }
        Ok(shapes)
    }

    /// The one direct mesh shape of the single selected object, whatever the selection mode.
    fn single_direct_shape(&self) -> Result<NativeId, OperationError> {
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

    /// The shape Material Attributes shows, or why there is none: it needs one object with one
    /// direct shape that has a material tab (a session material, or a saved Phong binding).
    pub(in crate::app) fn material_attributes_target(&self) -> Result<NativeId, String> {
        let shape = self.single_direct_shape().map_err(|e| e.to_string())?;
        if self.shape_has_material_tab(shape) {
            Ok(shape)
        } else {
            Err(NO_MATERIAL_REASON.into())
        }
    }

    /// Lighting/Shading > Material Attributes: shows the Attribute Editor on the selected
    /// shape's material tab through the same two operations MCP calls, so the guards are shared.
    pub(in crate::app) fn show_material_attributes(
        &mut self,
    ) -> Result<OperationReply, OperationError> {
        let shape = match self.material_attributes_target() {
            Ok(shape) => shape,
            Err(reason) => {
                self.status = reason.clone();
                self.error = true;
                return Err(OperationError::InvalidInput(reason));
            }
        };
        self.native_operation_result(Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        })?;
        self.native_operation_result(Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::Material(shape),
        })
    }

    /// Why the assign rows are disabled, or `None` when an assignment may be attempted.
    pub(in crate::app) fn session_material_disabled_reason(&self) -> Option<String> {
        if let Some(refusal) = self.history.scene().session_material_edit_refusal() {
            return Some(refusal.to_string());
        }
        self.session_material_targets().err().map(|e| e.to_string())
    }

    /// Assigns `choice` to the shape of every selected object, as one undo step, through the
    /// shared typed operation.
    pub(in crate::app) fn assign_material_to_selection(
        &mut self,
        choice: MaterialChoice,
    ) -> Result<OperationReply, OperationError> {
        let shapes = match self.session_material_targets() {
            Ok(shapes) => shapes,
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
                return Err(error);
            }
        };
        self.native_operation_result(Operation::AssignSessionMaterial {
            shapes,
            material: choice,
        })
    }

    /// Every session material in table order, with whether the target shape uses it.
    pub(in crate::app) fn session_material_choices(
        &self,
    ) -> Vec<(SessionMaterialId, String, bool)> {
        let scene = self.history.scene();
        // With several shapes selected a row is current only when all of them share it
        // (PROVISIONAL: the docs name no such mark).
        let current = self.session_material_targets().ok().and_then(|shapes| {
            let mut ids = shapes.into_iter().map(|shape| {
                scene
                    .shape(shape)
                    .ok()
                    .and_then(|s| scene.session_material_id(s))
            });
            let first = ids.next()?;
            ids.all(|id| id == first).then_some(first).flatten()
        });
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

    /// The session-material section of a material tab; `false` when `shape` has none (sourced
    /// scenes, which show the Phong section instead). The swatch is a named image; the
    /// R/G/B fields edit the colour.
    pub(in crate::app) fn session_material_section(
        &mut self,
        ui: &mut egui::Ui,
        shape: NativeId,
    ) -> bool {
        let scene = self.history.scene();
        let Some((shape, lambert)) = scene
            .shape(shape)
            .ok()
            .and_then(|shape| scene.session_material(shape).map(|l| (shape, l)))
        else {
            return false;
        };
        let id = scene.session_material_id(shape);
        let group = id.and_then(|id| {
            scene
                .session_shading_groups()
                .iter()
                .find(|group| group.material() == id)
                .map(|group| group.name().to_owned())
        });
        let users = id.map_or(0, |id| scene.session_material_shape_count(id));
        let name = lambert.name().to_owned();
        let colours = LambertAttribute::ALL.map(|attribute| lambert.attribute(attribute));
        let shape = shape.id();
        ui.label(format!("Material: {name}"));
        if let Some(group) = group {
            ui.label(format!("Shading group: {group}"));
        }
        ui.label(format!(
            "{users} shape {}",
            if users == 1 { "user" } else { "users" }
        ));
        for (attribute, colour) in LambertAttribute::ALL.into_iter().zip(colours) {
            let [r, g, b] = colour.map(azteccraft::view_scene::channel_to_u8);
            let title = colour::row_label(attribute);
            let label = format!("{title}, {name}");
            ui.horizontal_wrapped(|ui| {
                ui.label(title);
                // The swatch only displays; the R/G/B fields below edit.
                let (rect, swatch) =
                    ui.allocate_exact_size(egui::vec2(48.0, 18.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 2.0, Color32::from_rgb(r, g, b));
                swatch.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::accesskit::Role::Image, true, label.as_str())
                });
            });
            // Its own row: wrapped inside the title row it grew the side panel over several frames.
            ui.horizontal_wrapped(|ui| self.session_color_fields(ui, shape, attribute));
        }
        ui.label(NO_VIEWPORT_EFFECT);
        true
    }

    pub(in crate::app) fn lighting_shading_menu(&mut self, ui: &mut egui::Ui) {
        // Row order follows the Object-mode marking menu (Material Attributes, Assign New, Assign
        // Favorite, Assign Existing); its placement in this menu is PROVISIONAL.
        ui.menu_button(MENU_LABEL, |ui| {
            let reason = self.session_material_disabled_reason();
            let enabled = reason.is_none();
            let mut chosen = None;
            let mut show_attributes = false;
            let attributes = self.material_attributes_target();
            let row = ui.add_enabled(attributes.is_ok(), egui::Button::new("Material Attributes"));
            let row = match &attributes {
                Err(reason) => row.on_disabled_hover_text(reason),
                Ok(_) => row,
            };
            if row.clicked() {
                show_attributes = true;
                ui.close();
            }
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
                if let Ok(shapes) = self.session_material_targets()
                    && shapes.len() > 1
                {
                    ui.label(format!("Applies to {} selected shapes", shapes.len()));
                }
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
            if show_attributes {
                let _ = self.show_material_attributes();
            }
            if let Some(choice) = chosen {
                let _ = self.assign_material_to_selection(choice);
            }
        });
    }
}

mod colour;
#[cfg(test)]
pub(in crate::app) use colour::colour_edit_id;
pub(in crate::app) use colour::session_attribute_field_ids;
#[cfg(test)]
mod section_tests;
#[cfg(test)]
mod tests;
