//! An `XFRM` group for one authored transform: `CREA` then the `t`, `r` and `s` `DBL3`
//! leaves, always all three, in the internal units (centimeters, radians).
//!
//! The Maya 2025 file omits channels at their defaults and adds `DBLE v`, `ATTR` and
//! `FLGS` chunks. This group is shorter than any Maya wrote, which is INVENTED and
//! PROVISIONAL; see `.project-docs/binary-authored-writer.md`.

use super::chunk::{Align, TooLarge, group, leaf};
use super::profile::{DBL3_AUX, XFRM_CREA_AUX, XFRM_CREA_FLAGS, XFRM_GROUP_AUX};
use uuid::Uuid;

/// The 8-byte-aligned children of an `XFRM` group, as the container assigns them.
const CHILD: Align = Align::Eight;
const DOUBLE3_KIND: u8 = 0x20;

/// Internal-unit channels of one transform.
#[derive(Clone, Copy, Debug)]
pub(super) struct Channels {
    pub translation: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}

/// `CREA` payload: flag byte, NUL-terminated name, then the 16 identity bytes. The
/// identity is the stored v4 UUID in the Microsoft GUID byte order (first three fields
/// little-endian), which is where the Maya 2025 fixture puts the version and variant bits.
fn create(name: &str, uuid: Uuid) -> Vec<u8> {
    let mut payload = vec![XFRM_CREA_FLAGS];
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    payload.extend_from_slice(&uuid.to_bytes_le());
    payload
}

/// `DBL3` payload: NUL-terminated key, the kind byte, then three big-endian f64.
fn double3(key: &str, values: [f64; 3]) -> Vec<u8> {
    let mut payload = key.as_bytes().to_vec();
    payload.push(0);
    payload.push(DOUBLE3_KIND);
    for value in values {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    payload
}

/// One `XFRM` group, padded for a parent `Maya` group whose children align to four.
pub(super) fn xfrm(name: &str, uuid: Uuid, channels: Channels) -> Result<Vec<u8>, TooLarge> {
    let children = [
        leaf(*b"CREA", XFRM_CREA_AUX, &create(name, uuid), CHILD)?,
        leaf(
            *b"DBL3",
            DBL3_AUX,
            &double3("t", channels.translation),
            CHILD,
        )?,
        leaf(*b"DBL3", DBL3_AUX, &double3("r", channels.rotation), CHILD)?,
        leaf(*b"DBL3", DBL3_AUX, &double3("s", channels.scale), CHILD)?,
    ];
    group(*b"XFRM", XFRM_GROUP_AUX, &children, Align::Four)
}

#[cfg(test)]
mod tests;
