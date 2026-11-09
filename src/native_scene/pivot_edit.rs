//! Pivot edits (#39 slice B). `M x = t + rt + rp + Rs (K (x - sp) + sp + st - rp)` with
//! `Rs = R(ro) * RA` and `K = SH * S`, so moving rp by `d` keeps the matrix when
//! `rt' = rt - (I - Rs) d`, and moving sp by `d` keeps it when `st' = st - (I - K) d`. PROVISIONAL:
//! the compensation goes to rpt and spt as `xform -preserve` documents; Maya is not compared (#5).
use super::source::{SourceFormat, Storage, TransformField};
use super::{NativeError, NativeScene, NativeTransformNode, Scalar, TransformExtras};
use crate::scene::{NodeId, SceneError};
use glam::{DMat3, DVec3};

/// Whether a pivot operation changed the scene, so callers record an undo entry only for `Changed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotOutcome {
    Changed,
    Unchanged,
}

/// `d - M d`, exactly zero where `M` fixes `d` component-wise.
fn residual(m: DMat3, d: DVec3) -> DVec3 {
    d - m * d
}

/// `old - v` in declared units, touching only components with `v_i != 0.0` so a `-0.0` stays
/// `-0.0`. All-zero `v` returns `old` untouched; an all-zero result is stored as `None` because
/// the authored writers treat `Some([0.0; 3])` as a non-default extra.
fn shifted(
    old: Option<[f64; 3]>,
    v: DVec3,
    units: &super::Units,
) -> Result<Option<[f64; 3]>, NativeError> {
    if v == DVec3::ZERO {
        return Ok(old);
    }
    let mut out = old.unwrap_or_default();
    for (slot, component) in out.iter_mut().zip(v.to_array()) {
        if component != 0.0 {
            *slot += Storage::Centimeters.declared(-component, units)?;
            if !slot.is_finite() {
                return Err(NativeError::NonFinite);
            }
        }
    }
    Ok((!out.iter().all(|c| *c == 0.0)).then_some(out))
}

impl NativeScene {
    /// Sets the rotate and scale pivots of `id` in declared linear units and keeps the node's
    /// matrix by adjusting rpt and spt when `preserve`; otherwise only rp and sp change and the
    /// matrix moves like a plain attribute write. Nothing is mutated on error.
    pub fn set_pivots(
        &mut self,
        id: NodeId,
        rotate: Option<[f64; 3]>,
        scale: Option<[f64; 3]>,
        preserve: bool,
    ) -> Result<PivotOutcome, NativeError> {
        if rotate
            .iter()
            .chain(&scale)
            .flatten()
            .any(|v| !v.is_finite())
        {
            return Err(NativeError::NonFinite);
        }
        let index = self.editable(id)?;
        self.refuse_keyed(id)?;
        let internal = self.internal_transform(id)?;
        let rotation = internal
            .rotation_frame()
            .ok_or(NativeError::InvalidTransformExtras)?;
        let node = &self.nodes[index];
        let mut next = node.clone();
        let mut change = |target: Option<[f64; 3]>,
                          field: TransformField,
                          old_cm: [f64; 3]|
         -> Result<DVec3, NativeError> {
            let mut delta = DVec3::ZERO;
            let Some(target) = target else {
                return Ok(delta);
            };
            let slots = match field {
                TransformField::RotatePivot => &mut next.rotate_pivot,
                _ => &mut next.scale_pivot,
            };
            for (component, value) in target.into_iter().enumerate() {
                delta[component] =
                    Storage::Centimeters.internal(value, &self.units)? - old_cm[component];
                slots[component] = slots[component].replacement(
                    value,
                    self.source.as_ref(),
                    &self.units,
                    node.transform_id,
                    field,
                    component,
                )?;
            }
            Ok(delta)
        };
        let d_rp = change(
            rotate,
            TransformField::RotatePivot,
            internal.rotate_pivot_cm,
        )?;
        let d_sp = change(scale, TransformField::ScalePivot, internal.scale_pivot_cm)?;
        if preserve {
            let v_rp = residual(rotation, d_rp);
            let v_sp = residual(internal.scale_stage(), d_sp);
            if !v_rp.is_finite() || !v_sp.is_finite() {
                return Err(NativeError::NonFinite);
            }
            let binary = self
                .source
                .as_ref()
                .is_some_and(|source| source.format == SourceFormat::Binary);
            if binary && (v_rp != DVec3::ZERO || v_sp != DVec3::ZERO) {
                return Err(NativeError::PivotNeedsTranslateCorrection);
            }
            next.extras = TransformExtras {
                rotate_pivot_translate: shifted(
                    node.extras.rotate_pivot_translate,
                    v_rp,
                    &self.units,
                )?,
                scale_pivot_translate: shifted(
                    node.extras.scale_pivot_translate,
                    v_sp,
                    &self.units,
                )?,
                ..node.extras.clone()
            };
        }
        self.install(&[(index, next)])
    }

    /// Zero Pivot over each of `ids` independently: rp, sp, rpt and spt become zero and the node's
    /// translate absorbs `v = rt + (I - Rs) rp + Rs ((I - K) sp + st)`, so the matrix is kept.
    /// PROVISIONAL: follows `xform -zeroTransformPivots`. Nothing is mutated on error.
    pub fn zero_pivots(&mut self, ids: &[NodeId]) -> Result<PivotOutcome, NativeError> {
        let mut seen = std::collections::HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        self.authoring_preflight()?;
        self.validate()?;
        let mut writes = Vec::new();
        for id in ids {
            let index = self.editable_node(*id)?;
            self.refuse_keyed(*id)?;
            if let Some(next) = self.zeroed(index)? {
                writes.push((index, next));
            }
        }
        self.install(&writes)
    }

    /// The node with its pivots zeroed, or `None` when every pivot value is already +-0. Only the
    /// translate components with a nonzero `v` and the nonzero pivot words are written, so a
    /// `-0.0` elsewhere keeps its bits (a plain `t' = M(0)` would re-round them).
    fn zeroed(&self, index: usize) -> Result<Option<NativeTransformNode>, NativeError> {
        let node = &self.nodes[index];
        let nonzero = |v: &[Scalar; 3]| v.iter().any(|s| s.value != 0.0);
        let translate = |t: Option<[f64; 3]>| t.is_some_and(|v| v.iter().any(|c| *c != 0.0));
        let extras = &node.extras;
        if !nonzero(&node.rotate_pivot)
            && !nonzero(&node.scale_pivot)
            && !translate(extras.rotate_pivot_translate)
            && !translate(extras.scale_pivot_translate)
        {
            return Ok(None);
        }
        let internal = self.internal_transform(node.id)?;
        let rotation = internal
            .rotation_frame()
            .ok_or(NativeError::InvalidTransformExtras)?;
        let (rt, st) = internal.extras.map_or((DVec3::ZERO, DVec3::ZERO), |e| {
            (
                DVec3::from_array(e.rotate_pivot_translate_cm),
                DVec3::from_array(e.scale_pivot_translate_cm),
            )
        });
        let rp = DVec3::from_array(internal.rotate_pivot_cm);
        let sp = DVec3::from_array(internal.scale_pivot_cm);
        let v =
            rt + residual(rotation, rp) + rotation * (residual(internal.scale_stage(), sp) + st);
        if !v.is_finite() {
            return Err(NativeError::NonFinite);
        }
        let mut next = node.clone();
        let (source, units) = (self.source.as_ref(), &self.units);
        for (component, delta) in v.to_array().into_iter().enumerate() {
            if delta != 0.0 {
                let value =
                    node.channels[component].value + Storage::Centimeters.declared(delta, units)?;
                next.channels[component] = node.channels[component].replacement(
                    value,
                    source,
                    units,
                    node.transform_id,
                    TransformField::Translation,
                    component,
                )?;
            }
        }
        for (slots, field) in [
            (&mut next.rotate_pivot, TransformField::RotatePivot),
            (&mut next.scale_pivot, TransformField::ScalePivot),
        ] {
            for (component, slot) in slots.iter_mut().enumerate() {
                if slot.value != 0.0 {
                    *slot =
                        slot.replacement(0.0, source, units, node.transform_id, field, component)?;
                }
            }
        }
        next.extras.rotate_pivot_translate = None;
        next.extras.scale_pivot_translate = None;
        Ok(Some(next))
    }

    /// A keyed channel is rewritten on every frame change and the compensation is valid only at
    /// the current pose, so every pivot edit of a keyed node is refused (PROVISIONAL).
    fn refuse_keyed(&self, id: NodeId) -> Result<(), NativeError> {
        if self.key_rows(Some(id)).next().is_some() {
            return Err(NativeError::PivotOnKeyedNode);
        }
        Ok(())
    }

    /// Replaces the nodes at `writes` after checking each still composes a finite matrix, so an
    /// overflowing edit is a typed error and leaves the scene as it was.
    fn install(
        &mut self,
        writes: &[(usize, NativeTransformNode)],
    ) -> Result<PivotOutcome, NativeError> {
        let changed: Vec<_> = writes
            .iter()
            .filter(|(index, next)| self.nodes[*index] != *next)
            .collect();
        if changed.is_empty() {
            return Ok(PivotOutcome::Unchanged);
        }
        let previous: Vec<_> = changed
            .iter()
            .map(|(index, next)| {
                (
                    *index,
                    std::mem::replace(&mut self.nodes[*index], next.clone()),
                )
            })
            .collect();
        let finite = changed.iter().all(|(index, _)| {
            self.internal_transform(self.nodes[*index].id)
                .is_ok_and(|internal| internal.matrix().is_some())
        });
        if finite {
            return Ok(PivotOutcome::Changed);
        }
        for (index, node) in previous {
            self.nodes[index] = node;
        }
        Err(NativeError::NonFinite)
    }
}

#[cfg(test)]
mod tests;
