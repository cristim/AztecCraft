//! Window-free marking menu model and interaction state machine (#32).
//!
//! The menu holds no scene state and names no editor type: the action type `A` is chosen by the
//! caller, and time and pointer positions are passed in, so every timing path is testable
//! without a window. The egui wiring and the mesh menu layout live in the application crate.
//!
//! Every constant below is PROVISIONAL: the Maya 2027 help describes the gestures but gives no
//! radii or timings. Tests pin the values so a change is visible; they are not Maya evidence.

mod model;

pub use model::{Action, Availability, Entry, MarkingMenu, Resolved, Target, Zone, zone_at};

/// Pointer distance, in points, inside which no zone is chosen.
pub const DEAD_ZONE: f32 = 12.0;
#[cfg(test)]
mod tests;
