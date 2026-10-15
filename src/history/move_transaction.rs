use super::{Command, EditOutcome, History};
use crate::native_scene::{
    MoveTranslationBaseline, NativeError, NativeId, TransformTarget, WorldAxis, WorldMoveFrame,
};
use crate::view_scene::{ViewError, ViewScene};
use std::{fmt, mem::size_of};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoveId(u64);

impl MoveId {
    pub fn to_decimal(self) -> String {
        self.0.to_string()
    }

    pub fn from_decimal(value: &str) -> Result<Self, std::num::ParseIntError> {
        value.parse().map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveProvenance {
    Native,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveFinish {
    Commit,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveError {
    Native(NativeError),
    View(ViewError),
    Busy,
    StaleScene,
    StaleMove,
    WrongOwner,
    IdsExhausted,
}

impl From<NativeError> for MoveError {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}

impl From<ViewError> for MoveError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(error) => error.fmt(f),
            Self::View(error) => error.fmt(f),
            Self::Busy => f.write_str("finish or cancel the active Move manipulation"),
            Self::StaleScene => f.write_str("Move scene token is stale"),
            Self::StaleMove => f.write_str("Move identifier is absent or stale"),
            Self::WrongOwner => f.write_str("Move manipulation belongs to another input source"),
            Self::IdsExhausted => f.write_str("Move identifiers are exhausted"),
        }
    }
}

impl std::error::Error for MoveError {}

pub struct MoveState<'a> {
    pub id: MoveId,
    pub target: &'a TransformTarget,
    pub axis: WorldAxis,
    pub provenance: MoveProvenance,
    pub distance_cm: f64,
}

pub(super) struct MoveTransaction {
    id: MoveId,
    scene_token: Uuid,
    target: TransformTarget,
    axis: WorldAxis,
    provenance: MoveProvenance,
    frame: WorldMoveFrame,
    baseline: MoveTranslationBaseline,
    view_budget: usize,
    distance_cm: f64,
}

impl MoveTransaction {
    fn matches(
        &self,
        scene_token: Uuid,
        id: MoveId,
        provenance: MoveProvenance,
    ) -> Result<(), MoveError> {
        if scene_token != self.scene_token {
            return Err(MoveError::StaleScene);
        }
        if id != self.id {
            return Err(MoveError::StaleMove);
        }
        if provenance != self.provenance {
            return Err(MoveError::WrongOwner);
        }
        Ok(())
    }
}

impl History {
    pub fn active_move(&self) -> Option<MoveState<'_>> {
        self.active_move.as_ref().map(|active| MoveState {
            id: active.id,
            target: &active.target,
            axis: active.axis,
            provenance: active.provenance,
            distance_cm: active.distance_cm,
        })
    }

    pub fn begin_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        scene_token: Uuid,
        provenance: MoveProvenance,
        byte_budget: usize,
    ) -> Result<MoveId, MoveError> {
        if self.active_move.is_some() {
            return Err(MoveError::Busy);
        }
        if self
            .move_scene_token
            .is_some_and(|token| token != scene_token)
        {
            return Err(MoveError::StaleScene);
        }
        let next_id = self
            .next_move_id
            .checked_add(1)
            .ok_or(MoveError::IdsExhausted)?;
        let (frame, baseline) = self.scene.begin_move_translation(target)?;
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
        self.active_move = Some(MoveTransaction {
            id: MoveId(next_id),
            scene_token,
            target: TransformTarget {
                node: target.node,
                path,
            },
            axis,
            provenance,
            frame,
            baseline,
            view_budget,
            distance_cm: 0.0,
        });
        self.next_move_id = next_id;
        self.move_scene_token = Some(scene_token);
        Ok(MoveId(next_id))
    }

    pub fn update_move(
        &mut self,
        scene_token: Uuid,
        id: MoveId,
        provenance: MoveProvenance,
        distance_cm: f64,
    ) -> Result<EditOutcome, MoveError> {
        let active = self.active_move.as_mut().ok_or(MoveError::StaleMove)?;
        active.matches(scene_token, id, provenance)?;
        let translation = active.frame.translation(active.axis, distance_cm)?;
        let (previous, changed) = self
            .scene
            .preview_move_translation(&active.baseline, translation)?;
        if let Err(error) = ViewScene::from_native(&self.scene, active.view_budget) {
            self.scene.restore_move_translation(&previous)?;
            return Err(error.into());
        }
        active.distance_cm = distance_cm;
        Ok(if changed {
            EditOutcome::Changed
        } else {
            EditOutcome::Unchanged
        })
    }

    pub fn finish_move(
        &mut self,
        scene_token: Uuid,
        id: MoveId,
        provenance: MoveProvenance,
        finish: MoveFinish,
    ) -> Result<EditOutcome, MoveError> {
        let active = self.active_move.as_ref().ok_or(MoveError::StaleMove)?;
        active.matches(scene_token, id, provenance)?;
        let channels = self.scene.move_translation_changes(&active.baseline)?;
        let node = active.target.node;
        self.scene.restore_move_translation(&active.baseline)?;
        self.active_move = None;
        if finish == MoveFinish::Cancel || channels.iter().all(Option::is_none) {
            return Ok(if channels.iter().all(Option::is_none) {
                EditOutcome::Unchanged
            } else {
                EditOutcome::Changed
            });
        }
        self.execute(Command::EditNode {
            id: node,
            name: None,
            channels,
        })
        .map_err(Into::into)
    }

    pub fn cancel_active_move(&mut self) -> Result<EditOutcome, MoveError> {
        let Some(active) = &self.active_move else {
            return Ok(EditOutcome::Unchanged);
        };
        let channels = self.scene.move_translation_changes(&active.baseline)?;
        self.scene.restore_move_translation(&active.baseline)?;
        self.active_move = None;
        Ok(if channels.iter().all(Option::is_none) {
            EditOutcome::Unchanged
        } else {
            EditOutcome::Changed
        })
    }
}

#[cfg(test)]
mod tests;
