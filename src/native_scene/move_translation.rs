use super::*;
use glam::DMat3;
use std::sync::Arc;

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
    SelectedPivot,
    TranslationSource,
    ParentTransform,
    NumericalDisplacement,
}

impl fmt::Display for WorldMoveUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SelectedPivot => "selected rotation and scale pivots must be zero",
            Self::TranslationSource => "translation requires an existing supported source leaf",
            Self::ParentTransform => "parent transform is singular or numerically unstable",
            Self::NumericalDisplacement => "displacement cannot be represented accurately",
        })
    }
}

#[derive(Clone, Copy)]
pub struct WorldMoveFrame {
    origin_cm: DVec3,
    parent_linear: DMat3,
    inverse_parent: DMat3,
    parent_norm: f64,
    translation: [f64; 3],
    translation_cm: DVec3,
    linear_factor: f64,
    absent_leaf: [bool; 3],
}

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
        let local = self.inverse_parent * axis.direction();
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
        if !distance_cm.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if distance_cm == 0.0 {
            return Ok(self.translation);
        }
        let displacement = axis.direction() * distance_cm;
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
    pub(crate) fn begin_move_translation(
        &self,
        target: &TransformTarget,
        axis: WorldAxis,
    ) -> Result<(WorldMoveFrame, ChannelBaseline), NativeError> {
        let frame = self.world_move_frame(target)?;
        frame.require_leaves(axis)?;
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

    fn channel_owner(&self, baseline: &ChannelBaseline) -> Result<usize, NativeError> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.id == baseline.node && node.transform_id == baseline.owner)
            .ok_or(NativeError::InvalidIdentity)?;
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
        Ok(index)
    }

    pub(crate) fn preview_channels(
        &mut self,
        baseline: &ChannelBaseline,
        values: [f64; 3],
    ) -> Result<(ChannelBaseline, bool), NativeError> {
        if values.iter().any(|value| !value.is_finite()) {
            return Err(NativeError::NonFinite);
        }
        let index = self.channel_owner(baseline)?;
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
        let previous = ChannelBaseline {
            field: baseline.field,
            node: baseline.node,
            owner: baseline.owner,
            scalars: std::array::from_fn(|component| {
                self.nodes[index].channels[start + component].clone()
            }),
            source: baseline.source.clone(),
        };
        let changed = self.nodes[index].channels[start..start + 3] != replacement;
        self.nodes[index].channels[start..start + 3].clone_from_slice(&replacement);
        Ok((previous, changed))
    }

    pub(crate) fn restore_channels(
        &mut self,
        baseline: &ChannelBaseline,
    ) -> Result<(), NativeError> {
        let index = self.channel_owner(baseline)?;
        let start = field_start(baseline.field);
        self.nodes[index].channels[start..start + 3].clone_from_slice(&baseline.scalars);
        Ok(())
    }

    pub(crate) fn channel_changes(
        &self,
        baseline: &ChannelBaseline,
    ) -> Result<[Option<f64>; 9], NativeError> {
        let index = self.channel_owner(baseline)?;
        let start = field_start(baseline.field);
        let mut channels = [None; 9];
        for (component, original) in baseline.scalars.iter().enumerate() {
            let current = &self.nodes[index].channels[start + component];
            if current != original {
                channels[start + component] = Some(current.value);
            }
        }
        Ok(channels)
    }

    /// Product of the local matrices of the ancestors on `target.path` (the selected node
    /// excluded); `None` when one has no finite matrix.
    pub(super) fn ancestor_world(
        &self,
        target: &TransformTarget,
    ) -> Result<Option<DMat4>, NativeError> {
        let mut world = DMat4::IDENTITY;
        for id in target.path.iter().take(target.path.len().saturating_sub(1)) {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == *id)
                .ok_or(NativeError::InvalidInstancePath)?;
            let Some(local) = self.internal_transform(node.id)?.matrix() else {
                return Ok(None);
            };
            world *= local;
            if !world.is_finite() {
                return Ok(None);
            }
        }
        Ok(Some(world))
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
        let transform = self.internal_transform(target.node)?;
        if transform
            .rotate_pivot_cm
            .into_iter()
            .chain(transform.scale_pivot_cm)
            .any(|value| value != 0.0)
        {
            return Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::SelectedPivot,
            ));
        }
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
        let origin_cm = parent_world.transform_point3(transform.origin_cm());
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
