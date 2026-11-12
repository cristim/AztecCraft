//! Eligibility of a scene for writers that emit a whole file from authored content
//! (no retained source). Each writer maps `AuthoredIneligible` into its own refusal
//! enum with an exhaustive `match`, so the checks exist once.

use super::{
    GeometryViewFacts, NativeError, NativeId, NativeMeshShape, NativeScene, NativeTransformNode,
    Points, TransformExtras, Units,
};
use crate::scene::NodeId;
use std::collections::{HashMap, HashSet};

/// Names of the default Maya scene nodes that a file omitting that scene still
/// collides with on load. `BUILTINS` holds the remaining default names.
pub(super) const RESERVED: [&str; 11] = [
    "persp",
    "perspShape",
    "top",
    "topShape",
    "front",
    "frontShape",
    "side",
    "sideShape",
    "lambert1",
    "standardSurface1",
    "particleCloud1",
];

/// Further default scene names, which the ASCII decoder also treats as already present.
pub(super) const BUILTINS: [&str; 14] = [
    "time1",
    "hardwareRenderingGlobals",
    "renderPartition",
    "defaultShaderList1",
    "postProcessList1",
    "ikSystem",
    "renderGlobalsList1",
    "defaultRenderingList1",
    "initialShadingGroup",
    "initialParticleSE",
    "defaultRenderGlobals",
    "defaultResolution",
    "hardwareRenderGlobals",
    "defaultLightSet",
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredIneligible {
    SourcedScene,
    Curves,
    Keys,
    InvalidScene(NativeError),
    UnsupportedUnits,
    NonAuthoredHierarchy,
    InvalidName,
    ReservedName,
    NameCollision,
    EmptyMesh,
    NonDefaultPivot,
    TransformExtras,
    TransformRestriction,
    UnsupportedTransformEffect,
    TransformVisibility,
    ShapeRestriction,
    ShapeVisibility,
    Double3Points,
    PointOffsets,
    EdgeSmoothing,
    FaceHoles,
    UnsupportedMeshChannels,
    SessionUvs,
}

pub(super) fn names(scene: &NativeScene) -> impl Iterator<Item = (Option<NodeId>, &str)> {
    scene
        .nodes
        .iter()
        .map(|n| (Some(n.id), n.name()))
        .chain(scene.shapes.iter().map(|s| (None, s.name())))
}

/// Whether a writer carries session UVs: the ASCII writer does through its UV entry point, the
/// public ASCII entry point and the binary writer refuse them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UvPolicy {
    Refuse,
    Admit,
}

/// The first checks: a retained source, curve content or session keys cannot be written as
/// authored, nor can session UVs under `UvPolicy::Refuse`.
pub(super) fn check_content_kind(
    scene: &NativeScene,
    uvs: UvPolicy,
) -> Result<(), AuthoredIneligible> {
    if scene.source.is_some() {
        return Err(AuthoredIneligible::SourcedScene);
    }
    if !scene.curves.is_empty() {
        return Err(AuthoredIneligible::Curves);
    }
    if !scene.session_keys.is_empty() {
        return Err(AuthoredIneligible::Keys);
    }
    if uvs == UvPolicy::Refuse && !scene.session_uvs.is_empty() {
        return Err(AuthoredIneligible::SessionUvs);
    }
    Ok(())
}

/// Everything after the writer's own resource admission: validity, units, the flat
/// transform-over-one-mesh hierarchy, per-node eligibility and names.
pub(super) fn check_scene(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    check_valid_units(scene)?;
    if scene.shapes.len() != scene.nodes.len()
        || scene.nodes.len().checked_mul(2) != Some(scene.dag.len())
    {
        return Err(R::NonAuthoredHierarchy);
    }
    let edges = EdgeCounts::new(scene);
    let shapes: HashMap<NativeId, &NativeMeshShape> =
        scene.shapes.iter().map(|shape| (shape.id, shape)).collect();
    for node in &scene.nodes {
        // The root count mirrors validate() and cannot fail alone once the child count holds.
        if edges.roots(node.transform_id) != 1 || edges.children(node.transform_id) != 1 {
            return Err(R::NonAuthoredHierarchy);
        }
        let shape = edges
            .first_child(node.transform_id)
            .and_then(|id| shapes.get(&id))
            .ok_or(R::NonAuthoredHierarchy)?;
        if shape.mesh().point_count() == 0 || shape.mesh().faces().is_empty() {
            return Err(R::EmptyMesh);
        }
        eligibility(node, shape)?;
    }
    check_names(scene)
}

fn check_valid_units(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    scene.validate().map_err(AuthoredIneligible::InvalidScene)?;
    if scene.units != Units::default() {
        return Err(AuthoredIneligible::UnsupportedUnits);
    }
    Ok(())
}

/// A scene of bare transforms: every node is a root with no child, and there is no
/// shape: the shape-less case of the Binary writer, whose meshes go through `check_scene`.
pub(super) fn check_transforms_only(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    check_valid_units(scene)?;
    if !scene.shapes.is_empty() || scene.dag.len() != scene.nodes.len() {
        return Err(R::NonAuthoredHierarchy);
    }
    let edges = EdgeCounts::new(scene);
    for node in &scene.nodes {
        if edges.roots(node.transform_id) != 1 {
            return Err(R::NonAuthoredHierarchy);
        }
        node_eligibility(node)?;
    }
    check_names(scene)
}

fn check_names(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    let mut node_names: HashMap<&str, usize> = HashMap::new();
    for node in &scene.nodes {
        *node_names.entry(node.name()).or_default() += 1;
    }
    let mut seen = HashSet::new();
    for (_, name) in names(scene) {
        if !super::is_valid_name(name) {
            return Err(R::InvalidName);
        }
        // A repeated node name is a collision even when it is also reserved; a shape that
        // repeats a node's name is caught below, after that node's own checks.
        if node_names.get(name).is_some_and(|count| *count > 1) {
            return Err(R::NameCollision);
        }
        if RESERVED.contains(&name) || BUILTINS.contains(&name) {
            return Err(R::ReservedName);
        }
        if !seen.insert(name) {
            return Err(R::NameCollision);
        }
    }
    Ok(())
}

/// How many root edges and child edges each transform has, from one pass over the DAG.
struct EdgeCounts {
    roots: HashMap<NativeId, usize>,
    children: HashMap<NativeId, (usize, NativeId)>,
}

impl EdgeCounts {
    fn new(scene: &NativeScene) -> Self {
        let mut roots: HashMap<NativeId, usize> = HashMap::new();
        let mut children: HashMap<NativeId, (usize, NativeId)> = HashMap::new();
        for edge in &scene.dag {
            match edge.parent {
                None => *roots.entry(edge.child).or_default() += 1,
                Some(parent) => children.entry(parent).or_insert((0, edge.child)).0 += 1,
            }
        }
        Self { roots, children }
    }
    fn roots(&self, transform: NativeId) -> usize {
        self.roots.get(&transform).copied().unwrap_or(0)
    }
    fn children(&self, transform: NativeId) -> usize {
        self.children.get(&transform).map_or(0, |entry| entry.0)
    }
    fn first_child(&self, transform: NativeId) -> Option<NativeId> {
        self.children.get(&transform).map(|entry| entry.1)
    }
}

/// The one shape parented to `node`.
pub(super) fn shape_of<'a>(
    scene: &'a NativeScene,
    node: &NativeTransformNode,
) -> Result<&'a NativeMeshShape, AuthoredIneligible> {
    let edge = scene
        .dag
        .iter()
        .find(|edge| edge.parent == Some(node.transform_id))
        .ok_or(AuthoredIneligible::NonAuthoredHierarchy)?;
    scene
        .shape(edge.child)
        .map_err(|_| AuthoredIneligible::NonAuthoredHierarchy)
}

fn eligibility(
    node: &NativeTransformNode,
    shape: &NativeMeshShape,
) -> Result<(), AuthoredIneligible> {
    node_eligibility(node)?;
    shape_eligibility(shape)
}

fn node_eligibility(node: &NativeTransformNode) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    if node
        .rotate_pivot
        .iter()
        .chain(&node.scale_pivot)
        .any(|v| v.value.to_bits() != 0)
    {
        return Err(R::NonDefaultPivot);
    }
    if node.extras != TransformExtras::default() {
        return Err(R::TransformExtras);
    }
    if !node.restrictions.is_empty() {
        return Err(R::TransformRestriction);
    }
    if !node.unsupported_transform.is_empty() {
        return Err(R::UnsupportedTransformEffect);
    }
    if node.view != GeometryViewFacts::authored() {
        return Err(R::TransformVisibility);
    }
    Ok(())
}

fn shape_eligibility(shape: &NativeMeshShape) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    if !shape.restrictions.is_empty() {
        return Err(R::ShapeRestriction);
    }
    if shape.view != GeometryViewFacts::authored() {
        return Err(R::ShapeVisibility);
    }
    let mesh = shape.mesh();
    if matches!(mesh.points(), Points::Double3(_)) {
        return Err(R::Double3Points);
    }
    if mesh.has_point_offsets() {
        return Err(R::PointOffsets);
    }
    if mesh.edges().iter().any(|edge| edge.smooth() != Some(false)) {
        return Err(R::EdgeSmoothing);
    }
    if mesh.faces().iter().any(|face| !face.holes().is_empty()) {
        return Err(R::FaceHoles);
    }
    if mesh.has_unsupported_channels() {
        return Err(R::UnsupportedMeshChannels);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
