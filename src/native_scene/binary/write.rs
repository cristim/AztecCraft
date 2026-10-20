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

/// Aux word of the root and `XFRM` group headers. Observed as 0 in a public Maya 2026
/// file (`tests/fixtures/mayascenekit/UPSTREAM.md`) and in every private file checked.
const GROUP_AUX: u32 = 0;

/// Aux word of `DBL3` leaves, and of no other kind. A public Maya 2026 file stores
/// `0xb8010000` on all 7 of its `DBL3` chunks, but other kinds carry other values there
/// (0, `0x24000000`), so this must never be used for `HEAD` children, `MESH` or `DBLE`.
/// Source and pin: `tests/fixtures/mayascenekit/UPSTREAM.md`, checked by a test.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "first used by DBL3 emission in the transform commit"
    )
)]
const DBL3_AUX: u32 = 0xb801_0000;

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
