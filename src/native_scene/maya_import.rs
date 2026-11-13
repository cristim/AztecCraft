//! Merges the meshes of a parsed Maya scene into an authored scene (File > Import of `.ma` and
//! `.mb`). The source is read-only data: it must be a flat transform-over-one-mesh scene whose
//! values an authored writer can reproduce exactly, or the whole import is refused with a typed
//! reason. Names are prefixed (Maya's `-namespace`, spelled `<prefix>_<name>` because names here
//! cannot contain a colon) and clashes get the lowest free numeric suffix. No view admission here:
//! the history layer runs the display predicate on the candidate scene.

use super::{
    NativeError, NativeMesh, NativeScene, NativeTransform, SceneError, Units,
    authored::{self, AuthoredIneligible, BUILTINS, RESERVED},
    is_valid_name,
    session_material::SessionMaterials,
};
use crate::scene::NodeId;
use std::{collections::HashSet, fmt};

pub const MAX_PREFIX_BYTES: usize = 64;

/// Why a source scene or option cannot be imported. Fieldless so it carries no file content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MayaImportRefusal {
    InvalidPrefix,
    EmptyScene,
    /// The scene keeps its raw source records (every Maya Binary file, and any ASCII file the
    /// authored writer does not reproduce exactly: lights, cameras, shaders, scripts); import
    /// would drop them silently, so it refuses.
    UnrecognizedContent,
    /// More root objects than one selection can hold.
    TooManyObjects,
    /// Materials and shading are not carried, so a file that defines any is refused, not dropped.
    Materials,
    Curves,
    Keys,
    SessionUvs,
    InvalidScene,
    UnsupportedUnits,
    NonFlatHierarchy,
    EmptyMesh,
    NonDefaultPivot,
    TransformExtras,
    TransformRestriction,
    TransformEffect,
    Visibility,
    ShapeRestriction,
    Double3Points,
    PointOffsets,
    /// An edge is not recorded hard; the authored writers can only write hard edges.
    EdgeSmoothing,
    FaceHoles,
    UnsupportedMeshChannels,
}

impl MayaImportRefusal {
    /// Stable machine-readable class, shared by the MCP error details.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidPrefix => "invalid_prefix",
            Self::EmptyScene => "empty_scene",
            Self::UnrecognizedContent => "unrecognized_content",
            Self::TooManyObjects => "too_many_objects",
            Self::Materials => "materials",
            Self::Curves => "curves",
            Self::Keys => "keys",
            Self::SessionUvs => "session_uvs",
            Self::InvalidScene => "invalid_scene",
            Self::UnsupportedUnits => "unsupported_units",
            Self::NonFlatHierarchy => "non_flat_hierarchy",
            Self::EmptyMesh => "empty_mesh",
            Self::NonDefaultPivot => "non_default_pivot",
            Self::TransformExtras => "transform_extras",
            Self::TransformRestriction => "transform_restriction",
            Self::TransformEffect => "transform_effect",
            Self::Visibility => "visibility",
            Self::ShapeRestriction => "shape_restriction",
            Self::Double3Points => "double3_points",
            Self::PointOffsets => "point_offsets",
            Self::EdgeSmoothing => "edge_smoothing",
            Self::FaceHoles => "face_holes",
            Self::UnsupportedMeshChannels => "unsupported_mesh_channels",
        }
    }
}

impl fmt::Display for MayaImportRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPrefix => {
                "the import prefix must be 1 to 64 letters, digits or underscores and not start with a digit"
            }
            Self::EmptyScene => "the file contains no meshes to import",
            Self::UnrecognizedContent => {
                "the file contains records import cannot carry (only files an authored save reproduces exactly are accepted)"
            }
            Self::TooManyObjects => "the file contains more objects than one import can add",
            Self::Materials => "the file defines materials, which import does not carry",
            Self::Curves => "the file contains curves, which import does not support",
            Self::Keys => "the file contains animation keys, which import does not support",
            Self::SessionUvs => "the file contains session UV sets, which import does not support",
            Self::InvalidScene => "the file does not describe a valid scene",
            Self::UnsupportedUnits => {
                "the file uses units other than centimeters and degrees, which import does not convert"
            }
            Self::NonFlatHierarchy => {
                "import supports only root transforms that each hold exactly one mesh"
            }
            Self::EmptyMesh => "the file contains a mesh without points or faces",
            Self::NonDefaultPivot => "the file contains a transform with a non-default pivot",
            Self::TransformExtras => "the file contains transform attributes import does not carry",
            Self::TransformRestriction => "the file contains a transform with edit restrictions",
            Self::TransformEffect => "the file contains a transform effect import does not carry",
            Self::Visibility => "the file contains a node whose visibility is not plain visible",
            Self::ShapeRestriction => "the file contains a mesh with edit restrictions",
            Self::Double3Points => "the file contains double-precision mesh points",
            Self::PointOffsets => "the file contains mesh point offsets",
            Self::EdgeSmoothing => {
                "the file does not record its mesh edges as hard, and authored scenes can only hold hard edges"
            }
            Self::FaceHoles => "the file contains faces with holes",
            Self::UnsupportedMeshChannels => {
                "the file contains mesh channels (normals, colors or similar) import does not carry"
            }
        })
    }
}

impl From<AuthoredIneligible> for MayaImportRefusal {
    fn from(value: AuthoredIneligible) -> Self {
        use AuthoredIneligible as A;
        match value {
            A::SourcedScene | A::InvalidScene(_) | A::InvalidName | A::ReservedName => {
                Self::InvalidScene
            }
            A::NameCollision => Self::InvalidScene,
            A::Curves => Self::Curves,
            A::Keys => Self::Keys,
            // An import builds a flat authored scene and never creates session joints.
            A::Joints => Self::InvalidScene,
            A::SessionUvs => Self::SessionUvs,
            A::UnsupportedUnits => Self::UnsupportedUnits,
            A::NonAuthoredHierarchy => Self::NonFlatHierarchy,
            A::EmptyMesh => Self::EmptyMesh,
            A::NonDefaultPivot => Self::NonDefaultPivot,
            A::TransformExtras => Self::TransformExtras,
            A::TransformRestriction => Self::TransformRestriction,
            A::UnsupportedTransformEffect => Self::TransformEffect,
            A::TransformVisibility | A::ShapeVisibility => Self::Visibility,
            A::ShapeRestriction => Self::ShapeRestriction,
            A::Double3Points => Self::Double3Points,
            A::PointOffsets => Self::PointOffsets,
            A::EdgeSmoothing => Self::EdgeSmoothing,
            A::FaceHoles => Self::FaceHoles,
            A::UnsupportedMeshChannels => Self::UnsupportedMeshChannels,
        }
    }
}

/// The supported subset of Maya's Import options (PROVISIONAL, from the 2020 page and the
/// `file -import` flags): a name prefix. Group, Preserve References, time range and the other
/// namespace modes are not implemented.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MayaImportOptions {
    pub prefix: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PlannedObject {
    name: String,
    shape_name: String,
    mesh: NativeMesh,
    transform: NativeTransform,
}

/// Objects ready to append: names already resolved against the scene they were planned on.
#[derive(Clone, Debug, PartialEq)]
pub struct MayaImportPlan {
    objects: Vec<PlannedObject>,
}

impl MayaImportPlan {
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
    pub fn vertex_count(&self) -> usize {
        self.objects
            .iter()
            .map(|object| object.mesh.point_count())
            .sum()
    }
    pub fn face_count(&self) -> usize {
        self.objects
            .iter()
            .map(|object| object.mesh.faces().len())
            .sum()
    }
    /// The transform names the import will create, in file order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.objects.iter().map(|object| object.name.as_str())
    }
}

fn refuse(reason: MayaImportRefusal) -> NativeError {
    NativeError::MayaImport(reason)
}

impl NativeScene {
    /// Checks the source and resolves every name against this scene. Refuses a target that
    /// cannot take structural edits (opened `.mb` or `.ma`). The source may keep its retained
    /// bytes: only its meshes and transforms are read.
    pub fn plan_maya_import(
        &self,
        source: &NativeScene,
        options: &MayaImportOptions,
    ) -> Result<MayaImportPlan, NativeError> {
        self.structural_edit()?;
        if let Some(prefix) = &options.prefix
            && (prefix.len() > MAX_PREFIX_BYTES || !is_valid_name(prefix))
        {
            return Err(refuse(MayaImportRefusal::InvalidPrefix));
        }
        check_source(source)?;
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
            .try_reserve_exact(source.nodes.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for node in &source.nodes {
            let shape = authored::shape_of(source, node).map_err(|e| refuse(e.into()))?;
            let name = resolve(&prefixed(options, node.name()), &mut taken)?;
            let shape_name = resolve(&prefixed(options, shape.name()), &mut taken)?;
            objects.push(PlannedObject {
                name,
                shape_name,
                mesh: shape.mesh().clone(),
                transform: node.transform(),
            });
        }
        Ok(MayaImportPlan { objects })
    }

    /// Appends the planned objects as new root transforms, returning their ids in file order.
    /// Nothing changes when it fails.
    pub fn import_maya_objects(
        &mut self,
        plan: &MayaImportPlan,
    ) -> Result<Vec<NodeId>, NativeError> {
        self.structural_edit()?;
        let before = (
            self.nodes.len(),
            self.shapes.len(),
            self.dag.len(),
            self.next_id,
            self.next_native_id,
        );
        let result = self.append_maya_planned(plan);
        if result.is_err() {
            self.nodes.truncate(before.0);
            self.shapes.truncate(before.1);
            self.dag.truncate(before.2);
            self.next_id = before.3;
            self.next_native_id = before.4;
        }
        result
    }

    fn append_maya_planned(&mut self, plan: &MayaImportPlan) -> Result<Vec<NodeId>, NativeError> {
        let mut ids = Vec::new();
        ids.try_reserve_exact(plan.objects.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        for object in &plan.objects {
            self.validate_new_name(None, &object.name)?;
            if object.name == object.shape_name
                || authored::names(self)
                    .any(|(_, name)| name == object.shape_name || name == object.name)
            {
                return Err(NativeError::NameCollision);
            }
            let next = self
                .next_id
                .checked_add(1)
                .ok_or(SceneError::IdsExhausted)?;
            let id = NodeId::new(self.next_id);
            self.append(
                id,
                &object.name,
                object.shape_name.clone(),
                object.mesh.clone(),
                object.transform,
            )?;
            self.next_id = next;
            ids.push(id);
        }
        self.validate()?;
        Ok(ids)
    }
}

fn check_source(source: &NativeScene) -> Result<(), NativeError> {
    if source.nodes.is_empty() {
        return Err(refuse(MayaImportRefusal::EmptyScene));
    }
    // A retained source holds every record of the file, typed or not (all of Maya Binary, and
    // any ASCII file the authored writer would not reproduce byte for byte); only a source-free
    // authored projection is proved to lose nothing.
    if source.source.is_some() {
        return Err(refuse(MayaImportRefusal::UnrecognizedContent));
    }
    if source.nodes.len() > super::MAX_TRANSFORM_PATHS {
        return Err(refuse(MayaImportRefusal::TooManyObjects));
    }
    if !source.materials.is_empty() || source.session_materials != SessionMaterials::default() {
        return Err(refuse(MayaImportRefusal::Materials));
    }
    if !source.curves.is_empty() {
        return Err(refuse(MayaImportRefusal::Curves));
    }
    if !source.session_keys.is_empty() {
        return Err(refuse(MayaImportRefusal::Keys));
    }
    if !source.session_uvs.is_empty() {
        return Err(refuse(MayaImportRefusal::SessionUvs));
    }
    if source.units != Units::default() {
        return Err(refuse(MayaImportRefusal::UnsupportedUnits));
    }
    authored::check_structure(source).map_err(|e| refuse(e.into()))
}

fn prefixed(options: &MayaImportOptions, name: &str) -> String {
    match &options.prefix {
        Some(prefix) => format!("{prefix}_{name}"),
        None => name.to_owned(),
    }
}

/// `base` when free, else the lowest numbered variant (trailing digits replaced) that is free.
/// Reserves the result in `taken`.
fn resolve(base: &str, taken: &mut HashSet<String>) -> Result<String, NativeError> {
    let name = if taken.contains(base) {
        let stem = base.trim_end_matches(|c: char| c.is_ascii_digit());
        let attempts = taken.len().saturating_add(2);
        (1..=attempts)
            .map(|suffix| format!("{stem}{suffix}"))
            .find(|name| !taken.contains(name))
            .ok_or(NativeError::ResourceLimit)?
    } else {
        base.to_owned()
    };
    taken.insert(name.clone());
    Ok(name)
}

#[cfg(test)]
mod tests;
