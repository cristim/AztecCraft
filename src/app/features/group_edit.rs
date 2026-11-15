//! Edit > Group and Ungroup on the object selection (#42). The menu rows and Ctrl/Cmd+G dispatch
//! the same `Operation`s as MCP `group_selection` and `ungroup_selection`; the option boxes are
//! not implemented, so the documented command defaults apply.
use crate::app::*;
use azteccraft::native_scene::{
    GroupOptions, GroupPivot, GroupUnder, ReparentKeep, UngroupOptions,
};

/// Group Under Parent and Preserve Position are the `group` command defaults (2027); the Center
/// pivot is the menu default PROVISIONALLY (the Group Options page names the choices only).
const GROUP_DEFAULTS: GroupOptions = GroupOptions {
    under: GroupUnder::Parent,
    pivot: GroupPivot::Center,
    preserve_position: true,
};
/// Ungroup Under Parent and `-absolute` are the `ungroup` command defaults (2027).
const UNGROUP_DEFAULTS: UngroupOptions = UngroupOptions {
    under: GroupUnder::Parent,
    keep: ReparentKeep::World,
};

pub(in crate::app) const GROUP_OPTIONS_REASON: &str = "Group options are not implemented; the defaults apply: Under Parent, Center pivot, Preserve Position";
pub(in crate::app) const UNGROUP_OPTIONS_REASON: &str =
    "Ungroup options are not implemented; the defaults apply: Under Parent, Preserve Position";

impl Editor {
    pub(in crate::app) fn native_group(&mut self) {
        self.native_operation(Operation::Group {
            options: GROUP_DEFAULTS,
            expected_selection_generation: None,
        });
    }

    pub(in crate::app) fn native_ungroup(&mut self) {
        self.native_operation(Operation::Ungroup {
            options: UNGROUP_DEFAULTS,
            expected_selection_generation: None,
        });
    }

    /// Group and Ungroup need an object selection (Object or Hierarchy mode) in a scene that can
    /// be edited.
    pub(in crate::app) fn group_enabled(&self, authoring: bool) -> bool {
        authoring && !self.selection.is_empty() && !self.selection_mode.is_component()
    }

    pub(in crate::app) fn group_disabled_hover(
        &self,
        read_only: Option<&str>,
        verb: &str,
    ) -> String {
        if let Some(reason) = read_only {
            return reason.to_owned();
        }
        match self.selection_mode.component_noun() {
            Some(noun) => format!(
                "Object or Hierarchy selection required; {} {verb} is unsupported",
                noun.to_lowercase()
            ),
            None => format!("Select an object to {verb}"),
        }
    }
}

/// Maya's Ctrl+G. The modifier is `command` (Ctrl on Windows and Linux, Cmd on macOS) like the
/// Undo and Redo chords, which the Maya page does not settle for macOS (PROVISIONAL). Shift, Alt
/// and held-key repeats are other chords or nothing; G and Shift+G (Repeat Last, Repeat at mouse
/// position) are not implemented and stay unbound.
pub(in crate::app) fn group_chord(events: &[egui::Event]) -> bool {
    events.iter().any(|event| {
        matches!(event, egui::Event::Key {
            key: egui::Key::G,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } if modifiers.command && !modifiers.shift && !modifiers.alt)
    })
}
