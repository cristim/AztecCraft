use super::model::{Target, zone_at};
mod keyboard;

pub use keyboard::NavKey;

use super::{DEAD_ZONE, DRAW_DELAY, FLICK_DISTANCE, IDLE_DISMISS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
}

/// What the interaction decided; the caller acts on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// Dispatch the entry at the target. The menu is closed.
    Dispatch(Target),
    /// The target cannot be chosen (disabled, unsupported, or an empty dial zone on a release).
    /// A held menu closes; a sticky menu stays open and disarmed. The caller shows the reason.
    Rejected(Target),
    /// The menu closed without choosing anything.
    Dismissed,
}

/// Per-frame facts the caller reads from the frame's input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameInput {
    /// The frame carries a human pointer or key event. A frame woken by MCP or by the timer does
    /// not, and never extends the idle time.
    pub human_input: bool,
    pub primary_down: bool,
    pub secondary_down: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Closed,
    /// Secondary button down. `excursion` is set once the pointer left the dead zone.
    Held {
        origin: [f32; 2],
        t0: f64,
        excursion: bool,
        drawn: bool,
    },
    /// Sticky: the cursor picks, a Primary press arms an entry and its release dispatches.
    Open {
        origin: [f32; 2],
        armed: Option<Target>,
        /// A Primary press inside the menu has not released yet. Kept apart from `armed`, which
        /// the keyboard clears, so Escape can still swallow the pending release.
        primary_down: bool,
        /// Keyboard focus; see `keyboard.rs`.
        focus: Option<Target>,
    },
    /// The menu was dismissed by a press; keeps the overlay gate until that button releases so
    /// the release cannot click through to the viewport.
    Swallow {
        button: Button,
    },
}

/// Input state machine with injected time (seconds). Positions are points; non-finite time or
/// positions are ignored. The menu content is not known here: the caller passes `enabled` to
/// say whether a target can be chosen.
///
/// Clock rules: a release decides drawn or flick from `now - t0` itself, so it does not depend
/// on a tick having run during a long hold (a frame stall). Time going backwards makes that
/// elapsed time negative, which is below the draw delay (a fast flick), and never panics; a
/// backwards `touch` moves the idle reference into the future, which only delays the idle
/// dismissal.
#[derive(Clone, Debug)]
pub struct Interaction {
    phase: Phase,
    t_input: f64,
}

impl Default for Interaction {
    fn default() -> Self {
        Self {
            phase: Phase::Closed,
            t_input: 0.0,
        }
    }
}

fn finite(pos: [f32; 2]) -> bool {
    pos[0].is_finite() && pos[1].is_finite()
}

fn delta(from: [f32; 2], to: [f32; 2]) -> [f32; 2] {
    [to[0] - from[0], to[1] - from[1]]
}

fn dist(d: [f32; 2]) -> f64 {
    f64::from(d[0]).hypot(f64::from(d[1]))
}

impl Interaction {
    /// Held or Open: a remote mutation is refused while the user is in the menu.
    pub fn is_active(&self) -> bool {
        matches!(self.phase, Phase::Held { .. } | Phase::Open { .. })
    }

    /// Anything but Closed: the shared overlay gate (picks, hover, shortcuts) stays shut.
    pub fn holds_overlay(&self) -> bool {
        !matches!(self.phase, Phase::Closed)
    }

    /// Open: the menu stays up after a click, the cursor or the keys pick.
    pub fn is_sticky(&self) -> bool {
        matches!(self.phase, Phase::Open { .. })
    }

    /// The dial is painted: a held button past the draw delay, or a sticky menu.
    pub fn is_drawn(&self) -> bool {
        matches!(
            self.phase,
            Phase::Held { drawn: true, .. } | Phase::Open { .. }
        )
    }

    /// The press origin while Held or Open; zones resolve from it.
    pub fn origin(&self) -> Option<[f32; 2]> {
        match self.phase {
            Phase::Held { origin, .. } | Phase::Open { origin, .. } => Some(origin),
            _ => None,
        }
    }

    /// The entry a Primary press armed in a sticky menu.
    pub fn armed(&self) -> Option<Target> {
        match self.phase {
            Phase::Open { armed, .. } => armed,
            _ => None,
        }
    }

    fn set_primary_down(&mut self, to: bool) {
        if let Phase::Open { primary_down, .. } = &mut self.phase {
            *primary_down = to;
        }
    }

    fn set_armed(&mut self, to: Option<Target>) {
        if let Phase::Open { armed, .. } = &mut self.phase {
            *armed = to;
        }
    }

    fn touch(&mut self, now: f64) {
        self.t_input = now;
    }

    fn close(&mut self) -> Outcome {
        let was = self.holds_overlay();
        self.phase = Phase::Closed;
        if was {
            Outcome::Dismissed
        } else {
            Outcome::None
        }
    }

    /// A press. Secondary opens the menu when Closed. `target` is the entry under the pointer;
    /// the caller passes `None` for an empty zone, a list row past the end and any point outside
    /// the menu. In a sticky menu Primary on an enabled `target` arms it, on a disabled or
    /// unsupported one is rejected (the menu stays open), and on `None` dismisses and swallows
    /// the release; a Secondary press dismisses and swallows.
    pub fn press(
        &mut self,
        button: Button,
        pos: [f32; 2],
        target: Option<Target>,
        now: f64,
        enabled: &dyn Fn(Target) -> bool,
    ) -> Outcome {
        if !finite(pos) || !now.is_finite() {
            return Outcome::None;
        }
        match (self.phase, button) {
            (Phase::Closed, Button::Secondary) => {
                self.phase = Phase::Held {
                    origin: pos,
                    t0: now,
                    excursion: false,
                    drawn: false,
                };
                self.touch(now);
                Outcome::None
            }
            (Phase::Open { .. }, Button::Primary) => {
                self.touch(now);
                match target {
                    Some(t) if enabled(t) => {
                        self.set_armed(Some(t));
                        self.set_primary_down(true);
                        Outcome::None
                    }
                    Some(t) => {
                        self.set_primary_down(true);
                        Outcome::Rejected(t)
                    }
                    None => {
                        self.phase = Phase::Swallow {
                            button: Button::Primary,
                        };
                        Outcome::Dismissed
                    }
                }
            }
            (Phase::Open { .. }, Button::Secondary) => {
                self.touch(now);
                self.phase = Phase::Swallow {
                    button: Button::Secondary,
                };
                Outcome::Dismissed
            }
            _ => Outcome::None,
        }
    }

    /// Pointer motion while a menu is Held.
    pub fn moved(&mut self, pos: [f32; 2], now: f64) {
        if !finite(pos) || !now.is_finite() {
            return;
        }
        if let Phase::Held {
            origin, excursion, ..
        } = &mut self.phase
        {
            if zone_at(delta(*origin, pos), DEAD_ZONE).is_some() {
                *excursion = true;
            }
            self.t_input = now;
        }
    }

    /// A release.
    ///
    /// Held, Secondary: a flick (released before the dial is drawn, at least `FLICK_DISTANCE`
    /// from the origin; PROVISIONAL, the Maya help does not say that a quick gesture skips the
    /// dial) or a drawn release over a zone chooses it; a release in the dead zone after
    /// an excursion dismisses; any other release (a click) turns the menu sticky.
    /// Open, Primary: dispatches the armed entry when released over it; if it has become
    /// disabled since the press the release is `Rejected` and the menu stays open. Swallow: the
    /// matching release closes it. `enabled` is consulted for the chosen target.
    pub fn release(
        &mut self,
        button: Button,
        pos: [f32; 2],
        target: Option<Target>,
        now: f64,
        enabled: &dyn Fn(Target) -> bool,
    ) -> Outcome {
        if !finite(pos) || !now.is_finite() {
            return Outcome::None;
        }
        match (self.phase, button) {
            (
                Phase::Held {
                    origin,
                    t0,
                    excursion,
                    drawn,
                    ..
                },
                Button::Secondary,
            ) => {
                self.touch(now);
                let d = delta(origin, pos);
                let drawn = drawn || now - t0 >= DRAW_DELAY;
                let zone = zone_at(d, DEAD_ZONE);
                let flick = dist(d) >= f64::from(FLICK_DISTANCE);
                match zone {
                    Some(z) if flick || drawn => self.choose(Target::Zone(z), enabled, false),
                    None if excursion => self.close(),
                    _ => {
                        self.phase = Phase::Open {
                            origin,
                            armed: None,
                            primary_down: false,
                            focus: None,
                        };
                        Outcome::None
                    }
                }
            }
            (Phase::Open { armed, .. }, Button::Primary) => {
                self.touch(now);
                self.set_armed(None);
                self.set_primary_down(false);
                match (armed, target) {
                    (Some(a), Some(t)) if a == t => self.choose(t, enabled, true),
                    _ => Outcome::None,
                }
            }
            (Phase::Swallow { button: b }, _) if b == button => {
                self.phase = Phase::Closed;
                Outcome::None
            }
            _ => Outcome::None,
        }
    }

    /// Every choice goes through here: an enabled target dispatches and closes the menu, any
    /// other is rejected. A held menu closes either way (its button is up); a sticky menu stays
    /// open, so the user can pick another entry.
    fn choose(
        &mut self,
        target: Target,
        enabled: &dyn Fn(Target) -> bool,
        sticky: bool,
    ) -> Outcome {
        if enabled(target) {
            self.phase = Phase::Closed;
            Outcome::Dispatch(target)
        } else {
            if !sticky {
                self.phase = Phase::Closed;
            }
            Outcome::Rejected(target)
        }
    }

    /// Escape: dismisses Held and Open. A button still down (the Secondary of a Held menu, the
    /// Primary that armed an entry of a sticky one) is swallowed until it releases, so the
    /// release cannot click through to the viewport. A Swallow keeps swallowing and reports
    /// nothing (already dismissed).
    pub fn escape(&mut self) -> Outcome {
        self.dismiss_swallowing_a_held_button()
    }

    /// Window focus loss or the pointer leaving the window: same as `escape`. The release may
    /// never arrive; `tick` ends the swallow once the button is no longer down.
    pub fn focus_lost(&mut self) -> Outcome {
        self.dismiss_swallowing_a_held_button()
    }

    fn dismiss_swallowing_a_held_button(&mut self) -> Outcome {
        let swallow = match self.phase {
            Phase::Swallow { .. } => return Outcome::None,
            Phase::Held { .. } => Some(Button::Secondary),
            Phase::Open {
                primary_down: true, ..
            } => Some(Button::Primary),
            _ => None,
        };
        match swallow {
            Some(button) => {
                self.phase = Phase::Swallow { button };
                Outcome::Dismissed
            }
            None => self.close(),
        }
    }

    /// The per-frame pass. Call it after this frame's events have been fed in and before any
    /// remote request is applied. Human input resets the idle
    /// time; a frame without it never does. Closes the menu after `IDLE_DISMISS` of no human
    /// input, when a Held menu's Secondary button is no longer down, and when a Swallow's
    /// button is no longer down. Draws a Held menu after `DRAW_DELAY`.
    pub fn tick(&mut self, now: f64, input: FrameInput) -> Outcome {
        if !now.is_finite() || !self.holds_overlay() {
            return Outcome::None;
        }
        if input.human_input {
            self.touch(now);
        }
        if now - self.t_input >= IDLE_DISMISS {
            // A Swallow was reported as Dismissed when it began; ending it is silent.
            return if matches!(self.phase, Phase::Swallow { .. }) {
                self.phase = Phase::Closed;
                Outcome::None
            } else {
                self.close()
            };
        }
        match &mut self.phase {
            Phase::Held { t0, drawn, .. } => {
                if !input.secondary_down {
                    return self.close();
                }
                if now - *t0 >= DRAW_DELAY {
                    *drawn = true;
                }
            }
            Phase::Swallow { button } => {
                let down = match button {
                    Button::Primary => input.primary_down,
                    Button::Secondary => input.secondary_down,
                };
                if !down {
                    self.phase = Phase::Closed;
                }
            }
            // A release the window never saw (focus lost mid-press) must not keep it down.
            Phase::Open { primary_down, .. } if *primary_down && !input.primary_down => {
                *primary_down = false;
            }
            _ => {}
        }
        Outcome::None
    }

    /// Seconds until the next frame the interaction needs, or `None` when Closed (an idle
    /// editor schedules nothing).
    pub fn repaint_after(&self, now: f64) -> Option<f64> {
        let idle = (self.t_input + IDLE_DISMISS - now).max(0.0);
        match self.phase {
            Phase::Closed => None,
            Phase::Held {
                t0, drawn: false, ..
            } => Some(idle.min((t0 + DRAW_DELAY - now).max(0.0))),
            _ => Some(idle),
        }
    }
}
