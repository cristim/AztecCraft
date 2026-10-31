//! Session-only keys on the nine transform channels of authored nodes.
//!
//! The table is not saved, so writers refuse a scene that holds any. Keys sit on whole frames.
//! Evaluation interpolates linearly between two keys and holds outside them (PROVISIONAL: Maya's
//! default tangents are Auto, and tangent types are not implemented). A key frame returns the
//! exact stored value. Sourced scenes refuse keys like every other structural edit, and their own
//! animation curves are not evaluated.

use super::{NativeError, NativeScene, NativeTransformNode, Scalar, TransformField};
use crate::scene::NodeId;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Bound on keys per scene, so undo snapshots and a playback tick stay small.
pub const MAX_SESSION_KEYS: usize = 1 << 16;
/// Bound on frames, an AztecCraft choice. It keeps every frame inside `i32` arithmetic.
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
        self.len() == other.len() && bits(self) == bits(other)
    }
}

impl SessionKeys {
    pub(super) fn is_empty(&self) -> bool {
        self.curves.is_empty()
    }

    fn len(&self) -> usize {
        self.curves.values().flatten().map(BTreeMap::len).sum()
    }

    pub(super) fn validate(
        &self,
        nodes: &[NativeTransformNode],
        sourced: bool,
    ) -> Result<(), NativeError> {
        if self.curves.is_empty() {
            return Ok(());
        }
        if sourced || self.len() > MAX_SESSION_KEYS {
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
    }
}

/// Channel writes computed before any is applied, so a refused evaluation writes nothing.
pub struct EvaluatedWrites(Vec<(usize, [Scalar; 9])>);

impl EvaluatedWrites {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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

    /// True for a scene read from a file, whose animation this table does not hold.
    pub fn has_source(&self) -> bool {
        self.source.is_some()
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
        for (id, values) in &targets {
            let curves = self.session_keys.curves.entry(*id).or_default();
            for (channel, keyed) in channels.iter().enumerate() {
                if *keyed {
                    curves[channel].insert(frame, values[channel]);
                }
            }
        }
        let touched: Vec<NodeId> = targets.iter().map(|(id, _)| *id).collect();
        let writes = self.evaluated_writes_for(current_frame, Some((&touched, channels)))?;
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
        self.structural_edit()?;
        for id in nodes {
            self.node(*id)?;
        }
        for id in nodes {
            let Some(curves) = self.session_keys.curves.get_mut(id) else {
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
                self.session_keys.curves.remove(id);
            }
        }
        let writes = self.evaluated_writes_for(current_frame, Some((nodes, channels)))?;
        self.apply_evaluated(writes);
        Ok(())
    }

    /// What evaluating every keyed channel at `frame` would write. A channel already at its value
    /// writes nothing.
    pub fn evaluated_writes(&self, frame: i32) -> Result<EvaluatedWrites, NativeError> {
        self.evaluated_writes_for(frame, None)
    }

    fn evaluated_writes_for(
        &self,
        frame: i32,
        only: Option<(&[NodeId], ChannelMask)>,
    ) -> Result<EvaluatedWrites, NativeError> {
        let mut writes = Vec::new();
        if self.session_keys.is_empty() {
            return Ok(EvaluatedWrites(writes));
        }
        check_frame(frame)?;
        let indices: HashMap<NodeId, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect();
        for (id, curves) in &self.session_keys.curves {
            let mask = match only {
                Some((nodes, _)) if !nodes.contains(id) => continue,
                Some((_, mask)) => mask,
                None => ALL_CHANNELS,
            };
            let index = *indices.get(id).ok_or(NativeError::InvalidSessionKeys)?;
            let node = &self.nodes[index];
            let mut replacement = node.channels.clone();
            let mut changed = false;
            for (slot, curve) in curves.iter().enumerate() {
                let Some(value) = evaluate(curve, frame).filter(|_| mask[slot]) else {
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
        Ok(EvaluatedWrites(writes))
    }

    /// Applies writes computed from this scene. It cannot fail, so all of them land or none does.
    pub fn apply_evaluated(&mut self, writes: EvaluatedWrites) {
        for (index, channels) in writes.0 {
            if let Some(node) = self.nodes.get_mut(index) {
                node.channels = channels;
            }
        }
    }
}

#[cfg(test)]
mod tests;
