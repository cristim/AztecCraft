//! The Rigging set's Skeleton menu and Create Joints window (issue #363).
//!
//! Presentation only, like `primitive_options`: the window holds text drafts and Create dispatches
//! the same typed `Operation::CreateJoint` as MCP. It is not a scene draft; only a focused field
//! holds remote operations back. The numeric window stands in for Maya's interactive Joint Tool,
//! which is out of scope, so it matches no Maya window.

use super::primitive_options::option_box;
use crate::app::dispatch::JOINT_COMPONENT_REASON;
use crate::app::*;

/// The row text of a joint in the Outliner; other transforms use a hollow diamond.
pub(in crate::app) const JOINT_ROW_GLYPH: &str = "◎";

const CREATE_JOINTS: &str = "Create Joints";
const WINDOW_TITLE: &str = "Create Joints";
/// Over the viewport, because egui's default corner would cover the Outliner while a chain is built.
const WINDOW_POSITION: egui::Pos2 = egui::pos2(360.0, 230.0);
const OPTION_BOX_REASON: &str =
    "Joint Tool settings are not implemented; the Create Joints row opens a numeric window";

/// The Skeleton menu's Joints section after Create Joints, in Maya's order.
const JOINT_ROWS: [&str; 8] = [
    "Insert Joints",
    "Mirror Joints",
    "Orient Joint",
    "Remove Joints",
    "Connect Joint",
    "Disconnect Joint",
    "Reroot Skeleton",
    "Joint Labelling",
];

/// The Skeleton menu's IK section, in Maya's order.
const IK_ROWS: [&str; 10] = [
    "Create IK Handle",
    "Create IK Spline Handle",
    "Set Preferred Angle",
    "Assume Preferred Angle",
    "Enable IK Handle Snap",
    "Enable IK/FK Control",
    "Enable Selected IK Handles",
    "Disable Selected IK Handles",
    "Quick Rig",
    "HumanIK",
];

const FIELD_LABELS: [&str; 5] = ["X", "Y", "Z", "Radius", "Name"];
const NAME_FIELD: usize = 4;

pub(in crate::app) fn joint_field_ids() -> impl Iterator<Item = egui::Id> {
    (0..FIELD_LABELS.len()).map(joint_field_id)
}

fn joint_field_id(index: usize) -> egui::Id {
    egui::Id::new(("joint_option", index))
}

fn window_id() -> egui::Id {
    egui::Id::new("joint_options_window")
}

struct Dialog {
    texts: [String; FIELD_LABELS.len()],
    relative: bool,
    message: Option<String>,
}

impl Default for Dialog {
    fn default() -> Self {
        let radius = azteccraft::native_scene::DEFAULT_JOINT_RADIUS.to_string();
        Self {
            texts: ["0".into(), "0".into(), "0".into(), radius, String::new()],
            relative: false,
            message: None,
        }
    }
}

impl Dialog {
    /// Parses the drafts; an empty Name takes the lowest free default name. The values themselves
    /// are validated by the core, the same as for MCP.
    fn request(&self) -> Result<([f64; 3], f64, Option<String>), String> {
        let number = |index: usize| {
            let (label, text) = (FIELD_LABELS[index], self.texts[index].trim());
            text.parse::<f64>()
                .map_err(|_| format!("{label} must be a number"))
        };
        let name = self.texts[NAME_FIELD].trim();
        Ok((
            [number(0)?, number(1)?, number(2)?],
            number(3)?,
            (!name.is_empty()).then(|| name.to_owned()),
        ))
    }
}

#[derive(Default)]
pub(in crate::app) struct JointOptionsUi {
    dialog: Option<Dialog>,
    /// Any text field held focus at the end of the last frame; see `PrimitiveOptionsUi`.
    typing: bool,
}

/// What Create would do with the current selection, or why it cannot.
struct Placement {
    parent: Option<NodeId>,
    readout: String,
}

impl Editor {
    pub(in crate::app) fn skeleton_menu(
        &mut self,
        ui: &mut egui::Ui,
        read_only_reason: Option<&str>,
    ) {
        ui.menu_button("Skeleton", |ui| {
            ui.weak("Joints");
            ui.horizontal(|ui| {
                let mut row =
                    ui.add_enabled(read_only_reason.is_none(), egui::Button::new(CREATE_JOINTS));
                if let Some(reason) = read_only_reason {
                    row = row.on_disabled_hover_text(reason);
                }
                if row.clicked() {
                    self.open_joint_options();
                    ui.close();
                }
                option_box(
                    ui,
                    "Create Joints option box",
                    false,
                    Some(OPTION_BOX_REASON),
                );
            });
            for label in JOINT_ROWS {
                unsupported(ui, label);
            }
            ui.separator();
            ui.weak("IK");
            for label in IK_ROWS {
                unsupported(ui, label);
            }
        });
    }

    pub(in crate::app) fn open_joint_options(&mut self) {
        self.joint_options
            .dialog
            .get_or_insert_with(Dialog::default);
    }

    /// Escape closes the window under the same conditions as the primitive option window.
    pub(in crate::app) fn escape_closes_joint_window(&mut self, ctx: &egui::Context) -> bool {
        let closes = self.joint_options.dialog.is_some()
            && !self.files.is_closing()
            && !self.joint_options.typing
            && !self.has_draft()
            && self.history.active_manipulation().is_none()
            && self.native_move.is_none()
            && ctx.input(|input| input.key_pressed(Key::Escape));
        if closes {
            self.joint_options.dialog = None;
            self.consume_pointer_sequence();
        }
        closes
    }

    /// The parent the next Create uses: the single selected joint, or none for a new chain.
    fn joint_placement(&self) -> Result<Placement, String> {
        if let Some(reason) = self.history.scene().authoring_read_only_reason() {
            return Err(reason.to_string());
        }
        if self.selection_mode.is_component() {
            return Err(JOINT_COMPONENT_REASON.into());
        }
        if self.selection.len() > 1 {
            return Err("Select one joint to extend a chain, or nothing to start one".into());
        }
        let Some(id) = self.selection_presentation().0 else {
            return Ok(Placement {
                parent: None,
                readout: "None (starts a new chain)".into(),
            });
        };
        let scene = self.history.scene();
        match scene.node(id) {
            Ok(node) if scene.joint(node.transform_id()).is_some() => Ok(Placement {
                parent: Some(id),
                readout: node.name().to_owned(),
            }),
            Ok(_) => Err("The parent must be a joint; Deselect All to start a new chain".into()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub(in crate::app) fn joint_options_window(&mut self, ctx: &egui::Context) {
        let hidden = self.files.is_modal() || self.files.is_closing();
        let Some(mut dialog) = self.joint_options.dialog.take() else {
            self.joint_options.typing = !hidden && text_edit_focused(ctx);
            return;
        };
        // Hidden, not discarded, while a file modal or close is in progress.
        if hidden {
            self.joint_options.typing = false;
            self.joint_options.dialog = Some(dialog);
            return;
        }
        let placement = self.joint_placement();
        let mut open = true;
        let (mut create, mut close) = (false, false);
        egui::Window::new(WINDOW_TITLE)
            .id(window_id())
            .default_pos(WINDOW_POSITION)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                (create, close) = dialog_body(ui, &mut dialog, &placement);
            });
        if create && let Ok(placement) = placement {
            self.create_joint_from_dialog(&mut dialog, placement.parent);
        }
        self.joint_options.typing = text_edit_focused(ctx);
        if open && !close {
            self.joint_options.dialog = Some(dialog);
        }
    }

    fn create_joint_from_dialog(&mut self, dialog: &mut Dialog, parent: Option<NodeId>) {
        let (position, radius, name) = match dialog.request() {
            Ok(request) => request,
            Err(message) => {
                dialog.message = Some(message);
                return;
            }
        };
        self.native_operation(Operation::CreateJoint {
            parent,
            position,
            relative: dialog.relative,
            radius,
            name,
        });
        if self.error {
            dialog.message = Some(self.status.clone());
        } else {
            // A kept name would collide with the joint it just named.
            dialog.texts[NAME_FIELD].clear();
            dialog.message = None;
        }
    }
}

/// Returns (create clicked, close clicked).
fn dialog_body(
    ui: &mut egui::Ui,
    dialog: &mut Dialog,
    placement: &Result<Placement, String>,
) -> (bool, bool) {
    egui::Grid::new("joint_option_fields")
        .num_columns(2)
        .show(ui, |ui| {
            for (index, (label, text)) in
                FIELD_LABELS.iter().zip(dialog.texts.iter_mut()).enumerate()
            {
                let label = ui.label(*label);
                ui.add(
                    egui::TextEdit::singleline(text)
                        .id(joint_field_id(index))
                        .desired_width(120.0),
                )
                .labelled_by(label.id);
                ui.end_row();
            }
            ui.label("Parent");
            match placement {
                Ok(placement) => ui.label(placement.readout.as_str()),
                Err(_) => ui.weak("Unavailable"),
            };
            ui.end_row();
        });
    ui.checkbox(&mut dialog.relative, "Relative to parent");
    if let Some(message) = &dialog.message {
        ui.colored_label(Color32::from_rgb(255, 160, 130), message);
    }
    ui.horizontal(|ui| {
        let reason = placement.as_ref().err();
        let create = ui
            .add_enabled(reason.is_none(), egui::Button::new("Create"))
            .on_disabled_hover_text(reason.map_or("", String::as_str))
            .clicked();
        let close = ui.button("Close").clicked();
        (create, close)
    })
    .inner
}

#[cfg(test)]
#[path = "rigging/tests.rs"]
mod tests;
