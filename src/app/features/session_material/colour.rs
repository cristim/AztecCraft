//! Native editing of the selected shape's session material colours (#467 T5b): R/G/B text fields
//! and sliders for Color, Transparency, Ambient Color and Incandescence in the Attribute Editor
//! material tab. A commit is one history step per changed attribute, the same `Command`s MCP
//! `set_session_material_color` and `set_session_material_attribute` run.
use super::*;
use azteccraft::native_scene::LambertAttribute;

const LOCKED_REASON: &str = "The initial material is locked";

pub(in crate::app) fn channel_text(value: f64) -> String {
    // The table stores single precision, so the shortest f32 text round-trips exactly.
    (value as f32).to_string()
}

fn slot(attribute: LambertAttribute) -> usize {
    LambertAttribute::ALL
        .iter()
        .position(|candidate| *candidate == attribute)
        .expect("every attribute is in LambertAttribute::ALL")
}

/// Row title and the noun in status messages.
pub(super) fn row_label(attribute: LambertAttribute) -> &'static str {
    match attribute {
        LambertAttribute::Color => "Color",
        LambertAttribute::Transparency => "Transparency",
        LambertAttribute::AmbientColor => "Ambient Color",
        LambertAttribute::Incandescence => "Incandescence",
    }
}

fn status_noun(attribute: LambertAttribute) -> &'static str {
    match attribute {
        LambertAttribute::Color => "colour",
        LambertAttribute::Transparency => "transparency",
        LambertAttribute::AmbientColor => "ambient color",
        LambertAttribute::Incandescence => "incandescence",
    }
}

#[cfg(test)]
pub(in crate::app) fn colour_edit_id(channel: usize) -> egui::Id {
    attribute_edit_id(LambertAttribute::Color, channel)
}

pub(in crate::app) fn attribute_edit_id(attribute: LambertAttribute, channel: usize) -> egui::Id {
    egui::Id::unique(("session_material_colour", slot(attribute), channel))
}

pub(in crate::app) fn session_attribute_field_ids() -> impl Iterator<Item = egui::Id> {
    LambertAttribute::ALL
        .into_iter()
        .flat_map(|attribute| (0..3).map(move |channel| attribute_edit_id(attribute, channel)))
}

impl DraftRow {
    fn new(original: [f64; 3]) -> Self {
        Self {
            original,
            text: original.map(channel_text),
            intent: false,
        }
    }
    fn changed(&self) -> bool {
        self.intent
            || self
                .text
                .iter()
                .zip(self.original)
                .any(|(text, value)| *text != channel_text(value))
    }
}

impl SessionColorDraft {
    pub(in crate::app) fn changed(&self) -> bool {
        self.rows.iter().any(DraftRow::changed)
    }
}

impl Editor {
    /// Why the colours of `shape`'s material cannot be edited, `None` when they can.
    pub(in crate::app) fn session_color_refusal(&self, shape: NativeId) -> Option<String> {
        if let Some(refusal) = self.history.scene().session_material_edit_refusal() {
            return Some(refusal.to_string());
        }
        let scene = self.history.scene();
        let id = scene
            .shape(shape)
            .ok()
            .and_then(|shape| scene.session_material_id(shape))?;
        (id.index() == 0).then(|| LOCKED_REASON.to_string())
    }

    /// A fresh draft for `shape`'s editable session material, `None` when there is none.
    pub(in crate::app) fn new_session_color_draft(
        &self,
        shape: NativeId,
    ) -> Option<SessionColorDraft> {
        if self.session_color_refusal(shape).is_some() {
            return None;
        }
        let scene = self.history.scene();
        let shape_ref = scene.shape(shape).ok()?;
        let material = scene.session_material_id(shape_ref)?;
        let lambert = scene.session_material(shape_ref)?;
        Some(SessionColorDraft {
            scene_token: self.scene_token,
            shape,
            material,
            rows: LambertAttribute::ALL
                .map(|attribute| DraftRow::new(lambert.attribute(attribute))),
        })
    }

    /// Commits the pending draft, one history step per changed attribute. `false` keeps the
    /// draft and says why; nothing is executed unless every changed row parses.
    pub(in crate::app) fn commit_session_color_draft(&mut self) -> bool {
        let result = (|| {
            let draft = self.session_color_draft.as_ref().ok_or("No colour edit")?;
            if draft.scene_token != self.scene_token {
                return Err("Colour draft belongs to a replaced document".to_string());
            }
            let scene = self.history.scene();
            let current = scene.shape(draft.shape).ok().and_then(|shape| {
                let lambert = scene.session_material(shape)?;
                Some((
                    scene.session_material_id(shape)?,
                    LambertAttribute::ALL.map(|attribute| lambert.attribute(attribute)),
                ))
            });
            let original =
                LambertAttribute::ALL.map(|attribute| draft.rows[slot(attribute)].original);
            if current != Some((draft.material, original)) {
                return Err("The material changed since this edit began".to_string());
            }
            let mut commands = Vec::new();
            for attribute in LambertAttribute::ALL {
                let row = &draft.rows[slot(attribute)];
                if !row.changed() {
                    continue;
                }
                let mut rgb = [0.0; 3];
                for (value, text) in rgb.iter_mut().zip(&row.text) {
                    *value = text
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                        .ok_or_else(|| {
                            let mut noun = status_noun(attribute).to_string();
                            noun[..1].make_ascii_uppercase();
                            format!("{noun} channels must be numbers from 0 to 1")
                        })?;
                }
                let material = draft.material;
                commands.push((
                    attribute,
                    if attribute == LambertAttribute::Color {
                        Command::SetSessionMaterialColor {
                            material,
                            color: rgb,
                        }
                    } else {
                        Command::SetSessionMaterialAttribute {
                            material,
                            attribute,
                            rgb,
                        }
                    },
                ));
            }
            // Every row is validated above, so a refusal here is unexpected (a history limit); if a
            // later execute fails after an earlier one succeeded, the earlier steps stay as their own
            // undo steps and the draft is kept with the error shown.
            let mut outcome = (EditOutcome::Unchanged, "colour");
            for (attribute, command) in commands {
                let result = self
                    .history
                    .execute(command)
                    .map_err(|error| error.to_string())?;
                if result != EditOutcome::Unchanged {
                    outcome = (result, status_noun(attribute));
                }
            }
            Ok(outcome)
        })();
        match result {
            Ok((outcome, noun)) => {
                self.sync_fields();
                self.status = if outcome == EditOutcome::Unchanged {
                    "No change".into()
                } else {
                    format!("Material {noun} edited")
                };
                self.error = false;
                true
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                false
            }
        }
    }

    /// The R/G/B fields for `attribute` of `shape`, added to the caller's row, disabled with a
    /// reason when it cannot be edited.
    pub(in crate::app) fn session_color_fields(
        &mut self,
        ui: &mut egui::Ui,
        shape: NativeId,
        attribute: LambertAttribute,
    ) {
        let row = slot(attribute);
        let refusal = self.session_color_refusal(shape);
        let several = self.selection.len() > 1;
        let editable = refusal.is_none() && !several && !self.has_channel_draft();
        let reason = refusal
            .or_else(|| several.then(|| OperationError::MultipleSelection.to_string()))
            .or_else(|| {
                self.has_channel_draft()
                    .then(|| "Finish the channel edit first".to_string())
            });
        let shown: Option<[String; 3]> = match (&self.session_color_draft, editable) {
            (Some(draft), true) if draft.shape == shape => Some(draft.rows[row].text.clone()),
            _ => {
                let scene = self.history.scene();
                scene
                    .shape(shape)
                    .ok()
                    .and_then(|shape| scene.session_material(shape))
                    .map(|lambert| lambert.attribute(attribute).map(channel_text))
            }
        };
        let Some(mut text) = shown else { return };
        let mut apply = false;
        let mut blurred = false;
        let mut focused = false;
        let mut reverted = false;
        let mut touched = false;
        let fallback = self
            .session_color_draft
            .as_ref()
            .map_or([0.0; 3], |draft| draft.rows[row].original);
        let gesture_open = self
            .session_color_draft
            .as_ref()
            .is_some_and(SessionColorDraft::changed);
        let pointer_down = ui.input(|input| input.pointer.any_down());
        // The Color row keeps the bare channel names; the others are prefixed so every field and
        // slider has a distinct accessible name.
        let prefix = if attribute == LambertAttribute::Color {
            String::new()
        } else {
            format!("{} ", row_label(attribute))
        };
        ui.add_enabled_ui(editable && self.session_color_draft.is_some(), |ui| {
            {
                for (index, channel) in ["Red", "Green", "Blue"].into_iter().enumerate() {
                    let name = format!("{prefix}{channel}");
                    let label = ui.label(name.as_str());
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text[index])
                            .id(attribute_edit_id(attribute, index))
                            .desired_width(44.0),
                    );
                    let response = response.labelled_by(label.id);
                    let response = match &reason {
                        Some(reason) => response.on_disabled_hover_text(reason),
                        None => response,
                    };
                    touched |= response.changed();
                    blurred |= response.lost_focus();
                    focused |= response.has_focus();
                    if response.has_focus() {
                        apply |= ui.input(|input| input.key_pressed(Key::Enter));
                    }
                    // egui drops focus on Escape itself, so it shows up as a lost focus.
                    if response.has_focus() || response.lost_focus() {
                        reverted |= ui.input(|input| input.key_pressed(Key::Escape));
                    }
                    // The slider edits the same draft text. egui clamps the shown value itself and
                    // reports a change only for a user gesture, so out-of-range text is kept,
                    // and a whole gesture is one history step (commit when it ends).
                    let mut value = text[index]
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|value| value.is_finite())
                        .unwrap_or(fallback[index]);
                    ui.spacing_mut().slider_width = 48.0;
                    let slider = ui.add(
                        egui::Slider::new(&mut value, 0.0..=1.0)
                            .show_value(false)
                            .step_by(0.0),
                    );
                    let slider = match &reason {
                        Some(reason) => slider.on_disabled_hover_text(reason),
                        None => slider,
                    };
                    let slider_label = format!("{name} slider");
                    slider.widget_info(|| {
                        egui::WidgetInfo::slider(true, value, slider_label.as_str())
                    });
                    // Esc reverts, also in the middle of a drag (egui then ends the drag and the
                    // slider has lost focus, so the open gesture is detected from the button).
                    let escaped = ui.input(|input| input.key_pressed(Key::Escape))
                        && (slider.has_focus()
                            || slider.lost_focus()
                            || (pointer_down && gesture_open));
                    if escaped {
                        reverted = true;
                    } else if slider.changed() {
                        text[index] = channel_text(value);
                        touched = true;
                    }
                    apply |= slider.drag_stopped();
                    if slider.has_focus() {
                        apply |= ui.input(|input| input.key_pressed(Key::Enter));
                    }
                    blurred |= slider.lost_focus();
                    focused |= slider.has_focus();
                }
            }
        });
        if let Some(draft) = self
            .session_color_draft
            .as_mut()
            .filter(|d| d.shape == shape)
        {
            if reverted {
                draft.rows[row] = DraftRow::new(draft.rows[row].original);
            } else {
                draft.rows[row].text = text;
                draft.rows[row].intent |= touched;
            }
        }
        // One logical edit: Tab between the three fields keeps the draft open, so leaving the group
        // (or Enter) commits all three texts as one history step.
        let left_group = blurred && !focused && self.blur_commit_skip_frames == 0;
        if !reverted && (apply || left_group) {
            self.commit_pending();
        }
    }
}
