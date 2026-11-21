//! The film back of Maya's four built-in cameras, stored in the scene so Undo, New and Open own it
//! (issue #458). Only a scene without a retained source can be edited, and the authored writers
//! refuse a scene that holds an override, until `.ma` and `.mb` persistence proves it keeps the
//! value (later #458 slices). Absent means "no statement": the viewport derives `FilmBack::DEFAULT`.
use super::{NativeError, NativeScene};
use crate::viewport::FilmBack;

/// A built-in camera shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneCamera {
    Persp,
    Top,
    Front,
    Side,
}

impl SceneCamera {
    fn index(self) -> usize {
        match self {
            Self::Persp => 0,
            Self::Top => 1,
            Self::Front => 2,
            Self::Side => 3,
        }
    }
}

/// The per-camera overrides. Equality is by bits so an edit that only flips a `-0.0` is a change.
#[derive(Clone, Debug, Default)]
pub(super) struct CameraStore([Option<FilmBack>; 4]);

impl PartialEq for CameraStore {
    fn eq(&self, other: &Self) -> bool {
        self.0.iter().zip(&other.0).all(|pair| match pair {
            (Some(a), Some(b)) => a.same_bits(b),
            (None, None) => true,
            _ => false,
        })
    }
}

impl CameraStore {
    pub(super) fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }
}

impl NativeScene {
    /// The camera's film back: its override, else the Create Camera defaults.
    pub fn camera_film_back(&self, camera: SceneCamera) -> FilmBack {
        self.cameras.0[camera.index()].unwrap_or(FilmBack::DEFAULT)
    }

    /// Stores a whole film back. An invalid value or a scene opened from a file is refused with the
    /// scene untouched. Setting the default bits again removes the override (PROVISIONAL: a file
    /// that states the default values is indistinguishable here).
    pub fn set_camera_film_back(
        &mut self,
        camera: SceneCamera,
        film: FilmBack,
    ) -> Result<(), NativeError> {
        film.validate()
            .map_err(|invalid| NativeError::InvalidFilmBack(invalid.0))?;
        if self.source.is_some() {
            return Err(NativeError::SourcedCameraEdit);
        }
        self.cameras.0[camera.index()] = (!film.same_bits(&FilmBack::DEFAULT)).then_some(film);
        Ok(())
    }

    /// Whether any camera holds an override the authored writers cannot save yet.
    pub(super) fn has_camera_overrides(&self) -> bool {
        !self.cameras.is_empty()
    }
}

#[cfg(test)]
mod tests;
