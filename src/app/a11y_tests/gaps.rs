//! Recorded violations, one file per area. The gate fails both when a violation is new and when a
//! recorded one is gone, so a fix deletes its lines from the file of its area.

mod dialogs;
mod fields;
mod keyboard;
mod menus;
mod shell;
mod timeline;

pub(super) const AREAS: [(&str, &[&str]); 5] = [
    ("dialogs", dialogs::GAPS),
    ("fields", fields::GAPS),
    ("menus", menus::GAPS),
    ("shell", shell::GAPS),
    ("timeline", timeline::GAPS),
];

/// Gaps only Windows produces: the authored-scene encoder is Unix-only, so Save As is disabled
/// there, still without a reason until `describe_disabled` lands.
pub(super) const WINDOWS: [(&str, &[&str]); 2] = [
    ("dialogs", dialogs::WINDOWS_GAPS),
    ("menus", menus::WINDOWS_GAPS),
];

/// The recorded gaps of the platform the gate runs on.
pub(super) fn recorded() -> Vec<(&'static str, &'static [&'static str])> {
    let windows: &[(&str, &[&str])] = if cfg!(windows) { &WINDOWS } else { &[] };
    AREAS.iter().chain(windows).copied().collect()
}

/// Keyboard walk gaps, see `keyboard.rs`.
pub(super) const KEYBOARD: &[&str] = keyboard::GAPS;
