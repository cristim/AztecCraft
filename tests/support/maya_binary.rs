use std::sync::Arc;

pub(crate) fn specimen() -> Vec<u8> {
    scene(&[
        group(b"XFRM", &[creation(b"object", None),
            triple(b"t", [16777217.125, -2.0, 3.0]),
            triple(b"r", [0.0, 0.0, 0.25])].concat(), 4),
        group(b"DMSH", &[creation(b"shape", Some(b"object")),
            chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8)].concat(), 4),
    ].concat()).to_vec()
}

pub(crate) fn chunk(tag: &[u8; 4], bytes: &[u8], alignment: usize) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend(0x1234_5678u32.to_be_bytes());
    out.extend((bytes.len() as u64).to_be_bytes());
    out.extend(bytes);
    out.resize(16 + bytes.len().next_multiple_of(alignment), 0xa5);
    out
}

pub(crate) fn group(form: &[u8; 4], children: &[u8], alignment: usize) -> Vec<u8> {
    let mut body = form.to_vec();
    body.extend(children);
    chunk(
        if form == b"CONS" { b"LIS8" } else { b"FOR8" },
        &body,
        alignment,
    )
}

pub(crate) fn counted(bytes: &mut Vec<u8>, words: &[u32]) {
    bytes.extend((words.len() as u32).to_be_bytes());
    for word in words {
        bytes.extend(word.to_be_bytes());
    }
}

pub(crate) fn mesh_payload(corners: &[u32]) -> Vec<u8> {
    mesh_payload_with_uv(corners, &[0, 0, 1f32.to_bits(), 0, 0, 1f32.to_bits()], &[0, 1, 2])
}

pub(crate) fn mesh_payload_with_uv(corners: &[u32], uv: &[u32], ids: &[u32]) -> Vec<u8> {
    mesh_payload_from(
        &[0, 0, 0, 1f32.to_bits(), 0, 0, 0, 1f32.to_bits(), 0],
        &[0x8000_0000, 1, 1, 2, 2, 0],
        corners,
        uv,
        ids,
    )
}

pub(crate) fn mesh_payload_from(
    positions: &[u32],
    endpoints: &[u32],
    corners: &[u32],
    uv: &[u32],
    ids: &[u32],
) -> Vec<u8> {
    let mut bytes = b"o\0\x20".to_vec();
    counted(&mut bytes, positions);
    counted(&mut bytes, endpoints);
    counted(&mut bytes, corners);
    counted(&mut bytes, &[1e20f32.to_bits(), 0, 0]);
    for word in [0u32, 1, 0] {
        bytes.extend(word.to_be_bytes());
    }
    bytes.extend(b"uv\0");
    counted(&mut bytes, uv);
    counted(&mut bytes, ids);
    bytes.extend([0; 20]);
    bytes
}

pub(crate) fn creation(name: &[u8], parent: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = vec![if parent.is_some() { 4 } else { 0x80 }];
    bytes.extend(name);
    bytes.push(0);
    if let Some(parent) = parent {
        bytes.extend(parent);
        bytes.push(0);
    }
    bytes.extend([0x7f; 16]);
    chunk(b"CREA", &bytes, 8)
}

pub(crate) fn scene(children: &[u8]) -> Arc<[u8]> {
    scene_with_units(children, b"cm", b"deg")
}

pub(crate) fn scene_with_units(children: &[u8], linear: &[u8], angular: &[u8]) -> Arc<[u8]> {
    let header = [
        chunk(b"LUNI", linear, 8),
        chunk(b"AUNI", angular, 8),
        chunk(b"TUNI", b"film", 8),
    ]
    .concat();
    group(
        b"Maya",
        &[group(b"HEAD", &header, 4), children.to_vec()].concat(),
        1,
    )
    .into()
}

pub(crate) fn triple(key: &[u8], values: [f64; 3]) -> Vec<u8> {
    let mut bytes = key.to_vec();
    bytes.extend([0, 0x20]);
    for value in values {
        bytes.extend(value.to_be_bytes());
    }
    chunk(b"DBL3", &bytes, 8)
}
