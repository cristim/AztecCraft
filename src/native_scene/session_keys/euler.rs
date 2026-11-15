//! Euler filter: removes whole-turn jumps between neighbouring rotation keys.
//!
//! Each selected rotation curve is unrolled on its own. A key moves by a whole number of turns
//! until it lies within half a turn of the previous key, so the orientation at every key is
//! unchanged. Maya's filter may also swap an equivalent triple (x + 180, 180 - y, z + 180); this
//! one never does (PROVISIONAL, the 2027 `filterCurve` page does not state its algorithm).

use super::{ChannelMask, NativeError, NativeScene, check_frame};
use crate::native_scene::AngularUnit;
use crate::scene::NodeId;
use std::collections::HashSet;

/// Index range of the rotation channels in `KEY_CHANNELS`.
const ROTATION: std::ops::Range<usize> = 3..6;

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

/// Whole turns to add to `value` so it lies within half a turn of `previous`. A distance of
/// exactly half a turn stays, so filtering twice changes nothing more.
fn turns_to_add(previous: f64, value: f64, turn: f64) -> f64 {
    let ratio = (previous - value) / turn;
    if ratio >= 0.0 {
        (ratio - 0.5).ceil()
    } else {
        (ratio + 0.5).floor()
    }
}

impl NativeScene {
    /// Unrolls the keyed rotation curves of `channels` on `nodes`, then evaluates them at
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
            let Some(curves) = keys.curves.get_mut(id) else {
                continue;
            };
            for slot in ROTATION.filter(|slot| channels[*slot]) {
                let mut previous = None;
                for value in curves[slot].values_mut() {
                    found = true;
                    if let Some(before) = previous {
                        let shifted = *value + turns_to_add(before, *value, turn) * turn;
                        if !shifted.is_finite() {
                            return Err(NativeError::NonFinite);
                        }
                        *value = shifted;
                    }
                    previous = Some(*value);
                }
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
