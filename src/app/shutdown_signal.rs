//! SIGTERM as a close request (#261, PROVISIONAL policy, owner decision Q3/Q6): the first signal
//! asks the shared close guard to prompt (or to close a clean scene), a second signal before the
//! prompt is resolved exits at once so a hung UI cannot make `kill` useless. Autosave or recovery
//! for an unattended stop is a separate issue. Windows has no SIGTERM, so the stub never fires.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// 128 + SIGTERM, the status a shell reports for a process ended by SIGTERM.
#[cfg(unix)]
const FORCED_EXIT_STATUS: i32 = 143;

#[derive(Clone, Default)]
pub(super) struct ShutdownSignal {
    /// A SIGTERM arrived and the Editor has not started the close guard for it yet.
    requested: Arc<AtomicBool>,
    /// A close request from a SIGTERM is unresolved; the next SIGTERM exits the process.
    armed: Arc<AtomicBool>,
}

impl ShutdownSignal {
    pub(super) fn requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
    /// The Editor has handed the request to the close guard.
    pub(super) fn consume(&self) {
        self.requested.store(false, Ordering::SeqCst);
    }
    /// The close attempt ended without exiting (Cancel, refusal, failure): a later SIGTERM must
    /// prompt again rather than kill the app.
    pub(super) fn reset(&self) {
        self.requested.store(false, Ordering::SeqCst);
        self.armed.store(false, Ordering::SeqCst);
    }
    /// What the signal thread does for each SIGTERM.
    fn mark(&self) {
        self.requested.store(true, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
    }
    #[cfg(test)]
    pub(super) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }
    #[cfg(test)]
    pub(super) fn raise_for_test(&self) {
        self.mark();
    }
}

/// Registers SIGTERM handling. The conditional shutdown goes first so signal-hook runs it before
/// the thread's wake-up: the first signal sees `armed == false` and falls through.
#[cfg(unix)]
pub(super) fn install(ctx: &eframe::egui::Context) -> std::io::Result<ShutdownSignal> {
    use signal_hook::{consts::SIGTERM, flag, iterator::Signals};
    let state = ShutdownSignal::default();
    flag::register_conditional_shutdown(SIGTERM, FORCED_EXIT_STATUS, state.armed.clone())?;
    let mut signals = Signals::new([SIGTERM])?;
    let thread_state = state.clone();
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("sigterm".into())
        .spawn(move || {
            for _ in signals.forever() {
                thread_state.mark();
                ctx.request_repaint();
            }
        })?;
    Ok(state)
}

#[cfg(not(unix))]
pub(super) fn install(_ctx: &eframe::egui::Context) -> std::io::Result<ShutdownSignal> {
    Ok(ShutdownSignal::default())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const CHILD: &str = "AZTECCRAFT_SIGTERM_CHILD";

    fn wait_for(state: &ShutdownSignal) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !state.requested() {
            assert!(Instant::now() < deadline, "SIGTERM never reached the flag");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn first_sigterm_sets_the_flag_and_repaints_and_a_second_exits() {
        if std::env::var_os(CHILD).is_some() {
            let ctx = eframe::egui::Context::default();
            let state = install(&ctx).unwrap();
            for _ in 0..3 {
                let mut output = ctx.run_ui(eframe::egui::RawInput::default(), |_| {});
                output.textures_delta.clear();
            }
            assert!(
                !ctx.has_requested_repaint(),
                "repaint pending before the signal"
            );
            signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
            wait_for(&state);
            assert!(ctx.has_requested_repaint(), "no repaint was requested");
            signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
            std::thread::sleep(Duration::from_secs(10));
            panic!("the second SIGTERM did not exit");
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app::shutdown_signal::tests::first_sigterm_sets_the_flag_and_repaints_and_a_second_exits",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(FORCED_EXIT_STATUS));
    }
}
