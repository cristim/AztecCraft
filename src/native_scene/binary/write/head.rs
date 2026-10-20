//! The leading `HEAD` group of a fresh file: the child kinds Maya 2025 writes before any
//! scene content, with the payload and aux word that file stores for each.
//!
//! Omitted on purpose: `CHNG` (a wall-clock timestamp, which would make output
//! irreproducible), and the trailing `FINF`, `PLUG`, `BCSZ` and second `HEAD`, which
//! record the writing application and installed plug-ins. Writing them would fabricate
//! provenance, so a file is never presented as saved by Maya.

use super::chunk::{Align, TooLarge, group, leaf};
use super::profile::{
    AUNI_AUX, HEAD_GROUP_AUX, ICON_AUX, INCL_AUX, INFO_AUX, LUNI_AUX, MADE_AUX, OBJN_AUX, TDUR_AUX,
    TUNI_AUX, UVER_AUX, VERS, VERS_AUX,
};

/// Tag, aux and payload, in the order the Maya 2025 file lists them. The unit payloads
/// are the project's `Units::default()` (centimeter, degree, film).
const CHILDREN: [([u8; 4], u32, &[u8]); 11] = [
    (*b"VERS", VERS_AUX, VERS),
    (*b"UVER", UVER_AUX, b"undef"),
    (*b"MADE", MADE_AUX, b"undef"),
    (*b"ICON", ICON_AUX, b"undef"),
    (*b"INFO", INFO_AUX, b"undef"),
    (*b"OBJN", OBJN_AUX, b"undef"),
    (*b"INCL", INCL_AUX, b"undef("),
    (*b"LUNI", LUNI_AUX, b"cm"),
    (*b"TUNI", TUNI_AUX, b"film"),
    (*b"AUNI", AUNI_AUX, b"deg"),
    (*b"TDUR", TDUR_AUX, b"141120000"),
];

/// The `HEAD` group, padded for a parent `Maya` group whose children align to four.
pub(super) fn head() -> Result<Vec<u8>, TooLarge> {
    let children = CHILDREN
        .iter()
        .map(|(tag, aux, payload)| leaf(*tag, *aux, payload, Align::Eight))
        .collect::<Result<Vec<_>, _>>()?;
    group(*b"HEAD", HEAD_GROUP_AUX, &children, Align::Four)
}

#[cfg(test)]
mod tests;
