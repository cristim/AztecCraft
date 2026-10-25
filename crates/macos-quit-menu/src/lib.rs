//! macOS quit handling. The AppKit parts compile only on macOS; the pure termination logic in
//! `terminate_core` has no platform types and builds and tests on every OS.

#[cfg(target_os = "macos")]
mod quit_menu;
#[cfg(target_os = "macos")]
mod terminate;
mod terminate_core;

#[cfg(target_os = "macos")]
pub use quit_menu::{QuitMenuError, retarget_quit, route_quit_to_close};
#[cfg(target_os = "macos")]
pub use terminate::{
    TerminateHookError, finish_terminate, install_terminate_guard, install_terminate_guard_in,
    terminate_owed,
};
pub use terminate_core::{TerminateError, TerminateState, WindowFacts, choose_close_target};
