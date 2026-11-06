use super::*;

/// Where the Rotate and Scale handles sit and where the node's own axes point in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManipulatorFrame {
    /// Move and Rotate pivot: the parents' map of T + RT (Move and Rotate refuse a rotate pivot).
    pub origin_cm: DVec3,
    /// Where the node's scale leaves the world fixed: the world image of its scale pivot, which
    /// Scale requires to be zero. Differs from `origin_cm` with scalePivotTranslate, or a rotate
    /// pivot under a rotation.
    pub scale_origin_cm: DVec3,
    /// The node's X, Y and Z as seen from the world: the parents' linear map times the node's
    /// rotation (rotate order and rotate axis included), with a joint's inverse parent scale
    /// between the two. The node's own shear and scale never
    /// enter it. Not normalized, and mirrored or skewed when a parent has negative or uneven
    /// scale; the handles refuse such frames.
    pub object_axes: [DVec3; 3],
}

impl NativeScene {
    /// The handle frame of `target`. It does not check whether an edit is allowed; use
    /// `rotation_refusal` and `scale_refusal` for that.
    pub fn manipulator_frame(
        &self,
        target: &TransformTarget,
    ) -> Result<ManipulatorFrame, NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        self.resolve_transform_path(target.node, &target.path)?;
        let transform = self.internal_transform(target.node)?;
        let refused = NativeError::UnsupportedMove(WorldMoveUnsupported::ParentTransform);
        let parent_world = self.ancestor_world(target)?.ok_or(refused.clone())?;
        let origin_cm = parent_world.transform_point3(transform.origin_cm());
        let scale_origin_cm = transform
            .matrix()
            .map(|local| {
                (parent_world * local).transform_point3(DVec3::from_array(transform.scale_pivot_cm))
            })
            .ok_or(refused.clone())?;
        let axes = transform
            .rotation_frame()
            .map(|rotation| transform.compensated_parent(DMat3::from_mat4(parent_world)) * rotation)
            .ok_or(refused.clone())?;
        let object_axes = [axes.x_axis, axes.y_axis, axes.z_axis];
        if !origin_cm.is_finite() || object_axes.iter().any(|axis| !axis.is_finite()) {
            return Err(refused);
        }
        Ok(ManipulatorFrame {
            origin_cm,
            scale_origin_cm,
            object_axes,
        })
    }

    /// Why a rotation about `axis` in `space` would be refused at begin, if it would.
    pub fn rotation_refusal(
        &self,
        target: &TransformTarget,
        axis: RotateAxis,
        space: RotateSpace,
    ) -> Option<NativeError> {
        self.begin_rotation(target, axis, space).err()
    }

    /// Why a scale of `handle` in `space` would be refused at begin, if it would.
    pub fn scale_refusal(
        &self,
        target: &TransformTarget,
        handle: ScaleHandle,
        space: ScaleSpace,
    ) -> Option<NativeError> {
        self.begin_scale_frame(target, handle, space).err()
    }
}

#[cfg(test)]
mod tests;
