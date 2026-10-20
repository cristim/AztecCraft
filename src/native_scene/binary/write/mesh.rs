//! A `DMSH` group for one authored mesh shape: `CREA` then `MESH`, nothing else.
//!
//! Maya 2025 also writes `FLGS`, `DBLE vir/vif`, `STR` and `FLT2` children on every
//! shape; this group is shorter than any Maya wrote, which is INVENTED and PROVISIONAL
//! (`.project-docs/binary-authored-writer.md`). The `MESH` payload is the key, the float
//! kind byte, then counted big-endian word arrays: points, edge endpoints, face corners,
//! user normals (always none, as `n` holds user-specified normals only), the UV header,
//! the set name, UV points and UV ids (an empty `map1`), and a 20-byte zero trailer.

use super::AuthoredBinaryRefusal;
use super::chunk::{Align, TooLarge, group, leaf};
use super::profile::{DMSH_GROUP_AUX, MESH_AUX, MESH_KEY, SHAPE_CREA_AUX, SHAPE_CREA_FLAGS};
use crate::native_scene::{NativeError, NativeMesh, Points, authored::AuthoredIneligible};
use uuid::Uuid;

/// The 8-byte-aligned children of a `DMSH` group, as the container assigns them.
const CHILD: Align = Align::Eight;
const FLOAT_KIND: u8 = 0x20;
/// The three words that open the UV section; the fixture's `MESH` stores 0, 1, 0.
const UV_HEADER: [u32; 3] = [0, 1, 0];
const UV_SET: &[u8] = b"map1";
const TRAILER_BYTES: usize = 20;
/// Corner word flags: the edge is walked against its stored direction; last corner of a face.
const REVERSED: u32 = 0x8000_0000;
const TERMINAL: u32 = 0x6000_0000;
/// Edge word 0 bit 31. Provisional: set on boundary and non-manifold edges.
const NOT_INTERIOR: u32 = 0x8000_0000;
/// Corner words carry the edge index in 24 bits.
const MAX_EDGES: usize = 1 << 24;

fn invalid_topology() -> AuthoredBinaryRefusal {
    AuthoredIneligible::InvalidScene(NativeError::InvalidTopology).into()
}

/// `CREA` payload: flag byte, NUL-terminated name and parent name, then the 16 identity
/// bytes (the stored v4 UUID in the Microsoft GUID byte order, as in the Maya 2025 fixture).
fn create(name: &str, parent: &str, uuid: Uuid) -> Vec<u8> {
    let mut payload = vec![SHAPE_CREA_FLAGS];
    for text in [name, parent] {
        payload.extend_from_slice(text.as_bytes());
        payload.push(0);
    }
    payload.extend_from_slice(&uuid.to_bytes_le());
    payload
}

fn counted(
    out: &mut Vec<u8>,
    count: usize,
    words: impl IntoIterator<Item = u32>,
) -> Result<(), TooLarge> {
    out.extend_from_slice(&u32::try_from(count).map_err(|_| TooLarge)?.to_be_bytes());
    for word in words {
        out.extend_from_slice(&word.to_be_bytes());
    }
    Ok(())
}

/// Uses of each edge by the outer loops of all faces.
fn edge_uses(mesh: &NativeMesh) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    let mut uses = vec![0u8; mesh.edges().len()];
    for directed in mesh.faces().iter().flat_map(|face| face.outer()) {
        let slot = uses.get_mut(directed.edge).ok_or_else(invalid_topology)?;
        *slot = slot.saturating_add(1);
    }
    Ok(uses)
}

fn mesh_payload(mesh: &NativeMesh) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    let Points::Float3(points) = mesh.points() else {
        return Err(AuthoredIneligible::Double3Points.into());
    };
    if mesh.edges().len() > MAX_EDGES {
        return Err(TooLarge.into());
    }
    let mut out = MESH_KEY.to_vec();
    out.extend_from_slice(&[0, FLOAT_KIND]);
    let point_words = points.len().checked_mul(3).ok_or(TooLarge)?;
    counted(
        &mut out,
        point_words,
        points.iter().flatten().map(|value| value.to_bits()),
    )?;
    let uses = edge_uses(mesh)?;
    let mut endpoints = Vec::new();
    for (edge, uses) in mesh.edges().iter().zip(&uses) {
        let [first, second] = edge.endpoints();
        if first >= points.len() || second >= points.len() {
            return Err(invalid_topology());
        }
        let [first, second] = [first, second].map(u32::try_from);
        let (Ok(first), Ok(second)) = (first, second) else {
            return Err(TooLarge.into());
        };
        if first | second >= NOT_INTERIOR {
            return Err(TooLarge.into());
        }
        endpoints.push(first | if *uses == 2 { 0 } else { NOT_INTERIOR });
        endpoints.push(second);
    }
    counted(&mut out, endpoints.len(), endpoints.iter().copied())?;
    let mut corners = Vec::new();
    for face in mesh.faces() {
        let last = face
            .outer()
            .len()
            .checked_sub(1)
            .ok_or_else(invalid_topology)?;
        for (index, directed) in face.outer().iter().enumerate() {
            let mut word = u32::try_from(directed.edge).map_err(|_| invalid_topology())?;
            if directed.reversed {
                word |= REVERSED;
            }
            if index == last {
                word |= TERMINAL;
            }
            corners.push(word);
        }
    }
    counted(&mut out, corners.len(), corners.iter().copied())?;
    counted(&mut out, 0, [])?;
    for word in UV_HEADER {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out.extend_from_slice(UV_SET);
    out.push(0);
    counted(&mut out, 0, [])?;
    counted(&mut out, 0, [])?;
    out.extend_from_slice(&[0; TRAILER_BYTES]);
    Ok(out)
}

/// One `DMSH` group, padded for a parent `Maya` group whose children align to four.
pub(super) fn dmsh(
    name: &str,
    parent: &str,
    uuid: Uuid,
    mesh: &NativeMesh,
) -> Result<Vec<u8>, AuthoredBinaryRefusal> {
    let children = [
        leaf(*b"CREA", SHAPE_CREA_AUX, &create(name, parent, uuid), CHILD)?,
        leaf(*b"MESH", MESH_AUX, &mesh_payload(mesh)?, CHILD)?,
    ];
    Ok(group(*b"DMSH", DMSH_GROUP_AUX, &children, Align::Four)?)
}

#[cfg(test)]
mod tests;
