//! The Attribute Editor "Camera" tab (#458): the live camera's film back. Every control sends the
//! same typed `SetCameraFilmBack` operation as the MCP `set_camera_film_back` through
//! `native_operation_result`, so the human guards (modal, closing, pending draft, held drag) apply.
//! A text field commits only when it lost focus with text that differs from the stored value's own
//! text, so an untouched field never rewrites a value; text that does not parse is refused with
//! the field named and the value kept, and a value the operation refuses shows its error.
//!
//! The tab belongs to the session camera, like the Image Plane tab, so it needs no selected node
//! and always resolves. Units are Maya's: millimetres, inches, degrees; Film Translate and Roll
//! Pivot are normalized device units (PROVISIONAL, as in `set_camera_film_back`).
//!
//! PROVISIONAL (#5): the row order follows the Maya 2027 Attribute Editor Film Back page (Camera
//! Aperture, Lens Squeeze Ratio, Film Fit Offset, Film Offset, Pre Scale, Film Translate, Film
//! Roll Pivot, Film Roll Value, Film Roll Order, Post Scale), with Focal Length first, then Film
//! Fit, Overscan, Camera Scale and Display Film Gate, whose placement the pages do not state. The
//! always-present tab is ours (Maya opens a camera's attributes on its shape tab from the panel
//! menu). The page names no defaults or ranges beyond Film Offset 0 and the Create Camera
//! defaults; ranges are the operation's own. There are no sliders: Maya's film back rows are
//! fields, and the stored values are exact f64 that a slider would round.

use azteccraft::viewport::{FilmBack, FilmBackEdit, FilmFit, FilmRollOrder, Lens, Projection};
use eframe::egui::{self, RichText};

use super::image_plane_attributes::combo;
use crate::{
    app::Editor,
    editor_operations::{Operation, OperationReply},
};

const FOCAL: usize = 0;
const APERTURE_H: usize = 1;
const APERTURE_V: usize = 2;
const SQUEEZE: usize = 3;
const FIT_OFFSET: usize = 4;
const OFFSET_H: usize = 5;
const OFFSET_V: usize = 6;
const PRE_SCALE: usize = 7;
const TRANSLATE_H: usize = 8;
const TRANSLATE_V: usize = 9;
const PIVOT_H: usize = 10;
const PIVOT_V: usize = 11;
const ROLL: usize = 12;
const POST_SCALE: usize = 13;
const OVERSCAN: usize = 14;
const CAMERA_SCALE: usize = 15;
pub(in crate::app) const FIELDS: usize = 16;

/// Row labels with their unit, also the names the accessibility tree and the status line use.
const LABELS: [&str; FIELDS] = [
    "Focal Length (mm)",
    "Horizontal Film Aperture (in)",
    "Vertical Film Aperture (in)",
    "Lens Squeeze Ratio",
    "Film Fit Offset (in)",
    "Horizontal Film Offset (in)",
    "Vertical Film Offset (in)",
    "Pre Scale",
    "Horizontal Film Translate",
    "Vertical Film Translate",
    "Horizontal Film Roll Pivot",
    "Vertical Film Roll Pivot",
    "Film Roll Value (degrees)",
    "Post Scale",
    "Overscan",
    "Camera Scale",
];

pub(in crate::app) fn field_id(index: usize) -> egui::Id {
    egui::Id::unique(("camera_field", index))
}

pub(in crate::app) fn field_ids() -> impl Iterator<Item = egui::Id> {
    (0..FIELDS).map(field_id)
}

const FITS: [(FilmFit, &str); 4] = [
    (FilmFit::Fill, "Fill"),
    (FilmFit::Horizontal, "Horizontal"),
    (FilmFit::Vertical, "Vertical"),
    (FilmFit::Overscan, "Overscan"),
];

const ORDERS: [(FilmRollOrder, &str); 2] = [
    (FilmRollOrder::RotateTranslate, "Rotate Translate"),
    (FilmRollOrder::TranslateRotate, "Translate Rotate"),
];

/// The text each field shows: the shortest text that parses back to the stored f64.
fn texts(film: &FilmBack) -> [String; FIELDS] {
    [
        film.focal_mm,
        film.aperture_in[0],
        film.aperture_in[1],
        film.lens_squeeze,
        film.film_fit_offset_in,
        film.film_offset_in[0],
        film.film_offset_in[1],
        film.pre_scale,
        film.film_translate[0],
        film.film_translate[1],
        film.film_roll_pivot[0],
        film.film_roll_pivot[1],
        film.film_roll_degrees,
        film.post_scale,
        film.overscan,
        film.camera_scale,
    ]
    .map(|value| value.to_string())
}

/// The edit a blurred field asks for, or a message naming the field when its text is not a number.
/// Whether the number is acceptable (finite, positive, in range) is the operation's decision.
fn edit_for(index: usize, text: &str) -> Result<FilmBackEdit, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("{} must be a number", LABELS[index]))?;
    let mut edit = FilmBackEdit::default();
    match index {
        FOCAL => edit.focal_mm = Some(value),
        APERTURE_H => edit.aperture_in[0] = Some(value),
        APERTURE_V => edit.aperture_in[1] = Some(value),
        SQUEEZE => edit.lens_squeeze = Some(value),
        FIT_OFFSET => edit.film_fit_offset_in = Some(value),
        OFFSET_H => edit.film_offset_in[0] = Some(value),
        OFFSET_V => edit.film_offset_in[1] = Some(value),
        PRE_SCALE => edit.pre_scale = Some(value),
        TRANSLATE_H => edit.film_translate[0] = Some(value),
        TRANSLATE_V => edit.film_translate[1] = Some(value),
        PIVOT_H => edit.film_roll_pivot[0] = Some(value),
        PIVOT_V => edit.film_roll_pivot[1] = Some(value),
        ROLL => edit.film_roll_degrees = Some(value),
        POST_SCALE => edit.post_scale = Some(value),
        OVERSCAN => edit.overscan = Some(value),
        _ => edit.camera_scale = Some(value),
    }
    Ok(edit)
}

impl Editor {
    /// The film back the rows show: the camera's, or the defaults for a field-of-view lens (the
    /// first edit then switches the lens to a film back, as `set_camera_film_back` does).
    fn shown_film_back(&self) -> FilmBack {
        match self.camera.lens() {
            Lens::Film(film) => film,
            Lens::VerticalFov(_) => FilmBack::DEFAULT,
        }
    }

    pub(in crate::app) fn camera_section(&mut self, ui: &mut egui::Ui) {
        let film = self.shown_film_back();
        let enabled = !self.files.is_modal() && !self.files.is_closing();
        ui.label(RichText::new("Camera Attributes").strong());
        ui.weak(
            "Session only: the film back is not saved with the scene and Undo does not change it.",
        );
        if self.camera.projection() == Projection::Orthographic {
            ui.weak("No effect until the view is perspective.");
        }
        if matches!(self.camera.lens(), Lens::VerticalFov(_)) {
            ui.weak("The lens is set by field of view; the first edit switches it to a film back.");
        }
        let blocked = self.history.scene().blocked_camera_names();
        if !blocked.is_empty() {
            ui.weak(format!(
                "The open file's film back for camera {} is not applied: the editor cannot carry it.",
                blocked.join(", ")
            ));
        }
        ui.weak("These rows show the session lens, not the open file's film back.");
        ui.label(RichText::new("Film Back").strong());
        let current = texts(&film);
        if !self.camera_drafts_primed {
            self.camera_drafts.clone_from(&current);
            self.camera_baseline.clone_from(&current);
            self.camera_drafts_primed = true;
        }
        let mut blurred = Vec::new();
        let mut edits: Vec<FilmBackEdit> = Vec::new();
        egui::Grid::new("camera_rows")
            .num_columns(2)
            .spacing([8.0, 5.0])
            .show(ui, |ui| {
                for index in [FOCAL, APERTURE_H, APERTURE_V, SQUEEZE] {
                    self.camera_text_row(ui, index, enabled, &mut blurred);
                }
                if let Some(fit) = combo(ui, "Film Fit", &FITS, film.film_fit, enabled) {
                    edits.push(FilmBackEdit {
                        film_fit: Some(fit),
                        ..Default::default()
                    });
                }
                for index in [
                    FIT_OFFSET,
                    OFFSET_H,
                    OFFSET_V,
                    PRE_SCALE,
                    TRANSLATE_H,
                    TRANSLATE_V,
                    PIVOT_H,
                    PIVOT_V,
                    ROLL,
                ] {
                    self.camera_text_row(ui, index, enabled, &mut blurred);
                }
                if let Some(order) = combo(
                    ui,
                    "Film Roll Order",
                    &ORDERS,
                    film.film_roll_order,
                    enabled,
                ) {
                    edits.push(FilmBackEdit {
                        film_roll_order: Some(order),
                        ..Default::default()
                    });
                }
                for index in [POST_SCALE, OVERSCAN, CAMERA_SCALE] {
                    self.camera_text_row(ui, index, enabled, &mut blurred);
                }
                ui.label("Display Film Gate");
                let mut gate = film.display_film_gate;
                let response = ui.add_enabled(enabled, egui::Checkbox::new(&mut gate, ""));
                response.widget_info(|| {
                    egui::WidgetInfo::selected(
                        egui::Role::CheckBox,
                        enabled,
                        gate,
                        "Display Film Gate",
                    )
                });
                if !enabled {
                    super::primitive_options::describe_unavailable(
                        ui.ctx(),
                        response.id,
                        "a dialog is open",
                    );
                }
                if response.changed() {
                    edits.push(FilmBackEdit {
                        display_film_gate: Some(gate),
                        ..Default::default()
                    });
                }
                ui.end_row();
            });
        ui.weak(
            "Film Gate preset, Film Aspect Ratio and Fit Resolution Gate: not implemented yet.",
        );
        for index in blurred {
            // Compared with the text the field had when it gained focus, not the stored value: a
            // remote edit made while the field was focused but untouched must survive the blur.
            if self.camera_drafts[index] == self.camera_baseline[index] {
                continue;
            }
            match edit_for(index, &self.camera_drafts[index]) {
                Ok(edit) => edits.push(edit),
                Err(message) => {
                    self.status = message;
                    self.error = true;
                }
            }
        }
        // A field that is not being edited follows the stored value again: this is how refused
        // or unparsed text is replaced.
        for (index, text) in current.iter().enumerate() {
            let focused = ui.ctx().memory(|memory| memory.has_focus(field_id(index)));
            if !focused {
                self.camera_drafts[index].clone_from(text);
                self.camera_baseline[index].clone_from(text);
            }
        }
        for edit in edits {
            match self.native_operation_result(Operation::SetCameraFilmBack(edit)) {
                Ok(OperationReply::Changed) => {
                    self.status = "Film back set".into();
                    self.error = false;
                }
                Ok(_) => {
                    self.status = "No change".into();
                    self.error = false;
                }
                Err(_) => {}
            }
        }
    }

    /// One labelled text row; pushes its index to `blurred` when it lost focus.
    fn camera_text_row(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        enabled: bool,
        blurred: &mut Vec<usize>,
    ) {
        ui.label(LABELS[index]);
        let response = ui.add_enabled(
            enabled,
            egui::TextEdit::singleline(&mut self.camera_drafts[index])
                .id(field_id(index))
                .interactive(enabled)
                .desired_width(80.0),
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
            // Esc cancels the edit, as in Maya: the typed text is dropped, nothing is sent.
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.camera_drafts[index].clone_from(&self.camera_baseline[index]);
            } else {
                blurred.push(index);
            }
        }
        ui.end_row();
    }
}
