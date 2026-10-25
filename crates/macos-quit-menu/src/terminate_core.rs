//! Platform-free parts of the macOS terminate guard (#261): which window receives the close
//! request, and the bookkeeping that guarantees one reply per owed termination.
//! No AppKit types appear here, so this module builds and is tested on every OS.

use std::fmt;

/// What the hook needs to know about one application window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowFacts {
    pub is_key: bool,
    pub is_main: bool,
    pub is_visible: bool,
    pub is_miniaturized: bool,
    pub is_closable: bool,
}

/// Index of the window that should receive `performClose:`, or `None` when no window can be
/// closed (the caller then has nothing to guard). Non-closable windows are never chosen: AppKit
/// beeps on `performClose:` for them and queues no close request, which would leave the
/// termination owed forever. Preference: key, main, first visible, first miniaturized, first
/// closable.
pub fn choose_close_target(windows: &[WindowFacts]) -> Option<usize> {
    let first = |wanted: fn(&WindowFacts) -> bool| {
        windows
            .iter()
            .position(|window| window.is_closable && wanted(window))
    };
    first(|w| w.is_key)
        .or_else(|| first(|w| w.is_main))
        .or_else(|| first(|w| w.is_visible))
        .or_else(|| first(|w| w.is_miniaturized))
        .or_else(|| first(|_| true))
}

/// A reply was attempted with no termination owed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminateError {
    NotOwed,
}

impl fmt::Display for TerminateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOwed => f.write_str(
                "macOS terminate reply attempted with no termination owed; see known-issues (#261)",
            ),
        }
    }
}

impl std::error::Error for TerminateError {}

/// Tracks whether a termination is owed: `begin` makes it owed (the hook returned
/// `NSTerminateLater`), and exactly one `reply` must follow. After the reply the state is idle,
/// so a cancelled quit can be retried.
#[derive(Debug, Default)]
pub struct TerminateState {
    owed: bool,
    queued: Option<bool>,
}

impl TerminateState {
    pub const fn new() -> Self {
        Self {
            owed: false,
            queued: None,
        }
    }

    /// Whether a termination is waiting for its reply.
    pub fn owed(&self) -> bool {
        self.owed
    }

    /// Whether an AppKit reply has been queued but not yet delivered. A new terminate must not
    /// begin until it is: the stale reply would answer the new request instead.
    pub fn queued(&self) -> bool {
        self.queued.is_some()
    }

    /// The answer in the queued AppKit reply, if one is queued.
    pub fn queued_answer(&self) -> Option<bool> {
        self.queued
    }

    /// Records that an AppKit reply was queued. Call it only where a reply is really sent.
    pub fn mark_queued(&mut self, approved: bool) {
        self.queued = Some(approved);
    }

    /// Records that the queued AppKit reply ran or was dropped.
    pub fn delivered(&mut self) {
        self.queued = None;
    }

    /// Records a `terminate:`. Call it only after `choose_close_target` returned `Some`; with no
    /// target there is nothing to owe and the caller returns `NSTerminateCancel` without
    /// touching this state. Returns true when this call made the termination owed, false when
    /// one was already owed (the caller must not start a second close request).
    pub fn begin(&mut self) -> bool {
        !std::mem::replace(&mut self.owed, true)
    }

    /// Records the decision for the owed termination, then passes it to `send` exactly once.
    /// The state is updated before the deferred AppKit reply runs, so the owed flag is already
    /// clear when AppKit acts on it. Without an owed termination `send` is not called.
    pub fn reply(&mut self, approved: bool, send: impl FnOnce(bool)) -> Result<(), TerminateError> {
        if !self.owed {
            return Err(TerminateError::NotOwed);
        }
        self.owed = false;
        send(approved);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: WindowFacts = WindowFacts {
        is_key: false,
        is_main: false,
        is_visible: false,
        is_miniaturized: false,
        is_closable: true,
    };

    fn with(set: impl FnOnce(&mut WindowFacts)) -> WindowFacts {
        let mut facts = NONE;
        set(&mut facts);
        facts
    }

    // H2
    #[test]
    fn choose_close_target_prefers_key_main_visible_miniaturized_closable() {
        let key = with(|w| w.is_key = true);
        let main = with(|w| w.is_main = true);
        let visible = with(|w| w.is_visible = true);
        let mini = with(|w| w.is_miniaturized = true);
        assert_eq!(choose_close_target(&[]), None);
        assert_eq!(
            choose_close_target(&[NONE, mini, visible, main, key]),
            Some(4)
        );
        assert_eq!(choose_close_target(&[NONE, mini, visible, main]), Some(3));
        assert_eq!(choose_close_target(&[NONE, mini, visible]), Some(2));
        assert_eq!(choose_close_target(&[NONE, mini]), Some(1));
        assert_eq!(choose_close_target(&[NONE, NONE]), Some(0));
        // first match wins within a tier
        assert_eq!(choose_close_target(&[visible, visible]), Some(0));
    }

    #[test]
    fn choose_close_target_skips_non_closable_windows() {
        let locked_key = with(|w| {
            w.is_key = true;
            w.is_closable = false;
        });
        let locked = with(|w| w.is_closable = false);
        let visible = with(|w| w.is_visible = true);
        assert_eq!(choose_close_target(&[locked_key, visible]), Some(1));
        assert_eq!(choose_close_target(&[locked_key, locked]), None);
        assert_eq!(choose_close_target(&[locked_key, locked, NONE]), Some(2));
    }

    // H3
    #[test]
    fn terminate_state_replies_exactly_once_per_owed_termination() {
        let mut state = TerminateState::new();
        let mut sent = Vec::new();
        assert!(!state.owed());
        assert!(state.begin());
        assert!(state.owed());
        assert_eq!(state.reply(false, |a| sent.push(a)), Ok(()));
        assert!(!state.owed());
        assert_eq!(
            state.reply(true, |a| sent.push(a)),
            Err(TerminateError::NotOwed)
        );
        assert_eq!(sent, [false]);
        // a cancelled quit can be retried
        assert!(state.begin());
        assert_eq!(state.reply(true, |a| sent.push(a)), Ok(()));
        assert_eq!(sent, [false, true]);
    }

    #[test]
    fn second_terminate_while_owed_is_idempotent() {
        let mut state = TerminateState::new();
        let mut sent = Vec::new();
        assert!(state.begin());
        assert!(!state.begin());
        assert_eq!(state.reply(true, |a| sent.push(a)), Ok(()));
        assert_eq!(sent, [true]);
    }

    #[test]
    fn reply_without_owed_is_an_error_value_and_sends_nothing() {
        let mut state = TerminateState::new();
        let mut sent = Vec::new();
        assert_eq!(
            state.reply(true, |a| sent.push(a)),
            Err(TerminateError::NotOwed)
        );
        assert!(sent.is_empty());
        assert!(TerminateError::NotOwed.to_string().contains("#261"));
    }

    #[test]
    fn queued_reply_is_tracked_until_delivered() {
        let mut state = TerminateState::new();
        assert!(!state.queued());
        assert_eq!(state.queued_answer(), None);
        state.mark_queued(false);
        assert!(state.queued());
        assert_eq!(state.queued_answer(), Some(false));
        state.delivered();
        assert!(!state.queued());
        assert_eq!(state.queued_answer(), None);
        state.delivered();
        assert!(!state.queued());
    }

    #[test]
    fn reply_does_not_mark_a_reply_as_queued() {
        let mut state = TerminateState::new();
        assert!(state.begin());
        assert_eq!(state.reply(false, |_| {}), Ok(()));
        assert!(!state.queued());
    }
}
