//! The UV menu, the Planar Mapping options window and the read-only UV Editor (#360).
//!
//! Presentation only: Planar and Delete UVs dispatch the same typed operations as MCP. The UV
//! Editor reads the scene directly and never dispatches, so an open window cannot commit a draft
//! or be refused during a manipulation. Both windows are session state, and neither is a draft.
//! PROVISIONAL: Maya's docs do not place the UV Editor; here it is a floating window.

use crate::app::*;
use azteccraft::native_scene::{UvProjectAxis, UvSet};

const NO_SELECTION_REASON: &str = "Select one polygon object";
pub(in crate::app) const SEVERAL_SELECTED_REASON: &str =
    "Select one object: UV operations act on a single shape";
const COMPONENT_REASON: &str =
    "Use Object mode: projecting selected faces only is not implemented, tracked in #53";
const NO_SHAPE_REASON: &str = "The selected object needs exactly one direct mesh shape";
const SOURCED_REASON: &str = "UVs of a scene opened from a file cannot be edited yet";
const NOT_IMPLEMENTED_REASON: &str = "Not implemented, tracked in #53";

/// Maya's Create section rows above Planar, then the one below it, in documented order.
const CREATE_ROWS_BEFORE_PLANAR: [&str; 7] = [
    "Assign Checker Shader",
    "Automatic",
    "Best Plane Texturing Tool",
    "Camera-Based",
    "Contour Stretch",
    "Normal-Based",
    "Cylindrical",
];
const CREATE_ROWS_AFTER_PLANAR: [&str; 1] = ["Spherical"];
const CUT_SEW_ROWS_BEFORE_DELETE: [&str; 4] =
    ["Auto Seams", "Cut UV Edges", "Sew UV Edges", "Split UVs"];
const CUT_SEW_ROWS_AFTER_DELETE: [&str; 3] =
    ["Merge UVs", "3D Cut and Sew UV Tool", "3D Grab UV Tool"];

#[derive(Default)]
pub(in crate::app) struct UvUi {
    pub(in crate::app) editor_open: bool,
    pub(in crate::app) options_open: bool,
    /// The last Project from choice, for the session only.
    pub(in crate::app) axis: UvProjectAxis,
}

fn disabled_rows(ui: &mut egui::Ui, rows: &[&str]) {
    for row in rows {
        ui.add_enabled(false, egui::Button::new(*row))
            .on_disabled_hover_text(NOT_IMPLEMENTED_REASON);
    }
}

impl Editor {
    /// The shape UV operations target, or why there is none.
    pub(in crate::app) fn uv_target(&self) -> Result<NativeId, &'static str> {
        if self.selection_mode.is_component() {
            return Err(COMPONENT_REASON);
        }
        let shape = self.lead_uv_shape(true)?;
        if self.history.scene().has_source() {
            return Err(SOURCED_REASON);
        }
        Ok(shape)
    }

    fn lead_uv_shape(&self, single: bool) -> Result<NativeId, &'static str> {
        let target = if single {
            self.single_target()
        } else {
            self.lead_target()
        };
        let (node, _) = target.map_err(|error| match error {
            OperationError::MultipleSelection => SEVERAL_SELECTED_REASON,
            _ => NO_SELECTION_REASON,
        })?;
        let transform = self
            .history
            .scene()
            .node(node)
            .map_err(|_| NO_SELECTION_REASON)?
            .transform_id();
        let mut shapes = self.direct_shapes(transform);
        match (shapes.next(), shapes.next()) {
            (Some(shape), None) => Ok(shape),
            _ => Err(NO_SHAPE_REASON),
        }
    }

    pub(in crate::app) fn uv_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("UV", |ui| {
            let target = self.uv_target();
            let reason = target.err();
            ui.label("Editors");
            if ui.button("UV Editor").clicked() {
                self.uv.editor_open = true;
                ui.close();
            }
            disabled_rows(ui, &["UV Set Editor"]);
            ui.separator();
            ui.label("Create");
            disabled_rows(ui, &CREATE_ROWS_BEFORE_PLANAR);
            ui.horizontal(|ui| {
                let row = ui.add_enabled(reason.is_none(), egui::Button::new("Planar"));
                let row = match reason {
                    Some(reason) => row.on_disabled_hover_text(reason),
                    None => row,
                };
                if row.clicked() {
                    self.apply_planar();
                    ui.close();
                }
                let option_box =
                    features::primitive_options::option_box(ui, "Planar option box", true, None);
                if option_box.clicked() {
                    self.uv.options_open = true;
                    ui.close();
                }
            });
            disabled_rows(ui, &CREATE_ROWS_AFTER_PLANAR);
            ui.separator();
            ui.label("Cut/Sew");
            disabled_rows(ui, &CUT_SEW_ROWS_BEFORE_DELETE);
            let delete = ui.add_enabled(reason.is_none(), egui::Button::new("Delete UVs"));
            let delete = match reason {
                Some(reason) => delete.on_disabled_hover_text(reason),
                None => delete,
            };
            if delete.clicked() {
                if let Ok(shape) = target {
                    self.native_operation(Operation::DeleteUvs { shape });
                }
                ui.close();
            }
            disabled_rows(ui, &CUT_SEW_ROWS_AFTER_DELETE);
        });
    }

    fn apply_planar(&mut self) {
        match self.uv_target() {
            Ok(shape) => self.native_operation(Operation::ProjectUvPlanar {
                shape,
                axis: self.uv.axis,
            }),
            Err(reason) => {
                self.status = reason.into();
                self.error = true;
            }
        }
    }

    pub(in crate::app) fn uv_windows(&mut self, ctx: &egui::Context) {
        // Hidden, not closed, while a file modal or close is in progress.
        if self.files.is_modal() || self.files.is_closing() {
            return;
        }
        self.planar_options_window(ctx);
        self.uv_editor_window(ctx);
    }

    fn planar_options_window(&mut self, ctx: &egui::Context) {
        if !self.uv.options_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut close = false;
        let reason = self.uv_target().err();
        let mut axis = self.uv.axis;
        egui::Window::new("Planar Mapping Options")
            .id(egui::Id::new("planar_options_window"))
            .default_pos(PLANAR_WINDOW_POSITION)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Fit projection to: Bounding box");
                ui.add_enabled(false, egui::RadioButton::new(false, "Best plane"))
                    .on_disabled_hover_text(NOT_IMPLEMENTED_REASON);
                ui.label("Project from");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut axis, UvProjectAxis::X, "X axis");
                    ui.radio_value(&mut axis, UvProjectAxis::Y, "Y axis");
                    ui.radio_value(&mut axis, UvProjectAxis::Z, "Z axis");
                    ui.add_enabled(false, egui::RadioButton::new(false, "Camera"))
                        .on_disabled_hover_text(NOT_IMPLEMENTED_REASON);
                });
                for row in [
                    "Keep image width/height ratio",
                    "Insert projection before deformers",
                    "Create new UV Set",
                ] {
                    let mut unset = false;
                    ui.add_enabled(false, egui::Checkbox::new(&mut unset, row))
                        .on_disabled_hover_text(NOT_IMPLEMENTED_REASON);
                }
                ui.horizontal(|ui| {
                    let button = ui.add_enabled(reason.is_none(), egui::Button::new("Apply"));
                    let button = match reason {
                        Some(reason) => button.on_disabled_hover_text(reason),
                        None => button,
                    };
                    apply = button.clicked();
                    close = ui.button("Close").clicked();
                });
            });
        self.uv.axis = axis;
        if apply {
            self.apply_planar();
        }
        self.uv.options_open = open && !close;
    }

    fn uv_editor_window(&mut self, ctx: &egui::Context) {
        if !self.uv.editor_open {
            return;
        }
        let mut open = true;
        let shape = self.lead_uv_shape(false);
        let scene = self.history.scene();
        let sourced = scene.has_source();
        let set = shape.ok().and_then(|shape| scene.uv_set(shape));
        egui::Window::new("UV Editor")
            .id(egui::Id::new("uv_editor_window"))
            .default_pos(EDITOR_WINDOW_POSITION)
            .open(&mut open)
            .default_size([320.0, 340.0])
            .show(ctx, |ui| match (shape, set) {
                (Err(reason), _) => {
                    ui.label(reason);
                }
                (Ok(_), _) if sourced => {
                    ui.label("UVs of a scene opened from a file are not shown");
                }
                (Ok(_), None) => {
                    ui.label("This shape has no UVs");
                }
                (Ok(_), Some(set)) => {
                    ui.label("Read-only: map1");
                    draw_layout(ui, set);
                }
            });
        self.uv.editor_open = open;
    }
}

/// Over the viewport, because egui's default corner would cover the menus, Tool Box and Outliner.
const PLANAR_WINDOW_POSITION: egui::Pos2 = egui::pos2(360.0, 230.0);
const EDITOR_WINDOW_POSITION: egui::Pos2 = egui::pos2(720.0, 230.0);

/// One outline per face over the 0 to 1 square, with v up as in Maya's UV Editor.
fn draw_layout(ui: &mut egui::Ui, set: &UvSet) {
    let side = ui.available_size().min_elem().max(64.0);
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let grid = ui.visuals().weak_text_color();
    let line = ui.visuals().text_color();
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, grid),
        egui::StrokeKind::Inside,
    );
    let place = |uv: [f64; 2]| {
        egui::pos2(
            rect.left() + uv[0] as f32 * rect.width(),
            rect.bottom() - uv[1] as f32 * rect.height(),
        )
    };
    for corners in set.faces() {
        let points: Vec<_> = corners.iter().map(|uv| place(*uv)).collect();
        painter.add(egui::Shape::closed_line(
            points,
            egui::Stroke::new(1.0, line),
        ));
    }
}
