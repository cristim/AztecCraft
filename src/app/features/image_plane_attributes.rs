//! The Attribute Editor "Image Plane" tab, editable rows (#459 P9b-1). Every control sends the
//! same typed `SetImagePlane` operation as the MCP `set_image_plane` through `native_operation`,
//! so the human guards and the draft commit apply. A text field commits only when it lost focus
//! with text that differs from the stored value's own text, so an untouched field never rewrites a
//! value; text that does not parse is refused with the field named and the value kept.
//!
//! The row names follow the Maya Image Plane Attributes page (2022 and 2026 pages; PROVISIONAL
//! for 2027). Read-only and disabled rows are P9b-2.

use std::path::PathBuf;

use azteccraft::image_plane::{
    model::{CoverageParts, ImagePlane, ImagePlaneEdit, PlateDisplayMode, PlateFit},
    ops::ImagePlaneState,
};
use eframe::egui::{self, RichText};

use crate::{app::Editor, editor_operations::Operation};

/// The text fields, in `Editor::plane_drafts` order.
const NAME: usize = 0;
const OFFSET: usize = 1;
const CACHE: usize = 2;
const GAIN: usize = 3;
const DEPTH: usize = 4;
const COVERAGE_WIDTH: usize = 5;
const COVERAGE_HEIGHT: usize = 6;
const ORIGIN_X: usize = 7;
const ORIGIN_Y: usize = 8;
pub(in crate::app) const FIELDS: usize = 9;

/// Row labels, with the unit where there is one.
const LABELS: [&str; FIELDS] = [
    "Image Name",
    "Frame Offset (frames)",
    "Frame Cache (frames)",
    "Alpha Gain",
    "Depth (cm)",
    "Coverage Width (pixels)",
    "Coverage Height (pixels)",
    "Coverage Origin X (pixels)",
    "Coverage Origin Y (pixels)",
];

pub(in crate::app) fn field_id(index: usize) -> egui::Id {
    egui::Id::unique(("image_plane_field", index))
}

pub(in crate::app) fn field_ids() -> impl Iterator<Item = egui::Id> {
    (0..FIELDS).map(field_id)
}

const FITS: [(PlateFit, &str); 5] = [
    (PlateFit::Fill, "Fill"),
    (PlateFit::Best, "Best"),
    (PlateFit::Horizontal, "Horizontal"),
    (PlateFit::Vertical, "Vertical"),
    (PlateFit::ToSize, "To Size"),
];

const MODES: [(PlateDisplayMode, &str); 6] = [
    (PlateDisplayMode::None, "None"),
    (PlateDisplayMode::Outline, "Outline"),
    (PlateDisplayMode::Rgb, "RGB"),
    (PlateDisplayMode::Rgba, "RGBA"),
    (PlateDisplayMode::Luminance, "Luminance"),
    (PlateDisplayMode::Alpha, "Alpha"),
];

/// The text each field shows for the stored plane. With no coverage stored the full image is
/// shown, so an untouched coverage field never differs from it.
fn texts(plane: &ImagePlane, size: (u32, u32)) -> [String; FIELDS] {
    let c = plane
        .coverage
        .unwrap_or(azteccraft::image_plane::model::Coverage {
            origin_x: 0,
            origin_y: 0,
            width: size.0,
            height: size.1,
        });
    [
        plane.image_name.to_string_lossy().into_owned(),
        plane.frame_offset.to_string(),
        plane.frame_cache.to_string(),
        plane.alpha_gain.to_string(),
        plane.depth.to_string(),
        c.width.to_string(),
        c.height.to_string(),
        c.origin_x.to_string(),
        c.origin_y.to_string(),
    ]
}

/// The edit a blurred field asks for, or a message naming the field when its text does not parse.
fn edit_for(index: usize, text: &str) -> Result<ImagePlaneEdit, String> {
    let label = LABELS[index];
    let bad = |what: &str| format!("{label} must be {what}");
    // The name passes through unchanged (the operation's own rules decide what is valid); only
    // the numeric fields ignore surrounding spaces.
    let numeric = text.trim();
    let mut edit = ImagePlaneEdit::default();
    match index {
        NAME => edit.image_name = Some(PathBuf::from(text)),
        OFFSET => edit.frame_offset = Some(numeric.parse().map_err(|_| bad("a whole number"))?),
        CACHE => edit.frame_cache = Some(numeric.parse().map_err(|_| bad("a whole number"))?),
        GAIN | DEPTH => {
            let value: f32 = numeric
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| bad("a number"))?;
            if index == GAIN {
                edit.alpha_gain = Some(value);
            } else {
                edit.depth = Some(value);
            }
        }
        _ => {
            let value = numeric.parse().map_err(|_| bad("a whole number"))?;
            let mut parts = CoverageParts::default();
            match index {
                COVERAGE_WIDTH => parts.width = Some(value),
                COVERAGE_HEIGHT => parts.height = Some(value),
                ORIGIN_X => parts.origin_x = Some(value),
                _ => parts.origin_y = Some(value),
            }
            edit.coverage_parts = parts;
        }
    }
    Ok(edit)
}

impl Editor {
    /// The editable rows of the Image Plane tab.
    pub(in crate::app) fn image_plane_section(&mut self, ui: &mut egui::Ui) {
        let Some(ImagePlaneState { plane, image_size }) = self.image_plane.clone() else {
            ui.weak("There is no image plane.");
            return;
        };
        let enabled = !self.files.is_modal() && !self.files.is_closing();
        ui.label(RichText::new("Image Plane Attributes").strong());
        let current = texts(&plane, image_size);
        if !self.plane_drafts_primed {
            self.plane_drafts.clone_from(&current);
            self.plane_drafts_primed = true;
        }
        let mut blurred = Vec::new();
        let mut edits: Vec<ImagePlaneEdit> = Vec::new();
        egui::Grid::new("image_plane_rows")
            .num_columns(2)
            .spacing([8.0, 5.0])
            .show(ui, |ui| {
                self.plane_text_row(ui, NAME, enabled, &mut blurred);
                ui.label("Use Image Sequence");
                let mut sequence = plane.use_frame_extension;
                let response = ui.add_enabled(enabled, egui::Checkbox::new(&mut sequence, ""));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::CheckBox,
                        enabled,
                        sequence,
                        "Use Image Sequence",
                    )
                });
                if response.changed() {
                    edits.push(ImagePlaneEdit {
                        use_frame_extension: Some(sequence),
                        ..Default::default()
                    });
                }
                ui.end_row();
                self.plane_text_row(ui, OFFSET, enabled, &mut blurred);
                self.plane_text_row(ui, CACHE, enabled, &mut blurred);
                if let Some(mode) = combo(ui, "Display Mode", &MODES, plane.display_mode, enabled) {
                    edits.push(ImagePlaneEdit {
                        display_mode: Some(mode),
                        ..Default::default()
                    });
                }
                self.plane_text_row(ui, GAIN, enabled, &mut blurred);
                if let Some(fit) = combo(ui, "Fit", &FITS, plane.fit, enabled) {
                    edits.push(ImagePlaneEdit {
                        fit: Some(fit),
                        ..Default::default()
                    });
                }
                self.plane_text_row(ui, DEPTH, enabled, &mut blurred);
                for index in [COVERAGE_WIDTH, COVERAGE_HEIGHT, ORIGIN_X, ORIGIN_Y] {
                    self.plane_text_row(ui, index, enabled, &mut blurred);
                }
            });
        let can_reset = enabled && plane.coverage.is_some();
        let reset = ui.add_enabled(
            can_reset,
            egui::Button::new(super::image_plane::RESET_COVERAGE_LABEL),
        );
        let reason = if enabled {
            "The full image is already used"
        } else {
            "a dialog is open"
        };
        if !can_reset {
            super::primitive_options::describe_unavailable(ui.ctx(), reset.id, reason);
        }
        if reset.clicked() {
            edits.push(ImagePlaneEdit {
                coverage: Some(None),
                ..Default::default()
            });
        }
        // The Resolved File and Frame rows are P9b-2.
        for index in blurred {
            if self.plane_drafts[index] == current[index] {
                continue;
            }
            match edit_for(index, &self.plane_drafts[index]) {
                Ok(edit) => edits.push(edit),
                Err(message) => {
                    self.status = message;
                    self.error = true;
                }
            }
        }
        // A field that is not being edited follows the stored value again: this is how refused
        // or unparsed text is replaced. A field that just lost focus is refreshed after its commit.
        for (index, text) in current.iter().enumerate() {
            let focused = ui.ctx().memory(|memory| memory.has_focus(field_id(index)));
            if !focused {
                self.plane_drafts[index].clone_from(text);
            }
        }
        for edit in edits {
            self.native_operation(Operation::SetImagePlane(edit));
        }
    }

    /// One labelled text row; pushes its index to `blurred` when it lost focus.
    fn plane_text_row(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        enabled: bool,
        blurred: &mut Vec<usize>,
    ) {
        ui.label(LABELS[index]);
        let response = ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(&mut self.plane_drafts[index])
                .id(field_id(index))
                .interactive(enabled)
                .desired_width(if index == NAME { 220.0 } else { 80.0 }),
        );
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_label(LABELS[index]);
        });
        if !enabled {
            super::primitive_options::describe_unavailable(
                ui.ctx(),
                response.id,
                "a dialog is open",
            );
        }
        if response.lost_focus() {
            blurred.push(index);
        }
        ui.end_row();
    }
}

/// A labelled combo; returns the newly chosen value.
pub(super) fn combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &'static str,
    options: &[(T, &'static str)],
    current: T,
    enabled: bool,
) -> Option<T> {
    ui.label(label);
    let mut chosen = None;
    let shown = options
        .iter()
        .find_map(|(value, name)| (*value == current).then_some(*name))
        .unwrap_or_default();
    let combo = ui
        .add_enabled_ui(enabled, |ui| {
            egui::ComboBox::from_id_salt(label)
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (value, name) in options {
                        if ui.selectable_label(*value == current, *name).clicked() {
                            chosen = Some(*value);
                        }
                    }
                })
        })
        .inner;
    combo
        .response
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, enabled, label));
    if !enabled {
        super::primitive_options::describe_unavailable(
            ui.ctx(),
            combo.response.id,
            "a dialog is open",
        );
    }
    ui.end_row();
    chosen.filter(|value| *value != current)
}
