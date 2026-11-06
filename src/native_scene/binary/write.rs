//! Writer for a fresh Maya Binary file from an authored (source-less) scene.
//!
//! The entry point checks eligibility, builds the file (`HEAD`, then each transform with
//! its mesh shape) and returns the bytes only after the self-check has decoded them and
//! found the scene again. Maya acceptance of the output is unproven.

#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
mod check;
#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
pub(in crate::native_scene::binary) mod chunk;
#[cfg(test)]
mod fixtures;
#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
mod head;
#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
mod mesh;
#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
mod profile;
#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
mod transform;

use crate::native_scene::{NativeScene, SessionMaterials, authored, authored::AuthoredIneligible};
use chunk::TooLarge;

mod refusal;
pub use refusal::AuthoredRefusalCode;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredBinaryRefusal {
    Ineligible(AuthoredIneligible),
    TooLarge,
    /// A node has no stored UUID, and the file's creation chunks need one.
    MissingUuid,
    /// The scene holds a created material, an assignment or a reassignment; the file carries
    /// only Maya's default material and shading group.
    SessionMaterials,
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

#[cfg_attr(
    not(any(test, unix)),
    expect(dead_code, reason = "only the Unix save path calls the writer")
)]
pub(crate) fn encode_authored(scene: &NativeScene) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    authored::check_content_kind(scene)?;
    if scene.shapes().is_empty() {
        authored::check_transforms_only(scene)?;
    } else {
        authored::check_scene(scene)?;
    }
    // Every field is decided here, so a new `NativeScene` field breaks this build until it is.
    let NativeScene {
        // Covered by `check_content_kind` and the structure checks above.
        nodes: _,
        shapes: _,
        curves: _,
        dag: _,
        units: _,
        source: _,
        // Fresh per scene; the file stores no document identity.
        authored_document_id: _,
        next_id: _,
        next_native_id: _,
        // Diffuse edits bound into retained source bytes: set only with a source, which
        // `check_content_kind` refuses; without one `check_scene` rejects a non-empty table
        // as an invalid scene (tested), so nothing more is needed here.
        materials: _,
        // Ids of deleted source nodes: recorded only in a sourced scene, which
        // `check_content_kind` refuses.
        removed: _,
        // Created, assigned or reassigned materials: the file has only the default pair, so
        // any difference from the default would be lost on reopen.
        session_materials,
        // Session UVs make `check_content_kind` refuse the scene (`UnsupportedMeshChannels`),
        // so none reach this writer.
        session_uvs: _,
        // Session joints make `check_content_kind` refuse the scene (`Joints`), so none reach
        // this writer.
        session_joints: _,
    } = scene;
    if *session_materials != SessionMaterials::default() {
        return Err(AuthoredBinaryRefusal::SessionMaterials);
    }
    scene_file(scene)
}

#[cfg(test)]
mod msk_crosscheck;
#[cfg(test)]
mod tests;
