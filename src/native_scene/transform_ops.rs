//! Modify > Transform: Reset Transformations and Freeze Transformations (#313). Reset writes the
//! default translate (0), rotate (0) and scale (1) of the chosen channels. Freeze follows
//! `makeIdentity -apply true`: the chosen channels of the node and of every transform below it
//! are reset and their accumulated matrix is pushed into the mesh points beneath, so every point
//! keeps its world position. The mesh stores no normals (they derive from the points and the
//! edge smoothing), so a frozen mesh needs no separate normal work. A partial freeze leaves the
//! unchosen channels exactly as stored (like Maya) and bakes the difference into the points, which
//! can include shear when scale stays non-uniform. PROVISIONAL: the Reset
//! option defaults, the shear and rotate-order cases (#105) and the unsupported `-normal`,
//! `-preserveNormals` and `-jointOrient` flags are not compared with Maya (#5).
use super::reparent::{decompose, rotation_zyx};
use super::{
    MAX_TRANSFORM_DEPTH, MAX_TRANSFORM_PATHS, NativeError, NativeId, NativeScene, ReparentRefusal,
    TransformExtras, Units,
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
    /// The accumulated matrix collapses the mesh (zero or near-singular scale) or is not finite.
    Degenerate,
    /// Match Transformations needs at least one object to move and a target.
    TooFewObjects,
    /// The target lies below an object that would move, so moving it would move the target.
    TargetInsideMoved,
    /// The wanted world transform needs shear, a mirror or a channel change the stored TRS
    /// channels cannot hold exactly (for example rotation below a non-uniformly scaled parent).
    NotRepresentable,
    /// Align Objects needs at least two objects.
    AlignTooFew,
    /// Align Objects: a selected object has no mesh below it, so it has no bounding box.
    NoGeometry,
    /// Align Objects: one selected object lies below another, so moving it twice is ambiguous.
    Nested,
}

impl fmt::Display for TransformRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptySet => "no objects or no transform channels were given",
            Self::Keyed => "an animated (keyed) object cannot be reset, frozen or matched",
            Self::Joint => "joints cannot be reset, frozen or matched here (unsupported)",
            Self::NotPlain => {
                "freezing or matching needs default pivots and transform extras on the objects involved (and, for a freeze, on everything below them)"
            }
            Self::Instanced => "instanced objects cannot be frozen or matched",
            Self::Curve => "a curve below the object cannot be frozen; only meshes are baked",
            Self::Reflection => {
                "a negative scale mirrors the mesh; Preserve Normals is not implemented, so the freeze is refused"
            }
            Self::Degenerate => "the accumulated transform is zero-scaled or not finite",
            Self::TooFewObjects => "select at least two objects; the last selected is the target",
            Self::TargetInsideMoved => {
                "the target (last selected) is below a selected object that would move"
            }
            Self::AlignTooFew => "select at least two objects to align",
            Self::NoGeometry => "an aligned object needs a mesh below it to have a bounding box",
            Self::Nested => "an aligned object lies below another selected object",
            Self::NotRepresentable => {
                "the target transform cannot be matched exactly with the chosen channels (shear, mirroring or a non-uniformly scaled parent)"
            }
        })
    }
}

impl From<TransformRefusal> for NativeError {
    fn from(reason: TransformRefusal) -> Self {
        Self::InvalidTransformEdit(reason)
    }
}

/// Shortest transformed axis treated as a surviving mesh (a uniform scale of 1e-4 passes).
const MIN_AXIS_LENGTH: f64 = 1e-9;
/// Smallest determinant relative to the product of the axis lengths: below it the axes are
/// nearly coplanar and the mesh collapses, whatever its overall size.
const MIN_VOLUME_RATIO: f64 = 1e-9;

/// Whether `matrix` is finite and keeps a mesh three-dimensional, judged against its own scale.
pub(super) fn keeps_volume(matrix: DMat4) -> bool {
    let lengths =
        [matrix.x_axis, matrix.y_axis, matrix.z_axis].map(|axis| axis.truncate().length());
    let determinant = matrix.determinant();
    matrix.is_finite()
        && determinant.is_finite()
        && lengths.iter().all(|l| *l > MIN_AXIS_LENGTH)
        && determinant.abs() > MIN_VOLUME_RATIO * lengths.iter().product::<f64>()
}

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
            .top_most_with_parents(ids)
            .map_err(|error| match error {
                NativeError::InvalidReparent(ReparentRefusal::Instanced) => {
                    NativeError::from(TransformRefusal::Instanced)
                }
                other => other,
            })?
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
    pub(super) fn refuse_plain_target(&self, index: usize, bake: bool) -> Result<(), NativeError> {
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
        let mut internal = self.internal_transform(node_id)?;
        let local = internal.matrix().ok_or(NativeError::NonFinite)?;
        let effective = carry * local;
        // Only the chosen channels are reset; the others keep their stored bits. The baked matrix
        // is whatever turns the kept local matrix back into the effective one, so every world
        // position holds for any channel subset (it may carry shear when scale stays non-uniform).
        let mut values = [None; 9];
        for (on, base, default) in [
            (channels.translate, 0, 0.0),
            (channels.rotate, 3, 0.0),
            (channels.scale, 6, 1.0),
        ] {
            if on {
                values[base..base + 3].fill(Some(default));
                let field = match base {
                    0 => &mut internal.translation_cm,
                    3 => &mut internal.rotation_radians,
                    _ => &mut internal.scale,
                };
                *field = [default; 3];
            }
        }
        let kept = internal.matrix().ok_or(NativeError::NonFinite)?;
        let baked = kept.inverse() * effective;
        if !keeps_volume(baked) {
            return Err(TransformRefusal::Degenerate.into());
        }
        if baked.determinant() < 0.0 {
            return Err(TransformRefusal::Reflection.into());
        }
        self.nodes[index].channels = self.replacement_channels(index, values)?;
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

impl NativeScene {
    /// Modify > Match Transformations: every object of `ids` except the last (the target) gets
    /// the target's world translate, rotate and scale, limited to the chosen channels; unchosen
    /// channels keep their stored bits. Children are not compensated. Objects are processed
    /// shallowest first so a moved descendant is matched against its moved parent. All or
    /// nothing: the work runs on a copy. PROVISIONAL: world space and pivot-free objects only,
    /// the Match Pivots and bounding-box options are unbuilt, not compared with Maya (#5).
    pub fn match_transformations(
        &mut self,
        ids: &[NodeId],
        channels: TransformChannels,
    ) -> Result<(), NativeError> {
        let mut candidate = self.clone();
        candidate.match_in_place(ids, channels)?;
        candidate.validate()?;
        candidate.transform_paths()?;
        *self = candidate;
        Ok(())
    }

    fn match_in_place(
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
        let Some((&target, moved)) = ids.split_last().filter(|(_, moved)| !moved.is_empty()) else {
            return Err(TransformRefusal::TooFewObjects.into());
        };
        let target_index = self.editable_node(target)?;
        self.refuse_plain_target(target_index, true)?;
        let target_path = self.sole_target(target)?.path;
        let mut order = Vec::with_capacity(moved.len());
        for id in moved {
            let index = self.editable_node(*id)?;
            self.refuse_plain_target(index, true)?;
            let path = self.sole_target(*id)?.path;
            if target_path.contains(&self.nodes[index].transform_id) {
                return Err(TransformRefusal::TargetInsideMoved.into());
            }
            order.push((path.len(), *id));
        }
        order.sort_by_key(|(depth, _)| *depth);
        let goal = self.path_world(&target_path)?;
        for (_, id) in order {
            self.match_one(id, goal, channels)?;
        }
        Ok(())
    }

    /// Writes only the chosen channels of `id` so its world transform takes those components
    /// of `goal`, and refuses unless the rewritten node then reproduces the wanted world.
    fn match_one(
        &mut self,
        id: NodeId,
        goal: DMat4,
        channels: TransformChannels,
    ) -> Result<(), NativeError> {
        let index = self.editable_node(id)?;
        let path = self.sole_target(id)?.path;
        let parent = self.path_world(&path[..path.len() - 1])?;
        if !keeps_volume(parent) {
            return Err(TransformRefusal::Degenerate.into());
        }
        let desired = if channels == TransformChannels::ALL {
            goal
        } else {
            let now =
                decompose(self.path_world(&path)?).ok_or(TransformRefusal::NotRepresentable)?;
            let want = decompose(goal).ok_or(TransformRefusal::NotRepresentable)?;
            let pick = |on: bool, base: usize| if on { want } else { now }[base..base + 3].to_vec();
            let (t, r, s) = (
                pick(channels.translate, 0),
                pick(channels.rotate, 3),
                pick(channels.scale, 6),
            );
            DMat4::from_translation(DVec3::new(t[0], t[1], t[2]))
                * rotation_zyx([r[0].to_radians(), r[1].to_radians(), r[2].to_radians()])
                * DMat4::from_scale(DVec3::new(s[0], s[1], s[2]))
        };
        let wanted = parent.inverse() * desired;
        let local = if channels.rotate || channels.scale {
            decompose(wanted).ok_or(TransformRefusal::NotRepresentable)?
        } else if wanted.is_finite() {
            let t = wanted.w_axis;
            [t.x, t.y, t.z, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
        } else {
            return Err(NativeError::NonFinite);
        };
        let mut values = [None; 9];
        for (on, base) in [
            (channels.translate, 0),
            (channels.rotate, 3),
            (channels.scale, 6),
        ] {
            if on {
                for slot in base..base + 3 {
                    values[slot] = Some(local[slot]);
                }
            }
        }
        self.nodes[index].channels = self.replacement_channels(index, values)?;
        let result = self.path_world(&path)?;
        let size = [desired.x_axis, desired.y_axis, desired.z_axis]
            .iter()
            .fold(1.0_f64, |m, c| m.max(c.truncate().length()));
        let reach = desired.w_axis.truncate().abs().max_element().max(1.0);
        let linear_ok = [
            (result.x_axis, desired.x_axis),
            (result.y_axis, desired.y_axis),
            (result.z_axis, desired.z_axis),
        ]
        .iter()
        .all(|(a, b)| (a.truncate() - b.truncate()).abs().max_element() <= EXACT_TOLERANCE * size);
        let origin_ok = (result.w_axis - desired.w_axis)
            .truncate()
            .abs()
            .max_element()
            <= EXACT_TOLERANCE * reach.max(size);
        if linear_ok && origin_ok {
            Ok(())
        } else {
            Err(TransformRefusal::NotRepresentable.into())
        }
    }
}

/// Relative tolerance for accepting a rewritten node as the world transform it was meant to take.
const EXACT_TOLERANCE: f64 = 1e-9;

#[cfg(test)]
mod tests;
