use super::*;
use glam::DMat3;
use std::sync::Arc;

const MAX_PARENT_CONDITION: f64 = 1e12;
const ROUNDTRIP_EPSILON: f64 = 128.0 * f64::EPSILON;

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
}

pub(crate) struct MoveTranslationBaseline {
    node: NodeId,
    owner: NativeId,
    scalars: [Scalar; 3],
    source: Option<Arc<[u8]>>,
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
        if !represented_world.is_finite()
            || (represented_world - displacement).abs().max_element() > tolerance
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
    ) -> Result<(WorldMoveFrame, MoveTranslationBaseline), NativeError> {
        let frame = self.world_move_frame(target)?;
        let node = self.node(target.node)?;
        Ok((
            frame,
            MoveTranslationBaseline {
                node: target.node,
                owner: node.transform_id,
                scalars: std::array::from_fn(|component| node.channels[component].clone()),
                source: self.source.as_ref().map(|source| source.bytes.clone()),
            },
        ))
    }

    fn move_translation_owner(
        &self,
        baseline: &MoveTranslationBaseline,
    ) -> Result<usize, NativeError> {
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
        for (component, original) in baseline.scalars.iter().enumerate() {
            let current = &self.nodes[index].channels[component];
            if current.origin != original.origin {
                return Err(NativeError::InvalidRetention);
            }
            original.validate_origin(
                self.source.as_ref(),
                &self.units,
                baseline.owner,
                TransformField::Translation,
                component,
            )?;
            current.validate_origin(
                self.source.as_ref(),
                &self.units,
                baseline.owner,
                TransformField::Translation,
                component,
            )?;
        }
        Ok(index)
    }

    pub(crate) fn preview_move_translation(
        &mut self,
        baseline: &MoveTranslationBaseline,
        translation: [f64; 3],
    ) -> Result<(MoveTranslationBaseline, bool), NativeError> {
        if translation.iter().any(|value| !value.is_finite()) {
            return Err(NativeError::NonFinite);
        }
        let index = self.move_translation_owner(baseline)?;
        let mut replacement = baseline.scalars.clone();
        for (component, scalar) in replacement.iter_mut().enumerate() {
            if translation[component].to_bits() != scalar.value.to_bits() {
                *scalar = scalar.replacement(
                    translation[component],
                    self.source.as_ref(),
                    &self.units,
                    baseline.owner,
                    TransformField::Translation,
                    component,
                )?;
            }
        }
        let previous = MoveTranslationBaseline {
            node: baseline.node,
            owner: baseline.owner,
            scalars: std::array::from_fn(|component| self.nodes[index].channels[component].clone()),
            source: baseline.source.clone(),
        };
        let changed = self.nodes[index].channels[..3] != replacement;
        self.nodes[index].channels[..3].clone_from_slice(&replacement);
        Ok((previous, changed))
    }

    pub(crate) fn restore_move_translation(
        &mut self,
        baseline: &MoveTranslationBaseline,
    ) -> Result<(), NativeError> {
        let index = self.move_translation_owner(baseline)?;
        self.nodes[index].channels[..3].clone_from_slice(&baseline.scalars);
        Ok(())
    }

    pub(crate) fn move_translation_changes(
        &self,
        baseline: &MoveTranslationBaseline,
    ) -> Result<[Option<f64>; 9], NativeError> {
        let index = self.move_translation_owner(baseline)?;
        let mut channels = [None; 9];
        for (component, original) in baseline.scalars.iter().enumerate() {
            let current = &self.nodes[index].channels[component];
            if current != original {
                channels[component] = Some(current.value);
            }
        }
        Ok(channels)
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
                let Some(origin @ ScalarOrigin::Binary { .. }) = &scalar.origin else {
                    return Err(NativeError::UnsupportedMove(
                        WorldMoveUnsupported::TranslationSource,
                    ));
                };
                source.original(
                    origin,
                    selected.transform_id,
                    TransformField::Translation,
                    component,
                )?;
            }
        }
        let mut parent_world = DMat4::IDENTITY;
        for id in target.path.iter().take(target.path.len().saturating_sub(1)) {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == *id)
                .ok_or(NativeError::InvalidInstancePath)?;
            let local =
                self.internal_transform(node.id)?
                    .matrix()
                    .ok_or(NativeError::UnsupportedMove(
                        WorldMoveUnsupported::ParentTransform,
                    ))?;
            parent_world *= local;
            if !parent_world.is_finite() {
                return Err(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::ParentTransform,
                ));
            }
        }
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
        let origin_cm = parent_world.transform_point3(DVec3::from_array(transform.translation_cm));
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
        })
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
