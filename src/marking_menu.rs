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

pub use interaction::{Button, FrameInput, Interaction, NavKey, Outcome};

pub use model::{Entry, EntryState, MarkingMenu, Resolved, Target, Zone, zone_at};

/// Pointer distance, in points, inside which no zone is chosen.
pub const DEAD_ZONE: f32 = 12.0;
/// Seconds a held button waits before the dial is drawn.
pub const DRAW_DELAY: f64 = 0.25;
/// A press-drag-release released before the dial is drawn (`DRAW_DELAY`) and at least this many
/// points from the press origin is a flick: it chooses without the dial being drawn. PROVISIONAL.
pub const FLICK_DISTANCE: f32 = 24.0;
/// Seconds without human input after which a held or sticky menu dismisses itself.
pub const IDLE_DISMISS: f64 = 30.0;

#[cfg(test)]
mod guard_tests;
#[cfg(test)]
mod interaction_tests;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod tests;
