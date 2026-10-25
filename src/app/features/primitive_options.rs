//! The Create menu's option-box window (issue #174, PR B).
//!
//! Presentation only: the window holds text drafts and the last options that were created
//! successfully. Create dispatches the same typed `Operation::Create` as the menu and MCP, so the
//! human-draft and `commit_pending` rules apply. The window is not a scene draft and does not block
//! remote operations; only a focused field does, like every other text field.
//! PROVISIONAL: Maya's docs do not say whether Create keeps the window open or what a plain menu
//! click uses; here Create keeps it open and a plain click uses the remembered options.

use crate::app::*;
use azteccraft::primitive_options::{Axis, PrimitiveOptions};

/// Cube has the most numeric fields: three sizes and three subdivision counts.
const MAX_FIELDS: usize = 6;
const PRIMITIVES: [Primitive; 5] = [
    Primitive::Cube,
    Primitive::Plane,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Cone,
];

pub(in crate::app) fn option_field_ids() -> impl Iterator<Item = egui::Id> {
    (0..MAX_FIELDS).map(option_field_id)
}

fn option_field_id(index: usize) -> egui::Id {
    egui::Id::new(("primitive_option", index))
}

fn window_id() -> egui::Id {
    egui::Id::new("primitive_options_window")
}

fn slot(primitive: Primitive) -> usize {
    match primitive {
        Primitive::Cube => 0,
        Primitive::Plane => 1,
        Primitive::Sphere => 2,
        Primitive::Cylinder => 3,
        Primitive::Cone => 4,
    }
}

fn title(primitive: Primitive) -> &'static str {
    match primitive {
        Primitive::Cube => "Polygon Cube Options",
        Primitive::Plane => "Polygon Plane Options",
        Primitive::Sphere => "Polygon Sphere Options",
        Primitive::Cylinder => "Polygon Cylinder Options",
        Primitive::Cone => "Polygon Cone Options",
    }
}

/// Size labels, then subdivision labels; the order of `texts` and `assemble`.
fn labels(primitive: Primitive) -> (&'static [&'static str], &'static [&'static str]) {
    match primitive {
        Primitive::Cube => (
            &["Width", "Height", "Depth"],
            &[
                "Subdivisions width",
                "Subdivisions height",
                "Subdivisions depth",
            ],
        ),
        Primitive::Plane => (
            &["Width", "Height"],
            &["Subdivisions width", "Subdivisions height"],
        ),
        Primitive::Sphere => (&["Radius"], &["Subdivisions axis", "Subdivisions height"]),
        Primitive::Cylinder | Primitive::Cone => (
            &["Radius", "Height"],
            &["Subdivisions axis", "Subdivisions height"],
        ),
    }
}

fn texts(options: &PrimitiveOptions) -> Vec<String> {
    let (sizes, divisions): (Vec<f64>, Vec<u32>) = match *options {
        PrimitiveOptions::Cube {
            width,
            height,
            depth,
            sub_width,
            sub_height,
            sub_depth,
            ..
        } => (
            vec![width, height, depth],
            vec![sub_width, sub_height, sub_depth],
        ),
        PrimitiveOptions::Plane {
            width,
            height,
            sub_width,
            sub_height,
            ..
        } => (vec![width, height], vec![sub_width, sub_height]),
        PrimitiveOptions::Sphere {
            radius,
            sub_axis,
            sub_height,
            ..
        } => (vec![radius], vec![sub_axis, sub_height]),
        PrimitiveOptions::Cylinder {
            radius,
            height,
            sub_axis,
            sub_height,
            ..
        }
        | PrimitiveOptions::Cone {
            radius,
            height,
            sub_axis,
            sub_height,
            ..
        } => (vec![radius, height], vec![sub_axis, sub_height]),
    };
    sizes
        .iter()
        .map(f64::to_string)
        .chain(divisions.iter().map(u32::to_string))
        .collect()
}

fn axis_of(options: &PrimitiveOptions) -> Axis {
    match *options {
        PrimitiveOptions::Cube { axis, .. }
        | PrimitiveOptions::Plane { axis, .. }
        | PrimitiveOptions::Sphere { axis, .. }
        | PrimitiveOptions::Cylinder { axis, .. }
        | PrimitiveOptions::Cone { axis, .. } => axis,
    }
}

/// Parses the drafts and runs the same `validate` as MCP; the message names the offending field.
fn assemble(
    primitive: Primitive,
    texts: &[String],
    axis: Axis,
) -> Result<PrimitiveOptions, String> {
    let (size_labels, division_labels) = labels(primitive);
    if texts.len() != size_labels.len() + division_labels.len() {
        return Err("Option fields are out of sync; reopen the window".into());
    }
    let (size_texts, division_texts) = texts.split_at(size_labels.len());
    let mut sizes = Vec::with_capacity(size_texts.len());
    for (label, text) in size_labels.iter().zip(size_texts) {
        sizes.push(
            text.trim()
                .parse::<f64>()
                .map_err(|_| format!("{label} must be a number"))?,
        );
    }
    let mut divisions = Vec::with_capacity(division_texts.len());
    for (label, text) in division_labels.iter().zip(division_texts) {
        divisions.push(
            text.trim()
                .parse::<u32>()
                .map_err(|_| format!("{label} must be a whole number"))?,
        );
    }
    let options = match (primitive, sizes.as_slice(), divisions.as_slice()) {
        (Primitive::Cube, &[width, height, depth], &[sub_width, sub_height, sub_depth]) => {
            PrimitiveOptions::Cube {
                width,
                height,
                depth,
                sub_width,
                sub_height,
                sub_depth,
                axis,
            }
        }
        (Primitive::Plane, &[width, height], &[sub_width, sub_height]) => PrimitiveOptions::Plane {
            width,
            height,
            sub_width,
            sub_height,
            axis,
        },
        (Primitive::Sphere, &[radius], &[sub_axis, sub_height]) => PrimitiveOptions::Sphere {
            radius,
            sub_axis,
            sub_height,
            axis,
        },
        (Primitive::Cylinder, &[radius, height], &[sub_axis, sub_height]) => {
            PrimitiveOptions::Cylinder {
                radius,
                height,
                sub_axis,
                sub_height,
                axis,
            }
        }
        (Primitive::Cone, &[radius, height], &[sub_axis, sub_height]) => PrimitiveOptions::Cone {
            radius,
            height,
            sub_axis,
            sub_height,
            axis,
        },
        _ => return Err("Option fields are out of sync; reopen the window".into()),
    };
    options.validate(primitive).map_err(|e| e.to_string())?;
    Ok(options)
}

struct Dialog {
    primitive: Primitive,
    texts: Vec<String>,
    axis: Axis,
    message: Option<String>,
}

pub(in crate::app) struct PrimitiveOptionsUi {
    remembered: [PrimitiveOptions; 5],
    dialog: Option<Dialog>,
    /// Any text field (dialog, name, channel, ...) held focus at the end of the last frame; egui
    /// clears focus before Escape runs, so the key still belongs to that field.
    typing: bool,
}

impl Default for PrimitiveOptionsUi {
    fn default() -> Self {
        Self {
            remembered: PRIMITIVES.map(PrimitiveOptions::default_for),
            dialog: None,
            typing: false,
        }
    }
}

/// A small hollow square at the end of a menu row, like Maya's option box.
pub(in crate::app) fn option_box(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    reason: Option<&str>,
) -> egui::Response {
    let side = ui.spacing().interact_size.y;
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label)
            });
            if ui.is_rect_visible(rect) {
                let visuals = ui.style().interact_selectable(&response, false);
                ui.painter().rect_stroke(
                    rect.shrink(side * 0.28),
                    1.0,
                    egui::Stroke::new(1.5, visuals.fg_stroke.color),
                    egui::StrokeKind::Inside,
                );
            }
            response
        })
        .inner;
    match (enabled, reason) {
        (false, Some(reason)) => response.on_disabled_hover_text(reason),
        (true, _) => response.on_hover_text(label),
        _ => response,
    }
}

impl Editor {
    /// Options for a plain menu or shelf click: the last successfully created, else the defaults.
    pub(in crate::app) fn remembered_options(&self, primitive: Primitive) -> PrimitiveOptions {
        self.primitive_options.remembered[slot(primitive)]
    }

    /// Opens the window for `primitive`. One window at a time: another primitive replaces it.
    pub(in crate::app) fn open_primitive_options(&mut self, primitive: Primitive) {
        if self
            .primitive_options
            .dialog
            .as_ref()
            .is_some_and(|dialog| dialog.primitive == primitive)
        {
            return;
        }
        let options = self.remembered_options(primitive);
        self.primitive_options.dialog = Some(Dialog {
            primitive,
            texts: texts(&options),
            axis: axis_of(&options),
            message: None,
        });
    }

    /// Escape closes the window only when it has nothing else to cancel: a manipulation, a draft or
    /// a field being typed in take the key first (the existing cancel path runs for them).
    pub(in crate::app) fn escape_closes_primitive_options(&mut self, ctx: &egui::Context) -> bool {
        let closes = self.primitive_options.dialog.is_some()
            && !self.files.is_closing()
            && !self.primitive_options.typing
            && !self.has_draft()
            && self.history.active_manipulation().is_none()
            && self.native_move.is_none()
            && ctx.input(|input| input.key_pressed(Key::Escape));
        if closes {
            self.primitive_options.dialog = None;
            self.consume_pointer_sequence();
        }
        closes
    }

    pub(in crate::app) fn primitive_options_window(&mut self, ctx: &egui::Context) {
        let hidden = self.files.is_modal() || self.files.is_closing();
        let Some(mut dialog) = self.primitive_options.dialog.take() else {
            self.primitive_options.typing = !hidden && text_edit_focused(ctx);
            return;
        };
        // Hidden, not discarded, while a file modal or close is in progress.
        if hidden {
            self.primitive_options.typing = false;
            self.primitive_options.dialog = Some(dialog);
            return;
        }
        let read_only = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string());
        let mut open = true;
        let mut create = false;
        let mut close = false;
        egui::Window::new(title(dialog.primitive))
            .id(window_id())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                (create, close) = dialog_body(ui, &mut dialog, read_only.as_deref());
            });
        if create {
            self.create_from_dialog(&mut dialog);
        }
        self.primitive_options.typing = text_edit_focused(ctx);
        if open && !close {
            self.primitive_options.dialog = Some(dialog);
        }
    }

    fn create_from_dialog(&mut self, dialog: &mut Dialog) {
        match assemble(dialog.primitive, &dialog.texts, dialog.axis) {
            Err(message) => dialog.message = Some(message),
            Ok(options) => {
                self.native_operation(Operation::Create {
                    primitive: dialog.primitive,
                    options: Some(options),
                    name: None,
                });
                // native_operation sets `error` on every Create outcome.
                if !self.error {
                    self.primitive_options.remembered[slot(dialog.primitive)] = options;
                    dialog.message = None;
                } else {
                    dialog.message = Some(self.status.clone());
                }
            }
        }
    }
}

/// Returns (create clicked, close clicked).
fn dialog_body(ui: &mut egui::Ui, dialog: &mut Dialog, read_only: Option<&str>) -> (bool, bool) {
    let (size_labels, division_labels) = labels(dialog.primitive);
    egui::Grid::new("primitive_option_fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, (label, text)) in size_labels
                .iter()
                .chain(division_labels)
                .zip(dialog.texts.iter_mut())
                .enumerate()
            {
                let label = ui.label(*label);
                ui.add(
                    egui::TextEdit::singleline(text)
                        .id(option_field_id(index))
                        .desired_width(80.0),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
            ui.label("Axis");
            ui.horizontal(|ui| {
                for (axis, name) in [(Axis::X, "X"), (Axis::Y, "Y"), (Axis::Z, "Z")] {
                    ui.selectable_value(&mut dialog.axis, axis, name);
                }
                for name in ["Free", "Camera"] {
                    ui.add_enabled(false, egui::Button::new(name))
                        .on_disabled_hover_text(
                            "Axis modes other than X, Y and Z are not implemented",
                        );
                }
            });
            ui.end_row();
        });
    ui.separator();
    for (label, reason) in [
        (
            "Interactive creation",
            "Interactive creation is not implemented",
        ),
        ("Create UVs", "Meshes have no UVs yet"),
        ("Exit on completion", "Only applies to interactive creation"),
    ] {
        ui.add_enabled(false, egui::Checkbox::new(&mut false, label))
            .on_disabled_hover_text(reason);
    }
    if matches!(dialog.primitive, Primitive::Cylinder | Primitive::Cone) {
        for (label, reason) in [
            ("Subdivisions caps", "Cap divisions are not implemented"),
            ("Round cap", "Round caps are not implemented"),
        ] {
            ui.add_enabled(false, egui::Checkbox::new(&mut false, label))
                .on_disabled_hover_text(reason);
        }
    }
    if let Some(message) = &dialog.message {
        ui.colored_label(Color32::from_rgb(255, 160, 130), message);
    }
    ui.horizontal(|ui| {
        let create = ui
            .add_enabled(read_only.is_none(), egui::Button::new("Create"))
            .on_disabled_hover_text(read_only.unwrap_or_default())
            .clicked();
        let close = ui.button("Close").clicked();
        (create, close)
    })
    .inner
}

#[cfg(test)]
#[path = "primitive_options/tests.rs"]
mod tests;
