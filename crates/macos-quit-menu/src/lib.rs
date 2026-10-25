//! Retargets the default winit macOS Quit item (`terminate:`, Command-Q) to `performClose:`.
//!
//! `terminate:` exits the process inside AppKit and never produces a winit `CloseRequested`,
//! so the app's unsaved-changes guard is skipped. `performClose:` reaches the key window's
//! delegate (`windowShouldClose:`) and therefore the existing close guard.
//! Dock Quit, logout and shutdown still send `terminate:` directly and are not covered.
#![cfg(target_os = "macos")]

use std::fmt;

use objc2::MainThreadMarker;
use objc2::runtime::Sel;
use objc2::sel;
use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSMenu, NSMenuItem};

/// Why the Quit item could not be retargeted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitMenuError {
    /// The caller is not on the main thread.
    NotMainThread,
    /// The application has no main menu.
    NoMainMenu,
    /// No item with action `terminate:`, key equivalent "q" and the Command modifier exists.
    NotFound,
    /// More than one such item exists.
    Ambiguous,
}

impl fmt::Display for QuitMenuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMainThread => f.write_str(
                "macOS Quit menu must be routed from the main thread; see known-issues (#11)",
            ),
            Self::NoMainMenu => f.write_str(
                "macOS main menu is missing; Command-Q cannot be routed to the unsaved-changes \
                 guard, see known-issues (#11)",
            ),
            Self::NotFound => f.write_str(
                "macOS Quit menu item not found; the winit menu layout changed, see \
                 known-issues (#11)",
            ),
            Self::Ambiguous => f.write_str(
                "several macOS Quit menu items found; the winit menu layout changed, see \
                 known-issues (#11)",
            ),
        }
    }
}

impl std::error::Error for QuitMenuError {}

fn is_quit(item: &NSMenuItem, terminate: Sel) -> bool {
    item.action() == Some(terminate)
        && item.keyEquivalent().to_string() == "q"
        && item.keyEquivalentModifierMask() == NSEventModifierFlags::Command
}

/// Sets the unique `terminate:` / Command-"q" item of `menu` (searching its submenus) to `performClose:`.
/// Mutates nothing unless exactly one such item exists.
pub fn retarget_quit(menu: &NSMenu) -> Result<(), QuitMenuError> {
    let terminate = sel!(terminate:);
    let mut found = None;
    for top in menu.itemArray().iter() {
        let Some(submenu) = top.submenu() else {
            continue;
        };
        for item in submenu.itemArray().iter() {
            if is_quit(&item, terminate) {
                if found.is_some() {
                    return Err(QuitMenuError::Ambiguous);
                }
                found = Some(item);
            }
        }
    }
    let item = found.ok_or(QuitMenuError::NotFound)?;
    #[allow(unsafe_code)]
    // SAFETY: `performClose:` is a valid action selector that NSWindow implements and AppKit
    // routes through the responder chain; `setAction` only stores the selector.
    unsafe {
        item.setAction(Some(sel!(performClose:)));
    }
    Ok(())
}

/// Retargets Quit in the running application's main menu. Prints the error before returning it
/// so a launch that exits at startup leaves a trace.
pub fn route_quit_to_close() -> Result<(), QuitMenuError> {
    let result = MainThreadMarker::new()
        .ok_or(QuitMenuError::NotMainThread)
        .and_then(|mtm| {
            NSApplication::sharedApplication(mtm)
                .mainMenu()
                .ok_or(QuitMenuError::NoMainMenu)
        })
        .and_then(|menu| retarget_quit(&menu));
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result
}
