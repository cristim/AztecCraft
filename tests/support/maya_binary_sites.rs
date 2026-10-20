// Insertion fixtures (issue #19), included only by the binary decoder tests so no other includer needs dead-code allowances.

/// Header aux word shared by every `DBL3` and `DBLE` that the insertion fixtures write.
pub(crate) const SITE_AUX: u32 = 0x0c02_0000;

/// A chunk with a chosen aux word and zero padding, as Maya's own writer pads.
pub(crate) fn chunk_zero_pad(tag: &[u8; 4], aux: u32, bytes: &[u8], alignment: usize) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend(aux.to_be_bytes());
    out.extend((bytes.len() as u64).to_be_bytes());
    out.extend(bytes);
    out.resize(16 + bytes.len().next_multiple_of(alignment), 0);
    out
}

/// A group (aux 0, zero padding) in the same shape as [`group`].
pub(crate) fn group_zero_pad(form: &[u8; 4], children: &[u8], alignment: usize) -> Vec<u8> {
    let mut body = form.to_vec();
    body.extend(children);
    chunk_zero_pad(
        if form == b"CONS" { b"LIS8" } else { b"FOR8" },
        0,
        &body,
        alignment,
    )
}

/// A `CREA` chunk with aux 0 and zero padding.
pub(crate) fn creation_zero_pad(name: &[u8], parent: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = vec![if parent.is_some() { 4 } else { 0x80 }];
    bytes.extend(name);
    bytes.push(0);
    if let Some(parent) = parent {
        bytes.extend(parent);
        bytes.push(0);
    }
    bytes.extend([0x7f; 16]);
    chunk_zero_pad(b"CREA", 0, &bytes, 8)
}

/// A `DBL3` with a chosen aux word and zero padding.
pub(crate) fn triple_aux(key: &[u8], aux: u32, values: [f64; 3]) -> Vec<u8> {
    let mut bytes = key.to_vec();
    bytes.extend([0, 0x20]);
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
    chunk_zero_pad(b"DBL3", aux, &bytes, 8)
}

/// A single-double chunk (`DBLE`) with a chosen tag key, kind byte and aux word.
pub(crate) fn double_aux(key: &[u8], kind: u8, aux: u32, value: f64) -> Vec<u8> {
    let mut bytes = key.to_vec();
    bytes.extend([0, kind]);
    bytes.extend(value.to_be_bytes());
    chunk_zero_pad(b"DBLE", aux, &bytes, 8)
}

/// An `XFRM` group (child alignment 8, zero padding, under the root's 4) holding `CREA` then `parts`.
pub(crate) fn sited_transform(name: &[u8], parts: &[Vec<u8>]) -> Vec<u8> {
    let mut children = creation_zero_pad(name, None);
    for part in parts {
        children.extend(part);
    }
    group_zero_pad(b"XFRM", &children, 4)
}

/// A non-transform node (an `AMBL` group) that carries a `DBL3` with an unrelated key and its own aux word.
pub(crate) fn other_node_with_triple(name: &[u8], aux: u32) -> Vec<u8> {
    let children = [
        creation_zero_pad(name, None),
        triple_aux(b"x", aux, [0.0; 3]),
    ]
    .concat();
    group_zero_pad(b"AMBL", &children, 4)
}
