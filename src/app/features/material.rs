use crate::app::*;

impl Editor {
    pub(in crate::app) fn material_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(RichText::new("Materials").strong());
        ui.label("Common Material Attributes");
        let Some(transform) = self
            .selection_presentation()
            .0
            .and_then(|id| self.history.scene().node(id).ok())
            .map(|node| node.transform_id())
        else {
            return;
        };
        let scene = self.history.scene();
        let shapes = || {
            scene
                .dag_edges()
                .iter()
                .filter(|edge| edge.parent() == Some(transform))
                .filter_map(|edge| scene.shape(edge.child()).ok())
        };
        let mut direct = shapes();
        if direct.next().is_none() {
            ui.weak("No direct mesh shape. Select a mesh transform.");
            return;
        }
        if direct.next().is_some() {
            let mut choice = self.material_shape;
            egui::ComboBox::from_id_salt("material_shape")
                .selected_text(
                    shapes()
                        .find(|shape| Some(shape.id()) == choice)
                        .map(|shape| shape.name())
                        .unwrap_or("Choose direct shape"),
                )
                .show_ui(ui, |ui| {
                    for shape in shapes() {
                        ui.selectable_value(&mut choice, Some(shape.id()), shape.name());
                    }
                });
            if choice != self.material_shape && self.commit_pending() {
                self.material_shape = choice;
                self.sync_fields();
            }
        }
        let Some(shape) = self.material_shape else {
            return;
        };
        self.material_section(ui, shape);
    }

    /// The Phong summary and Diffuse editor for exactly `shape`. Callers must have bound
    /// `material_shape` to it: the draft is keyed to that selection.
    pub(in crate::app) fn material_section(&mut self, ui: &mut egui::Ui, shape: NativeId) {
        if self.material_shape != Some(shape) {
            ui.weak("Material selection is out of sync with this tab.");
            return;
        }
        let Ok(material) = self.history.scene().material_for_shape(shape) else {
            ui.weak("Material data editing unavailable for this shape.");
            return;
        };
        ui.label(format!(
            "Phong | {} shared shape users",
            material.shared_users
        ));
        ui.add_enabled(false, egui::Button::new("Color (connected)"));
        ui.weak("Material rendering unavailable");
        let channel_draft = self.has_channel_draft();
        let Some(draft) = self.material_draft.as_mut() else {
            return;
        };
        let mut apply = false;
        let mut blurred = false;
        ui.add_enabled_ui(material.editing_available && !channel_draft, |ui| {
            ui.horizontal(|ui| {
                ui.label("Diffuse");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut draft.text)
                        .id(draft.focus)
                        .desired_width(110.0),
                );
                draft.intent |= response.changed();
                blurred = response.lost_focus();
                apply |= response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter));
            });
            let mut interaction = draft
                .text
                .parse::<f64>()
                .unwrap_or(draft.original)
                .clamp(0.0, 1.0);
            let response = ui.add(egui::Slider::new(&mut interaction, 0.0..=1.0).show_value(false));
            if response.changed() {
                draft.text = interaction.to_string();
                draft.intent = true;
            }
            apply |= response.drag_stopped();
        });
        if apply || (blurred && self.blur_commit_skip_frames == 0) {
            self.commit_pending();
        }
    }
}
