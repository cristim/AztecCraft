use super::{Command, EditOutcome, History};
use crate::native_scene::{
    ChannelBaseline, NativeError, NativeId, NativeScene, RotateAxis, RotateFrame, RotateSpace,
    ScaleFrame, ScaleHandle, ScaleSpace, TransformTarget, WorldAxis, WorldMoveFrame,
};
use crate::view_scene::{ViewError, ViewScene};
use std::{fmt, mem::size_of};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManipulationId(u64);

impl ManipulationId {
    pub fn to_decimal(self) -> String {
        self.0.to_string()
    }

    pub fn from_decimal(value: &str) -> Result<Self, std::num::ParseIntError> {
        value.parse().map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationProvenance {
    Native,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationFinish {
    Commit,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManipulationError {
    Native(NativeError),
    View(ViewError),
    Busy,
    StaleScene,
    StaleManipulation,
    WrongOwner,
    IdsExhausted,
}

impl From<NativeError> for ManipulationError {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}

impl From<ViewError> for ManipulationError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}

impl fmt::Display for ManipulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(error) => error.fmt(f),
            Self::View(error) => error.fmt(f),
            Self::Busy => f.write_str("finish or cancel the active manipulation"),
            Self::StaleScene => f.write_str("manipulation scene token is stale"),
            Self::StaleManipulation => f.write_str("manipulation identifier is absent or stale"),
            Self::WrongOwner => f.write_str("manipulation belongs to another input source"),
            Self::IdsExhausted => f.write_str("manipulation identifiers are exhausted"),
        }
    }
}

impl std::error::Error for ManipulationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationKindState {
    Move {
        axis: WorldAxis,
    },
    Rotate {
        axis: RotateAxis,
        space: RotateSpace,
    },
    Scale {
        handle: ScaleHandle,
        space: ScaleSpace,
    },
}

/// Read-only view of the active manipulation; `parameter` is in the kind's internal unit
/// (centimeters for Move, degrees for Rotate, a unitless factor for Scale).
pub struct ManipulationState<'a> {
    pub id: ManipulationId,
    pub target: &'a TransformTarget,
    pub provenance: ManipulationProvenance,
    pub parameter: f64,
    pub kind: ManipulationKindState,
    /// The three channels this manipulation edits, as stored at begin.
    pub baseline_channels: [f64; 3],
    /// The same three channels now (the live preview); `None` if the node cannot be read.
    pub current_channels: Option<[f64; 3]>,
}

impl ManipulationState<'_> {
    pub fn move_axis(&self) -> Option<WorldAxis> {
        match self.kind {
            ManipulationKindState::Move { axis } => Some(axis),
            ManipulationKindState::Rotate { .. } | ManipulationKindState::Scale { .. } => None,
        }
    }

    pub fn move_distance_cm(&self) -> Option<f64> {
        self.move_axis().map(|_| self.parameter)
    }
}

/// The kind of manipulation and its kind-specific state; the guards in `History` are shared.
enum ManipulationKind {
    Move {
        axis: WorldAxis,
        frame: WorldMoveFrame,
        baseline: ChannelBaseline,
    },
    Rotate {
        axis: RotateAxis,
        frame: RotateFrame,
        baseline: ChannelBaseline,
    },
    Scale {
        handle: ScaleHandle,
        frame: ScaleFrame,
        baseline: ChannelBaseline,
    },
}

impl ManipulationKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Move { .. } => "Move",
            Self::Rotate { .. } => "Rotate",
            Self::Scale { .. } => "Scale",
        }
    }

    fn initial_parameter(&self) -> f64 {
        match self {
            Self::Move { .. } | Self::Rotate { .. } => 0.0,
            Self::Scale { .. } => 1.0,
        }
    }

    fn state(&self) -> ManipulationKindState {
        match self {
            Self::Move { axis, .. } => ManipulationKindState::Move { axis: *axis },
            Self::Rotate { axis, frame, .. } => ManipulationKindState::Rotate {
                axis: *axis,
                space: frame.space(),
            },
            Self::Scale { handle, frame, .. } => ManipulationKindState::Scale {
                handle: *handle,
                space: frame.space(),
            },
        }
    }

    fn baseline(&self) -> &ChannelBaseline {
        match self {
            Self::Move { baseline, .. }
            | Self::Rotate { baseline, .. }
            | Self::Scale { baseline, .. } => baseline,
        }
    }

    fn preview(
        &self,
        scene: &mut NativeScene,
        parameter: f64,
        declared: Option<f64>,
    ) -> Result<(ChannelBaseline, bool), NativeError> {
        let values = match self {
            Self::Move { axis, frame, .. } => frame.translation(*axis, parameter)?,
            Self::Rotate { axis, frame, .. } => {
                frame.rotation_declared(*axis, parameter, declared)?
            }
            Self::Scale { handle, frame, .. } => frame.scale(*handle, parameter)?,
        };
        scene.preview_channels(self.baseline(), values)
    }

    fn restore(&self, scene: &mut NativeScene) -> Result<(), NativeError> {
        scene.restore_channels(self.baseline())
    }

    fn changes(&self, scene: &NativeScene) -> Result<[Option<f64>; 9], NativeError> {
        scene.channel_changes(self.baseline())
    }
}

pub(super) struct MoveTransaction {
    id: ManipulationId,
    scene_token: Uuid,
    target: TransformTarget,
    provenance: ManipulationProvenance,
    kind: ManipulationKind,
    view_budget: usize,
    parameter: f64,
}

impl MoveTransaction {
    fn matches(
        &self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
    ) -> Result<(), ManipulationError> {
        if scene_token != self.scene_token {
            return Err(ManipulationError::StaleScene);
        }
        if id != self.id {
            return Err(ManipulationError::StaleManipulation);
        }
        if provenance != self.provenance {
            return Err(ManipulationError::WrongOwner);
        }
        Ok(())
    }
}

impl History {
    pub fn active_manipulation(&self) -> Option<ManipulationState<'_>> {
        self.active_manipulation
            .as_ref()
            .map(|active| ManipulationState {
                id: active.id,
                target: &active.target,
                provenance: active.provenance,
                parameter: active.parameter,
                kind: active.kind.state(),
                baseline_channels: active.kind.baseline().values(),
                current_channels: self.scene.node(active.target.node).ok().map(|node| {
                    let first = active.kind.baseline().first_channel();
                    let values = node.transform().values();
                    [values[first], values[first + 1], values[first + 2]]
                }),
            })
    }

    pub fn begin_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_manipulation(target, scene_token, provenance, byte_budget, |scene| {
            let (frame, baseline) = scene.begin_move_translation(target, axis)?;
            Ok(ManipulationKind::Move {
                axis,
                frame,
                baseline,
            })
        })
    }

    /// Begins a rotation in object or world space; `update_move` then takes the angle in degrees.
    pub fn begin_rotate(
        &mut self,
        target: &TransformTarget,
        axis: RotateAxis,
        space: RotateSpace,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_manipulation(target, scene_token, provenance, byte_budget, |scene| {
            let (frame, baseline) = scene.begin_rotation(target, axis, space)?;
            Ok(ManipulationKind::Rotate {
                axis,
                frame,
                baseline,
            })
        })
    }

    /// Begins a scale in object or world space; `update_move` then takes the unitless factor.
    pub fn begin_scale(
        &mut self,
        target: &TransformTarget,
        handle: ScaleHandle,
        space: ScaleSpace,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_manipulation(target, scene_token, provenance, byte_budget, |scene| {
            let (frame, baseline) = scene.begin_scale_frame(target, handle, space)?;
            Ok(ManipulationKind::Scale {
                handle,
                frame,
                baseline,
            })
        })
    }

    fn begin_manipulation(
        &mut self,
        target: &TransformTarget,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
        make_kind: impl FnOnce(&NativeScene) -> Result<ManipulationKind, NativeError>,
    ) -> Result<ManipulationId, ManipulationError> {
        if self.active_manipulation.is_some() {
            return Err(ManipulationError::Busy);
        }
        let next_id = self
            .next_manipulation_id
            .checked_add(1)
            .ok_or(ManipulationError::IdsExhausted)?;
        let kind = make_kind(&self.scene)?;
        let mut path = Vec::new();
        path.try_reserve_exact(target.path.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        path.extend_from_slice(&target.path);
        let metadata = path
            .capacity()
            .checked_mul(size_of::<NativeId>())
            .and_then(|bytes| bytes.checked_add(size_of::<MoveTransaction>()))
            .ok_or(ViewError::ResourceLimit)?;
        let view_budget = byte_budget
            .checked_sub(metadata)
            .ok_or(ViewError::ResourceLimit)?;
        ViewScene::from_native(&self.scene, view_budget)?;
        self.active_manipulation = Some(MoveTransaction {
            id: ManipulationId(next_id),
            scene_token,
            target: TransformTarget {
                node: target.node,
                path,
            },
            provenance,
            parameter: kind.initial_parameter(),
            kind,
            view_budget,
        });
        self.next_manipulation_id = next_id;
        Ok(ManipulationId(next_id))
    }

    pub fn update_move(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        distance_cm: f64,
    ) -> Result<EditOutcome, ManipulationError> {
        self.update_active(scene_token, id, provenance, distance_cm, None)
    }

    /// Like `update_move`, for a Rotate whose angle was requested in the scene's angular unit.
    /// `degrees` drives the preview parameter; `declared` is the exact requested value that a
    /// single-channel result is stored from (see `RotateFrame::rotation`).
    pub fn update_move_declared(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        degrees: f64,
        declared: Option<f64>,
    ) -> Result<EditOutcome, ManipulationError> {
        self.update_active(scene_token, id, provenance, degrees, declared)
    }

    fn update_active(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        distance_cm: f64,
        declared: Option<f64>,
    ) -> Result<EditOutcome, ManipulationError> {
        let active = self
            .active_manipulation
            .as_mut()
            .ok_or(ManipulationError::StaleManipulation)?;
        active.matches(scene_token, id, provenance)?;
        let (previous, changed) = active.kind.preview(
            Self::touch(&mut self.scene, &mut self.revision),
            distance_cm,
            declared,
        )?;
        if let Err(error) = ViewScene::from_native(&self.scene, active.view_budget) {
            Self::touch(&mut self.scene, &mut self.revision).restore_channels(&previous)?;
            return Err(error.into());
        }
        active.parameter = distance_cm;
        Ok(if changed {
            EditOutcome::Changed
        } else {
            EditOutcome::Unchanged
        })
    }

    pub fn finish_move(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        finish: ManipulationFinish,
    ) -> Result<EditOutcome, ManipulationError> {
        self.active_manipulation
            .as_ref()
            .ok_or(ManipulationError::StaleManipulation)?
            .matches(scene_token, id, provenance)?;
        // Clear before any fallible scene work so a failure cannot leave an unfinishable move.
        let active = self
            .active_manipulation
            .take()
            .ok_or(ManipulationError::StaleManipulation)?;
        let channels = active.kind.changes(&self.scene)?;
        let node = active.target.node;
        active.kind.restore(self.scene_mut())?;
        if finish == ManipulationFinish::Cancel || channels.iter().all(Option::is_none) {
            return Ok(if channels.iter().all(Option::is_none) {
                EditOutcome::Unchanged
            } else {
                EditOutcome::Changed
            });
        }
        self.execute_labeled(
            Command::EditNode {
                id: node,
                name: None,
                channels,
            },
            active.kind.label().to_string(),
        )
        .map_err(Into::into)
    }

    pub fn cancel_active_move(&mut self) -> Result<EditOutcome, ManipulationError> {
        // Clear before any fallible scene work so a failure cannot leave an unfinishable move.
        let Some(active) = self.active_manipulation.take() else {
            return Ok(EditOutcome::Unchanged);
        };
        let channels = active.kind.changes(&self.scene)?;
        active.kind.restore(self.scene_mut())?;
        Ok(if channels.iter().all(Option::is_none) {
            EditOutcome::Unchanged
        } else {
            EditOutcome::Changed
        })
    }
}

#[cfg(test)]
mod tests;
