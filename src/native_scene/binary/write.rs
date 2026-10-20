//! Writer for a fresh Maya Binary file from an authored (source-less) scene.
//!
//! The entry point checks eligibility, builds the file (`HEAD`, then each transform with
//! its mesh shape) and returns the bytes only after the self-check has decoded them and
//! found the scene again. Maya acceptance of the output is unproven.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
mod check;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
pub(in crate::native_scene::binary) mod chunk;
#[cfg(test)]
mod fixtures;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
mod head;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
mod mesh;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
mod profile;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "only the Editor caller is missing")
)]
mod transform;

use crate::native_scene::{NativeScene, authored, authored::AuthoredIneligible};
use chunk::TooLarge;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredBinaryRefusal {
    Ineligible(AuthoredIneligible),
    TooLarge,
    /// A node has no stored UUID, and the file's creation chunks need one.
    MissingUuid,
    /// Some shape is assigned a non-initial shading group, which this file cannot carry.
    MaterialAssigned,
    /// The decoder did not read back what was written; no bytes are returned.
    SelfCheckMismatch,
}
impl From<AuthoredIneligible> for AuthoredBinaryRefusal {
    fn from(ineligible: AuthoredIneligible) -> Self {
        Self::Ineligible(ineligible)
    }
}
impl From<TooLarge> for AuthoredBinaryRefusal {
    fn from(_: TooLarge) -> Self {
        Self::TooLarge
    }
}

/// The file for a scene of bare transforms or of transforms over one mesh each, whose
/// eligibility the caller has checked. The bytes are decoded and compared with the scene
/// before they are returned.
fn scene_file(scene: &NativeScene) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    let mut children = Vec::new();
    children
        .try_reserve_exact(scene.nodes().len() + scene.shapes().len() + 1)
        .map_err(|_| TooLarge)?;
    children.push(head::head()?);
    for node in scene.nodes() {
        let uuid = node
            .transform_uuid()
            .ok_or(AuthoredBinaryRefusal::MissingUuid)?;
        let internal = scene
            .internal_transform(node.id())
            .map_err(AuthoredIneligible::InvalidScene)?;
        let channels = transform::Channels {
            translation: internal.translation_cm,
            rotation: internal.rotation_radians,
            scale: internal.scale,
        };
        children.push(transform::xfrm(node.name(), uuid, channels)?);
        if !scene.shapes().is_empty() {
            let shape = authored::shape_of(scene, node)?;
            let uuid = shape.uuid().ok_or(AuthoredBinaryRefusal::MissingUuid)?;
            children.push(mesh::dmsh(shape.name(), node.name(), uuid, shape.mesh())?);
        }
    }
    let bytes = chunk::group(*b"Maya", profile::ROOT_AUX, &children, chunk::Align::One)?;
    check::verify(scene, &bytes)?;
    Ok(bytes)
}

#[cfg_attr(not(test), expect(dead_code, reason = "no Editor caller yet"))]
pub(crate) fn encode_authored(scene: &NativeScene) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    authored::check_content_kind(scene)?;
    if scene.shapes().is_empty() {
        authored::check_transforms_only(scene)?;
    } else {
        authored::check_scene(scene)?;
    }
    if scene
        .session_materials
        .assignments
        .values()
        .any(|group| *group != 0)
    {
        return Err(AuthoredBinaryRefusal::MaterialAssigned);
    }
    scene_file(scene)
}

#[cfg(test)]
mod tests;
