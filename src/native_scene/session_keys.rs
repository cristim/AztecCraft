//! Transform session keys and authored built-in camera focal curves.
//!
//! Transform keys prevent Save; native codecs persist the bounded focal profile.
//! Keys sit on whole frames. Focal curves hold outside their keyed span.
//! Evaluation interpolates linearly between two keys and, outside them, follows the curve's pre-
//! and post-infinity (constant by default; see `infinity`) (PROVISIONAL: Maya's default tangents
//! are Auto, and tangent types are not implemented). A key frame returns the
//! exact stored value. Sourced scenes refuse keys like every other structural edit, and their own
//! animation curves are not evaluated.

mod camera;
mod euler;
mod infinity;
pub(in crate::native_scene) mod persistence;
pub use euler::check_euler_channels;
use infinity::evaluate_with;
pub use infinity::{Infinity, InfinityPair};

use super::{NativeError, NativeScene, NativeTransformNode, Scalar, TransformField};
use crate::scene::NodeId;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Bound on keys per scene, so undo snapshots and a playback tick stay small.
pub const MAX_SESSION_KEYS: usize = 1 << 16;
/// Bound on frames, an AztecCraft choice. It keeps every frame inside `i32` arithmetic.
/// The frame a new timeline starts at, and the frame a new `History` counts its pose from.
pub const DEFAULT_FRAME: i32 = 1;
pub const MAX_FRAME: i32 = 1_000_000;

/// The keyable channels, in `NativeTransform::values` order.
pub const KEY_CHANNELS: [&str; 9] = [
    "translate_x",
    "translate_y",
    "translate_z",
    "rotate_x",
    "rotate_y",
    "rotate_z",
    "scale_x",
    "scale_y",
    "scale_z",
];

/// Which of the nine channels a key command touches.
pub type ChannelMask = [bool; 9];
pub const ALL_CHANNELS: ChannelMask = [true; 9];

/// A frame from untrusted time: finite, whole and inside the bound.
pub fn whole_frame(time: f64) -> Result<i32, NativeError> {
    if !time.is_finite() || time.fract() != 0.0 || time.abs() > f64::from(MAX_FRAME) {
        return Err(NativeError::InvalidFrame);
    }
    Ok(time as i32)
}

fn check_frame(frame: i32) -> Result<(), NativeError> {
    if frame.unsigned_abs() > MAX_FRAME.unsigned_abs() {
        return Err(NativeError::InvalidFrame);
    }
    Ok(())
}

/// One stored key, as the keys read lists it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyRow {
    pub node: NodeId,
    /// An index into `KEY_CHANNELS`.
    pub channel: usize,
    pub frame: i32,
    pub value: f64,
}

type Curve = BTreeMap<i32, f64>;

fn evaluate(curve: &Curve, frame: i32) -> Option<f64> {
    if let Some(value) = curve.get(&frame) {
        return Some(*value);
    }
    let before = curve.range(..frame).next_back();
    let after = curve.range(frame..).next();
    match (before, after) {
        (Some((start, a)), Some((end, b))) => {
            let t = f64::from(frame - start) / f64::from(end - start);
            Some(a * (1.0 - t) + b * t)
        }
        (Some((_, value)), None) | (None, Some((_, value))) => Some(*value),
        (None, None) => None,
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SessionKeys {
    /// A node appears only while at least one of its channels holds a key.
    curves: BTreeMap<NodeId, [Curve; 9]>,
    camera_curves: [Curve; 4],
    /// Only curves that hold a key and differ from `(Constant, Constant)` appear.
    infinity: BTreeMap<(NodeId, usize), InfinityPair>,
}

/// Bitwise, like `NativeTransform` equality, so a key that changes only a zero's sign is an edit.
impl PartialEq for SessionKeys {
    fn eq(&self, other: &Self) -> bool {
        let bits = |keys: &Self| {
            keys.curves
                .iter()
                .flat_map(|(node, curves)| {
                    curves.iter().enumerate().flat_map(move |(index, curve)| {
                        curve
                            .iter()
                            .map(move |(frame, value)| (*node, index, *frame, value.to_bits()))
                    })
                })
                .collect::<Vec<_>>()
        };
        self.len() == other.len()
            && bits(self) == bits(other)
            && self
                .camera_curves
                .iter()
                .zip(&other.camera_curves)
                .all(|(a, b)| {
                    a.len() == b.len()
                        && a.iter()
                            .zip(b)
                            .all(|((af, av), (bf, bv))| af == bf && av.to_bits() == bv.to_bits())
                })
            && self.infinity == other.infinity
    }
}

impl SessionKeys {
    pub(super) fn is_empty(&self) -> bool {
        self.curves.is_empty() && self.camera_curves.iter().all(BTreeMap::is_empty)
    }

    fn infinity_of(&self, node: NodeId, channel: usize) -> InfinityPair {
        self.infinity
            .get(&(node, channel))
            .copied()
            .unwrap_or_default()
    }

    fn set_infinity(&mut self, node: NodeId, channel: usize, pair: InfinityPair) {
        if pair == InfinityPair::default() {
            self.infinity.remove(&(node, channel));
        } else {
            self.infinity.insert((node, channel), pair);
        }
    }

    /// Forgets the settings of curves that no longer hold a key.
    fn drop_unkeyed_infinity(&mut self) {
        let curves = &self.curves;
        self.infinity.retain(|(node, channel), _| {
            curves
                .get(node)
                .is_some_and(|curves| !curves[*channel].is_empty())
        });
    }

    fn len(&self) -> usize {
        self.curves
            .values()
            .flatten()
            .map(BTreeMap::len)
            .sum::<usize>()
            + self.camera_curves.iter().map(BTreeMap::len).sum::<usize>()
    }

    pub(super) fn validate(
        &self,
        nodes: &[NativeTransformNode],
        sourced: bool,
    ) -> Result<(), NativeError> {
        if self.is_empty() {
            return if self.infinity.is_empty() {
                Ok(())
            } else {
                Err(NativeError::InvalidSessionKeys)
            };
        }
        let keyed = |node: &NodeId, channel: usize| {
            self.curves
                .get(node)
                .is_some_and(|curves| channel < curves.len() && !curves[channel].is_empty())
        };
        if self.infinity.iter().any(|((node, channel), pair)| {
            !keyed(node, *channel) || *pair == InfinityPair::default()
        }) {
            return Err(NativeError::InvalidSessionKeys);
        }
        if sourced || self.len() > MAX_SESSION_KEYS {
            return Err(NativeError::InvalidSessionKeys);
        }
        if self.camera_curves.iter().flatten().any(|(frame, value)| {
            check_frame(*frame).is_err() || !value.is_finite() || !(2.5..=100000.0).contains(value)
        }) {
            return Err(NativeError::InvalidSessionKeys);
        }
        let known: HashSet<NodeId> = nodes.iter().map(|node| node.id).collect();
        for (node, curves) in &self.curves {
            if !known.contains(node)
                || curves.iter().all(BTreeMap::is_empty)
                || curves
                    .iter()
                    .flatten()
                    .any(|(frame, value)| check_frame(*frame).is_err() || !value.is_finite())
            {
                return Err(NativeError::InvalidSessionKeys);
            }
        }
        Ok(())
    }

    pub(super) fn prune(&mut self, nodes: &[NativeTransformNode]) {
        let known: HashSet<NodeId> = nodes.iter().map(|node| node.id).collect();
        self.curves.retain(|node, _| known.contains(node));
        self.drop_unkeyed_infinity();
    }
}

/// Channel writes computed before any is applied, so a refused evaluation writes nothing.
pub struct EvaluatedWrites {
    transforms: Vec<(usize, [Scalar; 9])>,
    cameras: Vec<(super::SceneCamera, crate::camera_optics::CameraOptics)>,
}

impl EvaluatedWrites {
    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty() && self.cameras.is_empty()
    }
}

impl NativeScene {
    /// Refuses every time operation on a scene not declared in `film` (24 fps), the only time unit
    /// this editor plays.
    pub fn check_time_unit(&self) -> Result<(), NativeError> {
        if self.units.time == "film" {
            Ok(())
        } else {
            Err(NativeError::UnsupportedTimeUnit)
        }
    }

    pub fn session_key_count(&self) -> usize {
        self.session_keys.len()
    }

    /// Every stored key in node, channel and frame order; only `node`'s when given.
    pub fn key_rows(&self, node: Option<NodeId>) -> impl Iterator<Item = KeyRow> + '_ {
        self.session_keys
            .curves
            .iter()
            .filter(move |(id, _)| node.is_none_or(|node| node == **id))
            .flat_map(|(node, curves)| {
                curves.iter().enumerate().flat_map(move |(channel, curve)| {
                    curve.iter().map(move |(frame, value)| KeyRow {
                        node: *node,
                        channel,
                        frame: *frame,
                        value: *value,
                    })
                })
            })
    }

    /// The frames that hold a key on any of `nodes`.
    pub fn key_frames(&self, nodes: &[NodeId]) -> BTreeSet<i32> {
        nodes
            .iter()
            .filter_map(|node| self.session_keys.curves.get(node))
            .flatten()
            .flat_map(BTreeMap::keys)
            .copied()
            .collect()
    }

    /// Keys every channel in `channels` on `nodes` at `frame` with the channel's current value,
    /// then evaluates those channels at `current_frame`, so a key at another frame stays
    /// consistent. Several nodes are all keyed, or none is.
    pub fn set_keys(
        &mut self,
        nodes: &[NodeId],
        frame: i32,
        channels: ChannelMask,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        self.check_time_unit()?;
        check_frame(frame)?;
        check_frame(current_frame)?;
        self.structural_edit()?;
        let mut unique = HashSet::new();
        let mut targets = Vec::new();
        for id in nodes.iter().filter(|id| unique.insert(**id)) {
            let node = self.node(*id)?;
            self.check_effects(node.transform_id)?;
            let values = node.transform().values();
            if values.iter().any(|value| !value.is_finite()) {
                return Err(NativeError::NonFinite);
            }
            targets.push((*id, values));
        }
        let added = targets
            .iter()
            .flat_map(|(id, _)| {
                channels
                    .iter()
                    .enumerate()
                    .filter(|(_, keyed)| **keyed)
                    .map(|(channel, _)| {
                        self.session_keys
                            .curves
                            .get(id)
                            .is_none_or(|curves| !curves[channel].contains_key(&frame))
                    })
            })
            .filter(|new| *new)
            .count();
        if self.session_keys.len().saturating_add(added) > MAX_SESSION_KEYS {
            return Err(NativeError::KeyLimit);
        }
        if !channels.contains(&true) {
            return Ok(());
        }
        let touched: Vec<NodeId> = targets.iter().map(|(id, _)| *id).collect();
        let mut keys = self.session_keys.clone();
        for (id, values) in &targets {
            let curves = keys.curves.entry(*id).or_default();
            for (channel, keyed) in channels.iter().enumerate() {
                if *keyed {
                    curves[channel].insert(frame, values[channel]);
                }
            }
        }
        let writes = self.evaluated_writes_for(&keys, current_frame, Some((&touched, channels)))?;
        self.session_keys = keys;
        self.apply_evaluated(writes);
        Ok(())
    }

    /// Removes the keys of `channels` on `nodes` at `frame`, or at every frame when `frame` is
    /// `None`. A channel left with no key keeps its pose; the others are evaluated at
    /// `current_frame`.
    pub fn delete_keys(
        &mut self,
        nodes: &[NodeId],
        frame: Option<i32>,
        channels: ChannelMask,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        self.check_time_unit()?;
        if let Some(frame) = frame {
            check_frame(frame)?;
        }
        check_frame(current_frame)?;
        self.structural_edit()?;
        for id in nodes {
            self.node(*id)?;
        }
        let mut keys = self.session_keys.clone();
        for id in nodes {
            let Some(curves) = keys.curves.get_mut(id) else {
                continue;
            };
            for (curve, keyed) in curves.iter_mut().zip(channels) {
                match (keyed, frame) {
                    (false, _) => {}
                    (true, Some(frame)) => {
                        curve.remove(&frame);
                    }
                    (true, None) => curve.clear(),
                }
            }
            if curves.iter().all(BTreeMap::is_empty) {
                keys.curves.remove(id);
            }
        }
        keys.drop_unkeyed_infinity();
        // Nothing removed: re-evaluating would overwrite a held edit for no reason.
        if keys == self.session_keys {
            return Ok(());
        }
        let writes = self.evaluated_writes_for(&keys, current_frame, Some((nodes, channels)))?;
        self.session_keys = keys;
        self.apply_evaluated(writes);
        Ok(())
    }

    /// What evaluating every keyed channel at `frame` would write. A channel already at its value
    /// writes nothing.
    pub fn evaluated_writes(&self, frame: i32) -> Result<EvaluatedWrites, NativeError> {
        self.evaluated_writes_for(&self.session_keys, frame, None)
    }

    /// Evaluates `keys`, which a command passes before it installs them, so a refusal leaves the
    /// scene's own table untouched.
    fn evaluated_writes_for(
        &self,
        keys: &SessionKeys,
        frame: i32,
        only: Option<(&[NodeId], ChannelMask)>,
    ) -> Result<EvaluatedWrites, NativeError> {
        check_frame(frame)?;
        let mut writes = Vec::new();
        if keys.is_empty() {
            return Ok(EvaluatedWrites {
                transforms: writes,
                cameras: Vec::new(),
            });
        }
        let only = only.map(|(nodes, mask)| (nodes.iter().copied().collect::<HashSet<_>>(), mask));
        let indices: HashMap<NodeId, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect();
        for (id, curves) in &keys.curves {
            let mask = match &only {
                Some((nodes, _)) if !nodes.contains(id) => continue,
                Some((_, mask)) => *mask,
                None => ALL_CHANNELS,
            };
            let index = *indices.get(id).ok_or(NativeError::InvalidSessionKeys)?;
            let node = &self.nodes[index];
            let mut replacement = node.channels.clone();
            let mut changed = false;
            for (slot, curve) in curves.iter().enumerate() {
                let pair = keys.infinity_of(*id, slot);
                let Some(value) = evaluate_with(curve, frame, pair).filter(|_| mask[slot]) else {
                    continue;
                };
                if !value.is_finite() {
                    return Err(NativeError::NonFinite);
                }
                if value.to_bits() != replacement[slot].value.to_bits() {
                    let field = [
                        TransformField::Translation,
                        TransformField::Rotation,
                        TransformField::Scale,
                    ][slot / 3];
                    replacement[slot] = replacement[slot].replacement(
                        value,
                        self.source.as_ref(),
                        &self.units,
                        node.transform_id,
                        field,
                        slot % 3,
                    )?;
                    changed = true;
                }
            }
            if changed {
                writes.push((index, replacement));
            }
        }
        let mut cameras = Vec::new();
        for camera in super::SceneCamera::ALL {
            let Some(value) = evaluate(&keys.camera_curves[camera.index()], frame) else {
                continue;
            };
            let mut optics = self.camera_optics(camera);
            if optics.focal_length_mm.to_bits() != value.to_bits() {
                optics.focal_length_mm = value;
                super::camera::validate_optics(optics)?;
                cameras.push((camera, optics));
            }
        }
        Ok(EvaluatedWrites {
            transforms: writes,
            cameras,
        })
    }

    /// Applies writes computed from this scene. It cannot fail, so all of them land or none does.
    pub fn apply_evaluated(&mut self, writes: EvaluatedWrites) {
        for (index, channels) in writes.transforms {
            if let Some(node) = self.nodes.get_mut(index) {
                node.channels = channels;
            }
        }
        for (camera, optics) in writes.cameras {
            self.cameras[camera.index()] =
                (optics != crate::camera_optics::CameraOptics::node_default()).then_some(optics);
        }
    }
}

#[cfg(test)]
mod tests;
