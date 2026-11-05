//! Session-only UVs of authored shapes, held as Maya's default `map1` set.
//!
//! The table is not saved, so writers refuse a scene that holds any. One (u, v) belongs to each
//! outer corner of each face, in face order, so faces never share a UV and there are no seams.
//! Sourced scenes refuse the edit like every other structural edit.

use super::{NativeError, NativeId, NativeMesh, NativeMeshShape, NativeScene};
use std::collections::{BTreeMap, HashSet};

/// The default UV set name in Maya; other sets are not supported.
pub const DEFAULT_UV_SET: &str = "map1";

/// The UV menu's Project from axis, as object-space X, Y or Z.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UvProjectAxis {
    X,
    /// PROVISIONAL: the Planar Mapping options page states no default.
    #[default]
    Y,
    Z,
}

impl UvProjectAxis {
    /// The object-space axes that become u and v. PROVISIONAL: Maya's pages do not say which
    /// axis maps to which, or their direction.
    fn plane(self) -> [usize; 2] {
        match self {
            Self::X => [2, 1],
            Self::Y => [0, 2],
            Self::Z => [0, 1],
        }
    }
}

/// One shape's `map1`: the UVs of each face's outer corners.
#[derive(Clone, Debug, PartialEq)]
pub struct UvSet {
    faces: Vec<Vec<[f64; 2]>>,
}

impl UvSet {
    pub fn faces(&self) -> &[Vec<[f64; 2]>] {
        &self.faces
    }
    pub fn corner_count(&self) -> usize {
        self.faces.iter().map(Vec::len).sum()
    }
}

fn planar(mesh: &NativeMesh, axis: UvProjectAxis) -> Result<UvSet, NativeError> {
    let [u_axis, v_axis] = axis.plane();
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for index in 0..mesh.point_count() {
        let point = mesh.effective_point_cm(index)?;
        for (slot, value) in [point[u_axis], point[v_axis]].into_iter().enumerate() {
            low[slot] = low[slot].min(value);
            high[slot] = high[slot].max(value);
        }
    }
    let extent = [high[0] - low[0], high[1] - low[1]];
    if !extent.iter().all(|side| side.is_finite() && *side > 0.0) {
        return Err(NativeError::DegenerateUvProjection);
    }
    let mut faces = Vec::new();
    faces
        .try_reserve_exact(mesh.faces().len())
        .map_err(|_| NativeError::ResourceLimit)?;
    for face in mesh.faces() {
        if !face.holes().is_empty() {
            return Err(NativeError::UnsupportedUvTopology);
        }
        let mut corners = Vec::new();
        corners
            .try_reserve_exact(face.outer().len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for directed in face.outer() {
            let point = mesh.effective_point_cm(mesh.directed_start(*directed)?)?;
            corners.push([
                (point[u_axis] - low[0]) / extent[0],
                (point[v_axis] - low[1]) / extent[1],
            ]);
        }
        faces.push(corners);
    }
    Ok(UvSet { faces })
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SessionUvs {
    sets: BTreeMap<NativeId, UvSet>,
}

impl SessionUvs {
    pub(super) fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    pub(super) fn validate(
        &self,
        shapes: &[NativeMeshShape],
        sourced: bool,
    ) -> Result<(), NativeError> {
        if self.sets.is_empty() {
            return Ok(());
        }
        if sourced {
            return Err(NativeError::InvalidSessionUvs);
        }
        for (id, set) in &self.sets {
            let shape = shapes
                .iter()
                .find(|shape| shape.id == *id)
                .ok_or(NativeError::InvalidSessionUvs)?;
            let faces = shape.mesh.faces();
            if faces.len() != set.faces.len()
                || faces
                    .iter()
                    .zip(&set.faces)
                    .any(|(face, corners)| face.outer().len() != corners.len())
                || set.faces.iter().flatten().flatten().any(|v| !v.is_finite())
            {
                return Err(NativeError::InvalidSessionUvs);
            }
        }
        Ok(())
    }

    pub(super) fn prune(&mut self, shapes: &[NativeMeshShape]) {
        let known: HashSet<NativeId> = shapes.iter().map(|shape| shape.id).collect();
        self.sets.retain(|shape, _| known.contains(shape));
    }
}

impl NativeScene {
    /// True for a scene read from a file, whose UVs this table does not hold.
    pub fn has_source(&self) -> bool {
        self.source.is_some()
    }

    /// This shape's `map1`, or `None` when it has no UVs.
    pub fn uv_set(&self, shape: NativeId) -> Option<&UvSet> {
        self.session_uvs.sets.get(&shape)
    }

    /// Replaces the shape's `map1` with a planar projection along `axis`, fitted to the
    /// object-space bounding box so both directions fill 0 through 1.
    pub fn project_uv_planar(
        &mut self,
        shape: NativeId,
        axis: UvProjectAxis,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        self.shape(shape)?;
        self.check_effects(shape)?;
        let set = planar(&self.shape(shape)?.mesh, axis)?;
        self.session_uvs.sets.insert(shape, set);
        Ok(())
    }

    /// Removes the shape's `map1`. A shape without UVs is left alone.
    pub fn delete_uvs(&mut self, shape: NativeId) -> Result<(), NativeError> {
        self.structural_edit()?;
        self.shape(shape)?;
        self.check_effects(shape)?;
        self.session_uvs.sets.remove(&shape);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
