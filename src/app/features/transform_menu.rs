//! Modify > Reset, Freeze and Match Transformations on the object selection (#313). The rows
//! dispatch the same `Operation`s as MCP `reset_transformations`, `freeze_transformations` and
//! `match_transformations` with all three channels (Match: the last selected is the target; Maya's
//! pages call it the "source", so the naming is inverted); the option boxes are not built, so their
//! rows are visibly disabled with a reason. Maya's Lock and Preserve Normals and Joint orient
//! options are unbuilt too (PROVISIONAL; the native mesh stores no normals).
use crate::app::*;
use azteccraft::native_scene::TransformChannels;

pub(in crate::app) const RESET_OPTIONS_REASON: &str = "Reset Transformations options are not implemented; all of Translate, Rotate and Scale are reset; see #313";
pub(in crate::app) const FREEZE_OPTIONS_REASON: &str = "Freeze Transformations options are not implemented; Translate, Rotate and Scale are frozen; Joint orient, Lock Normals and Preserve Normals are unbuilt; see #313";
pub(in crate::app) const MATCH_OPTIONS_REASON: &str = "Match Transformations options are not implemented; Translate, Rotate and Scale are matched in world space (PROVISIONAL); Match Pivots and bounding box sizes are unbuilt; see #313";
const MATCH_NEEDS_TWO: &str = "Select at least two objects; the last selected is the target";

impl Editor {
    pub(in crate::app) fn native_reset_transformations(&mut self) {
        self.native_operation(Operation::ResetTransformations {
            channels: TransformChannels::ALL,
            expected_selection_generation: None,
        });
    }

    pub(in crate::app) fn native_freeze_transformations(&mut self) {
        self.native_operation(Operation::FreezeTransformations {
            channels: TransformChannels::ALL,
            expected_selection_generation: None,
        });
    }

    /// Why the Reset and Freeze rows are disabled, or `None` when they apply. The same test
    /// as Group: an object selection in an editable scene, no joint.
    fn transform_rows_reason(&self, read_only: Option<&str>) -> Option<String> {
        if let Some(reason) = read_only {
            return Some(reason.to_owned());
        }
        if self.group_enabled(true) {
            return None;
        }
        Some(match self.selection_mode.component_noun() {
            Some(noun) => format!(
                "Object or Hierarchy selection required; {} transformations are unsupported",
                noun.to_lowercase()
            ),
            None if self.selection.is_empty() => "Select an object first".to_owned(),
            None => {
                "Joints cannot be reset, frozen or matched here (unsupported); deselect the joint"
                    .to_owned()
            }
        })
    }

    pub(in crate::app) fn native_match_transformations(&mut self) {
        self.native_operation(Operation::MatchTransformations {
            channels: TransformChannels::ALL,
            expected_selection_generation: None,
        });
    }

    /// Why the Match row is disabled: the Reset and Freeze reasons, plus one object is not enough.
    fn match_row_reason(&self, read_only: Option<&str>) -> Option<String> {
        self.transform_rows_reason(read_only)
            .or_else(|| (self.selection.len() < 2).then(|| MATCH_NEEDS_TWO.to_owned()))
    }

    pub(in crate::app) fn modify_menu(&mut self, ui: &mut egui::Ui, read_only: Option<&str>) {
        ui.menu_button("Modify", |ui| {
            unsupported(ui, NOT_IMPLEMENTED_YET);
            let reason = self.transform_rows_reason(read_only);
            for freeze in [false, true] {
                let (label, options) = if freeze {
                    ("Freeze Transformations", FREEZE_OPTIONS_REASON)
                } else {
                    ("Reset Transformations", RESET_OPTIONS_REASON)
                };
                let clicked = ui
                    .horizontal(|ui| {
                        let row = ui.add_enabled(reason.is_none(), egui::Button::new(label));
                        let row = match &reason {
                            Some(reason) => {
                                features::primitive_options::describe_unavailable(
                                    ui.ctx(),
                                    row.id,
                                    reason,
                                );
                                row.on_disabled_hover_text(reason)
                            }
                            None => row,
                        };
                        let option_box = features::primitive_options::option_box(
                            ui,
                            &format!("{label} option box"),
                            false,
                            Some(options),
                        );
                        features::primitive_options::describe_unavailable(
                            ui.ctx(),
                            option_box.id,
                            options,
                        );
                        row.clicked()
                    })
                    .inner;
                if clicked {
                    ui.close();
                    if freeze {
                        self.native_freeze_transformations();
                    } else {
                        self.native_reset_transformations();
                    }
                }
            }
            let reason = self.match_row_reason(read_only);
            let clicked = ui
                .horizontal(|ui| {
                    let row = ui
                        .add_enabled(reason.is_none(), egui::Button::new("Match Transformations"));
                    let row = match &reason {
                        Some(reason) => {
                            features::primitive_options::describe_unavailable(
                                ui.ctx(),
                                row.id,
                                reason,
                            );
                            row.on_disabled_hover_text(reason)
                        }
                        None => row,
                    };
                    let option_box = features::primitive_options::option_box(
                        ui,
                        "Match Transformations option box",
                        false,
                        Some(MATCH_OPTIONS_REASON),
                    );
                    features::primitive_options::describe_unavailable(
                        ui.ctx(),
                        option_box.id,
                        MATCH_OPTIONS_REASON,
                    );
                    row.clicked()
                })
                .inner;
            if clicked {
                ui.close();
                self.native_match_transformations();
            }
        });
    }
}
