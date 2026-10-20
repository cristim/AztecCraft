//! Writer for a fresh Maya Binary file from an authored (source-less) scene.
//!
//! Incomplete: only the framing builders exist so far, and the entry point below is
//! a stub that no caller may use until the emission and self-check commits land.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "builders gain callers as emission lands")
)]
mod chunk;

use crate::native_scene::{NativeScene, authored, authored::AuthoredIneligible};
use chunk::{Align, TooLarge, group};

/// Aux word of group headers; every Maya-written file observed has 0 here.
const GROUP_AUX: u32 = 0;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AuthoredBinaryRefusal {
    Ineligible(AuthoredIneligible),
    TooLarge,
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
    authored::check_scene(scene)?;
    Ok(group(*b"Maya", GROUP_AUX, &[], Align::One)?)
}

#[cfg(test)]
mod tests;
