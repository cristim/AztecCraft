//! In-session camera bookmarks (Maya View > Bookmarks). A bookmark is a named camera pose; the
//! store is session-only and never part of the scene.

use crate::viewport::{Camera, InvalidCameraPose};
use std::fmt;

pub const MAX_BOOKMARKS: usize = 64;
pub const MAX_NAME_CHARS: usize = 64;

/// The pose a bookmark restores: the camera's own f32 values, so apply is bit-exact. The lens and
/// clip planes are not part of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BookmarkPose {
    pub target: [f32; 3],
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
}

impl BookmarkPose {
    pub fn of(camera: &Camera) -> Self {
        Self {
            target: camera.target().to_array(),
            distance: camera.distance(),
            yaw: camera.yaw(),
            pitch: camera.pitch(),
        }
    }

    pub fn apply(self, camera: &mut Camera) -> Result<(), InvalidCameraPose> {
        camera.set_pose(self.target.into(), self.distance, self.yaw, self.pitch)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CameraBookmark {
    pub name: String,
    pub pose: BookmarkPose,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    TooLong,
    ControlCharacter,
    SurroundingWhitespace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BookmarkError {
    InvalidName(NameProblem),
    Duplicate,
    NotFound,
    LimitReached,
}

impl fmt::Display for BookmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(NameProblem::Empty) => f.write_str("Bookmark name is empty"),
            Self::InvalidName(NameProblem::TooLong) => {
                write!(f, "Bookmark name exceeds {MAX_NAME_CHARS} characters")
            }
            Self::InvalidName(NameProblem::ControlCharacter) => {
                f.write_str("Bookmark name contains a control character")
            }
            Self::InvalidName(NameProblem::SurroundingWhitespace) => {
                f.write_str("Bookmark name has leading or trailing whitespace")
            }
            Self::Duplicate => f.write_str("A bookmark with that name already exists"),
            Self::NotFound => f.write_str("No bookmark has that name"),
            Self::LimitReached => write!(f, "At most {MAX_BOOKMARKS} bookmarks are allowed"),
        }
    }
}

impl std::error::Error for BookmarkError {}

/// Unicode Cf characters that hide or reorder text (bidi controls, zero-width, BOM, soft hyphen)
/// plus the line and paragraph separators; std has no Cf predicate.
fn is_invisible_format(c: char) -> bool {
    matches!(c,
        '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}' | '\u{e0000}'..='\u{e007f}')
}

pub fn validate_name(name: &str) -> Result<(), BookmarkError> {
    let problem = if name.is_empty() {
        NameProblem::Empty
    } else if name
        .chars()
        .any(|c| c.is_control() || is_invisible_format(c))
    {
        NameProblem::ControlCharacter
    } else if name.trim() != name {
        NameProblem::SurroundingWhitespace
    } else if name.chars().count() > MAX_NAME_CHARS {
        NameProblem::TooLong
    } else {
        return Ok(());
    };
    Err(BookmarkError::InvalidName(problem))
}

/// Ordered by creation; names are unique and case-sensitive.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraBookmarks {
    items: Vec<CameraBookmark>,
}

impl CameraBookmarks {
    pub fn list(&self) -> &[CameraBookmark] {
        &self.items
    }

    pub fn get(&self, name: &str) -> Result<&CameraBookmark, BookmarkError> {
        self.items
            .iter()
            .find(|b| b.name == name)
            .ok_or(BookmarkError::NotFound)
    }

    pub fn create(&mut self, name: &str, pose: BookmarkPose) -> Result<(), BookmarkError> {
        validate_name(name)?;
        if self.items.iter().any(|b| b.name == name) {
            return Err(BookmarkError::Duplicate);
        }
        if self.items.len() >= MAX_BOOKMARKS {
            return Err(BookmarkError::LimitReached);
        }
        self.items.push(CameraBookmark {
            name: name.to_owned(),
            pose,
        });
        Ok(())
    }

    /// Returns whether anything changed; renaming to the same name is a no-op.
    pub fn rename(&mut self, name: &str, new_name: &str) -> Result<bool, BookmarkError> {
        let index = self.position(name)?;
        validate_name(new_name)?;
        if name == new_name {
            return Ok(false);
        }
        if self.items.iter().any(|b| b.name == new_name) {
            return Err(BookmarkError::Duplicate);
        }
        self.items[index].name = new_name.to_owned();
        Ok(true)
    }

    pub fn delete(&mut self, name: &str) -> Result<(), BookmarkError> {
        let index = self.position(name)?;
        self.items.remove(index);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    fn position(&self, name: &str) -> Result<usize, BookmarkError> {
        self.items
            .iter()
            .position(|b| b.name == name)
            .ok_or(BookmarkError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(distance: f32) -> BookmarkPose {
        BookmarkPose {
            target: [1.0, 2.0, 3.0],
            distance,
            yaw: 0.5,
            pitch: 0.25,
        }
    }

    #[test]
    fn names_are_validated() {
        let long = "a".repeat(MAX_NAME_CHARS + 1);
        let cases = [
            ("", NameProblem::Empty),
            ("a\nb", NameProblem::ControlCharacter),
            ("a\u{7f}", NameProblem::ControlCharacter),
            ("a\u{202e}b", NameProblem::ControlCharacter),
            ("a\u{200b}b", NameProblem::ControlCharacter),
            ("a\u{2028}b", NameProblem::ControlCharacter),
            ("a\u{e0041}b", NameProblem::ControlCharacter),
            ("a\u{61c}b", NameProblem::ControlCharacter),
            (" a", NameProblem::SurroundingWhitespace),
            ("a\u{a0}", NameProblem::SurroundingWhitespace),
            (long.as_str(), NameProblem::TooLong),
        ];
        for (name, problem) in cases {
            assert_eq!(
                validate_name(name),
                Err(BookmarkError::InvalidName(problem)),
                "{name:?}"
            );
        }
        assert_eq!(validate_name(&"a".repeat(MAX_NAME_CHARS)), Ok(()));
        assert_eq!(validate_name(&"\u{e9}".repeat(MAX_NAME_CHARS)), Ok(()));
        assert_eq!(validate_name("front view 2"), Ok(()));
    }

    #[test]
    fn create_keeps_order_and_refuses_duplicates_case_sensitively() {
        let mut store = CameraBookmarks::default();
        store.create("b", pose(1.0)).unwrap();
        store.create("a", pose(2.0)).unwrap();
        store.create("B", pose(3.0)).unwrap();
        let names: Vec<_> = store.list().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["b", "a", "B"]);
        assert_eq!(store.create("a", pose(9.0)), Err(BookmarkError::Duplicate));
        assert_eq!(store.get("a").unwrap().pose, pose(2.0));
    }

    #[test]
    fn create_is_bounded() {
        let mut store = CameraBookmarks::default();
        for i in 0..MAX_BOOKMARKS {
            store.create(&format!("v{i}"), pose(1.0)).unwrap();
        }
        assert_eq!(
            store.create("extra", pose(1.0)),
            Err(BookmarkError::LimitReached)
        );
        store.delete("v0").unwrap();
        store.create("extra", pose(1.0)).unwrap();
    }

    #[test]
    fn rename_validates_before_mutating_and_keeps_position() {
        let mut store = CameraBookmarks::default();
        store.create("a", pose(1.0)).unwrap();
        store.create("b", pose(2.0)).unwrap();
        assert_eq!(store.rename("a", "b"), Err(BookmarkError::Duplicate));
        assert_eq!(
            store.rename("a", ""),
            Err(BookmarkError::InvalidName(NameProblem::Empty))
        );
        assert_eq!(store.rename("zz", "c"), Err(BookmarkError::NotFound));
        assert_eq!(store.rename("a", "a"), Ok(false));
        assert_eq!(store.rename("a", "c"), Ok(true));
        let names: Vec<_> = store.list().iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["c", "b"]);
        assert_eq!(store.get("c").unwrap().pose, pose(1.0));
        assert_eq!(store.get("a"), Err(BookmarkError::NotFound));
    }

    #[test]
    fn delete_and_clear() {
        let mut store = CameraBookmarks::default();
        store.create("a", pose(1.0)).unwrap();
        store.create("b", pose(2.0)).unwrap();
        assert_eq!(store.delete("zz"), Err(BookmarkError::NotFound));
        store.delete("a").unwrap();
        assert_eq!(store.list().len(), 1);
        store.clear();
        assert!(store.list().is_empty());
    }

    #[test]
    fn pose_round_trips_bit_exactly_through_the_camera() {
        let mut camera = Camera::default();
        camera
            .set_pose([0.1, -0.0, 3.3e5].into(), 7.123_456, 6.1, -1.2)
            .unwrap();
        let captured = BookmarkPose::of(&camera);
        let mut other = Camera::default();
        other
            .set_pose([9.0, 9.0, 9.0].into(), 1.0, 0.0, 0.3)
            .unwrap();
        assert!(!camera.same_capture(&other));
        captured.apply(&mut other).unwrap();
        // Same lens and clip planes (both default), so the whole capture must match.
        assert!(camera.same_capture(&other));
    }

    #[test]
    fn apply_leaves_lens_and_clip_planes_alone() {
        let mut camera = Camera::default();
        let near = camera.near();
        let far = camera.far();
        BookmarkPose::of(&Camera::default())
            .apply(&mut camera)
            .unwrap();
        assert_eq!((camera.near(), camera.far()), (near, far));
    }
}
