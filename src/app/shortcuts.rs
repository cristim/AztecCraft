use super::*;
use azteccraft::move_handles::ManipulatorSize;

pub(super) const SHADED_TEXTURED_NAME: &str = "Hardware Texturing (6)";
const USE_ALL_LIGHTS_NAME: &str = "Use All Lights (7)";

/// One reason string for the hotkey status and the disabled menu row.
pub(super) fn unsupported_shading_reason(name: &str) -> String {
    format!("{name} is not implemented yet")
}

impl Editor {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) -> Option<ActiveTool> {
        if ctx.egui_wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
            || self.files.is_modal()
            || self.files.is_closing()
        {
            return None;
        }
        let tool_intent = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.is_none() => match key {
                    Key::Q => Some(ActiveTool::Select),
                    Key::W => Some(ActiveTool::Move),
                    Key::E => Some(ActiveTool::Rotate),
                    Key::R => Some(ActiveTool::Scale),
                    _ => None,
                },
                _ => None,
            })
        });
        let shading_key = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: key @ (Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7),
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.is_none() => Some(*key),
                _ => None,
            })
        });
        // Shift is ignored for grow: Shift+= arrives as Plus on a US layout. Shift+- is "_".
        // Key repeat steps again while held (PROVISIONAL, the docs do not say).
        let size_key = ctx.input(|i| {
            i.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.ctrl && !modifiers.alt && !modifiers.command => match key {
                    Key::Plus | Key::Equals => Some(true),
                    Key::Minus if !modifiers.shift => Some(false),
                    _ => None,
                },
                _ => None,
            })
        });
        // Maya's Alt+Home returns to the default view; Alt chords are otherwise camera navigation.
        let default_view = ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(event, egui::Event::Key {
                    key: Key::Home,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.alt && !modifiers.ctrl && !modifiers.shift && !modifiers.command)
            })
        });
        let (history, frame, frame_all) = ctx.input(|i| {
            (
                history_chord(&i.events),
                i.modifiers.is_none() && i.key_pressed(Key::F),
                i.events.iter().any(|event| {
                    matches!(event, egui::Event::Key {
                        key: Key::A,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } if modifiers.is_none())
                }),
            )
        });
        // Maya documents Ctrl+A on every platform, so the modifier is explicit Ctrl, never Command.
        let toggle_sidebar = ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(event, egui::Event::Key {
                    key: Key::A,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if modifiers.ctrl && !modifiers.mac_cmd && !modifiers.shift && !modifiers.alt)
            })
        });
        if toggle_sidebar {
            self.native_operation(Operation::SetSidebarPanelVisible {
                panel: features::attribute_editor::ctrl_a_target(self.sidebar_dock),
                visible: true,
            });
        }
        match shading_key {
            Some(Key::Num4) => {
                self.native_operation(Operation::SetShadingMode(ShadingMode::Wireframe))
            }
            Some(Key::Num5) => {
                self.native_operation(Operation::SetShadingMode(ShadingMode::SmoothShadeAll));
            }
            Some(Key::Num6) => self.unsupported_shading_status(SHADED_TEXTURED_NAME),
            Some(Key::Num7) => self.unsupported_shading_status(USE_ALL_LIGHTS_NAME),
            _ => {}
        }
        if self.active_tool != ActiveTool::Select
            && let Some(grow) = size_key
        {
            self.step_manipulator_size(grow);
        }
        if let Some(operation) = history {
            self.native_operation(operation);
        }
        if frame {
            self.frame_selected();
        }
        if frame_all {
            self.frame_all();
        }
        if default_view {
            self.default_view();
        }
        if let Some(chord) = ctx.input(|i| menus::file_chord(&i.events)) {
            let actions = self.file_actions();
            if let Some(index) = menus::chord_entry(&actions, chord)
                && actions[index].enabled
            {
                (actions[index].run)(self);
            }
        }
        tool_intent
    }
    fn step_manipulator_size(&mut self, grow: bool) {
        let current = self.manipulation.manipulator_size();
        let next = if grow {
            current.grown()
        } else {
            current.shrunk()
        };
        match next {
            Some(size) => {
                // A refusal below sets its own status; a success must not leave a stale bound message.
                self.status.clear();
                self.error = false;
                self.native_operation(Operation::SetManipulatorSize(size));
            }
            None => {
                let (lowest, highest) = ManipulatorSize::key_limits();
                let (side, name, key_limit, mcp_limit) = if grow {
                    ("above", "largest", highest, ManipulatorSize::MAX)
                } else {
                    ("below", "smallest", lowest, ManipulatorSize::MIN)
                };
                self.status = format!(
                    "Manipulator size is at or {side} its {name} key step ({key_limit:.3}); set_manipulator_size reaches {mcp_limit}"
                );
                self.error = false;
            }
        }
    }
    fn unsupported_shading_status(&mut self, name: &str) {
        self.status = unsupported_shading_reason(name);
        self.error = false;
    }
    pub(super) fn cancel_on_escape(&mut self, ctx: &egui::Context) {
        // egui clears text focus before this pass, so cancellation cannot rely on has_focus.
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            let cancelled = self.cancel_move();
            self.consume_pointer_sequence();
            if let Err(error) = cancelled {
                self.set_error_status(&error);
                self.error = true;
                return;
            }
            self.sync_fields();
            self.status = "Edit cancelled".into();
            self.error = false;
        }
    }
}

/// The last history chord wins, so one frame steps once and never undoes then redoes its own step.
/// Modifiers are per event because egui's `i.modifiers` keeps only the frame-final state; `repeat` is
/// allowed so a held chord keeps stepping (once per frame).
fn history_chord(events: &[egui::Event]) -> Option<Operation> {
    events
        .iter()
        .filter_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && !modifiers.alt => {
                // Alt chords belong to Maya camera and marking menus, as in file_chord.
                // Ctrl/Cmd Y is the documented Redo; Shift+Z stays as an extra.
                match (key, modifiers.shift) {
                    (Key::Z, false) => Some(Operation::Undo),
                    (Key::Z, true) | (Key::Y, false) => Some(Operation::Redo),
                    _ => None,
                }
            }
            _ => None,
        })
        .next_back()
}
