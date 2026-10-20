//! Writer for a fresh Maya Binary file from an authored (source-less) scene.
//!
//! Incomplete: the framing builders, the profile constants and the `HEAD` group exist,
//! and the entry point refuses every scene until emission and the self-check are
//! complete, so no partial file is ever returned.

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

use crate::native_scene::{NativeScene, authored, authored::AuthoredIneligible};
use chunk::TooLarge;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredBinaryRefusal {
    Ineligible(AuthoredIneligible),
    TooLarge,
    /// Emission is incomplete; nothing is written until every part lands.
    NotYetImplemented,
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
