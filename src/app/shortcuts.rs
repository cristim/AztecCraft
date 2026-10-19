use super::*;

pub(super) const SHADED_TEXTURED_NAME: &str = "Hardware Texturing (6)";
const USE_ALL_LIGHTS_NAME: &str = "Use All Lights (7)";

/// One reason string for the hotkey status and the disabled menu row.
pub(super) fn unsupported_shading_reason(name: &str) -> String {
    format!("{name} is not implemented in this build")
}

impl Editor {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) -> Option<ActiveTool> {
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) || self.files.is_modal()
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
        let (undo, redo, frame, save, save_as) = ctx.input(|i| {
            (
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.is_none() && i.key_pressed(Key::F),
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::S),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::S),
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
        if undo {
            self.history_step(false);
        }
        if redo {
            self.history_step(true);
        }
        if frame {
            self.frame_selected();
        }
        if save && self.files.save_available() {
            self.native_operation(Operation::SaveScene);
        }
        if save_as && self.files.save_as_available() {
            self.save_as_dialog();
        }
        tool_intent
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
