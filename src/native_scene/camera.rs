//! The film back of Maya's four built-in cameras, stored in the scene so Undo, New and Open own it
//! (issue #458). Only a scene without a retained source can be edited. The authored `.ma` writer
//! saves an override; the `.mb` writer refuses one until its slice proves it keeps the value.
//! Absent means "no statement": the viewport derives `FilmBack::DEFAULT`.
use super::{NativeError, NativeScene};
use crate::viewport::{FilmBack, FilmFit, FilmRollOrder};

/// A built-in camera shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneCamera {
    Persp,
    Top,
    Front,
    Side,
}

impl SceneCamera {
    pub(super) const ALL: [Self; 4] = [Self::Persp, Self::Top, Self::Front, Self::Side];

    pub(super) fn shape_name(self) -> &'static str {
        match self {
            Self::Persp => "perspShape",
            Self::Top => "topShape",
            Self::Front => "frontShape",
            Self::Side => "sideShape",
        }
    }

    pub(super) fn from_shape_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|camera| camera.shape_name() == name)
    }

    pub(super) fn index(self) -> usize {
        match self {
            Self::Persp => 0,
            Self::Top => 1,
            Self::Front => 2,
            Self::Side => 3,
        }
    }
}

/// The camera node attributes a film back is written as, in the order the writer emits them
/// (`fl` focalLength, `hfa`/`vfa` film apertures, `ff` filmFit, `ffo` filmFitOffset, `hfo`/`vfo`
/// film offsets, `lsr` lensSqueezeRatio, `ovr` overscan, `cs` cameraScale, `psc`/`ptsc` pre and
/// post scale, `frv` filmRollValue in degrees, `dfg` displayFilmGate, `fth`/`ftv` film translate,
/// `hrp`/`vrp` horizontalRollPivot and verticalRollPivot, `fro` filmRollOrder). The short names and
/// the `ff` ids are from the Maya camera node page (checked by the reviewer, 2026 release); the
/// `fro` ids are not on the page.
pub(super) const FILM_ATTRIBUTES: [&str; 19] = [
    "fl", "hfa", "vfa", "ff", "ffo", "hfo", "vfo", "lsr", "ovr", "cs", "psc", "ptsc", "frv", "dfg",
    "fth", "ftv", "hrp", "vrp", "fro",
];

/// The Maya `filmFit` enum ids (0 fill, 1 horizontal, 2 vertical, 3 overscan; node page) and the
/// `filmRollOrder` ids (0 rotate-translate, 1 translate-rotate; PROVISIONAL, the pages list none).
pub(super) fn fit_id(fit: FilmFit) -> i32 {
    match fit {
        FilmFit::Fill => 0,
        FilmFit::Horizontal => 1,
        FilmFit::Vertical => 2,
        FilmFit::Overscan => 3,
    }
}

pub(super) fn fit_from_id(id: i32) -> Option<FilmFit> {
    [
        FilmFit::Fill,
        FilmFit::Horizontal,
        FilmFit::Vertical,
        FilmFit::Overscan,
    ]
    .into_iter()
    .find(|fit| fit_id(*fit) == id)
}

pub(super) fn order_id(order: FilmRollOrder) -> i32 {
    match order {
        FilmRollOrder::RotateTranslate => 0,
        FilmRollOrder::TranslateRotate => 1,
    }
}

pub(super) fn order_from_id(id: i32) -> Option<FilmRollOrder> {
    [
        FilmRollOrder::RotateTranslate,
        FilmRollOrder::TranslateRotate,
    ]
    .into_iter()
    .find(|order| order_id(*order) == id)
}

/// The per-camera overrides and the cameras a file stated in a way the editor cannot carry.
/// Equality is by bits so an edit that only flips a `-0.0` is a change.
#[derive(Clone, Debug, Default)]
pub(super) struct CameraStore {
    films: [Option<FilmBack>; 4],
    blocked: [bool; 4],
}

impl PartialEq for CameraStore {
    fn eq(&self, other: &Self) -> bool {
        self.blocked == other.blocked
            && self.films.iter().zip(&other.films).all(|pair| match pair {
                (Some(a), Some(b)) => a.same_bits(b),
                (None, None) => true,
                _ => false,
            })
    }
}

impl CameraStore {
    pub(super) fn is_empty(&self) -> bool {
        self.films.iter().all(Option::is_none) && !self.blocked.contains(&true)
    }
}

impl NativeScene {
    /// The camera's film back: its override, else the Create Camera defaults.
    pub fn camera_film_back(&self, camera: SceneCamera) -> FilmBack {
        self.cameras.films[camera.index()].unwrap_or(FilmBack::DEFAULT)
    }

    /// False for a camera a file stated with a value the editor cannot carry (an unknown
    /// `filmRollOrder` id): the viewport keeps its default lens and the camera cannot be edited.
    pub fn camera_projection_available(&self, camera: SceneCamera) -> bool {
        !self.cameras.blocked[camera.index()]
    }

    /// Stores a whole film back. An invalid value, a blocked camera or a scene opened from a file
    /// is refused with the scene untouched. Setting the default bits again removes the override
    /// (an explicit default stated by a file is kept until then: see `state_camera_film_back`).
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
        if self.cameras.blocked[camera.index()] {
            return Err(NativeError::BlockedCamera);
        }
        self.cameras.films[camera.index()] = (!film.same_bits(&FilmBack::DEFAULT)).then_some(film);
        Ok(())
    }

    /// A file's complete statement of a camera, kept even when it equals the defaults so that
    /// saving the file again does not drop it.
    pub(super) fn state_camera_film_back(&mut self, camera: SceneCamera, film: FilmBack) {
        self.cameras.films[camera.index()] = Some(film);
    }

    /// A camera a file stated with a value the editor cannot carry.
    pub(super) fn block_camera(&mut self, camera: SceneCamera) {
        self.cameras.blocked[camera.index()] = true;
    }

    pub(super) fn camera_override(&self, camera: SceneCamera) -> Option<FilmBack> {
        self.cameras.films[camera.index()]
    }

    /// Whether any camera holds an override or a blocked statement.
    pub(super) fn has_camera_overrides(&self) -> bool {
        !self.cameras.is_empty()
    }

    pub(super) fn has_blocked_camera(&self) -> bool {
        self.cameras.blocked.contains(&true)
    }
}

#[cfg(test)]
mod tests;
