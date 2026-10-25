//! macOS quit handling. The AppKit parts compile only on macOS; the pure termination logic in
//! `terminate_core` has no platform types and builds and tests on every OS.

#[cfg(target_os = "macos")]
mod quit_menu;
mod terminate_core;

#[cfg(target_os = "macos")]
pub use quit_menu::{QuitMenuError, retarget_quit, route_quit_to_close};
pub use terminate_core::{
    ReplySink, TerminateError, TerminateState, WindowFacts, choose_close_target,
};
