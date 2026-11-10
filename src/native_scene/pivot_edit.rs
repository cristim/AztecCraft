//! Pivot edits (#39 slice B). `M x = t + rt + rp + Rs (K (x - sp) + sp + st - rp)` with
//! `Rs = R(ro) * RA` and `K = SH * S`, so moving rp by `d` keeps the matrix when
//! `rt' = rt - (I - Rs) d`, and moving sp by `d` keeps it when `st' = st - (I - K) d`. PROVISIONAL:
//! the compensation goes to rpt and spt as `xform -preserve` documents; Maya is not compared (#5).
use super::source::{SourceFormat, Storage, TransformField};
use super::{
    MAX_TRANSFORM_DEPTH, MAX_TRANSFORM_PATHS, NativeError, NativeId, NativeScene,
    NativeTransformNode, Scalar, TransformExtras,
};
use crate::scene::{NodeId, SceneError};
use glam::{DMat3, DMat4, DVec3};

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
    ) -> Result<(), NativeError> {
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
        let next = self.pivoted(index, rotate, scale, preserve)?;
        self.install(&[(index, next)])
    }

    /// The node at `index` with its pivots set; `set_pivots` without the checks of the scene.
    fn pivoted(
        &self,
        index: usize,
        rotate: Option<[f64; 3]>,
        scale: Option<[f64; 3]>,
        preserve: bool,
    ) -> Result<NativeTransformNode, NativeError> {
        let node = &self.nodes[index];
        let internal = self.internal_transform(node.id)?;
        let rotation = internal
            .rotation_frame()
            .ok_or(NativeError::InvalidTransformExtras)?;
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
                slots[component] = slots[component]
                    .replacement(
                        value,
                        self.source.as_ref(),
                        &self.units,
                        node.transform_id,
                        field,
                        component,
                    )
                    .map_err(absent_pivot_leaf(field))?;
                // From the stored word: an unedited leaf keeps its original word, so a unit
                // round trip of the declared value must not read as a change.
                delta[component] = slots[component].internal(
                    self.source.as_ref(),
                    &self.units,
                    node.transform_id,
                    field,
                    component,
                )? - old_cm[component];
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
        Ok(next)
    }

    /// Zero Pivot over each of `ids` independently: rp, sp, rpt and spt become zero and the node's
    /// translate absorbs `v = rt + (I - Rs) rp + Rs ((I - K) sp + st)`, so the matrix is kept.
    /// PROVISIONAL: follows `xform -zeroTransformPivots`. Nothing is mutated on error.
    pub fn zero_pivots(&mut self, ids: &[NodeId]) -> Result<(), NativeError> {
        let mut writes = Vec::new();
        for index in self.pivot_targets(ids)? {
            if let Some(next) = self.zeroed(index)? {
                writes.push((index, next));
            }
        }
        self.install(&writes)
    }

    /// Center Pivot over each of `ids` independently: rp and sp become the centre of the
    /// node's `hierarchy_bounds_local` and the matrix is kept like `set_pivots` with preserve.
    /// A node with no geometry is left as it is. PROVISIONAL (P-B1, P-B9). Nothing is mutated on
    /// error.
    fn center_pivots(&mut self, ids: &[NodeId]) -> Result<(), NativeError> {
        let mut writes = Vec::new();
        for index in self.pivot_targets(ids)? {
            let Some((low, high)) = self.hierarchy_bounds_local(self.nodes[index].id)? else {
                continue;
            };
            let centre = low * 0.5 + high * 0.5;
            let mut declared = [0.0; 3];
            for (slot, component) in declared.iter_mut().zip(centre.to_array()) {
                *slot = Storage::Centimeters.declared(component, &self.units)?;
            }
            writes.push((
                index,
                self.pivoted(index, Some(declared), Some(declared), true)?,
            ));
        }
        self.install(&writes)
    }

    /// Modify > Center Pivot or Zero Pivot over each of `ids`.
    pub fn reset_pivots(&mut self, ids: &[NodeId], mode: PivotReset) -> Result<(), NativeError> {
        match mode {
            PivotReset::Center => self.center_pivots(ids),
            PivotReset::Zero => self.zero_pivots(ids),
        }
    }

    /// The node indexes of `ids` once the scene allows pivot edits: no duplicate ids, every node
    /// editable and not keyed.
    fn pivot_targets(&self, ids: &[NodeId]) -> Result<Vec<usize>, NativeError> {
        let mut seen = std::collections::HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        self.authoring_preflight()?;
        self.validate()?;
        ids.iter()
            .map(|id| {
                let index = self.editable_node(*id)?;
                self.refuse_keyed(*id)?;
                Ok(index)
            })
            .collect()
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
                    *slot = slot
                        .replacement(0.0, source, units, node.transform_id, field, component)
                        .map_err(absent_pivot_leaf(field))?;
                }
            }
        }
        next.extras.rotate_pivot_translate = None;
        next.extras.scale_pivot_translate = None;
        Ok(Some(next))
    }

    /// The bounding box, in cm and in the object space of `id`, of the mesh points and curve
    /// control points of its direct shapes and of every descendant's, each mapped through the
    /// local matrices below `id` (`id`'s own matrix and its ancestors are not applied). Shapes
    /// proved invisible, directly or through a descendant transform, are left out; geometry whose
    /// visibility is unproved is `UnsupportedView`. `None` when no geometry is included.
    /// PROVISIONAL: members and space follow `xform -bb` and the Center Pivot page (known-issues).
    pub(crate) fn hierarchy_bounds_local(
        &self,
        id: NodeId,
    ) -> Result<Option<(DVec3, DVec3)>, NativeError> {
        let index = self.dag_index()?;
        let shapes: std::collections::HashMap<NativeId, _> = self
            .shapes
            .iter()
            .map(|s| (s.id, (&s.view, GeometryPoints::Mesh(&s.mesh))))
            .chain(
                self.curves
                    .iter()
                    .map(|c| (c.id, (&c.view, GeometryPoints::Curve(&c.control_vertices)))),
            )
            .collect();
        let mut bounds: Option<(DVec3, DVec3)> = None;
        let mut visited = 0usize;
        let mut pending = vec![(
            self.node(id)?.transform_id,
            DMat4::IDENTITY,
            Ok(true),
            0usize,
        )];
        while let Some((parent, to_root, visible, depth)) = pending.pop() {
            for &child in index.children(Some(parent)) {
                if let Some(node_id) = index.node(child) {
                    visited += 1;
                    if visited > MAX_TRANSFORM_PATHS || depth >= MAX_TRANSFORM_DEPTH {
                        return Err(NativeError::ResourceLimit);
                    }
                    let node = self.node(node_id)?;
                    let local = self
                        .internal_transform(node_id)?
                        .matrix()
                        .ok_or(NativeError::NonFinite)?;
                    let visible = visible.clone().and_then(|v| {
                        if node.unsupported_transform.is_empty() {
                            Ok(v & node.view.visible()?)
                        } else {
                            Err(NativeError::UnsupportedView("unproved transform effects"))
                        }
                    });
                    pending.push((child, to_root * local, visible, depth + 1));
                } else if let Some((view, points)) = shapes.get(&child) {
                    if !visible.clone()? || !view.visible()? {
                        continue;
                    }
                    points.for_each(|p| {
                        let world = to_root.transform_point3(p);
                        if !world.is_finite() {
                            return Err(NativeError::NonFinite);
                        }
                        let (lo, hi) = bounds.get_or_insert((world, world));
                        *lo = lo.min(world);
                        *hi = hi.max(world);
                        Ok(())
                    })?;
                }
            }
        }
        Ok(bounds)
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
    fn install(&mut self, writes: &[(usize, NativeTransformNode)]) -> Result<(), NativeError> {
        let changed: Vec<_> = writes
            .iter()
            .filter(|(index, next)| self.nodes[*index] != *next)
            .collect();
        if changed.is_empty() {
            return Ok(());
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
            return Ok(());
        }
        for (index, node) in previous {
            self.nodes[index] = node;
        }
        Err(NativeError::NonFinite)
    }
}

/// The pivot of a node, as named by a refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotLeaf {
    Rotate,
    Scale,
}

impl PivotLeaf {
    pub fn name(self) -> &'static str {
        match self {
            Self::Rotate => "rotate",
            Self::Scale => "scale",
        }
    }
}

/// Names the pivot when `replacement` refuses to insert its absent leaf. Only the rp and sp
/// `replacement` calls use it: restricted nodes are refused earlier, in `editable`.
fn absent_pivot_leaf(field: TransformField) -> impl Fn(NativeError) -> NativeError {
    move |error| match (error, field) {
        (NativeError::UnsupportedEdit, TransformField::RotatePivot) => {
            NativeError::AbsentPivotLeaf {
                field: PivotLeaf::Rotate,
            }
        }
        (NativeError::UnsupportedEdit, TransformField::ScalePivot) => {
            NativeError::AbsentPivotLeaf {
                field: PivotLeaf::Scale,
            }
        }
        (error, _) => error,
    }
}

/// Which pivots `reset_pivots` resets: the Modify menu's Center Pivot and Zero Pivot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotReset {
    Center,
    Zero,
}

/// The point source of one shape or curve.
enum GeometryPoints<'a> {
    Mesh(&'a super::NativeMesh),
    Curve(&'a [[f64; 3]]),
}

impl GeometryPoints<'_> {
    fn for_each(
        &self,
        mut f: impl FnMut(DVec3) -> Result<(), NativeError>,
    ) -> Result<(), NativeError> {
        match self {
            Self::Mesh(mesh) => (0..mesh.point_count())
                .try_for_each(|i| f(DVec3::from_array(mesh.effective_point_cm(i)?))),
            Self::Curve(points) => points.iter().try_for_each(|p| f(DVec3::from_array(*p))),
        }
    }
}

#[cfg(test)]
mod tests;
