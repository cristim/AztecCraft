//! Euler filter: removes whole-turn jumps and equivalent-triple flips between neighbouring
//! rotation keys, without changing the orientation at any key.
//!
//! When a node's three rotation curves are all selected and share their key frames, each key is
//! taken in two equivalent forms: its own triple, and the triple (a + 180, 180 - b, c + 180) where
//! b is the middle axis of the node's rotate order. Each form is then moved by whole turns per
//! axis to lie nearest the previous (already filtered) key, and the form with the smaller total
//! distance wins; a tie keeps the key's own form. Otherwise each selected curve is unrolled on
//! its own. Maya's 2027 `filterCurve` page states no algorithm, so the choice of forms, the
//! distance measure and the tie rule are PROVISIONAL.

use super::{ChannelMask, NativeError, NativeScene, check_frame};
use crate::native_scene::AngularUnit;
use crate::native_scene::transform_extras::order_axes;
use crate::scene::NodeId;
use std::collections::HashSet;

/// Index range of the rotation channels in `KEY_CHANNELS`.
const ROTATION: std::ops::Range<usize> = 3..6;
/// Largest key magnitude, in whole turns, the filter accepts. Beyond it a shift by turns loses
/// more precision than a key may lose (PROVISIONAL bound).
const MAX_TURNS: f64 = 1.0e6;

/// Refuses a mask that names anything but rotation channels, or none.
pub fn check_euler_channels(channels: ChannelMask) -> Result<(), NativeError> {
    if channels
        .iter()
        .enumerate()
        .any(|(slot, on)| *on && !ROTATION.contains(&slot))
    {
        return Err(NativeError::EulerNotRotation);
    }
    if !channels.contains(&true) {
        return Err(NativeError::EulerNoRotationKeys);
    }
    Ok(())
}

/// `value` moved by whole turns nearer `previous`, or unchanged unless that is strictly nearer,
/// so a second pass never moves it again.
fn nearest(previous: f64, value: f64, turn: f64) -> f64 {
    let shifted = value + ((previous - value) / turn).round() * turn;
    if (previous - shifted).abs() < (previous - value).abs() {
        shifted
    } else {
        value
    }
}

fn distance(previous: [f64; 3], triple: [f64; 3]) -> f64 {
    (0..3)
        .map(|axis| (previous[axis] - triple[axis]).abs())
        .sum()
}

/// The form of `triple` nearest `previous`: its own, or the equivalent flipped one.
fn next_triple(previous: [f64; 3], triple: [f64; 3], axes: [usize; 3], turn: f64) -> [f64; 3] {
    let near = |triple: [f64; 3]| {
        let mut out = triple;
        for axis in 0..3 {
            out[axis] = nearest(previous[axis], triple[axis], turn);
        }
        out
    };
    let own = near(triple);
    let half = turn / 2.0;
    let mut flipped = triple;
    flipped[axes[0]] += half;
    flipped[axes[1]] = half - flipped[axes[1]];
    flipped[axes[2]] += half;
    let flipped = near(flipped);
    if distance(previous, flipped) < distance(previous, own) {
        flipped
    } else {
        own
    }
}

impl NativeScene {
    /// Filters the keyed rotation curves of `channels` on `nodes`, then evaluates them at
    /// `current_frame`. Every node is filtered or none is. A node without a key on a named
    /// channel is skipped; no key on any of them is `EulerNoRotationKeys`.
    pub fn euler_filter(
        &mut self,
        nodes: &[NodeId],
        channels: ChannelMask,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        check_euler_channels(channels)?;
        self.check_time_unit()?;
        check_frame(current_frame)?;
        self.structural_edit()?;
        let turn = match self.units.angular {
            AngularUnit::Degree => 360.0,
            AngularUnit::Radian => std::f64::consts::TAU,
            AngularUnit::Unsupported(_) => {
                return Err(NativeError::UnsupportedView("angular unit"));
            }
        };
        let mut unique = HashSet::new();
        let mut keys = self.session_keys.clone();
        let mut found = false;
        for id in nodes.iter().filter(|id| unique.insert(**id)) {
            let node = self.node(*id)?;
            self.check_effects(node.transform_id)?;
            let axes = order_axes(node.extras.rotate_order.unwrap_or(0))
                .ok_or(NativeError::UnsupportedView("rotate order"))?;
            let Some(curves) = keys.curves.get_mut(id) else {
                continue;
            };
            let selected: Vec<usize> = ROTATION.filter(|slot| channels[*slot]).collect();
            if selected
                .iter()
                .flat_map(|slot| curves[*slot].values())
                .any(|value| value.abs() > MAX_TURNS * turn)
            {
                return Err(NativeError::EulerValueTooLarge);
            }
            found |= selected.iter().any(|slot| !curves[*slot].is_empty());
            let frames: Vec<i32> = curves[ROTATION.start].keys().copied().collect();
            let shared = selected.len() == 3
                && ROTATION
                    .clone()
                    .all(|slot| curves[slot].keys().copied().eq(frames.iter().copied()));
            if shared {
                let mut previous: Option<[f64; 3]> = None;
                for frame in &frames {
                    let own = [0, 1, 2].map(|axis| curves[ROTATION.start + axis][frame]);
                    let next = previous.map_or(own, |before| next_triple(before, own, axes, turn));
                    for axis in 0..3 {
                        curves[ROTATION.start + axis].insert(*frame, next[axis]);
                    }
                    previous = Some(next);
                }
            } else {
                for slot in selected {
                    let mut previous = None;
                    for value in curves[slot].values_mut() {
                        if let Some(before) = previous {
                            *value = nearest(before, *value, turn);
                        }
                        previous = Some(*value);
                    }
                }
            }
            if curves.iter().flatten().any(|(_, value)| !value.is_finite()) {
                return Err(NativeError::NonFinite);
            }
        }
        if !found {
            return Err(NativeError::EulerNoRotationKeys);
        }
        if keys == self.session_keys {
            return Ok(());
        }
        let writes = self.evaluated_writes_for(&keys, current_frame, Some((nodes, channels)))?;
        self.session_keys = keys;
        self.apply_evaluated(writes);
        Ok(())
    }
}
