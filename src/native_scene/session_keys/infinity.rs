//! Pre- and post-infinity: what a curve does before its first key and after its last.
//!
//! Maya's `setInfinity` page names the five modes but not their formulas, so the behaviour below
//! is PROVISIONAL: `Constant` holds the end value; `Linear` extends the end segment's slope (this
//! editor has no tangents, so that is the slope of the first or last segment, and a single key is
//! flat); `Cycle` repeats the keyed span; `CycleRelative` repeats it with the first-to-last rise
//! added per cycle; `Oscillate` repeats it alternately forwards and backwards. A curve with one key
//! has no span, so every mode holds its value.

use super::{ChannelMask, Curve, NativeError, NativeScene, check_frame, evaluate};
use crate::scene::NodeId;
use std::collections::HashSet;

/// How a curve continues beyond its first or last key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Infinity {
    #[default]
    Constant,
    Linear,
    Cycle,
    CycleRelative,
    Oscillate,
}

impl Infinity {
    pub fn name(self) -> &'static str {
        match self {
            Self::Constant => "constant",
            Self::Linear => "linear",
            Self::Cycle => "cycle",
            Self::CycleRelative => "cycle_relative",
            Self::Oscillate => "oscillate",
        }
    }
}

/// A curve's `(pre, post)` settings.
pub type InfinityPair = (Infinity, Infinity);

/// The value of `curve` at `frame`, honouring `infinity` outside the first and last key.
pub(super) fn evaluate_with(curve: &Curve, frame: i32, infinity: InfinityPair) -> Option<f64> {
    let (&first, &first_value) = curve.iter().next()?;
    let (&last, &last_value) = curve.iter().next_back()?;
    let mode = if frame < first {
        infinity.0
    } else if frame > last {
        infinity.1
    } else {
        return evaluate(curve, frame);
    };
    let span = i64::from(last) - i64::from(first);
    if span == 0 {
        return Some(first_value);
    }
    match mode {
        Infinity::Constant => evaluate(curve, frame),
        Infinity::Linear => {
            let (edge, value, neighbour, neighbour_value) = if frame < first {
                let (&second, &second_value) = curve.iter().nth(1)?;
                (first, first_value, second, second_value)
            } else {
                let (&before, &before_value) = curve.iter().rev().nth(1)?;
                (last, last_value, before, before_value)
            };
            let slope = (neighbour_value - value) / f64::from(neighbour - edge);
            Some(value + slope * (f64::from(frame) - f64::from(edge)))
        }
        Infinity::Cycle | Infinity::CycleRelative | Infinity::Oscillate => {
            let offset = i64::from(frame) - i64::from(first);
            let cycles = offset.div_euclid(span);
            let mut inside = offset.rem_euclid(span);
            if mode == Infinity::Oscillate && cycles.rem_euclid(2) == 1 {
                inside = span - inside;
            }
            let base = evaluate(curve, i32::try_from(i64::from(first) + inside).ok()?)?;
            if mode == Infinity::CycleRelative {
                Some(base + cycles as f64 * (last_value - first_value))
            } else {
                Some(base)
            }
        }
    }
}

impl NativeScene {
    /// The `(pre, post)` settings of one curve; constant on both sides when never set.
    pub fn infinity_of(&self, node: NodeId, channel: usize) -> InfinityPair {
        self.session_keys.infinity_of(node, channel)
    }

    /// Sets the pre and/or post infinity of every keyed curve of `channels` on `nodes`, then
    /// evaluates them at `current_frame`. Every node is set or none is. A named channel without a
    /// key is skipped; no key on any of them is `InfinityNoKeys`.
    pub fn set_infinity(
        &mut self,
        nodes: &[NodeId],
        channels: ChannelMask,
        pre: Option<Infinity>,
        post: Option<Infinity>,
        current_frame: i32,
    ) -> Result<(), NativeError> {
        self.check_time_unit()?;
        check_frame(current_frame)?;
        self.structural_edit()?;
        let mut unique = HashSet::new();
        let mut keys = self.session_keys.clone();
        let mut found = false;
        for id in nodes.iter().filter(|id| unique.insert(**id)) {
            let node = self.node(*id)?;
            self.check_effects(node.transform_id)?;
            let Some(curves) = keys.curves.get(id) else {
                continue;
            };
            let keyed: Vec<usize> = (0..curves.len())
                .filter(|channel| channels[*channel] && !curves[*channel].is_empty())
                .collect();
            for channel in keyed {
                found = true;
                let (old_pre, old_post) = keys.infinity_of(*id, channel);
                keys.set_infinity(
                    *id,
                    channel,
                    (pre.unwrap_or(old_pre), post.unwrap_or(old_post)),
                );
            }
        }
        if !found {
            return Err(NativeError::InfinityNoKeys);
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
