//! Recorded violations, one file per area. The gate fails both when a violation is new and when a
//! recorded one is gone, so a fix deletes its lines from the file of its area.

mod shell;

pub(super) const AREAS: [(&str, &[&str]); 1] = [("shell", shell::GAPS)];
