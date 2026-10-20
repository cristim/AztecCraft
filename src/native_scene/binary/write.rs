//! Writer for a fresh Maya Binary file from an authored (source-less) scene.
//!
//! Incomplete: the framing builders, the profile constants and the `HEAD` group exist,
//! and the entry point refuses every scene until emission and the self-check are
//! complete, so no partial file is ever returned.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the emission that follows")
)]
mod check;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "builders gain callers as emission lands")
)]
mod chunk;
#[cfg(test)]
mod fixtures;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the emission that follows")
)]
mod head;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the emission that follows")
)]
mod profile;
#[cfg(test)]
mod sha256;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the emission that follows")
)]
mod transform;

use crate::native_scene::{NativeScene, authored, authored::AuthoredIneligible};
use chunk::TooLarge;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredBinaryRefusal {
    Ineligible(AuthoredIneligible),
    TooLarge,
    /// Emission is incomplete; nothing is written until every part lands.
    NotYetImplemented,
    /// A node has no stored UUID, and the file's creation chunks need one.
    MissingUuid,
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

/// The file for a scene of bare transforms, whose eligibility the caller has checked.
/// The bytes are decoded and compared with the scene before they are returned.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used by the entry point once emission is complete"
    )
)]
fn transforms_file(scene: &NativeScene) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    let mut children = Vec::new();
    children
        .try_reserve_exact(scene.nodes().len() + 1)
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
    Err(AuthoredBinaryRefusal::NotYetImplemented)
}

#[cfg(test)]
mod tests;
