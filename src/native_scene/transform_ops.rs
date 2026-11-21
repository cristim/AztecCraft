//! Modify > Transform: Reset Transformations and Freeze Transformations (#313). Reset writes the
//! default translate (0), rotate (0) and scale (1) of the chosen channels. Freeze follows
//! `makeIdentity -apply true`: the chosen channels of the node and of every transform below it
//! are reset and their accumulated matrix is pushed into the mesh points beneath, so every point
//! keeps its world position. The mesh stores no normals (they derive from the points and the
//! edge smoothing), so a frozen mesh needs no separate normal work. PROVISIONAL: the Reset
//! option defaults, the shear and rotate-order cases (#105) and the unsupported `-normal`,
//! `-preserveNormals` and `-jointOrient` flags are not compared with Maya (#5).
use super::reparent::{decompose, rotation_zyx};
use super::{
    MAX_TRANSFORM_DEPTH, MAX_TRANSFORM_PATHS, NativeError, NativeId, NativeScene, TransformExtras,
    Units,
};
use crate::scene::{NodeId, SceneError};
use glam::{DMat4, DVec3};
use std::collections::HashSet;
use std::fmt;

/// Which of translate, rotate and scale a reset or freeze covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransformChannels {
    pub translate: bool,
    pub rotate: bool,
    pub scale: bool,
}

impl TransformChannels {
    pub const ALL: Self = Self {
        translate: true,
        rotate: true,
        scale: true,
    };

    /// `makeIdentity`: with none of the three given, all three apply.
    pub fn or_all(self) -> Self {
        if self.translate || self.rotate || self.scale {
            self
        } else {
            Self::ALL
        }
    }

    fn is_none(self) -> bool {
        !(self.translate || self.rotate || self.scale)
    }

    fn is_all(self) -> bool {
        self == Self::ALL
    }
}

/// Why a reset or freeze was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformRefusal {
    /// No object was given, or no channel was chosen.
    EmptySet,
    /// A keyed channel is rewritten on every frame change.
    Keyed,
    /// Joints are not covered (their orient and bone-length rules differ).
    Joint,
    /// Pivots, pivot translates, shear or other extras would have to be baked too.
    NotPlain,
    /// An instanced object has no single parent or world.
    Instanced,
    /// A curve below the node: only meshes are baked.
    Curve,
    /// The accumulated matrix mirrors the mesh; Preserve Normals is not implemented.
    Reflection,
    /// The accumulated matrix collapses the mesh or is not finite.
    Degenerate,
    /// A partial freeze needs the local matrix as translate, XYZ rotate and scale.
    NotRepresentable,
}

impl fmt::Display for TransformRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptySet => "no objects or no transform channels were given",
            Self::Keyed => "an animated (keyed) object cannot be reset or frozen",
            Self::Joint => "joints cannot be reset or frozen here (unsupported)",
            Self::NotPlain => {
                "freezing needs default pivots and transform extras on the object and everything below it"
            }
            Self::Instanced => "instanced objects cannot be frozen",
            Self::Curve => "a curve below the object cannot be frozen; only meshes are baked",
            Self::Reflection => {
                "a negative scale mirrors the mesh; Preserve Normals is not implemented, so the freeze is refused"
            }
            Self::Degenerate => "the accumulated transform is zero-scaled or not finite",
            Self::NotRepresentable => {
                "a partial freeze needs translate, XYZ rotate and positive scale (no shear); freeze all three channels"
            }
        })
    }
}

impl From<TransformRefusal> for NativeError {
    fn from(reason: TransformRefusal) -> Self {
        Self::InvalidTransformEdit(reason)
    }
}

/// Smallest determinant treated as a surviving mesh.
const MIN_DETERMINANT: f64 = 1e-12;

impl NativeScene {
    /// Writes the default of every chosen channel on each of `ids`; children move with their
    /// parent, nothing is compensated. All or nothing.
    pub fn reset_transformations(
        &mut self,
        ids: &[NodeId],
        channels: TransformChannels,
    ) -> Result<(), NativeError> {
        if ids.is_empty() || channels.is_none() {
            return Err(TransformRefusal::EmptySet.into());
        }
        let mut seen = HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        self.authoring_preflight()?;
        self.validate()?;
        let mut edits = Vec::with_capacity(ids.len());
        for id in ids {
            let index = self.editable_node(*id)?;
            self.refuse_plain_target(index, false)?;
            let mut values = [None; 9];
            for (on, base, default) in [
                (channels.translate, 0, 0.0),
                (channels.rotate, 3, 0.0),
                (channels.scale, 6, 1.0),
            ] {
                if on {
                    values[base..base + 3].fill(Some(default));
                }
            }
            edits.push((*id, values));
        }
        let mut candidate = self.clone();
        candidate.edit_nodes(&edits)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// Freezes the chosen channels of the top-most of `ids` and of every transform below them
    /// into the meshes beneath. All or nothing; a sourced scene is refused like Group.
    pub fn freeze_transformations(
        &mut self,
        ids: &[NodeId],
        channels: TransformChannels,
    ) -> Result<(), NativeError> {
        let mut candidate = self.clone();
        candidate.freeze_in_place(ids, channels)?;
        candidate.validate()?;
        candidate.transform_paths()?;
        *self = candidate;
        Ok(())
    }

    fn freeze_in_place(
        &mut self,
        ids: &[NodeId],
        channels: TransformChannels,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        if self.units != Units::default() {
            return Err(NativeError::UnsupportedEdit);
        }
        if ids.is_empty() || channels.is_none() {
            return Err(TransformRefusal::EmptySet.into());
        }
        let mut seen = HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        let roots: Vec<usize> = self
            .top_most_with_parents(ids)?
            .into_iter()
            .map(|(index, _)| index)
            .collect();
        let mut visited = 0usize;
        for index in roots {
            self.freeze_node(index, DMat4::IDENTITY, channels, 0, &mut visited)?;
        }
        Ok(())
    }

    /// Refuses what a reset or freeze of node `index` cannot handle. `bake` also demands default
    /// pivots and extras, which only a freeze has to carry into the mesh.
    fn refuse_plain_target(&self, index: usize, bake: bool) -> Result<(), NativeError> {
        let node = &self.nodes[index];
        if self.joint(node.transform_id).is_some() {
            return Err(TransformRefusal::Joint.into());
        }
        if self.key_rows(Some(node.id)).next().is_some() {
            return Err(TransformRefusal::Keyed.into());
        }
        let plain = node
            .rotate_pivot
            .iter()
            .chain(&node.scale_pivot)
            .all(|v| v.value.to_bits() == 0)
            && node.extras == TransformExtras::default();
        if bake && !plain {
            return Err(TransformRefusal::NotPlain.into());
        }
        Ok(())
    }

    /// Freezes node `index`, whose parent frame already absorbed `carry`, then everything below.
    fn freeze_node(
        &mut self,
        index: usize,
        carry: DMat4,
        channels: TransformChannels,
        depth: usize,
        visited: &mut usize,
    ) -> Result<(), NativeError> {
        *visited += 1;
        if *visited > MAX_TRANSFORM_PATHS || depth >= MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        let node_id = self.nodes[index].id;
        self.editable_node(node_id)?;
        self.refuse_plain_target(index, true)?;
        let transform_id = self.nodes[index].transform_id;
        if self
            .dag
            .iter()
            .filter(|edge| edge.child == transform_id)
            .count()
            != 1
        {
            return Err(TransformRefusal::Instanced.into());
        }
        let local = self
            .internal_transform(node_id)?
            .matrix()
            .ok_or(NativeError::NonFinite)?;
        let effective = carry * local;
        let (kept, values) = if channels.is_all() {
            (
                DMat4::IDENTITY,
                [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            )
        } else {
            let mut values = decompose(effective).ok_or(TransformRefusal::NotRepresentable)?;
            for (on, base, default) in [
                (channels.translate, 0, 0.0),
                (channels.rotate, 3, 0.0),
                (channels.scale, 6, 1.0),
            ] {
                if on {
                    values[base..base + 3].fill(default);
                }
            }
            let matrix = DMat4::from_translation(DVec3::new(values[0], values[1], values[2]))
                * rotation_zyx([
                    values[3].to_radians(),
                    values[4].to_radians(),
                    values[5].to_radians(),
                ])
                * DMat4::from_scale(DVec3::new(values[6], values[7], values[8]));
            (matrix, values)
        };
        let baked = kept.inverse() * effective;
        let determinant = baked.determinant();
        if !baked.is_finite() || !determinant.is_finite() || determinant.abs() <= MIN_DETERMINANT {
            return Err(TransformRefusal::Degenerate.into());
        }
        if determinant < 0.0 {
            return Err(TransformRefusal::Reflection.into());
        }
        self.nodes[index].channels = self.replacement_channels(index, values.map(Some))?;
        let children: Vec<NativeId> = self
            .dag
            .iter()
            .filter(|edge| edge.parent == Some(transform_id))
            .map(|edge| edge.child)
            .collect();
        for child in children {
            if let Some(next) = self.nodes.iter().position(|n| n.transform_id == child) {
                self.freeze_node(next, baked, channels, depth + 1, visited)?;
            } else if let Some(shape) = self.shapes.iter().position(|s| s.id == child) {
                if self.dag.iter().filter(|edge| edge.child == child).count() != 1 {
                    return Err(TransformRefusal::Instanced.into());
                }
                self.shapes[shape].mesh.transform_points(baked)?;
            } else if self.curves.iter().any(|c| c.id == child) {
                return Err(TransformRefusal::Curve.into());
            } else {
                return Err(NativeError::UnsupportedEdit);
            }
        }
        Ok(())
    }
}
