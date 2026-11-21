//! Session-only lambert materials assigned to whole authored shapes.
//!
//! The table never touches source appearance: sourced scenes keep their `MaterialDiffuse` path
//! and refuse session edits. An authored scene's table is written to `.ma` (#156 S3c); `.mb`
//! refuses it. Names follow Maya 2025.1 (#177): the
//! initial shading group's shader is `standardSurface1`, and new lamberts start at `lambert2`
//! with `lambert2SG` because Maya's unconnected `lambert1` always exists. Maya 2026 is
//! unverified, and the initial shader's colour is a placeholder equal to the viewport base fill.

use super::{NativeError, NativeId, NativeMeshShape, NativeScene};
use crate::shading::SURFACE_BASE_RGB;
use std::collections::{BTreeMap, HashSet};

/// Bound on materials per scene. Kept far below what `inspect_scene` can list (see the
/// inspection budget test) so a full table never makes inspection `too_large`.
pub const MAX_SESSION_MATERIALS: usize = 512;

/// Bound on shapes in one assignment (PROVISIONAL: Maya's docs state no limit). It bounds the
/// remote list and the work of one history step.
pub const MAX_ASSIGN_SHAPES: usize = 1024;

/// Colour of a newly created lambert: Maya's lambert `color` default (0.5 grey, 2027 node
/// reference). Exactly representable in f32, so it passes `quantize_color` unchanged.
pub const DEFAULT_LAMBERT_COLOR: [f64; 3] = [0.5; 3];

/// An index into the current table. It is valid only at the current history position: undoing a
/// create frees its index for the next create, so callers must take ids from operation outcomes
/// and never cache them across edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionMaterialId(usize);

impl SessionMaterialId {
    pub fn from_index(index: usize) -> Self {
        Self(index)
    }
    pub fn index(self) -> usize {
        self.0
    }
}

/// Which material a whole-shape assignment uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MaterialChoice {
    Existing(SessionMaterialId),
    /// Creates a lambert with this colour inside the same edit.
    New {
        color: [f64; 3],
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lambert {
    pub(super) name: String,
    pub(super) color: [f64; 3],
}

impl Lambert {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Linear RGB, each channel within 0 through 1.
    pub fn color(&self) -> [f64; 3] {
        self.color
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShadingGroup {
    pub(super) name: String,
    pub(super) material: SessionMaterialId,
}

impl ShadingGroup {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn material(&self) -> SessionMaterialId {
        self.material
    }
}

/// Group 0 is the initial shading group; shapes without an entry resolve to it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SessionMaterials {
    pub(super) materials: Vec<Lambert>,
    pub(super) groups: Vec<ShadingGroup>,
    pub(super) assignments: BTreeMap<NativeId, usize>,
}

impl Default for SessionMaterials {
    fn default() -> Self {
        Self {
            materials: vec![Lambert {
                name: "standardSurface1".into(),
                color: SURFACE_BASE_RGB.map(|channel| f64::from(channel) / 255.0),
            }],
            groups: vec![ShadingGroup {
                name: "initialShadingGroup".into(),
                material: SessionMaterialId(0),
            }],
            assignments: BTreeMap::new(),
        }
    }
}

/// `N` of a `lambert<N>` or `lambert<N>SG` name, with `N` a `u32` without leading zeros.
pub(super) fn lambert_number(name: &str) -> Option<u32> {
    let digits = name.strip_prefix("lambert")?;
    let digits = digits.strip_suffix("SG").unwrap_or(digits);
    if digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn check_color(color: [f64; 3]) -> Result<(), NativeError> {
    if color.iter().any(|channel| !channel.is_finite()) {
        return Err(NativeError::NonFinite);
    }
    if color.iter().any(|channel| !(0.0..=1.0).contains(channel)) {
        return Err(NativeError::SessionColorOutOfRange);
    }
    Ok(())
}

/// Validates `color`, then rounds each channel to the nearest f32: Maya stores `float3`, so a
/// colour that is not f32-exact could not survive a save and reopen. Range is checked on the
/// given value, before rounding.
fn quantize_color(color: [f64; 3]) -> Result<[f64; 3], NativeError> {
    check_color(color)?;
    Ok(color.map(|channel| f64::from(channel as f32)))
}

impl SessionMaterials {
    pub(super) fn validate(
        &self,
        shapes: &[NativeMeshShape],
        sourced: bool,
    ) -> Result<(), NativeError> {
        if self.materials.is_empty()
            || self.materials.len() > MAX_SESSION_MATERIALS
            || self.groups.len() != self.materials.len()
            || self
                .groups
                .iter()
                .enumerate()
                .any(|(index, group)| group.material.0 != index)
        {
            return Err(NativeError::InvalidSessionMaterials);
        }
        for material in &self.materials {
            check_color(material.color)?;
        }
        // Material 0's placeholder (83/255) is not f32-exact; it is locked and never written.
        if self.materials[1..]
            .iter()
            .any(|material| material.color.map(|c| f64::from(c as f32) == c) != [true; 3])
        {
            return Err(NativeError::InvalidSessionMaterials);
        }
        if sourced && !self.assignments.is_empty() {
            return Err(NativeError::InvalidSessionMaterials);
        }
        let mut known = HashSet::new();
        known
            .try_reserve(shapes.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        known.extend(shapes.iter().map(|shape| shape.id));
        for (shape, group) in &self.assignments {
            if *group >= self.groups.len() || !known.contains(shape) {
                return Err(NativeError::InvalidSessionMaterials);
            }
        }
        Ok(())
    }

    pub(super) fn prune(&mut self, shapes: &[NativeMeshShape]) {
        let known: HashSet<NativeId> = shapes.iter().map(|shape| shape.id).collect();
        self.assignments.retain(|shape, _| known.contains(shape));
    }

    /// Adds `lambert<N>` with `lambert<N>SG`, where `N` is one above the highest `lambert<N>`
    /// any material, group or name in `scene_names` (transforms and shapes) holds, and at least
    /// 2 because Maya's `lambert1` always exists: Maya 2025.1 gave `lambert10` beside a
    /// transform `lambert9`, and `lambert2` again after deleting `lambert2`. Counting
    /// `lambert<N>SG` names too is PROVISIONAL. Both names are then free.
    pub(super) fn add<'a>(
        &mut self,
        color: [f64; 3],
        scene_names: impl Iterator<Item = &'a str>,
    ) -> Result<SessionMaterialId, NativeError> {
        let color = quantize_color(color)?;
        if self.materials.len() >= MAX_SESSION_MATERIALS {
            return Err(NativeError::ResourceLimit);
        }
        let highest = (self.materials.iter().map(Lambert::name))
            .chain(self.groups.iter().map(ShadingGroup::name))
            .filter_map(lambert_number)
            .chain(scene_names.filter_map(lambert_number))
            .fold(1, u32::max);
        let suffix = highest.checked_add(1).ok_or(NativeError::ResourceLimit)?;
        let id = SessionMaterialId(self.materials.len());
        self.materials.push(Lambert {
            name: format!("lambert{suffix}"),
            color,
        });
        self.groups.push(ShadingGroup {
            name: format!("lambert{suffix}SG"),
            material: id,
        });
        Ok(id)
    }

    fn group_of(&self, material: SessionMaterialId) -> Result<usize, NativeError> {
        self.groups
            .iter()
            .position(|group| group.material == material)
            .ok_or(NativeError::UnknownSessionMaterial(material))
    }
}

impl NativeScene {
    /// Stores a table read back from a file on the source-free candidate; `validate` still
    /// checks it.
    pub(super) fn attach_authored_session_materials(&mut self, table: SessionMaterials) {
        self.session_materials = table;
    }

    pub fn session_materials(&self) -> &[Lambert] {
        &self.session_materials.materials
    }
    pub fn session_shading_groups(&self) -> &[ShadingGroup] {
        &self.session_materials.groups
    }

    /// The lambert every instance of this authored shape displays; `None` for sourced scenes,
    /// whose appearance the session table does not own. Taking the shape itself means an unknown
    /// id cannot be asked about; the lookup is one `BTreeMap` probe, O(log n) in assignments.
    pub fn session_material(&self, shape: &NativeMeshShape) -> Option<&Lambert> {
        let id = self.session_material_id(shape)?;
        self.session_materials.materials.get(id.0)
    }

    /// True when a session material was explicitly assigned to the shape (not the initial group).
    pub fn has_assigned_session_material(&self, shape: &NativeMeshShape) -> bool {
        self.source.is_none() && self.session_materials.assignments.contains_key(&shape.id)
    }

    /// The id behind [`Self::session_material`]; valid only at the current history position.
    pub fn session_material_id(&self, shape: &NativeMeshShape) -> Option<SessionMaterialId> {
        if self.source.is_some() {
            return None;
        }
        let group = self.session_materials.assignments.get(&shape.id).copied();
        let group = self.session_materials.groups.get(group.unwrap_or(0))?;
        Some(group.material)
    }

    /// Why session-material edits are refused on this scene, without the full `validate` pass
    /// that `structural_edit` runs, so a UI can ask every frame. `None` means an edit may be
    /// attempted; the edit itself still validates and can fail. Same refusal as the edits give:
    /// read-only source first, then any retained source.
    pub fn session_material_edit_refusal(&self) -> Option<NativeError> {
        self.authoring_read_only_reason().or_else(|| {
            self.source
                .is_some()
                .then_some(NativeError::UnsupportedEdit)
        })
    }

    /// Number of authored shapes that display this material; 0 for an unknown id and for
    /// sourced scenes, which the table does not own.
    // Caller: the Attribute Editor "shared by" line (#467 T5a).
    pub fn session_material_shape_count(&self, material: SessionMaterialId) -> usize {
        let table = &self.session_materials;
        if self.source.is_some() {
            return 0;
        }
        let Ok(group) = table.group_of(material) else {
            return 0;
        };
        if group == 0 {
            self.shapes.len().saturating_sub(table.assignments.len())
        } else {
            table.assignments.values().filter(|g| **g == group).count()
        }
    }

    pub fn create_session_material(
        &mut self,
        color: [f64; 3],
    ) -> Result<SessionMaterialId, NativeError> {
        self.structural_edit()?;
        let names = self
            .nodes
            .iter()
            .map(|n| n.name())
            .chain(self.shapes.iter().map(|s| s.name()));
        self.session_materials.add(color, names)
    }

    /// Assigns the whole shape, so every instance of it changes.
    pub fn assign_session_material(
        &mut self,
        shape: NativeId,
        choice: MaterialChoice,
    ) -> Result<SessionMaterialId, NativeError> {
        self.assign_session_material_to_shapes(&[shape], choice)
    }

    /// Assigns every listed shape in one edit, creating at most one material for
    /// `MaterialChoice::New`. All-or-nothing: the list is validated (non-empty, within
    /// [`MAX_ASSIGN_SHAPES`], no duplicates, every shape present) before the table changes.
    pub fn assign_session_material_to_shapes(
        &mut self,
        shapes: &[NativeId],
        choice: MaterialChoice,
    ) -> Result<SessionMaterialId, NativeError> {
        self.structural_edit()?;
        if shapes.is_empty() {
            return Err(NativeError::InvalidShapeList("no shapes given"));
        }
        if shapes.len() > MAX_ASSIGN_SHAPES {
            return Err(NativeError::InvalidShapeList("too many shapes"));
        }
        let mut seen = HashSet::new();
        seen.try_reserve(shapes.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for shape in shapes {
            if !seen.insert(*shape) {
                return Err(NativeError::InvalidShapeList("duplicate shape"));
            }
            if !self.shapes.iter().any(|candidate| candidate.id == *shape) {
                return Err(NativeError::MissingShape(*shape));
            }
        }
        // Field borrows rather than `authored::names(self)`, which would also borrow the
        // material table taken mutably below.
        let names = self
            .nodes
            .iter()
            .map(|n| n.name())
            .chain(self.shapes.iter().map(|s| s.name()));
        let materials = &mut self.session_materials;
        let (material, group) = match choice {
            MaterialChoice::Existing(material) => (material, materials.group_of(material)?),
            MaterialChoice::New { color } => {
                let material = materials.add(color, names)?;
                (material, materials.groups.len() - 1)
            }
        };
        for shape in shapes {
            if group == 0 {
                materials.assignments.remove(shape);
            } else {
                materials.assignments.insert(*shape, group);
            }
        }
        Ok(material)
    }

    pub fn set_session_material_color(
        &mut self,
        material: SessionMaterialId,
        color: [f64; 3],
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        let color = quantize_color(color)?;
        if material.0 == 0 {
            return Err(NativeError::InitialSessionMaterialLocked);
        }
        self.session_materials
            .materials
            .get_mut(material.0)
            .ok_or(NativeError::UnknownSessionMaterial(material))?
            .color = color;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
