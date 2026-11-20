//! The RMB mesh marking menu in the viewport (issue #32, slice T2b): the entry layout, the
//! per-frame input pass that drives `azteccraft::marking_menu::Interaction`, and the dispatch.
//!
//! The menu holds no scene state. A chosen mode becomes the same `SelectionModeIntent` the Status
//! Line and the F8 to F11 keys produce, so the draft, read-only and release-frame drag rules are
//! shared; Extrude opens the Extrude Options window the shelf button opens. Marking menus are a
//! UI widget with no MCP tool of their own: every enabled entry maps to an existing tool (see
//! `MeshAction::mcp_tool`), and a held or open menu makes remote mutations return `BusyEditing`.
//!
//! The layout is original and PROVISIONAL: the Maya 2027 Marking Menus page describes the
//! gestures and the Ctrl, Shift and hotkey variants but does not give the polygon menu's
//! positions or entries.

mod layout;
mod paint;

use crate::app::{Editor, SelectionModeIntent};
use azteccraft::marking_menu::{Button, FrameInput, MarkingMenu, NavKey, Outcome, Target};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect};
use layout::{MeshAction, hit, mesh_menu};

/// The keyboard opener: no Maya key is documented (PROVISIONAL), Shift+F10 is the platform's
/// context-menu chord.
const KEY_OPEN: (Modifiers, Key) = (Modifiers::SHIFT, Key::F10);

fn point(pos: Pos2) -> [f32; 2] {
    [pos.x, pos.y]
}

/// What the menu pass hands back to the viewport.
#[derive(Default)]
pub(in crate::app) struct MenuFrame {
    pub(in crate::app) intent: Option<SelectionModeIntent>,
    /// A Primary release this frame belonged to the menu (chose, rejected or dismissed): the
    /// viewport must not read it as a pick or marquee.
    pub(in crate::app) primary_release_taken: bool,
}

fn human_event(event: &Event) -> bool {
    matches!(
        event,
        Event::PointerButton { .. }
            | Event::PointerMoved(_)
            | Event::MouseWheel { .. }
            | Event::Key { .. }
    )
}

fn nav_key(key: Key, modifiers: Modifiers) -> Option<NavKey> {
    Some(match key {
        Key::ArrowRight | Key::ArrowDown => NavKey::Next,
        Key::ArrowLeft | Key::ArrowUp => NavKey::Prev,
        Key::Tab if modifiers.shift => NavKey::Prev,
        Key::Tab => NavKey::Next,
        _ => NavKey::Number(match key {
            Key::Num1 => 1,
            Key::Num2 => 2,
            Key::Num3 => 3,
            Key::Num4 => 4,
            Key::Num5 => 5,
            Key::Num6 => 6,
            Key::Num7 => 7,
            Key::Num8 => 8,
            Key::Num9 => 9,
            _ => return None,
        }),
    })
}

impl Editor {
    fn marking_menu_for_mode(&self) -> MarkingMenu<MeshAction> {
        mesh_menu(self.selection_mode, self.extrude_refusal())
    }

    /// Whether a Secondary press at `pos` may open the menu now.
    fn marking_menu_may_open(
        &self,
        ui: &egui::Ui,
        viewport: Rect,
        pos: Pos2,
        modifiers: Modifiers,
    ) -> bool {
        viewport.contains(pos)
            && modifiers.is_none()
            && ui.ctx().layer_id_at(pos) == Some(ui.layer_id())
            && !self.overlay_open(ui.ctx())
            && !self.files.is_modal()
            && !self.files.is_closing()
            && self.native_move.is_none()
            && self.history.active_manipulation().is_none()
            && !crate::app::text_edit_focused(ui.ctx())
    }

    /// The per-frame menu pass, run after the viewport is captured and before picks are read, so
    /// a chosen mode reaches `can_select` as an intent and the menu is already closed by then.
    /// Nothing is scheduled or allocated while the menu is Closed and no opening input arrives.
    pub(in crate::app) fn marking_menu_pass(&mut self, ui: &egui::Ui, viewport: Rect) -> MenuFrame {
        let mut frame = MenuFrame::default();
        let events = ui.input(|input| input.events.clone());
        let now = ui.input(|input| input.time);
        let opening = events.iter().any(|event| {
            matches!(
                event,
                Event::PointerButton {
                    button: PointerButton::Secondary,
                    pressed: true,
                    ..
                }
            ) || matches!(event, Event::Key { key, pressed: true, modifiers, .. }
                if (*modifiers, *key) == KEY_OPEN)
        });
        if !self.marking_menu.holds_overlay() && !opening {
            return frame;
        }
        let menu = self.marking_menu_for_mode();
        let enabled = |target: Target| menu.is_enabled(target);
        let mut outcomes = Vec::new();
        if self.files.is_modal() || self.files.is_closing() {
            outcomes.push(self.marking_menu.focus_lost());
        }
        for event in &events {
            let target_at = |origin: Option<[f32; 2]>, pos: Pos2| {
                origin.and_then(|origin| hit(&menu, origin, point(pos)))
            };
            match event {
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    let origin = self.marking_menu.origin();
                    let target = target_at(origin, *pos);
                    match (button, pressed) {
                        (PointerButton::Secondary, true)
                            if !self.marking_menu.holds_overlay()
                                && !self.marking_menu_may_open(ui, viewport, *pos, *modifiers) => {}
                        (PointerButton::Secondary, true) => {
                            outcomes.push(self.marking_menu.press(
                                Button::Secondary,
                                point(*pos),
                                None,
                                now,
                                &enabled,
                            ));
                        }
                        (PointerButton::Primary, true) if self.marking_menu.holds_overlay() => {
                            outcomes.push(self.marking_menu.press(
                                Button::Primary,
                                point(*pos),
                                target,
                                now,
                                &enabled,
                            ));
                        }
                        (PointerButton::Secondary | PointerButton::Primary, false) => {
                            let button = if *button == PointerButton::Primary {
                                Button::Primary
                            } else {
                                Button::Secondary
                            };
                            let held = self.marking_menu.holds_overlay();
                            outcomes.push(self.marking_menu.release(
                                button,
                                point(*pos),
                                target,
                                now,
                                &enabled,
                            ));
                            frame.primary_release_taken |= held && button == Button::Primary;
                        }
                        _ => {}
                    }
                }
                Event::PointerMoved(pos) => self.marking_menu.moved(point(*pos), now),
                Event::PointerGone | Event::WindowFocused(false) => {
                    outcomes.push(self.marking_menu.focus_lost());
                }
                Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => {
                    if *key == Key::Escape {
                        outcomes.push(self.marking_menu.escape());
                    } else if (*modifiers, *key) == KEY_OPEN {
                        let pointer = ui.input(|input| input.pointer.latest_pos());
                        let origin = pointer.filter(|p| viewport.contains(*p));
                        if self.marking_menu_may_open(
                            ui,
                            viewport,
                            origin.unwrap_or(viewport.center()),
                            Modifiers::NONE,
                        ) {
                            self.marking_menu
                                .key_open(point(origin.unwrap_or(viewport.center())), now);
                        }
                    } else if self.marking_menu.is_active() && self.marking_menu.origin().is_some()
                    {
                        if let Some(nav) = nav_key(*key, *modifiers) {
                            self.marking_menu.key_nav(nav, &menu.nav_order(), now);
                        } else if matches!(key, Key::Enter | Key::Space) {
                            outcomes.push(self.marking_menu.key_activate(now, &enabled));
                        }
                    }
                }
                _ => {}
            }
        }
        let (primary_down, secondary_down) = ui.input(|input| {
            (
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Secondary),
            )
        });
        outcomes.push(self.marking_menu.tick(
            now,
            FrameInput {
                human_input: events.iter().any(human_event),
                primary_down,
                secondary_down,
            },
        ));
        for outcome in outcomes {
            self.apply_menu_outcome(&menu, outcome, &mut frame);
        }
        // Keys that moved menu focus must not also move egui's own focus or fire shortcuts.
        if self.marking_menu.is_active() {
            ui.input_mut(|input| {
                for key in [
                    Key::ArrowLeft,
                    Key::ArrowRight,
                    Key::ArrowUp,
                    Key::ArrowDown,
                    Key::Tab,
                    Key::Enter,
                    Key::Space,
                ] {
                    input.consume_key(Modifiers::NONE, key);
                }
                input.consume_key(Modifiers::SHIFT, Key::Tab);
            });
        }
        if let Some(wait) = self.marking_menu.repaint_after(now) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        if self.marking_menu.is_drawn() {
            self.marking_menu_help(ui, &menu);
            paint::paint(ui, &self.marking_menu, &menu);
        }
        frame
    }

    fn apply_menu_outcome(
        &mut self,
        menu: &MarkingMenu<MeshAction>,
        outcome: Outcome,
        frame: &mut MenuFrame,
    ) {
        match outcome {
            Outcome::Dispatch(target) => {
                if let azteccraft::marking_menu::Resolved::Dispatch(action) = menu.resolve(target) {
                    match action.intent() {
                        Some(intent) => frame.intent = Some(intent),
                        None => self.open_extrude_options(),
                    }
                }
            }
            Outcome::Rejected(target) => self.set_menu_help(menu, target),
            Outcome::None | Outcome::Dismissed => {}
        }
    }

    /// The Help Line text for an entry that cannot be chosen: its label and reason.
    fn set_menu_help(&mut self, menu: &MarkingMenu<MeshAction>, target: Target) {
        if let Some(entry) = menu.entry(target)
            && let Some(reason) = entry.disabled_reason()
        {
            self.status = format!("{}: {reason}", entry.label);
            self.error = false;
        }
    }

    /// While the dial is drawn, the entry under the pointer or the keyboard focus that cannot be
    /// chosen shows its reason in the Help Line.
    fn marking_menu_help(&mut self, ui: &egui::Ui, menu: &MarkingMenu<MeshAction>) {
        let under_pointer = self.marking_menu.origin().and_then(|origin| {
            let pos = ui.input(|input| input.pointer.latest_pos())?;
            hit(menu, origin, point(pos))
        });
        if let Some(target) = self.marking_menu.focused().or(under_pointer) {
            self.set_menu_help(menu, target);
        }
    }
}

#[cfg(test)]
mod a11y_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod pass_tests;
