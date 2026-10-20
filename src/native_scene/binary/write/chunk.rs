//! Byte-exact chunk builders for the Maya Binary container (FOR8 groups and leaves).
//!
//! The builders know the framing only: header (tag, aux, big-endian u64 payload
//! size), payload, then zero padding to the *enclosing* group's child alignment.
//! The aux word is a parameter; the writer entry is the only place that chooses it.

use crate::maya_binary::container::MAX_BYTES;

const HEADER_BYTES: usize = 16;
const GROUP_TAG: [u8; 4] = *b"FOR8";

/// Child alignment of the enclosing group, which pads the chunk after its payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::native_scene::binary) enum Align {
    One = 1,
    Four = 4,
    Eight = 8,
}

/// A chunk, or the file it would sit in, is over the container limit or overflows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::native_scene::binary) struct TooLarge;

/// Bytes a chunk with `payload` payload bytes occupies: header, payload, padding.
pub(super) fn chunk_len(payload: usize, align: Align) -> Result<usize, TooLarge> {
    let mask = align as usize - 1;
    let padded = payload.checked_add(mask).ok_or(TooLarge)? & !mask;
    let total = HEADER_BYTES.checked_add(padded).ok_or(TooLarge)?;
    if total > MAX_BYTES {
        return Err(TooLarge);
    }
    Ok(total)
}

fn open(
    tag: [u8; 4],
    aux: u32,
    payload: usize,
    align: Align,
) -> Result<(Vec<u8>, usize), TooLarge> {
    let total = chunk_len(payload, align)?;
    let size = u64::try_from(payload).map_err(|_| TooLarge)?;
    let mut out = Vec::new();
    out.try_reserve_exact(total).map_err(|_| TooLarge)?;
    out.extend_from_slice(&tag);
    out.extend_from_slice(&aux.to_be_bytes());
    out.extend_from_slice(&size.to_be_bytes());
    Ok((out, total))
}

pub(in crate::native_scene::binary) fn leaf(
    tag: [u8; 4],
    aux: u32,
    payload: &[u8],
    align: Align,
) -> Result<Vec<u8>, TooLarge> {
    let (mut out, total) = open(tag, aux, payload.len(), align)?;
    out.extend_from_slice(payload);
    out.resize(total, 0);
    Ok(out)
}

/// A `FOR8` group. Each child must already be padded to this group's own child
/// alignment, which is the one the container assigns to `form`.
pub(super) fn group(
    form: [u8; 4],
    aux: u32,
    children: &[Vec<u8>],
    align: Align,
) -> Result<Vec<u8>, TooLarge> {
    let payload = children
        .iter()
        .try_fold(form.len(), |sum, child| sum.checked_add(child.len()))
        .ok_or(TooLarge)?;
    let (mut out, total) = open(GROUP_TAG, aux, payload, align)?;
    out.extend_from_slice(&form);
    for child in children {
        out.extend_from_slice(child);
    }
    out.resize(total, 0);
    Ok(out)
}

#[cfg(test)]
mod tests;
