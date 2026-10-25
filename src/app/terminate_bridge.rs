//! Seam between the Editor's close guard and an OS termination request that is waiting for the
//! app's answer (#261). The trait is platform-neutral so the guard is testable everywhere; only
//! the macOS implementation touches AppKit, through the `macos-quit-menu` crate.

pub(super) trait TerminateBridge {
    /// An OS termination (Dock Quit, logout) is waiting for the Editor's decision.
    fn owed(&self) -> bool;
    /// Answers the owed termination: `true` lets the app exit, `false` keeps it running.
    fn finish(&mut self, approved: bool);
}

#[cfg(target_os = "macos")]
pub(super) struct MacBridge;

#[cfg(target_os = "macos")]
impl TerminateBridge for MacBridge {
    fn owed(&self) -> bool {
        macos_quit_menu::terminate_owed()
    }
    fn finish(&mut self, approved: bool) {
        if let Err(error) = macos_quit_menu::finish_terminate(approved) {
            eprintln!("could not answer the pending termination: {error}");
        }
    }
}

/// Platforms without an OS termination hook never owe an answer.
#[cfg(not(target_os = "macos"))]
pub(super) struct NullBridge;

#[cfg(not(target_os = "macos"))]
impl TerminateBridge for NullBridge {
    fn owed(&self) -> bool {
        false
    }
    fn finish(&mut self, _approved: bool) {}
}

pub(super) fn platform_bridge() -> Box<dyn TerminateBridge> {
    #[cfg(target_os = "macos")]
    return Box::new(MacBridge);
    #[cfg(not(target_os = "macos"))]
    Box::new(NullBridge)
}
