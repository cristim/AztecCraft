//! Recorded violations, one file per area. The gate fails both when a violation is new and when a
//! recorded one is gone, so a fix deletes its lines from the file of its area.

mod dialogs;
mod fields;
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
