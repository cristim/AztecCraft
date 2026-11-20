//! Window-free marking menu model and interaction state machine (#32).
//!
//! The menu holds no scene state and names no editor type: the action type `A` is chosen by the
//! caller, and time and pointer positions are passed in, so every timing path is testable
//! without a window. The egui wiring and the mesh menu layout live in the application crate.
//!
//! Every constant below is PROVISIONAL: the Maya 2027 help describes the gestures but gives no
//! radii or timings. Tests pin the values so a change is visible; they are not Maya evidence.

mod interaction;
mod model;

pub use interaction::{Button, FrameInput, Interaction, Outcome};

pub use model::{Action, Availability, Entry, MarkingMenu, Resolved, Target, Zone, zone_at};

/// Pointer distance, in points, inside which no zone is chosen.
pub const DEAD_ZONE: f32 = 12.0;
/// Seconds a held button waits before the dial is drawn.
pub const DRAW_DELAY: f64 = 0.25;
/// A press-drag-release shorter than this, in seconds, and at least `FLICK_DISTANCE` long is a
/// flick: it chooses without the dial being drawn.
pub const FLICK_TIME: f64 = 0.25;
/// Points a flick must travel from the press origin.
pub const FLICK_DISTANCE: f32 = 24.0;
/// Seconds without human input after which a held or sticky menu dismisses itself.
pub const IDLE_DISMISS: f64 = 30.0;

#[cfg(test)]
mod tests;
