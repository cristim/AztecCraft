use super::*;
use glam::DMat3;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};

const MAX_PARENT_CONDITION: f64 = 1e12;
pub(super) const ROUNDTRIP_EPSILON: f64 = 128.0 * f64::EPSILON;
// A move is refused when the stored translation cannot resolve it to about one part in 2^20.
pub(super) const MIN_RESOLUTION: f64 = 1.0 / 1_048_576.0;

// Upper bound of the spacing between f64 values near `value`.
pub(super) fn ulp_bound(value: f64) -> f64 {
    value.abs().max(f64::MIN_POSITIVE) * f64::EPSILON
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldAxis {
    X,
    Y,
    Z,
}

impl WorldAxis {
    pub fn direction(self) -> DVec3 {
        match self {
            Self::X => DVec3::X,
            Self::Y => DVec3::Y,
            Self::Z => DVec3::Z,
        }
    }
}

/// The Move tool's Axis Orientation: the axes a Move handle is aligned to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveSpace {
    /// Maya's default.
    #[default]
    World,
    /// The node's own rotation (rotate order, no Rotate Axis) inside its parents' map.
    Object,
    /// The parents' map alone; the world for a root node.
    Parent,
    /// The node's rotation R(rotate order) without the Rotate Axis, inside the parents' map: Maya's
    /// Along Rotation Axis, which differs from Object only when the Rotate Axis is non-zero (Maya
    /// 2027 Move Tool page, GUID-9BCE8B23-6DA6-4C7F-B897-7E25D6E8A865). PROVISIONAL derivation
    /// pending the #5 reference check: the node matrix is RA * R in row form, so Object (R * RA
    /// here) includes RA and this RA-free frame is the only one that can differ. Under a
    /// non-uniform or negative parent scale the normalised axes can be skewed or left-handed (as
    /// for Object; PROVISIONAL).
    AlongRotationAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelLeaf {
    Translate,
    Rotate,
    Scale,
}

impl ChannelLeaf {
    pub fn name(self, component: usize) -> String {
        let axis = ["X", "Y", "Z"].get(component).copied().unwrap_or("?");
        match self {
            Self::Translate => format!("translate{axis}"),
            Self::Rotate => format!("rotate{axis}"),
            Self::Scale => format!("scale{axis}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldMoveUnsupported {
    TranslationSource,
    ParentTransform,
    NumericalDisplacement,
    /// One object selected through several instances: its instances share channels.
    SharedInstances,
    /// A selected ancestor also carries another instance of this uncovered selected object, whose
    /// translation would be applied twice there.
    AlternatePathAncestor,
}

impl fmt::Display for WorldMoveUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TranslationSource => "translation requires an existing supported source leaf",
            Self::ParentTransform => "parent transform is singular or numerically unstable",
            Self::NumericalDisplacement => "displacement cannot be represented accurately",
            Self::SharedInstances => {
                "the same object is selected through several instances and shares one translation"
            }
            Self::AlternatePathAncestor => {
                "a selected ancestor also carries another instance of this object, so it would move twice"
            }
        })
    }
}

/// Why `world_move_frames` refused: the scene as a whole, or one named target (by index).
#[derive(Debug)]
pub(crate) enum MoveBeginError {
    Scene(NativeError),
    Member { index: usize, source: NativeError },
}

#[derive(Clone, Copy)]
pub struct WorldMoveFrame {
    origin_cm: DVec3,
    parent_linear: DMat3,
    inverse_parent: DMat3,
    parent_norm: f64,
    /// The node's own rotation R(rotate order), without Rotate Axis, shear or scale.
    object_rotation: DMat3,
    translation: [f64; 3],
    translation_cm: DVec3,
    linear_factor: f64,
    absent_leaf: [bool; 3],
}

/// The changed channels of each node an edit touches, as `Command::EditNodes` takes them.
pub(crate) type ChannelEdits = Vec<(NodeId, [Option<f64>; 9])>;

// Exact scalars of one transform field (translate, rotate or scale) captured at begin or preview.
pub(crate) struct ChannelBaseline {
    field: TransformField,
    node: NodeId,
    owner: NativeId,
    scalars: [Scalar; 3],
    source: Option<Arc<[u8]>>,
}

impl ChannelBaseline {
    /// The three stored channel values at begin, in the scene's declared units.
    pub(crate) fn values(&self) -> [f64; 3] {
        self.scalars.each_ref().map(|scalar| scalar.value)
    }

    pub(crate) fn node(&self) -> NodeId {
        self.node
    }

    /// Index of the first of the three channels in `NativeTransform::values`.
    pub(crate) fn first_channel(&self) -> usize {
        field_start(self.field)
    }
}

impl fmt::Debug for WorldMoveFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorldMoveFrame").finish_non_exhaustive()
    }
}

impl WorldMoveFrame {
    pub fn origin_cm(&self) -> DVec3 {
        self.origin_cm
    }

    // Maya binary files omit default (zero) translate leaves. An absent leaf can be edited only
    // where the file recorded an insertion site (`absent_leaf` is false there); elsewhere the
    // edit could never be saved.
    pub fn require_leaves(&self, axis: WorldAxis) -> Result<(), NativeError> {
        self.require_leaves_in(MoveSpace::World, axis)
    }

    /// The unit world direction of `axis` in `space`. Parent and Object axes are normalized, so a
    /// parent's scale never changes the distance moved.
    pub fn direction(&self, space: MoveSpace, axis: WorldAxis) -> Result<DVec3, NativeError> {
        let unit = axis.direction();
        let along = match space {
            MoveSpace::World => return Ok(unit),
            MoveSpace::Parent => self.parent_linear * unit,
            MoveSpace::Object => self.parent_linear * (self.object_rotation * unit),
            MoveSpace::AlongRotationAxis => self.parent_linear * (self.object_rotation * unit),
        };
        let length = along.length();
        let direction = along / length;
        if !length.is_finite() || !direction.is_finite() {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform,
            ));
        }
        Ok(direction)
    }

    pub fn require_leaves_in(&self, space: MoveSpace, axis: WorldAxis) -> Result<(), NativeError> {
        let local = self.inverse_parent * self.direction(space, axis)?;
        for (component, delta) in local.to_array().into_iter().enumerate() {
            if delta != 0.0 && self.absent_leaf[component] {
                return Err(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Translate,
                    component,
                });
            }
        }
        Ok(())
    }

    pub fn translation(&self, axis: WorldAxis, distance_cm: f64) -> Result<[f64; 3], NativeError> {
        self.translation_in(MoveSpace::World, axis, distance_cm)
    }

    pub fn translation_in(
        &self,
        space: MoveSpace,
        axis: WorldAxis,
        distance_cm: f64,
    ) -> Result<[f64; 3], NativeError> {
        if !distance_cm.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if distance_cm == 0.0 {
            return Ok(self.translation);
        }
        let displacement = self.direction(space, axis)? * distance_cm;
        let local_delta = self.inverse_parent * displacement;
        let bound = self.parent_norm * local_delta.abs().max_element();
        let tolerance = ROUNDTRIP_EPSILON * bound.max(distance_cm.abs()).max(1.0);
        let forward = self.parent_linear * local_delta;
        if !local_delta.is_finite()
            || !bound.is_finite()
            || !forward.is_finite()
            || (forward - displacement).abs().max_element() > tolerance
        {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::NumericalDisplacement,
            ));
        }
        let mut translation = self.translation;
        for (component, delta) in local_delta.to_array().into_iter().enumerate() {
            if delta != 0.0 {
                translation[component] += delta / self.linear_factor;
            }
        }
        let mut internal = self.translation_cm.to_array();
        for component in 0..3 {
            if translation[component].to_bits() != self.translation[component].to_bits() {
                internal[component] = translation[component] * self.linear_factor;
            }
        }
        let represented_delta = DVec3::from_array(internal) - self.translation_cm;
        let represented_world = self.parent_linear * represented_delta;
        // Rounding of the stored channel, of its centimeter product and of the baseline product
        // limits how exactly any move can be stored; it grows with |translation|.
        let mut storage_precision: f64 = 0.0;
        for component in 0..3 {
            if translation[component].to_bits() != self.translation[component].to_bits() {
                storage_precision = storage_precision.max(
                    ulp_bound(translation[component]) * self.linear_factor
                        + ulp_bound(internal[component])
                        + ulp_bound(self.translation_cm[component]),
                );
            }
        }
        let world_precision = self.parent_norm * storage_precision;
        if !represented_world.is_finite()
            || !world_precision.is_finite()
            || world_precision > MIN_RESOLUTION * distance_cm.abs()
            || (represented_world - displacement).abs().max_element() > tolerance + world_precision
        {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::NumericalDisplacement,
            ));
        }
        Ok(translation)
    }
}

impl NativeScene {
    #[cfg(test)]
    pub(crate) fn begin_move_translation(
        &self,
        target: &TransformTarget,
        axis: WorldAxis,
    ) -> Result<(WorldMoveFrame, ChannelBaseline), NativeError> {
        self.begin_move_translation_in(target, MoveSpace::World, axis)
    }

    pub(crate) fn begin_move_translation_in(
        &self,
        target: &TransformTarget,
        space: MoveSpace,
        axis: WorldAxis,
    ) -> Result<(WorldMoveFrame, ChannelBaseline), NativeError> {
        let frame = self.world_move_frame(target)?;
        frame.require_leaves_in(space, axis)?;
        let node = self.node(target.node)?;
        Ok((
            frame,
            self.channel_baseline(node, TransformField::Translation),
        ))
    }

    pub(super) fn channel_baseline(
        &self,
        node: &NativeTransformNode,
        field: TransformField,
    ) -> ChannelBaseline {
        let start = field_start(field);
        ChannelBaseline {
            field,
            node: node.id,
            owner: node.transform_id,
            scalars: std::array::from_fn(|component| node.channels[start + component].clone()),
            source: self.source.as_ref().map(|source| source.bytes.clone()),
        }
    }

    /// Source and origin checks for the node at `index`, which the caller found by identity.
    fn check_channel_owner(
        &self,
        index: usize,
        baseline: &ChannelBaseline,
    ) -> Result<(), NativeError> {
        let same_source = match (&baseline.source, &self.source) {
            (None, None) => true,
            (Some(expected), Some(actual)) => Arc::ptr_eq(expected, &actual.bytes),
            _ => false,
        };
        if !same_source {
            return Err(NativeError::InvalidRetention);
        }
        let start = field_start(baseline.field);
        for (component, original) in baseline.scalars.iter().enumerate() {
            let current = &self.nodes[index].channels[start + component];
            if current.origin != original.origin {
                return Err(NativeError::InvalidRetention);
            }
            original.validate_origin(
                self.source.as_ref(),
                &self.units,
                baseline.owner,
                baseline.field,
                component,
            )?;
            current.validate_origin(
                self.source.as_ref(),
                &self.units,
                baseline.owner,
                baseline.field,
                component,
            )?;
        }
        Ok(())
    }

    pub(crate) fn preview_channels(
        &mut self,
        baseline: &ChannelBaseline,
        values: [f64; 3],
    ) -> Result<(ChannelBaseline, bool), NativeError> {
        let (mut previous, changed) =
            self.preview_channels_many(std::iter::once((baseline, values)))?;
        let previous = previous.pop().ok_or(NativeError::InvalidIdentity)?;
        Ok((previous, changed))
    }

    pub(crate) fn restore_channels(
        &mut self,
        baseline: &ChannelBaseline,
    ) -> Result<(), NativeError> {
        self.restore_channels_many(std::iter::once(baseline))
    }

    pub(crate) fn channel_changes(
        &self,
        baseline: &ChannelBaseline,
    ) -> Result<[Option<f64>; 9], NativeError> {
        let mut changes = self.channel_changes_many(std::iter::once(baseline))?;
        Ok(changes.pop().map_or([None; 9], |(_, channels)| channels))
    }

    fn node_indices(&self) -> HashMap<NodeId, usize> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect()
    }

    fn channel_owner_in(
        &self,
        indices: &HashMap<NodeId, usize>,
        baseline: &ChannelBaseline,
    ) -> Result<usize, NativeError> {
        let index = *indices
            .get(&baseline.node)
            .ok_or(NativeError::InvalidIdentity)?;
        if self.nodes[index].transform_id != baseline.owner {
            return Err(NativeError::InvalidIdentity);
        }
        self.check_channel_owner(index, baseline)?;
        Ok(index)
    }

    /// `preview_channels` for several baselines, all or nothing: every replacement is computed
    /// before the first write. Returns each member's previous baseline and whether any changed.
    pub(crate) fn preview_channels_many<'a>(
        &mut self,
        items: impl Iterator<Item = (&'a ChannelBaseline, [f64; 3])>,
    ) -> Result<(Vec<ChannelBaseline>, bool), NativeError> {
        let indices = self.node_indices();
        let mut writes = Vec::new();
        let mut previous = Vec::new();
        let mut changed = false;
        for (baseline, values) in items {
            if values.iter().any(|value| !value.is_finite()) {
                return Err(NativeError::NonFinite);
            }
            let index = self.channel_owner_in(&indices, baseline)?;
            let start = field_start(baseline.field);
            let mut replacement = baseline.scalars.clone();
            for (component, scalar) in replacement.iter_mut().enumerate() {
                if values[component].to_bits() != scalar.value.to_bits() {
                    *scalar = scalar.replacement(
                        values[component],
                        self.source.as_ref(),
                        &self.units,
                        baseline.owner,
                        baseline.field,
                        component,
                    )?;
                }
            }
            changed |= self.nodes[index].channels[start..start + 3] != replacement;
            previous.push(ChannelBaseline {
                field: baseline.field,
                node: baseline.node,
                owner: baseline.owner,
                scalars: std::array::from_fn(|component| {
                    self.nodes[index].channels[start + component].clone()
                }),
                source: baseline.source.clone(),
            });
            writes.push((index, start, replacement));
        }
        for (index, start, replacement) in writes {
            self.nodes[index].channels[start..start + 3].clone_from_slice(&replacement);
        }
        Ok((previous, changed))
    }

    /// `restore_channels` for several baselines; every owner is checked before the first write.
    pub(crate) fn restore_channels_many<'a>(
        &mut self,
        baselines: impl Iterator<Item = &'a ChannelBaseline> + Clone,
    ) -> Result<(), NativeError> {
        let indices = self.node_indices();
        for baseline in baselines.clone() {
            self.channel_owner_in(&indices, baseline)?;
        }
        for baseline in baselines {
            let index = indices[&baseline.node];
            let start = field_start(baseline.field);
            self.nodes[index].channels[start..start + 3].clone_from_slice(&baseline.scalars);
        }
        Ok(())
    }

    /// `channel_changes` for several baselines: the nodes whose channels differ, with the changed
    /// values. Members that did not change are omitted.
    pub(crate) fn channel_changes_many<'a>(
        &self,
        baselines: impl Iterator<Item = &'a ChannelBaseline>,
    ) -> Result<ChannelEdits, NativeError> {
        let indices = self.node_indices();
        let mut changes = Vec::new();
        for baseline in baselines {
            let index = self.channel_owner_in(&indices, baseline)?;
            let start = field_start(baseline.field);
            let mut channels = [None; 9];
            for (component, original) in baseline.scalars.iter().enumerate() {
                let current = &self.nodes[index].channels[start + component];
                if current != original {
                    channels[start + component] = Some(current.value);
                }
            }
            if channels.iter().any(Option::is_some) {
                changes.push((baseline.node, channels));
            }
        }
        Ok(changes)
    }

    /// World frame in which the selected node's local matrix applies: the ancestors on
    /// `target.path` composed with their offsetParentMatrix and inheritsTransform, then the
    /// selected node's own prefix (`it`, `opm`). `None` when one has no finite matrix.
    pub(super) fn ancestor_world(
        &self,
        target: &TransformTarget,
    ) -> Result<Option<DMat4>, NativeError> {
        let mut world = DMat4::IDENTITY;
        let ancestors = target.path.len().saturating_sub(1);
        for id in target.path.iter().take(ancestors) {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == *id)
                .ok_or(NativeError::InvalidInstancePath)?;
            let Some(composed) = self.internal_transform(node.id)?.compose_world(world) else {
                return Ok(None);
            };
            world = composed;
        }
        let Some(id) = target.path.last() else {
            return Ok(Some(world));
        };
        let selected = self
            .nodes
            .iter()
            .find(|node| node.transform_id == *id)
            .ok_or(NativeError::InvalidInstancePath)?;
        let prefix = self.internal_transform(selected.id)?.frame_prefix(world);
        Ok(prefix.is_finite().then_some(prefix))
    }

    pub fn world_move_frame(
        &self,
        target: &TransformTarget,
    ) -> Result<WorldMoveFrame, NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        let selected = self.resolve_transform_path(target.node, &target.path)?;
        self.editable(target.node)?;
        self.world_move_frame_of(target, selected)
    }

    /// Scene-level refusals that do not depend on a member: read-only format and an invalid
    /// scene. Run once for a whole set of movers.
    fn move_scene_check(&self) -> Result<(), NativeError> {
        self.authoring_preflight()?;
        self.validate()
    }

    /// The per-member part of `world_move_frame`, after the scene-level checks.
    fn member_move_frame(&self, target: &TransformTarget) -> Result<WorldMoveFrame, NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        let selected = self.resolve_transform_path(target.node, &target.path)?;
        self.editable_node(target.node)?;
        self.world_move_frame_of(target, selected)
    }

    /// Resolves every target (depth and instance path), covered ones included, so a bogus
    /// target is refused even when another selected target would cover it.
    pub(crate) fn resolve_move_targets(
        &self,
        targets: &[TransformTarget],
    ) -> Result<(), MoveBeginError> {
        for (index, target) in targets.iter().enumerate() {
            let member = |source| MoveBeginError::Member { index, source };
            if target.path.len() > MAX_TRANSFORM_DEPTH {
                return Err(member(NativeError::ResourceLimit));
            }
            self.resolve_transform_path(target.node, &target.path)
                .map_err(member)?;
        }
        Ok(())
    }

    /// The translation baseline of `node` alone, for a selected lead that gets no mover.
    pub(crate) fn translation_baseline(
        &self,
        node: NodeId,
    ) -> Result<ChannelBaseline, NativeError> {
        Ok(self.channel_baseline(self.node(node)?, TransformField::Translation))
    }

    /// Frames and translation baselines for every target, after one scene-level check (a single
    /// validation for the whole set). Fails whole, naming the offending target's index.
    #[cfg(test)]
    pub(crate) fn world_move_frames(
        &self,
        targets: &[&TransformTarget],
        axis: Option<WorldAxis>,
    ) -> Result<Vec<(WorldMoveFrame, ChannelBaseline)>, MoveBeginError> {
        self.world_move_frames_in(targets, MoveSpace::World, axis)
    }

    pub(crate) fn world_move_frames_in(
        &self,
        targets: &[&TransformTarget],
        space: MoveSpace,
        axis: Option<WorldAxis>,
    ) -> Result<Vec<(WorldMoveFrame, ChannelBaseline)>, MoveBeginError> {
        self.move_scene_check().map_err(MoveBeginError::Scene)?;
        let mut movers = Vec::new();
        movers
            .try_reserve_exact(targets.len())
            .map_err(|_| MoveBeginError::Scene(NativeError::ResourceLimit))?;
        for (index, target) in targets.iter().enumerate() {
            let member = |source| MoveBeginError::Member { index, source };
            let frame = self.member_move_frame(target).map_err(member)?;
            if let Some(axis) = axis {
                frame.require_leaves_in(space, axis).map_err(member)?;
            }
            let node = self.node(target.node).map_err(member)?;
            movers.push((
                frame,
                self.channel_baseline(node, TransformField::Translation),
            ));
        }
        Ok(movers)
    }

    fn world_move_frame_of(
        &self,
        target: &TransformTarget,
        selected: &NativeTransformNode,
    ) -> Result<WorldMoveFrame, NativeError> {
        let transform = self.internal_transform(target.node)?;
        let mut absent_leaf = [false; 3];
        if let Some(source) = &self.source {
            if source.format != SourceFormat::Binary {
                return Err(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::TranslationSource,
                ));
            }
            for (component, scalar) in selected
                .field(TransformField::Translation)
                .iter()
                .enumerate()
            {
                let origin = match &scalar.origin {
                    Some(origin @ ScalarOrigin::Binary { .. }) => origin,
                    None => {
                        absent_leaf[component] =
                            !source.can_insert(selected.transform_id, TransformField::Translation);
                        continue;
                    }
                    #[cfg(test)]
                    Some(_) => {
                        return Err(NativeError::UnsupportedMove(
                            WorldMoveUnsupported::TranslationSource,
                        ));
                    }
                };
                source.original(
                    origin,
                    selected.transform_id,
                    TransformField::Translation,
                    component,
                )?;
            }
        }
        let parent_world = self
            .ancestor_world(target)?
            .ok_or(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform,
            ))?;
        let parent_linear = DMat3::from_mat4(parent_world);
        let determinant = parent_linear.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform,
            ));
        }
        let inverse_parent = parent_linear.inverse();
        let parent_norm = infinity_norm(parent_linear);
        let condition = parent_norm * infinity_norm(inverse_parent);
        // The Move handle sits at the rotate pivot, T + RT + RP in the parent's space. The pivots
        // are fixed points of the rotate and scale stages, so they never enter the channel maths.
        // PROVISIONAL: the docs do not place the Move handle relative to the pivot (plan section 1).
        let origin_cm = parent_world.transform_point3(transform.pivot_origin_cm());
        if !inverse_parent.is_finite()
            || !condition.is_finite()
            || condition > MAX_PARENT_CONDITION
            || !origin_cm.is_finite()
        {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform,
            ));
        }
        Ok(WorldMoveFrame {
            origin_cm,
            parent_linear,
            inverse_parent,
            parent_norm,
            object_rotation: transform
                .rotation_only()
                .ok_or(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::ParentTransform,
                ))?,
            translation: selected.transform().translation,
            translation_cm: DVec3::from_array(transform.translation_cm),
            linear_factor: self.units.linear_factor()?,
            absent_leaf,
        })
    }
}

pub(super) fn field_start(field: TransformField) -> usize {
    match field {
        TransformField::Translation => 0,
        TransformField::Rotation => 3,
        TransformField::Scale => 6,
        TransformField::RotatePivot | TransformField::ScalePivot => {
            unreachable!("pivots are never edited through a channel baseline")
        }
    }
}

fn infinity_norm(matrix: DMat3) -> f64 {
    let columns = matrix.to_cols_array_2d();
    (0..3)
        .map(|row| columns.iter().map(|column| column[row].abs()).sum::<f64>())
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests;
