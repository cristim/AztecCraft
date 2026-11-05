//! Native Extrude of the selected face and its Extrude Options window (issue #44, slice 1).
//!
//! The window is presentation only, like the primitive options: it holds a text draft of the
//! Thickness and dispatches the same typed `Operation::ExtrudeFace` that MCP `extrude_face` uses,
//! so the human-draft and `commit_pending` rules apply. PROVISIONAL: Maya's docs do not say whether
//! the window stays open after Extrude (here it does), and for one planar face Thickness is applied
//! as the core `distance`, the normal displacement (polyExtrudeFacet `localTranslateZ`).

use crate::app::*;
use std::fmt;

/// Why Extrude cannot run from the UI; every other refusal arrives as an `OperationError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum ExtrudeBlocker {
    NotFaceMode,
    NoFaceSelected,
}

impl fmt::Display for ExtrudeBlocker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFaceMode => "Extrude needs Face selection mode (F11)",
            Self::NoFaceSelected => "Select a face first",
        })
    }
}

pub(in crate::app) fn thickness_field_id() -> egui::Id {
    egui::Id::new("extrude_option_thickness")
}

fn window_id() -> egui::Id {
    egui::Id::new("extrude_options_window")
}

struct Dialog {
    thickness: String,
    message: Option<String>,
}

#[derive(Default)]
pub(in crate::app) struct ExtrudeOptionsUi {
    dialog: Option<Dialog>,
    /// A text field held focus last frame; egui clears focus before Escape runs.
    typing: bool,
}

fn parse_thickness(text: &str) -> Result<f64, String> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| "Thickness must be a number".to_string())?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("Thickness must be a finite number".into())
    }
}

impl Editor {
    pub(in crate::app) fn extrude_blocker(&self) -> Option<ExtrudeBlocker> {
        if self.selection_mode != SelectionMode::Face {
            Some(ExtrudeBlocker::NotFaceMode)
        } else if self.remembered_face.is_none() {
            Some(ExtrudeBlocker::NoFaceSelected)
        } else {
            None
        }
    }

    /// Extrudes the selected face by `thickness` through the shared typed operation. Core refusals
    /// (all faces, undefined normal, stale face) set the error status and change nothing.
    pub(in crate::app) fn extrude_selected_face(
        &mut self,
        thickness: f64,
    ) -> Result<(), ExtrudeBlocker> {
        if let Some(blocker) = self.extrude_blocker() {
            self.status = blocker.to_string();
            self.error = true;
            return Err(blocker);
        }
        let target = self
            .remembered_face
            .clone()
            .ok_or(ExtrudeBlocker::NoFaceSelected)?;
        self.native_operation(Operation::ExtrudeFace {
            target,
            distance: thickness,
        });
        Ok(())
    }

    pub(in crate::app) fn open_extrude_options(&mut self) {
        if self.extrude_options.dialog.is_none() {
            self.extrude_options.dialog = Some(Dialog {
                thickness: "0".into(),
                message: None,
            });
        }
    }

    /// Escape closes the window only when no manipulation, draft or typed field wants the key.
    pub(in crate::app) fn escape_closes_extrude_options(&mut self, ctx: &egui::Context) -> bool {
        let closes = self.extrude_options.dialog.is_some()
            && !self.files.is_closing()
            && !self.extrude_options.typing
            && !self.has_draft()
            && self.history.active_manipulation().is_none()
            && self.native_move.is_none()
            && ctx.input(|input| input.key_pressed(Key::Escape));
        if closes {
            self.extrude_options.dialog = None;
            self.consume_pointer_sequence();
        }
        closes
    }

    pub(in crate::app) fn extrude_options_window(&mut self, ctx: &egui::Context) {
        let hidden = self.files.is_modal() || self.files.is_closing();
        let Some(mut dialog) = self.extrude_options.dialog.take() else {
            self.extrude_options.typing = !hidden && text_edit_focused(ctx);
            return;
        };
        // Hidden, not discarded, while a file modal or close is in progress.
        if hidden {
            self.extrude_options.typing = false;
            self.extrude_options.dialog = Some(dialog);
            return;
        }
        let refusal = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string())
            .or_else(|| self.extrude_blocker().map(|blocker| blocker.to_string()));
        let mut open = true;
        let mut extrude = false;
        let mut close = false;
        egui::Window::new("Extrude Options")
            .id(window_id())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                (extrude, close) = dialog_body(ui, &mut dialog, refusal.as_deref());
            });
        self.extrude_options.dialog = Some(dialog);
        if extrude {
            self.extrude_from_dialog();
        }
        self.extrude_options.typing = text_edit_focused(ctx);
        if !open || close {
            self.extrude_options.dialog = None;
        }
    }

    fn extrude_from_dialog(&mut self) {
        let Some(mut dialog) = self.extrude_options.dialog.take() else {
            return;
        };
        dialog.message = match parse_thickness(&dialog.thickness) {
            Err(message) => {
                self.status = message.clone();
                self.error = true;
                Some(message)
            }
            Ok(thickness) => match self.extrude_selected_face(thickness) {
                Err(blocker) => Some(blocker.to_string()),
                Ok(()) => self.error.then(|| self.status.clone()),
            },
        };
        self.extrude_options.dialog = Some(dialog);
    }
}

/// Returns (extrude clicked, close clicked).
fn dialog_body(ui: &mut egui::Ui, dialog: &mut Dialog, refusal: Option<&str>) -> (bool, bool) {
    egui::Grid::new("extrude_option_fields")
        .num_columns(2)
        .show(ui, |ui| {
            let label = ui.label("Thickness");
            ui.add(
                egui::TextEdit::singleline(&mut dialog.thickness)
                    .id(thickness_field_id())
                    .desired_width(80.0),
            )
            .labelled_by(label.id);
            ui.end_row();
            for (label, value) in [
                ("Divisions", "1"),
                ("Offset", "0"),
                ("Smoothing angle", "30"),
                ("Curve", "Selected"),
                ("Taper", "1"),
                ("Twist", "0"),
            ] {
                ui.label(label);
                ui.add_enabled(false, egui::TextEdit::singleline(&mut value.to_string()))
                    .on_disabled_hover_text(NOT_IMPLEMENTED_YET);
                ui.end_row();
            }
        });
    ui.add_enabled(false, egui::Checkbox::new(&mut true, "Keep faces together"))
        .on_disabled_hover_text(
            "It matters only with several faces; one face is selected at a time (#30)",
        );
    ui.separator();
    if let Some(message) = &dialog.message {
        ui.colored_label(Color32::from_rgb(255, 160, 130), message);
    }
    ui.horizontal(|ui| {
        let extrude = ui
            .add_enabled(refusal.is_none(), egui::Button::new("Extrude"))
            .on_disabled_hover_text(refusal.unwrap_or_default())
            .clicked();
        let close = ui.button("Close").clicked();
        (extrude, close)
    })
    .inner
}

#[cfg(test)]
#[path = "extrude_options/tests.rs"]
mod tests;
