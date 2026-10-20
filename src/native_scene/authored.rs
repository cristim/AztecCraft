//! Eligibility of a scene for writers that emit a whole file from authored content
//! (no retained source). Each writer maps `AuthoredIneligible` into its own refusal
//! enum with an exhaustive `match`, so the checks exist once.

use super::{
    GeometryViewFacts, NativeError, NativeMeshShape, NativeScene, NativeTransformNode, Points,
    TransformExtras, Units,
};
use crate::scene::NodeId;

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
}

pub(super) fn names(scene: &NativeScene) -> impl Iterator<Item = (Option<NodeId>, &str)> {
    scene
        .nodes
        .iter()
        .map(|n| (Some(n.id), n.name()))
        .chain(scene.shapes.iter().map(|s| (None, s.name())))
}

/// The first checks: a retained source or curve content cannot be written as authored.
pub(super) fn check_content_kind(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    if scene.source.is_some() {
        return Err(AuthoredIneligible::SourcedScene);
    }
    if !scene.curves.is_empty() {
        return Err(AuthoredIneligible::Curves);
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
    for node in &scene.nodes {
        let (mut roots, mut children) = (0, 0);
        for edge in &scene.dag {
            if edge.parent.is_none() && edge.child == node.transform_id {
                roots += 1;
            }
            if edge.parent == Some(node.transform_id) {
                children += 1;
            }
        }
        if roots != 1 || children != 1 {
            return Err(R::NonAuthoredHierarchy);
        }
        let shape = shape_of(scene, node)?;
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
    for node in &scene.nodes {
        let roots = scene
            .dag
            .iter()
            .filter(|edge| edge.parent.is_none() && edge.child == node.transform_id)
            .count();
        if roots != 1 {
            return Err(R::NonAuthoredHierarchy);
        }
        node_eligibility(node)?;
    }
    check_names(scene)
}

fn check_names(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    use AuthoredIneligible as R;
    for (index, (id, name)) in names(scene).enumerate() {
        scene.validate_new_name(id, name).map_err(|e| match e {
            NativeError::InvalidName => R::InvalidName,
            NativeError::NameCollision => R::NameCollision,
            other => R::InvalidScene(other),
        })?;
        if RESERVED.contains(&name) || BUILTINS.contains(&name) {
            return Err(R::ReservedName);
        }
        if names(scene).take(index).any(|(_, earlier)| earlier == name) {
            return Err(R::NameCollision);
        }
    }
    Ok(())
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
