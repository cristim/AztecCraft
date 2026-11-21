//! Native colour editing for the selected shape's session material (#467 T5b): three R/G/B text
//! fields in the Attribute Editor material tab. A commit is one `SetSessionMaterialColor` history
//! step, the same `Command` MCP `set_session_material_color` runs.
use super::*;

const LOCKED_REASON: &str = "The initial material is locked";

pub(in crate::app) fn channel_text(value: f64) -> String {
    // The table stores single precision, so the shortest f32 text round-trips exactly.
    (value as f32).to_string()
}

pub(in crate::app) fn colour_edit_id(channel: usize) -> egui::Id {
    egui::Id::unique(("session_material_colour", channel))
}

impl SessionColorDraft {
    pub(in crate::app) fn changed(&self) -> bool {
        self.intent
            || self
                .text
                .iter()
                .zip(self.original)
                .any(|(text, value)| *text != channel_text(value))
    }
}

impl Editor {
    /// Why the colour of `shape`'s material cannot be edited, `None` when it can.
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
        let original = scene.session_material(shape_ref)?.color();
        Some(SessionColorDraft {
            scene_token: self.scene_token,
            shape,
            material,
            original,
            text: original.map(channel_text),
            intent: false,
        })
    }

    /// Commits the pending colour draft as one history step. `false` keeps the draft and says why.
    pub(in crate::app) fn commit_session_color_draft(&mut self) -> bool {
        let result = (|| {
            let draft = self.session_color_draft.as_ref().ok_or("No colour edit")?;
            if draft.scene_token != self.scene_token {
                return Err("Colour draft belongs to a replaced document".to_string());
            }
            let scene = self.history.scene();
            let current = scene.shape(draft.shape).ok().and_then(|shape| {
                Some((
                    scene.session_material_id(shape)?,
                    scene.session_material(shape)?.color(),
                ))
            });
            if current != Some((draft.material, draft.original)) {
                return Err("The material changed since this edit began".to_string());
            }
            let mut color = [0.0; 3];
            for (value, text) in color.iter_mut().zip(&draft.text) {
                *value = text
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
                    .ok_or("Colour channels must be numbers from 0 to 1")?;
            }
            self.history
                .execute(Command::SetSessionMaterialColor {
                    material: draft.material,
                    color,
                })
                .map_err(|error| error.to_string())
        })();
        match result {
            Ok(outcome) => {
                self.sync_fields();
                self.status = if outcome == EditOutcome::Unchanged {
                    "No change"
                } else {
                    "Material colour edited"
                }
                .into();
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

    /// The R/G/B fields for `shape`, added to the caller's row, disabled with a reason when the
    /// colour cannot be edited.
    pub(in crate::app) fn session_color_fields(&mut self, ui: &mut egui::Ui, shape: NativeId) {
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
            (Some(draft), true) if draft.shape == shape => Some(draft.text.clone()),
            _ => {
                let scene = self.history.scene();
                scene
                    .shape(shape)
                    .ok()
                    .and_then(|shape| scene.session_material(shape))
                    .map(|lambert| lambert.color().map(channel_text))
            }
        };
        let Some(mut text) = shown else { return };
        let mut apply = false;
        let mut blurred = false;
        let mut focused = false;
        let mut reverted = false;
        let mut touched = false;
        ui.add_enabled_ui(editable && self.session_color_draft.is_some(), |ui| {
            {
                for (index, name) in ["Red", "Green", "Blue"].into_iter().enumerate() {
                    let label = ui.label(name);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut text[index])
                            .id(colour_edit_id(index))
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
                }
            }
        });
        if let Some(draft) = self
            .session_color_draft
            .as_mut()
            .filter(|d| d.shape == shape)
        {
            if reverted {
                draft.text = draft.original.map(channel_text);
                draft.intent = false;
            } else {
                draft.text = text;
                draft.intent |= touched;
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
