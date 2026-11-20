//! Keyboard drive of a sticky menu. Every key goes through the same `choose` rules as the
//! pointer, so a disabled or unsupported entry is rejected, never dispatched.
//!
//! The keys are PROVISIONAL (the Maya help names none for marking menus); the caller maps them:
//! Right, Down and Tab are `Next`; Left, Up and Shift+Tab are `Prev`; the digits 1 to 9 are
//! `Number`; Enter and Space call `key_activate`; Escape calls `escape`.

use super::{Interaction, Outcome, Phase, Target, finite};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    /// Focus the next entry in navigation order, wrapping around.
    Next,
    /// Focus the previous entry, wrapping around.
    Prev,
    /// Focus the entry at this 1-based place in navigation order (disabled entries count, so
    /// the number matches what the user sees).
    Number(usize),
}

impl Interaction {
    /// Opens the menu from the keyboard at `origin` (where the dial is placed), sticky and with
    /// nothing focused. Only a Closed menu opens; the caller applies the same gates as for the
    /// pointer trigger.
    pub fn key_open(&mut self, origin: [f32; 2], now: f64) -> Outcome {
        if !finite(origin) || !now.is_finite() || !matches!(self.phase, Phase::Closed) {
            return Outcome::None;
        }
        self.phase = Phase::Open {
            origin,
            armed: None,
            primary_down: false,
            focus: None,
        };
        self.touch(now);
        Outcome::None
    }

    /// The entry keyboard focus is on in a sticky menu.
    pub fn focused(&self) -> Option<Target> {
        match self.phase {
            Phase::Open { focus, .. } => focus,
            _ => None,
        }
    }

    /// Moves focus. `order` is `MarkingMenu::nav_order()`. Disabled and unsupported entries take
    /// focus too, so a screen reader can read their reasons; choosing one is still rejected
    /// (`key_activate`). PROVISIONAL: the Maya help names no menu keyboard behaviour. Only a
    /// sticky menu reacts.
    pub fn key_nav(&mut self, key: NavKey, order: &[Target], now: f64) -> Outcome {
        if !now.is_finite() {
            return Outcome::None;
        }
        let Phase::Open { focus, .. } = self.phase else {
            return Outcome::None;
        };
        self.touch(now);
        self.set_armed(None);
        let next_focus = match key {
            NavKey::Number(n) => {
                let Some(target) = n.checked_sub(1).and_then(|i| order.get(i)).copied() else {
                    return Outcome::None;
                };
                target
            }
            NavKey::Next | NavKey::Prev => {
                let at = focus.and_then(|f| order.iter().position(|c| *c == f));
                let last = order.len().saturating_sub(1);
                let index = match (key, at) {
                    _ if order.is_empty() => return Outcome::None,
                    (NavKey::Next, None) => 0,
                    (NavKey::Next, Some(i)) => (i + 1) % order.len(),
                    (_, None) => last,
                    (_, Some(i)) => (i + last) % order.len(),
                };
                order[index]
            }
        };
        self.set_focus(Some(next_focus));
        Outcome::None
    }

    /// Enter or Space: chooses the focused entry through the same rules as a pointer release.
    /// An entry that cannot be chosen is `Rejected`; the menu stays open and focus stays on it.
    pub fn key_activate(&mut self, now: f64, enabled: &dyn Fn(Target) -> bool) -> Outcome {
        if !now.is_finite() {
            return Outcome::None;
        }
        let Phase::Open {
            focus: Some(target),
            ..
        } = self.phase
        else {
            return Outcome::None;
        };
        self.touch(now);
        self.set_armed(None);
        // Focus stays: a rejected entry keeps it (the reason is read from it), and a dispatch
        // closes the menu.
        self.choose(target, enabled, true)
    }

    fn set_focus(&mut self, to: Option<Target>) {
        if let Phase::Open { focus, .. } = &mut self.phase {
            *focus = to;
        }
    }
}
