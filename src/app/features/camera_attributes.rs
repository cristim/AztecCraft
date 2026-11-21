//! The Attribute Editor "Camera" tab (#458): the live camera's film back. The tab belongs to the
//! session camera, like the Image Plane tab, so it needs no selected node and always resolves.
//! PROVISIONAL (Maya shows camera attributes on the camera shape's own tab, reached from the
//! panel menu View > Camera Attribute Editor; the always-present tab is ours).
use crate::app::Editor;
use eframe::egui::{self, RichText};

impl Editor {
    pub(in crate::app) fn camera_section(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Camera Attributes").strong());
        ui.weak(
            "Film Back rows: not implemented yet in this tab (set_camera_film_back edits them).",
        );
    }
}
