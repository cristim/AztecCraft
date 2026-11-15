//! The frame a component Move (#41) turns a world drag into mesh-local point edits with: the
//! instance's world matrix, its linear part's inverse, and the handle directions and centre.

use super::move_translation::{MAX_PARENT_CONDITION, infinity_norm};
use super::*;
/// An instance's world matrix `W` (mesh-local centimetres to world centimetres) and the inverse of
/// its linear part `L`. A world displacement `v` moves a mesh point by `L^-1 v`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComponentMoveFrame {
    world: DMat4,
    inverse_linear: DMat3,
}

impl ComponentMoveFrame {
    /// `None` when `world` is not finite, or its linear part is singular or too ill-conditioned
    /// to invert accurately.
    fn new(world: DMat4) -> Option<Self> {
        let linear = DMat3::from_mat4(world);
        let determinant = linear.determinant();
        if !world.is_finite() || !determinant.is_finite() || determinant == 0.0 {
            return None;
        }
        let inverse_linear = linear.inverse();
        let condition = infinity_norm(linear) * infinity_norm(inverse_linear);
        (inverse_linear.is_finite() && condition.is_finite() && condition <= MAX_PARENT_CONDITION)
            .then_some(Self {
                world,
                inverse_linear,
            })
    }

    /// The mesh-local delta, in centimetres, that displaces every point by `world_vector_cm` in the
    /// world. Fails with `NonFinite` when the vector or the result is not finite.
    pub fn local_delta_cm(&self, world_vector_cm: DVec3) -> Result<[f64; 3], NativeError> {
        let local = self.inverse_linear * world_vector_cm;
        if !world_vector_cm.is_finite() || !local.is_finite() {
            return Err(NativeError::NonFinite);
        }
        Ok(local.to_array())
    }

    /// The world position of a mesh-local point.
    pub fn world_point_cm(&self, local_cm: [f64; 3]) -> DVec3 {
        self.world.transform_point3(DVec3::from_array(local_cm))
    }

    /// The centre of the world-space bounding box of `points_cm` (mesh-local centimetres), the
    /// Move handle origin. PROVISIONAL: Maya's docs say only "the centre of the selected components".
    pub fn bbox_center_world_cm(&self, points_cm: &[[f64; 3]]) -> Result<DVec3, NativeError> {
        if points_cm.is_empty() {
            return Err(NativeError::InvalidTopology);
        }
        let (min, max) = points_cm.iter().fold(
            (DVec3::INFINITY, DVec3::NEG_INFINITY),
            |(min, max), point| {
                let world = self.world_point_cm(*point);
                (min.min(world), max.max(world))
            },
        );
        let centre = (min + max) * 0.5;
        if !centre.is_finite() {
            return Err(NativeError::NonFinite);
        }
        Ok(centre)
    }
}

impl NativeScene {
    /// The frame of the instance at `target`. Refuses a singular, non-finite or ill-conditioned
    /// world matrix with the parent-transform reason the Move handles use.
    pub fn component_move_frame(
        &self,
        target: &TransformTarget,
    ) -> Result<ComponentMoveFrame, NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        self.resolve_transform_path(target.node, &target.path)?;
        ComponentMoveFrame::new(self.path_world(&target.path)?).ok_or(NativeError::UnsupportedMove(
            WorldMoveUnsupported::ParentTransform,
        ))
    }

    /// The unit world directions of the node's own X, Y and Z (Move Axis Orientation Object), from
    /// `manipulator_frame` with its refusals. A zero-length axis refuses.
    pub fn component_move_object_axes(
        &self,
        target: &TransformTarget,
    ) -> Result<[DVec3; 3], NativeError> {
        let axes = self.manipulator_frame(target)?.object_axes;
        let mut unit = [DVec3::ZERO; 3];
        for (out, axis) in unit.iter_mut().zip(axes) {
            let length = axis.length();
            *out = axis / length;
            if !length.is_finite() || length == 0.0 || !out.is_finite() {
                return Err(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::ParentTransform,
                ));
            }
        }
        Ok(unit)
    }
}

#[cfg(test)]
mod tests;
