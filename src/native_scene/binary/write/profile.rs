//! The Maya Binary profile the authored writer emits: one `VERS` string and the aux word
//! of every chunk kind the writer emits, all
//! taken from a single public Maya 2025 file so that no two Maya releases are mixed. Source: `tests/fixtures/gt-tools/UPSTREAM.md`; each value is
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

/// `DMSH` group header aux of a plain (non-skinned) shape: 0 at 0xe5c in the Maya 2025
/// file. Its only `MESH` sits in a skinned intermediate shape (group aux `0x4d020000`,
/// `CREA` aux `0x47020000` at 0xfd8 and 0xfec), which is not what a fresh mesh is.
/// Taking the plain shape's pair for a plain, user-created shape is PROVISIONAL.
pub(super) const DMSH_GROUP_AUX: u32 = 0;
/// `CREA` leaf aux inside a plain `DMSH` group: 0 at 0xe70. PROVISIONAL, see above.
pub(super) const SHAPE_CREA_AUX: u32 = 0;
/// `CREA` flag byte of a shape (`0x04`, a name and a parent name follow): at 0xe70 and 0xfec.
pub(super) const SHAPE_CREA_FLAGS: u8 = 0x04;
/// `MESH` leaf aux: `0xa8000000` at 0x11324, the file's only `MESH`.
pub(super) const MESH_AUX: u32 = 0xa800_0000;
/// Key of the geometry in the `MESH` payload. The file's only `MESH` stores `ci` (its
/// shape is intermediate, `io` 1), which would not describe a visible shape. `o` is the key
/// the repo's synthetic wire fixtures use and the private corpus stores (aggregate only);
/// it is not in the Maya 2025 file. PROVISIONAL, a one-constant change.
pub(super) const MESH_KEY: &[u8] = b"o";

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
