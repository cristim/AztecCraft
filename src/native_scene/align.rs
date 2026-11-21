//! Modify > Snap Align Objects > Align Objects (#313): moves the selected objects along the world
//! axes so their world-space bounding boxes line up. Only translate channels are written.
//!
//! Sourced from the Maya 2027 Align Objects pages: the modes are Min (side closest to 0), Mid
//! (centres), Max (side farthest from 0) and Dist (even spread over the total distance), they
//! work on the objects' bounding boxes, axes are chosen in world space, and Align To is Selection
//! Average or Last Selected Object (Min, Mid and Max only). PROVISIONAL (#5), because the pages
//! omit them: the box is the axis-aligned world box of every mesh point below the object, Dist
//! keeps the outermost objects (by box centre) fixed and equalises the gaps between the boxes,
//! Stack is unbuilt, and the defaults are this editor's choice.
use super::transform_ops::keeps_volume;
use super::{
    DagEdge, NativeError, NativeId, NativeScene, Points, TransformRefusal, TransformTarget, Units,
};
use crate::scene::{NodeId, SceneError};
use glam::DVec3;
use std::collections::HashSet;

/// What one world axis does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignMode {
    /// The axis is left alone.
    None,
    Min,
    Mid,
    Max,
    Dist,
}

/// The reference Min, Mid and Max align to (ignored by Dist).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignTo {
    SelectionAverage,
    LastSelected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AlignOptions {
    /// The mode of the world X, Y and Z axes.
    pub modes: [AlignMode; 3],
    pub to: AlignTo,
}

/// Relative tolerance for accepting a rewritten node as the world position it was meant to take.
const EXACT_TOLERANCE: f64 = 1e-9;

struct Subject {
    id: NodeId,
    index: usize,
    path: Vec<NativeId>,
    min: DVec3,
    max: DVec3,
}

impl NativeScene {
    /// Aligns `ids` (the last is the reference for `AlignTo::LastSelected`). All or nothing: the
    /// work runs on a copy.
    pub fn align_objects(
        &mut self,
        ids: &[NodeId],
        options: AlignOptions,
    ) -> Result<(), NativeError> {
        let mut candidate = self.clone();
        candidate.align_in_place(ids, options)?;
        candidate.validate()?;
        candidate.transform_paths()?;
        *self = candidate;
        Ok(())
    }

    fn align_in_place(&mut self, ids: &[NodeId], options: AlignOptions) -> Result<(), NativeError> {
        self.structural_edit()?;
        if self.units != Units::default() {
            return Err(NativeError::UnsupportedEdit);
        }
        if options.modes.iter().all(|mode| *mode == AlignMode::None) {
            return Err(TransformRefusal::EmptySet.into());
        }
        let mut seen = HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        if ids.len() < 2 {
            return Err(TransformRefusal::AlignTooFew.into());
        }
        let targets = self.transform_paths()?;
        let mut subjects = Vec::with_capacity(ids.len());
        for id in ids {
            let index = self.editable_node(*id)?;
            self.refuse_plain_target(index, true)?;
            let path = self.sole_target(*id)?.path;
            let (min, max) = self.world_box(&path, &targets)?;
            subjects.push(Subject {
                id: *id,
                index,
                path,
                min,
                max,
            });
        }
        for a in &subjects {
            let own = self.nodes[a.index].transform_id;
            if subjects
                .iter()
                .any(|b| b.index != a.index && b.path.contains(&own))
            {
                return Err(TransformRefusal::Nested.into());
            }
        }
        let mut deltas = vec![DVec3::ZERO; subjects.len()];
        for (axis, mode) in options.modes.iter().enumerate() {
            let per = axis_deltas(&subjects, axis, *mode, options.to);
            for (delta, value) in deltas.iter_mut().zip(per) {
                delta[axis] = value;
            }
        }
        for (subject, delta) in subjects.iter().zip(deltas) {
            if delta != DVec3::ZERO {
                self.translate_world(subject, delta)?;
            }
        }
        Ok(())
    }

    /// The world axis-aligned box of every mesh point at or below `path`.
    fn world_box(
        &self,
        path: &[NativeId],
        targets: &[TransformTarget],
    ) -> Result<(DVec3, DVec3), NativeError> {
        let (mut min, mut max) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for target in targets.iter().filter(|t| t.path.starts_with(path)) {
            let Some(&last) = target.path.last() else {
                continue;
            };
            let world = self.path_world(&target.path)?;
            for DagEdge { child, .. } in self.dag.iter().filter(|e| e.parent == Some(last)) {
                if self.curves.iter().any(|c| c.id == *child) {
                    return Err(TransformRefusal::Curve.into());
                }
                let Some(shape) = self.shapes.iter().find(|s| s.id == *child) else {
                    continue;
                };
                if shape.mesh.has_point_offsets() {
                    return Err(NativeError::UnsupportedEdit);
                }
                let mut include = |p: [f64; 3]| {
                    let w = world.transform_point3(DVec3::from_array(p));
                    min = min.min(w);
                    max = max.max(w);
                };
                match shape.mesh.points() {
                    Points::Float3(points) => points.iter().for_each(|p| include(p.map(f64::from))),
                    Points::Double3(points) => points.iter().for_each(|p| include(*p)),
                }
            }
        }
        if min.x > max.x {
            return Err(TransformRefusal::NoGeometry.into());
        }
        if !(min.is_finite() && max.is_finite()) {
            return Err(NativeError::NonFinite);
        }
        Ok((min, max))
    }

    /// Moves the node so its world position shifts by `delta`, writing only its translate.
    fn translate_world(&mut self, subject: &Subject, delta: DVec3) -> Result<(), NativeError> {
        let parent = self.path_world(&subject.path[..subject.path.len() - 1])?;
        if !keeps_volume(parent) {
            return Err(TransformRefusal::Degenerate.into());
        }
        let before = self.path_world(&subject.path)?;
        let local = DVec3::from_array(self.internal_transform(subject.id)?.translation_cm)
            + parent.inverse().transform_vector3(delta);
        if !local.is_finite() {
            return Err(NativeError::NonFinite);
        }
        let mut values = [None; 9];
        for (slot, value) in values.iter_mut().zip(local.to_array()) {
            *slot = Some(value);
        }
        self.nodes[subject.index].channels = self.replacement_channels(subject.index, values)?;
        let after = self.path_world(&subject.path)?;
        let want = before.w_axis.truncate() + delta;
        let reach = want
            .abs()
            .max_element()
            .max(delta.abs().max_element())
            .max(1.0);
        if (after.w_axis.truncate() - want).abs().max_element() > EXACT_TOLERANCE * reach
            || after.x_axis != before.x_axis
            || after.y_axis != before.y_axis
            || after.z_axis != before.z_axis
        {
            return Err(TransformRefusal::NotRepresentable.into());
        }
        Ok(())
    }
}

/// The world movement along `axis` of each subject for `mode`.
fn axis_deltas(subjects: &[Subject], axis: usize, mode: AlignMode, to: AlignTo) -> Vec<f64> {
    let mut deltas = vec![0.0; subjects.len()];
    let lo = |s: &Subject| s.min[axis];
    let hi = |s: &Subject| s.max[axis];
    let mid = |s: &Subject| 0.5 * (s.min[axis] + s.max[axis]);
    let pick: fn(&Subject, usize) -> f64 = match mode {
        AlignMode::None => return deltas,
        AlignMode::Min => |s, a| s.min[a],
        AlignMode::Max => |s, a| s.max[a],
        AlignMode::Mid => |s, a| 0.5 * (s.min[a] + s.max[a]),
        AlignMode::Dist => {
            let mut order: Vec<usize> = (0..subjects.len()).collect();
            order.sort_by(|a, b| {
                mid(&subjects[*a])
                    .total_cmp(&mid(&subjects[*b]))
                    .then(a.cmp(b))
            });
            let (first, last) = (order[0], order[order.len() - 1]);
            let inner = &order[1..order.len() - 1];
            let sizes: f64 = inner
                .iter()
                .map(|i| hi(&subjects[*i]) - lo(&subjects[*i]))
                .sum();
            let span = lo(&subjects[last]) - hi(&subjects[first]);
            let gap = (span - sizes) / (order.len() - 1) as f64;
            let mut cursor = hi(&subjects[first]);
            for i in inner {
                let start = cursor + gap;
                deltas[*i] = start - lo(&subjects[*i]);
                cursor = start + (hi(&subjects[*i]) - lo(&subjects[*i]));
            }
            return deltas;
        }
    };
    let reference = match to {
        AlignTo::SelectionAverage => {
            subjects.iter().map(|s| pick(s, axis)).sum::<f64>() / subjects.len() as f64
        }
        AlignTo::LastSelected => pick(&subjects[subjects.len() - 1], axis),
    };
    for (delta, subject) in deltas.iter_mut().zip(subjects) {
        *delta = reference - pick(subject, axis);
    }
    deltas
}
