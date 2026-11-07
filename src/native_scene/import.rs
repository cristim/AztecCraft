//! Adds parsed OBJ objects to a scene as flat authored transform-over-mesh pairs, the same shape
//! `create` makes, so the authored writers accept them. No view admission here: the history
//! layer runs the display predicate on the candidate scene.

use super::{
    NativeError, NativeMesh, NativeScene, NativeTransform, SceneError,
    authored::{self, BUILTINS, RESERVED},
};
use crate::{obj::import::ParsedObj, scene::NodeId};
use std::collections::HashSet;

/// Objects ready to append: names already resolved against the scene they were planned on.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjImportPlan {
    objects: Vec<PlannedObject>,
}

#[derive(Clone, Debug, PartialEq)]
struct PlannedObject {
    name: String,
    shape_name: String,
    points: Vec<[f32; 3]>,
    loops: Vec<Vec<usize>>,
}

impl ObjImportPlan {
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
    pub fn vertex_count(&self) -> usize {
        self.objects.iter().map(|object| object.points.len()).sum()
    }
    pub fn face_count(&self) -> usize {
        self.objects.iter().map(|object| object.loops.len()).sum()
    }
}

impl NativeScene {
    /// Resolves names and builds each object's points and loops. Refuses a scene that cannot take
    /// structural edits (opened `.mb` or `.ma`). Points are the vertices an object's faces use,
    /// in ascending OBJ order; unused vertices are dropped.
    pub fn plan_obj_import(&self, parsed: &ParsedObj) -> Result<ObjImportPlan, NativeError> {
        self.structural_edit()?;
        let mut taken: HashSet<String> = authored::names(self)
            .map(|(_, name)| name.to_owned())
            .chain(
                RESERVED
                    .iter()
                    .chain(BUILTINS.iter())
                    .map(|name| (*name).to_owned()),
            )
            .collect();
        let mut objects = Vec::new();
        objects
            .try_reserve_exact(parsed.objects.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for object in &parsed.objects {
            let (name, shape_name) = free_names(&object.name, &mut taken)?;
            let mut used: Vec<u32> = object.faces.iter().flatten().copied().collect();
            used.sort_unstable();
            used.dedup();
            let points = used
                .iter()
                .map(|index| {
                    usize::try_from(*index)
                        .ok()
                        .and_then(|index| parsed.vertices.get(index))
                        .copied()
                        .ok_or(NativeError::InvalidTopology)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let loops = object
                .faces
                .iter()
                .map(|face| {
                    face.iter()
                        .map(|corner| {
                            used.binary_search(corner)
                                .map_err(|_| NativeError::InvalidTopology)
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            objects.push(PlannedObject {
                name,
                shape_name,
                points,
                loops,
            });
        }
        Ok(ObjImportPlan { objects })
    }

    /// Appends the planned objects as new root transforms with identity transforms, returning
    /// their ids in file order. Nothing changes when it fails.
    pub fn import_polygon_objects(
        &mut self,
        plan: &ObjImportPlan,
    ) -> Result<Vec<NodeId>, NativeError> {
        self.structural_edit()?;
        let before = (
            self.nodes.len(),
            self.shapes.len(),
            self.dag.len(),
            self.next_id,
            self.next_native_id,
        );
        let result = self.append_planned(plan);
        if result.is_err() {
            self.nodes.truncate(before.0);
            self.shapes.truncate(before.1);
            self.dag.truncate(before.2);
            self.next_id = before.3;
            self.next_native_id = before.4;
        }
        result
    }

    fn append_planned(&mut self, plan: &ObjImportPlan) -> Result<Vec<NodeId>, NativeError> {
        let mut ids = Vec::new();
        ids.try_reserve_exact(plan.objects.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for object in &plan.objects {
            self.validate_new_name(None, &object.name)?;
            if authored::names(self)
                .any(|(_, name)| name == object.shape_name || name == object.name)
            {
                return Err(NativeError::NameCollision);
            }
            let next = self
                .next_id
                .checked_add(1)
                .ok_or(SceneError::IdsExhausted)?;
            let id = NodeId::new(self.next_id);
            let mesh = NativeMesh::from_loops(object.points.clone(), object.loops.clone());
            self.append(
                id,
                &object.name,
                object.shape_name.clone(),
                mesh,
                NativeTransform::default(),
            )?;
            self.next_id = next;
            ids.push(id);
        }
        self.validate()?;
        Ok(ids)
    }
}

/// Maps every byte outside `[A-Za-z0-9_]` to `_` and guards an empty or digit-led name, then
/// finds the lowest numbered variant (trailing digits replaced) whose transform and
/// `<name>Shape` names are both free. Reserves both names in `taken`.
fn free_names(raw: &str, taken: &mut HashSet<String>) -> Result<(String, String), NativeError> {
    let mut base: String = raw
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                char::from(byte)
            } else {
                '_'
            }
        })
        .collect();
    if base.is_empty() || base.starts_with(|c: char| c.is_ascii_digit()) {
        base.insert(0, '_');
    }
    let free = |name: &str| !taken.contains(name) && !taken.contains(&format!("{name}Shape"));
    let name = if free(&base) {
        base
    } else {
        let stem = base.trim_end_matches(|c: char| c.is_ascii_digit());
        let attempts = taken.len().saturating_add(2);
        (1..=attempts)
            .map(|suffix| format!("{stem}{suffix}"))
            .find(|name| free(name))
            .ok_or(NativeError::ResourceLimit)?
    };
    let shape = format!("{name}Shape");
    taken.insert(name.clone());
    taken.insert(shape.clone());
    Ok((name, shape))
}

#[cfg(test)]
mod tests;
