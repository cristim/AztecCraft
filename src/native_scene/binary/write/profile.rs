//! The Maya Binary profile the authored writer emits: one `VERS` string and the aux word
//! of every chunk kind, all taken from a single public Maya 2025 file so that no two Maya
//! releases are mixed. Source: `tests/fixtures/gt-tools/UPSTREAM.md`; each value is
//! pinned to that file's bytes by `profile/tests.rs`. Autodesk documents none of them.
//!
//! The aux word is a per-chunk-kind property in that file, never a file-wide constant, so
//! each kind has its own name and a value is never reused for another kind.

/// `VERS` payload: the exact string the Maya 2025 file stores.
pub(super) const VERS: &[u8] = b"2025ff03";

/// Root `FOR8 Maya` header aux.
pub(super) const ROOT_AUX: u32 = 0x4702_0000;
/// Leading `HEAD` group header aux.
pub(super) const HEAD_GROUP_AUX: u32 = 0;
/// `XFRM` group header aux.
pub(super) const XFRM_GROUP_AUX: u32 = 0;
/// `CREA` leaf aux inside an `XFRM` group.
pub(super) const XFRM_CREA_AUX: u32 = 0;
/// `DBL3` leaf aux inside an `XFRM` group.
pub(super) const DBL3_AUX: u32 = 0x4d02_0000;

pub(super) const VERS_AUX: u32 = 0;
pub(super) const UVER_AUX: u32 = 0;
pub(super) const MADE_AUX: u32 = 0;
pub(super) const ICON_AUX: u32 = 0;
pub(super) const INFO_AUX: u32 = 0;
pub(super) const OBJN_AUX: u32 = 0;
pub(super) const INCL_AUX: u32 = 0;
pub(super) const LUNI_AUX: u32 = 0xa800_0000;
pub(super) const TUNI_AUX: u32 = 0xa800_0000;
pub(super) const AUNI_AUX: u32 = 0xa800_0000;
pub(super) const TDUR_AUX: u32 = 0xa800_0000;

/// `CREA` flag byte of a root-level transform the user created. In the Maya 2025 file
/// the one user node `Dummy_Mesh` carries 0x8c, while its four default cameras (persp,
/// top, front, side; `CREA` at 0x394, 0x5bc, 0x814, 0xa4c) carry 0x8d. That the two values
/// separate user-created from default/startup nodes is an INFERENCE from one file, not
/// something it states, and "owns a mesh" is not the distinction.
pub(super) const XFRM_CREA_FLAGS: u8 = 0x8c;

#[cfg(test)]
mod tests;
