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

/// Receives the single answer to an owed termination (the production sink schedules
/// `replyToApplicationShouldTerminate:` on the main queue).
pub trait ReplySink {
    fn reply(&mut self, approved: bool);
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Owed,
    Replied,
}

/// `Idle -> Owed -> Replied`, and `Replied -> Owed` again on the next `terminate:` (a cancelled
/// quit can be retried). Owed means the hook returned `NSTerminateLater`; exactly one reply must
/// follow.
#[derive(Debug, Default)]
pub struct TerminateState {
    phase: Phase,
}

impl TerminateState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a termination is waiting for its reply.
    pub fn owed(&self) -> bool {
        self.phase == Phase::Owed
    }

    /// Records a `terminate:`. Returns true when this call made the termination owed, false when
    /// one was already owed (the caller must not start a second close request).
    pub fn begin(&mut self) -> bool {
        let newly_owed = self.phase != Phase::Owed;
        self.phase = Phase::Owed;
        newly_owed
    }

    /// Sends the one reply for the owed termination. Without an owed termination nothing is sent.
    pub fn reply(
        &mut self,
        approved: bool,
        sink: &mut impl ReplySink,
    ) -> Result<(), TerminateError> {
        if !self.owed() {
            return Err(TerminateError::NotOwed);
        }
        self.phase = Phase::Replied;
        sink.reply(approved);
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

    #[derive(Default)]
    struct Recorder(Vec<bool>);

    impl ReplySink for Recorder {
        fn reply(&mut self, approved: bool) {
            self.0.push(approved);
        }
    }

    // H3
    #[test]
    fn terminate_state_replies_exactly_once_per_owed_termination() {
        let mut state = TerminateState::new();
        let mut sink = Recorder::default();
        assert!(!state.owed());
        assert!(state.begin());
        assert!(state.owed());
        assert_eq!(state.reply(false, &mut sink), Ok(()));
        assert!(!state.owed());
        assert_eq!(state.reply(true, &mut sink), Err(TerminateError::NotOwed));
        assert_eq!(sink.0, [false]);
        // a cancelled quit can be retried
        assert!(state.begin());
        assert_eq!(state.reply(true, &mut sink), Ok(()));
        assert_eq!(sink.0, [false, true]);
    }

    #[test]
    fn second_terminate_while_owed_is_idempotent() {
        let mut state = TerminateState::new();
        let mut sink = Recorder::default();
        assert!(state.begin());
        assert!(!state.begin());
        assert_eq!(state.reply(true, &mut sink), Ok(()));
        assert_eq!(sink.0, [true]);
    }

    #[test]
    fn reply_without_owed_is_an_error_value_and_sends_nothing() {
        let mut state = TerminateState::new();
        let mut sink = Recorder::default();
        assert_eq!(state.reply(true, &mut sink), Err(TerminateError::NotOwed));
        assert!(sink.0.is_empty());
        assert!(TerminateError::NotOwed.to_string().contains("#261"));
    }
}
