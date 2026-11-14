use super::move_translation::{ChannelBaseline, MIN_RESOLUTION, ROUNDTRIP_EPSILON, ulp_bound};
use super::*;
use glam::DMat3;
use source::Storage;
use std::f64::consts::{PI, TAU};

// Euler decomposition is refused this close to gimbal lock (|cos y| below the bound).
const MIN_COS_PITCH: f64 = 1e-6;
// Non-intended channels this close to their baseline are restored to the baseline bits.
const SNAP_TOLERANCE: f64 = 1e-12;
const MAX_ANGLE_RADIANS: f64 = 1e6;

/// Object-space rotation axis; the node's own X, Y or Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateAxis {
    X,
    Y,
    Z,
}

impl RotateAxis {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// Whether a ring turns about the node's own axes, the world axes, or the gimbal axes, where
/// each ring edits exactly one rotate channel (Maya's Gimbal orientation, xyz order only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateSpace {
    Object,
    World,
    Gimbal,
}

// Parent linear maps may deviate from rotation x uniform scale by this relative amount.
const PARENT_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotateUnsupported {
    ParentTransform,
    NonPositiveScale,
    RotateOrder,
    RotateAxis,
    Gimbal,
    NumericalAngle,
}

impl fmt::Display for RotateUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ParentTransform => {
                "world rotation needs parents that are a rotation times a uniform positive scale"
            }
            Self::NonPositiveScale => "world rotation needs positive scale on the selected node",
            Self::RotateOrder => "rotation needs the xyz rotate order (Rotate order is read-only)",
            Self::RotateAxis => "rotation needs a zero rotate axis (Rotate axis is read-only)",
            Self::Gimbal => "rotation is too close to gimbal lock",
            Self::NumericalAngle => "rotation cannot be represented accurately",
        })
    }
}

/// Baseline rotation of one node, as declared channel values and internal radians.
#[derive(Clone)]
pub struct RotateFrame {
    space: RotateSpace,
    parent_linear: DMat3,
    declared: [f64; 3],
    radians: [f64; 3],
    absent_leaf: [bool; 3],
    units: Units,
}

impl fmt::Debug for RotateFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RotateFrame").finish_non_exhaustive()
    }
}

// True when `linear` is a rotation times one positive scale, within `PARENT_TOLERANCE`.
pub(super) fn rotation_times_uniform_scale(linear: DMat3) -> bool {
    let columns = [linear.x_axis, linear.y_axis, linear.z_axis];
    let scale = columns.iter().map(|column| column.length()).sum::<f64>() / 3.0;
    if !linear.is_finite() || !scale.is_normal() || linear.determinant() <= 0.0 {
        return false;
    }
    let tolerance = PARENT_TOLERANCE * scale;
    columns
        .iter()
        .all(|column| (column.length() - scale).abs() <= tolerance)
        && (0..3)
            .all(|i| ((i + 1)..3).all(|j| columns[i].dot(columns[j]).abs() <= tolerance * scale))
}

// Channel edits are xyz Euler deltas, and R * RA * Rd * RA^-1 != R * Rd.
fn require_xyz_without_rotate_axis(transform: &InternalTransform) -> Result<(), NativeError> {
    if let Some(extras) = transform.extras {
        if extras.rotate_order != 0 {
            return Err(NativeError::UnsupportedRotate(
                RotateUnsupported::RotateOrder,
            ));
        }
        if extras.rotate_axis_radians.iter().any(|value| *value != 0.0) {
            return Err(NativeError::UnsupportedRotate(
                RotateUnsupported::RotateAxis,
            ));
        }
    }
    Ok(())
}

pub(super) fn rotation_matrix([x, y, z]: [f64; 3]) -> DMat3 {
    DMat3::from_rotation_z(z) * DMat3::from_rotation_y(y) * DMat3::from_rotation_x(x)
}

// Shift `value` by whole turns to the representative nearest `base`.
fn unwrap_toward(value: f64, base: f64) -> f64 {
    value + TAU * ((base - value) / TAU).round()
}

impl RotateFrame {
    // Maya binary files omit default (zero) rotate leaves and the encoder cannot insert them.
    // An X ring changes rotateX only; Y and Z rings can change all three channels. A gimbal ring
    // changes its own channel only.
    // A world ring can change all three channels, whichever axis it turns about.
    pub fn require_leaves(&self, axis: RotateAxis) -> Result<(), NativeError> {
        let needed = match self.space {
            RotateSpace::Object if axis == RotateAxis::X => 0..1,
            RotateSpace::Gimbal => axis.index()..axis.index() + 1,
            _ => 0..3,
        };
        for component in needed {
            if self.absent_leaf[component] {
                return Err(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Rotate,
                    component,
                });
            }
        }
        Ok(())
    }

    pub fn space(&self) -> RotateSpace {
        self.space
    }

    /// Declared rotate channel values after turning by `degrees` about `axis`, the node's own
    /// axis in object space or the world axis in world space.
    pub fn rotation(&self, axis: RotateAxis, degrees: f64) -> Result<[f64; 3], NativeError> {
        self.rotation_declared(axis, degrees, None)
    }

    /// `rotation` for an angle also known in the scene's angular unit (`declared`). A radian
    /// scene then uses it as the delta, so a single-channel result is the exact
    /// `baseline + declared` instead of a degree round trip.
    pub fn rotation_declared(
        &self,
        axis: RotateAxis,
        degrees: f64,
        declared: Option<f64>,
    ) -> Result<[f64; 3], NativeError> {
        if !degrees.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if degrees == 0.0 {
            return Ok(self.declared);
        }
        let delta = match (&self.units.angular, declared) {
            (AngularUnit::Radian, Some(angle)) if angle.is_finite() => angle,
            _ => degrees.to_radians(),
        };
        let refused = NativeError::UnsupportedRotate(RotateUnsupported::NumericalAngle);
        if !delta.is_finite() || delta.abs() > MAX_ANGLE_RADIANS {
            return Err(refused);
        }
        let mut radians = match (self.space, axis) {
            (RotateSpace::Object, RotateAxis::X) | (RotateSpace::Gimbal, _) => {
                let mut radians = self.radians;
                radians[axis.index()] += delta;
                radians
            }
            (RotateSpace::Object, _) => self.decompose(
                rotation_matrix(self.radians)
                    * match axis {
                        RotateAxis::Y => DMat3::from_rotation_y(delta),
                        _ => DMat3::from_rotation_z(delta),
                    },
            )?,
            (RotateSpace::World, _) => {
                // P = s * Rp, so Rp^T e_axis is the normalized `axis` row of P. Turning the world
                // by d about e_axis equals turning the parent-local frame by d about that row.
                let row = DVec3::new(
                    self.parent_linear.x_axis[axis.index()],
                    self.parent_linear.y_axis[axis.index()],
                    self.parent_linear.z_axis[axis.index()],
                );
                let step = DMat3::from_axis_angle(row.normalize(), delta);
                self.decompose(step * rotation_matrix(self.radians))?
            }
        };
        // Heuristic: the channel that moves most is taken as intended; a genuine change below
        // SNAP_TOLERANCE on any other channel is snapped away (see known-issues.md).
        let intended = match self.space {
            RotateSpace::Object | RotateSpace::Gimbal => axis.index(),
            RotateSpace::World => (0..3)
                .max_by(|a, b| {
                    let moved = |i: usize| (radians[i] - self.radians[i]).abs();
                    moved(*a).total_cmp(&moved(*b))
                })
                .unwrap_or(0),
        };
        let mut resolution: f64 = 0.0;
        for (component, (value, &base)) in radians.iter_mut().zip(&self.radians).enumerate() {
            if component != intended && (*value - base).abs() <= SNAP_TOLERANCE * (base.abs() + 1.0)
            {
                *value = base;
            }
            if value.to_bits() != base.to_bits() {
                resolution = resolution.max(ulp_bound(*value) + ulp_bound(base));
            }
        }
        if !radians.iter().all(|value| value.is_finite())
            || resolution > MIN_RESOLUTION * delta.abs()
        {
            return Err(refused);
        }
        let mut declared = self.declared;
        let changed: Vec<usize> = (0..3)
            .filter(|&component| radians[component].to_bits() != self.radians[component].to_bits())
            .collect();
        for &component in &changed {
            declared[component] = Storage::Radians.declared(radians[component], &self.units)?;
        }
        // One changed channel that moved by the requested delta is computed in declared units:
        // converting through radians and back is not an exact round trip.
        if let [component] = changed[..] {
            let step = match &self.units.angular {
                AngularUnit::Degree => degrees,
                _ => delta,
            };
            let moved = radians[component] - self.radians[component];
            let tolerance = SNAP_TOLERANCE * (self.radians[component].abs() + 1.0);
            if (moved - delta).abs() <= tolerance {
                declared[component] = self.declared[component] + step;
            } else if (moved + delta).abs() <= tolerance {
                declared[component] = self.declared[component] - step;
            }
        }
        Ok(declared)
    }

    // Decomposes `target` to Maya xyz Euler angles (R = Rz Ry Rx), choosing the representation
    // nearest the baseline channels.
    fn decompose(&self, target: DMat3) -> Result<[f64; 3], NativeError> {
        let pitch = (-target.x_axis.z).clamp(-1.0, 1.0).asin();
        if !target.is_finite() || pitch.cos().abs() < MIN_COS_PITCH {
            return Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal));
        }
        let roll = target.y_axis.z.atan2(target.z_axis.z);
        let yaw = target.x_axis.y.atan2(target.x_axis.x);
        let rebuilt = rotation_matrix([roll, pitch, yaw]);
        let drift = (rebuilt - target).to_cols_array().map(f64::abs);
        if drift.into_iter().fold(0.0, f64::max) > ROUNDTRIP_EPSILON {
            return Err(NativeError::UnsupportedRotate(
                RotateUnsupported::NumericalAngle,
            ));
        }
        let mut best: Option<([f64; 3], f64)> = None;
        for branch in [[roll, pitch, yaw], [roll + PI, PI - pitch, yaw + PI]] {
            let candidate: [f64; 3] = std::array::from_fn(|component| {
                unwrap_toward(branch[component], self.radians[component])
            });
            let distance: f64 = (0..3)
                .map(|component| (candidate[component] - self.radians[component]).abs())
                .sum();
            if best.is_none_or(|(_, nearest)| distance < nearest) {
                best = Some((candidate, distance));
            }
        }
        best.map(|(angles, _)| angles)
            .ok_or(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal))
    }
}

impl NativeScene {
    pub(crate) fn begin_rotation(
        &self,
        target: &TransformTarget,
        axis: RotateAxis,
        space: RotateSpace,
    ) -> Result<(RotateFrame, ChannelBaseline), NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        let selected = self.resolve_transform_path(target.node, &target.path)?;
        self.editable(target.node)?;
        self.units.factors()?;
        let transform = self.internal_transform(target.node)?;
        // The rotate pivot is the fixed point of the rotation stage, so it never enters the
        // channel maths: only the handle origin needs it (`ManipulatorFrame::handle_origin`).
        // Channel edits are xyz Euler deltas, and R * RA * Rd * RA^-1 != R * Rd: refuse before
        // any source or leaf check so the reason is the same in every format (Task 2d lifts it).
        require_xyz_without_rotate_axis(&transform)?;
        let mut absent_leaf = [false; 3];
        if let Some(source) = &self.source {
            if source.format != SourceFormat::Binary {
                return Err(NativeError::ReadOnlyFormat);
            }
            for (component, scalar) in selected.field(TransformField::Rotation).iter().enumerate() {
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
                    TransformField::Rotation,
                    component,
                )?;
            }
        }
        let parent_linear = match space {
            RotateSpace::Object => DMat3::IDENTITY,
            RotateSpace::World => self.parent_linear(target, transform.scale)?,
            // The channel edit never reads P; `gimbal_axes` checks the parent for the handles.
            RotateSpace::Gimbal => DMat3::IDENTITY,
        };
        let frame = RotateFrame {
            space,
            parent_linear,
            declared: std::array::from_fn(|component| selected.channels[3 + component].value),
            radians: transform.rotation_radians,
            absent_leaf,
            units: self.units.clone(),
        };
        frame.require_leaves(axis)?;
        Ok((
            frame,
            self.channel_baseline(selected, TransformField::Rotation),
        ))
    }

    // Product of the ancestors' local linear maps (the path excluding the selected node). World
    // rotation also requires positive scale on the selected node: a policy choice consistent with
    // Move, not a mathematical need (the node's scale sits to the right of its rotation).
    fn parent_linear(
        &self,
        target: &TransformTarget,
        scale: [f64; 3],
    ) -> Result<DMat3, NativeError> {
        if scale.iter().any(|value| value.is_nan() || *value <= 0.0) {
            return Err(NativeError::UnsupportedRotate(
                RotateUnsupported::NonPositiveScale,
            ));
        }
        self.parent_rotation_scale(target)
    }

    // The compensated parent linear map, refused unless it is a rotation times a uniform
    // positive scale.
    fn parent_rotation_scale(&self, target: &TransformTarget) -> Result<DMat3, NativeError> {
        let refused = NativeError::UnsupportedRotate(RotateUnsupported::ParentTransform);
        let parent_world = self.ancestor_world(target)?.ok_or(refused.clone())?;
        let linear = DMat3::from_mat4(parent_world);
        if rotation_times_uniform_scale(linear) {
            // A joint's rotation sits under IS too; two negative parent scales pass the check
            // above with an inverse that is not uniform.
            Ok(self
                .internal_transform(target.node)?
                .compensated_parent(linear))
        } else {
            Err(refused)
        }
    }

    // The compensated parent map must itself be rotation x uniform scale: a ring drag measures
    // its angle in the world plane of P * axis, which equals the channel delta only then, and a
    // joint's inverse parent scale can break what the raw ancestors satisfy.
    fn gimbal_parent(&self, target: &TransformTarget) -> Result<DMat3, NativeError> {
        let parent = self.parent_rotation_scale(target)?;
        if rotation_times_uniform_scale(parent) {
            Ok(parent)
        } else {
            Err(NativeError::UnsupportedRotate(
                RotateUnsupported::ParentTransform,
            ))
        }
    }

    /// The world directions of the X, Y and Z gimbal rings: `P * (Rz Ry e_x)`, `P * (Rz e_y)` and
    /// `P * e_z`, with P the compensated parent map. Turning about ring `a` changes only rotate
    /// channel `a`. Not normalized (P may carry a uniform scale). Refused for a
    /// non-xyz rotate order, a non-zero rotate axis, or a parent that is not rotation x uniform
    /// scale (the channel edit itself, `begin_rotation`, does not need the parent). It does not check source or leaf rules; `rotation_refusal` does.
    pub fn gimbal_axes(&self, target: &TransformTarget) -> Result<[DVec3; 3], NativeError> {
        if target.path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        self.resolve_transform_path(target.node, &target.path)?;
        let transform = self.internal_transform(target.node)?;
        require_xyz_without_rotate_axis(&transform)?;
        let parent = self.gimbal_parent(target)?;
        let [_, y, z] = transform.rotation_radians;
        let axes = [
            parent * (DMat3::from_rotation_z(z) * DMat3::from_rotation_y(y) * DVec3::X),
            parent * (DMat3::from_rotation_z(z) * DVec3::Y),
            parent * DVec3::Z,
        ];
        Ok(axes)
    }
}

#[cfg(test)]
mod tests;
