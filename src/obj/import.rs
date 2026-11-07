//! Streaming Wavefront OBJ reader: pure data in, pure data out, no scene types.
//!
//! Every limit below is PROVISIONAL (Maya help is silent) and listed in the "Application policy
//! constants" table in `.project-docs/development.md`. Errors carry a keyword class and a
//! 1-based line number only, never file text.

mod error;
pub use error::{Limit, ObjImportError, SyntaxKind, UnsupportedKind};

pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const MAX_VERTICES: usize = 262_144;
pub const MAX_FACES: usize = 262_144;
pub const MAX_CORNERS: usize = 1_048_576;
pub const MAX_CORNERS_PER_FACE: usize = 1024;
pub const MAX_OBJECTS: usize = 1024;
pub const MAX_NAME_BYTES: usize = 256;
/// Largest accepted coordinate magnitude, chosen so f32 rounding and world math stay finite.
pub const MAX_COORD: f64 = 1e12;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedObj {
    pub vertices: Vec<[f32; 3]>,
    /// Objects that own at least one face, in file order. An empty name means the file gave none.
    pub objects: Vec<ParsedObject>,
    pub ignored_records: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedObject {
    pub name: String,
    /// Global 0-based vertex indices, each face with distinct corners.
    pub faces: Vec<Vec<u32>>,
}

#[cfg(test)]
mod tests;
