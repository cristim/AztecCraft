use super::move_translation::ChannelBaseline;
use super::rotate_frame::rotation_times_uniform_scale;
use super::*;
use glam::DMat3;

// A world axis counts as aligned with a local axis when the other two components of its image
// are below this bound (about 6e-8 degrees of tilt); a larger tilt would shear the node.
const ALIGN_TOLERANCE: f64 = 1e-9;

/// Whether a drag scales along the node's own axes or along the world axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleSpace {
    Object,
    World,
}

/// What a scale drag grabs: one axis, or the center box for all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleHandle {
    X,
    Y,
    Z,
    Uniform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleUnsupported {
    SelectedPivot,
    Shear,
    ParentTransform,
    NotAxisAligned,
    InvalidFactor,
    Unrepresentable,
}

impl fmt::Display for ScaleUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SelectedPivot => "the selected scale pivot must be zero",
            Self::Shear => "world scale needs zero shear on the selected node",
            Self::ParentTransform => {
                "world scale needs parents that are a rotation times a uniform positive scale"
            }
            Self::NotAxisAligned => "that world axis is not aligned with a local axis",
            Self::InvalidFactor => "scale factors must be finite and greater than zero",
            Self::Unrepresentable => "scaled channel overflows or underflows",
        })
    }
}

/// Baseline scale of one node; scale channels are unitless, so declared equals internal.
#[derive(Clone, Debug)]
pub struct ScaleFrame {
    space: ScaleSpace,
    // The local scale channel a handle edits; `None` for a world axis that is not aligned.
    axis_component: [Option<usize>; 3],
    scale: [f64; 3],
    absent_leaf: [bool; 3],
}

impl ScaleFrame {
    pub fn space(&self) -> ScaleSpace {
        self.space
    }

    // The channels `handle` edits: all three for the center box, else the aligned local axis.
    fn components(&self, handle: ScaleHandle) -> Result<std::ops::Range<usize>, NativeError> {
        let index = match handle {
            ScaleHandle::X => 0,
            ScaleHandle::Y => 1,
            ScaleHandle::Z => 2,
            ScaleHandle::Uniform => return Ok(0..3),
        };
        let component = self.axis_component[index].ok_or(NativeError::UnsupportedScale(
            ScaleUnsupported::NotAxisAligned,
        ))?;
        Ok(component..component + 1)
    }

    // Maya binary files omit default (one) scale leaves and the encoder cannot insert them.
    pub fn require_leaves(&self, handle: ScaleHandle) -> Result<(), NativeError> {
        for component in self.components(handle)? {
            if self.absent_leaf[component] {
                return Err(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Scale,
                    component,
                });
            }
        }
        Ok(())
    }

    /// Scale channel values after multiplying the handle's channels by `factor`.
    pub fn scale(&self, handle: ScaleHandle, factor: f64) -> Result<[f64; 3], NativeError> {
        if !factor.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if factor <= 0.0 {
            return Err(NativeError::UnsupportedScale(
                ScaleUnsupported::InvalidFactor,
            ));
        }
        let mut scale = self.scale;
        for component in self.components(handle)? {
            let scaled = scale[component] * factor;
            if !scaled.is_finite() || (scaled == 0.0 && scale[component] != 0.0) {
                return Err(NativeError::UnsupportedScale(
                    ScaleUnsupported::Unrepresentable,
                ));
            }
            scale[component] = scaled;
        }
        Ok(scale)
    }
}

impl NativeScene {
    pub(crate) fn begin_scale_frame(
        &self,
        target: &TransformTarget,
        handle: ScaleHandle,
        space: ScaleSpace,
    ) -> Result<(ScaleFrame, ChannelBaseline), NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        let selected = self.resolve_transform_path(target.node, &target.path)?;
        self.editable(target.node)?;
        let transform = self.internal_transform(target.node)?;
        if transform
            .scale_pivot_cm
            .into_iter()
            .any(|value| value != 0.0)
        {
            return Err(NativeError::UnsupportedScale(
                ScaleUnsupported::SelectedPivot,
            ));
        }
        // SH^-1 * D * SH is not diagonal for a non-uniform D, so one scale channel cannot express a
        // world-axis scale of a sheared node. Refused before any source or leaf check.
        if space == ScaleSpace::World
            && transform
                .extras
                .is_some_and(|extras| extras.shear.iter().any(|value| *value != 0.0))
        {
            return Err(NativeError::UnsupportedScale(ScaleUnsupported::Shear));
        }
        let mut absent_leaf = [false; 3];
        if let Some(source) = &self.source {
            if source.format != SourceFormat::Binary {
                return Err(NativeError::ReadOnlyFormat);
            }
            for (component, scalar) in selected.field(TransformField::Scale).iter().enumerate() {
                let origin = match &scalar.origin {
                    Some(origin @ ScalarOrigin::Binary { .. }) => origin,
                    None => {
                        absent_leaf[component] = true;
                        continue;
                    }
                    #[cfg(test)]
                    Some(_) => return Err(NativeError::UnsupportedEdit),
                };
                source.original(
                    origin,
                    selected.transform_id,
                    TransformField::Scale,
                    component,
                )?;
            }
        }
        let axis_component = match space {
            ScaleSpace::Object => [Some(0), Some(1), Some(2)],
            ScaleSpace::World => {
                let rotation = transform
                    .rotation_frame()
                    .ok_or(NativeError::InvalidTransformExtras)?;
                self.world_scale_components(target, rotation)?
            }
        };
        let frame = ScaleFrame {
            space,
            axis_component,
            scale: std::array::from_fn(|component| selected.channels[6 + component].value),
            absent_leaf,
        };
        frame.require_leaves(handle)?;
        Ok((
            frame,
            self.channel_baseline(selected, TransformField::Scale),
        ))
    }

    // For each world axis, the local scale channel it maps to through the parent chain and the
    // node's R(ro) * RA frame. Scaling along a world axis edits a single channel only when that
    // axis is a local axis of the node's world orientation; otherwise the result would shear.
    fn world_scale_components(
        &self,
        target: &TransformTarget,
        rotation: DMat3,
    ) -> Result<[Option<usize>; 3], NativeError> {
        let refused = NativeError::UnsupportedScale(ScaleUnsupported::ParentTransform);
        let parent = self.ancestor_world(target)?.ok_or(refused.clone())?;
        let linear = DMat3::from_mat4(parent);
        // Policy: every handle, the center box included, needs a uniform parent. A uniform
        // factor would commute with any parent, but one rule is simpler and matches the others.
        if !rotation_times_uniform_scale(linear) {
            return Err(refused);
        }
        let linear = self
            .internal_transform(target.node)?
            .compensated_parent(linear);
        let uniform =
            (linear.x_axis.length() + linear.y_axis.length() + linear.z_axis.length()) / 3.0;
        let orientation = (linear / uniform) * rotation;
        Ok(std::array::from_fn(|axis| {
            let image = [
                orientation.x_axis[axis],
                orientation.y_axis[axis],
                orientation.z_axis[axis],
            ];
            let local = (0..3).max_by(|a, b| image[*a].abs().total_cmp(&image[*b].abs()))?;
            (0..3)
                .filter(|other| *other != local)
                .all(|other| image[other].abs() <= ALIGN_TOLERANCE)
                .then_some(local)
        }))
    }
}

#[cfg(test)]
mod tests;
