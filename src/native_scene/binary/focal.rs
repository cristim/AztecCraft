//! Canonical authored animCurveTU profile; Maya acceptance remains provisional.
use super::{BinaryError, ErrorKind, fail, write};
use crate::{
    maya_binary::container::{self, Container},
    native_scene::{
        GeometryViewFacts, NativeScene, Scalar, SceneCamera,
        camera::CameraBinding,
        session_keys::{MAX_FRAME, MAX_SESSION_KEYS, persistence::curve_name},
    },
};
use std::sync::Arc;
use write::chunk::{self, Align, TooLarge};
const TICKS_PER_FRAME: i64 = 5_880_000; // ponytail: film24 only; add proved time-unit conversion before broadening.
const AUX: u32 = 0xe3010000;
fn bad() -> BinaryError {
    fail(ErrorKind::Unsupported, 0)
}
fn attr(key: &str, kind: u8, value: &[u8]) -> Vec<u8> {
    let mut p = key.as_bytes().to_vec();
    p.extend_from_slice(&[0, kind]);
    p.extend_from_slice(value);
    p
}
fn uuid(camera: SceneCamera) -> [u8; 16] {
    let mut b = [0; 16];
    b[0] = camera.index() as u8 + 1;
    b[7] = 0x40;
    b[8] = 0x80;
    b
}
pub(super) fn authored(scene: &NativeScene) -> Result<Vec<Vec<u8>>, TooLarge> {
    let mut groups = Vec::new();
    groups.try_reserve_exact(5).map_err(|_| TooLarge)?;
    let mut connections = Vec::new();
    connections.try_reserve_exact(4).map_err(|_| TooLarge)?;
    for camera in SceneCamera::ALL {
        let count = scene.camera_focal_keys(camera).count();
        if count > MAX_SESSION_KEYS {
            return Err(TooLarge);
        }
        let mut keys = Vec::new();
        keys.try_reserve_exact(count).map_err(|_| TooLarge)?;
        keys.extend(scene.camera_focal_keys(camera));
        if keys.is_empty() {
            continue;
        }
        let name = curve_name(camera);
        let mut create = vec![0x80];
        create.extend_from_slice(name.as_bytes());
        create.push(0);
        create.extend_from_slice(&uuid(camera));
        let mut data = Vec::new();
        data.try_reserve_exact(keys.len().checked_mul(16).ok_or(TooLarge)?)
            .map_err(|_| TooLarge)?;
        for (frame, value) in &keys {
            let ticks = i64::from(*frame)
                .checked_mul(TICKS_PER_FRAME)
                .ok_or(TooLarge)?;
            data.extend_from_slice(&ticks.to_be_bytes());
            data.extend_from_slice(&value.to_be_bytes());
        }
        let children = [
            chunk::leaf(*b"CREA", 0, &create, Align::Eight)?,
            chunk::leaf(
                *b"DBLE",
                AUX,
                &attr("tan", 0x20, &2f64.to_be_bytes()),
                Align::Eight,
            )?,
            chunk::leaf(
                *b"DBLE",
                AUX,
                &attr("wgt", 0x20, &0f64.to_be_bytes()),
                Align::Eight,
            )?,
            chunk::leaf(
                *b"FLGS",
                AUX,
                &attr(
                    "ktv",
                    0x28,
                    &u32::try_from(keys.len())
                        .map_err(|_| TooLarge)?
                        .to_be_bytes(),
                ),
                Align::Eight,
            )?,
            chunk::leaf(
                *b"CMPD",
                AUX,
                &attr(&format!("ktv[0:{}]", keys.len() - 1), 0x20, &data),
                Align::Eight,
            )?,
        ];
        groups.push(chunk::group(*b"PCTU", 0, &children, Align::Four)?);
        let mut connection = vec![0];
        connection.extend_from_slice(format!("{name}.o\0:{}.fl\0", camera.shape_name()).as_bytes());
        connections.push(chunk::leaf(*b"CWFL", 0, &connection, Align::Eight)?);
    }
    if !connections.is_empty() {
        let conn = chunk::group(*b"CONN", 0, &connections, Align::Four)?;
        let mut cons = chunk::group(*b"CONS", 0, &[conn], Align::Four)?;
        cons[..4].copy_from_slice(b"LIS8");
        groups.push(cons);
    }
    Ok(groups)
}
fn payload(c: &Container, i: usize) -> Result<&[u8], BinaryError> {
    let chunk = c.chunk(i).ok_or_else(bad)?;
    c.source().get(chunk.payload_range()).ok_or_else(bad)
}

pub(super) fn decode(
    c: &Container,
    retained_bytes: usize,
    retained_appearance_bytes: usize,
) -> Result<Option<NativeScene>, BinaryError> {
    let Some(first) = c.chunks().iter().position(|v| v.form() == Some(*b"PCTU")) else {
        return Ok(None);
    };
    let offset = c.chunk(first).ok_or_else(bad)?.header_range().start;
    if c.chunk(first).ok_or_else(bad)?.parent() != Some(0) {
        return Err(bad());
    }
    let retained_base = retained_bytes
        .checked_add(retained_appearance_bytes)
        .and_then(|n| n.checked_add(c.source().len()))
        .and_then(|n| n.checked_add(c.metadata_bytes()?))
        .ok_or_else(super::resource)?;
    // Prefix Vec/Arc overlap, bounded keys and whole-file re-emission stay accounted while parsing.
    let scratch = offset
        .checked_mul(3)
        .and_then(|n| n.checked_add(MAX_SESSION_KEYS * 128))
        .and_then(|n| n.checked_add(container::MAX_BYTES * 2))
        .ok_or_else(super::resource)?;
    super::admission::Budget::new(
        retained_base
            .checked_add(scratch)
            .ok_or_else(super::resource)?,
    )?;
    let prefix = c.source().get(..offset).ok_or_else(bad)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(prefix.len())
        .map_err(|_| super::resource())?;
    bytes.extend_from_slice(prefix);
    let size = u64::try_from(bytes.len().checked_sub(16).ok_or_else(bad)?).map_err(|_| bad())?;
    bytes
        .get_mut(8..16)
        .ok_or_else(bad)?
        .copy_from_slice(&size.to_be_bytes());
    let bytes: Arc<[u8]> = bytes.into();
    let static_container = container::inspect(bytes.clone())?;
    let retained = retained_base
        .checked_add(scratch)
        .and_then(|n| n.checked_add(static_container.metadata_bytes()?))
        .ok_or_else(super::resource)?;
    let mut scene = super::decode_with_retained_bytes(bytes, retained, 0)?;
    // Convert only a byte-proven authored static profile. Full re-emission below is the gate.
    for (i, chunk) in static_container.chunks().iter().enumerate() {
        if chunk.tag() != *b"CREA" {
            continue;
        }
        let p = payload(&static_container, i)?;
        let tail = p.len().checked_sub(16).ok_or_else(bad)?;
        let name_end = p
            .get(1..tail)
            .and_then(|p| p.iter().position(|v| *v == 0))
            .map(|v| v + 1)
            .ok_or_else(bad)?;
        let name = std::str::from_utf8(p.get(1..name_end).ok_or_else(bad)?).map_err(|_| bad())?;
        let id = uuid::Uuid::from_bytes_le(
            p.get(tail..)
                .ok_or_else(bad)?
                .try_into()
                .map_err(|_| bad())?,
        );
        let parent = chunk
            .parent()
            .and_then(|p| static_container.chunk(p))
            .and_then(|p| p.form());
        if parent == Some(*b"XFRM")
            && let Some(node) = scene.nodes.iter_mut().find(|n| n.name == name)
        {
            node.transform_uuid = Some(id);
        }
        if parent == Some(*b"DMSH")
            && let Some(shape) = scene.shapes.iter_mut().find(|s| s.name == name)
        {
            shape.uuid = Some(id);
        }
    }
    for node in &mut scene.nodes {
        node.channels = node.channels.clone().map(|v| Scalar::new(v.value));
        node.rotate_pivot = node.rotate_pivot.clone().map(|v| Scalar::new(v.value));
        node.scale_pivot = node.scale_pivot.clone().map(|v| Scalar::new(v.value));
        node.view = GeometryViewFacts::authored();
    }
    for shape in &mut scene.shapes {
        shape.view = GeometryViewFacts::authored();
        shape.mesh.set_authored_smoothing();
    }
    scene.source = None;
    scene.camera_bindings = std::array::from_fn(|_| None::<CameraBinding>);
    let mut seen = [false; 4];
    for (i, chunk) in c.chunks().iter().enumerate().skip(first) {
        if chunk.form() != Some(*b"PCTU") {
            continue;
        }
        if chunk.parent() != Some(0) {
            return Err(bad());
        }
        let mut children = [0usize; 5];
        let mut child_count = 0;
        for (index, child) in c
            .chunks()
            .iter()
            .enumerate()
            .filter(|(_, v)| v.parent() == Some(i))
        {
            *children.get_mut(child_count).ok_or_else(bad)? = index;
            let _ = child;
            child_count += 1;
        }
        if child_count != 5 {
            return Err(bad());
        }
        let create = payload(c, children[0])?;
        let camera = SceneCamera::ALL
            .into_iter()
            .find(|camera| {
                let mut want = vec![0x80];
                want.extend_from_slice(curve_name(*camera).as_bytes());
                want.push(0);
                want.extend_from_slice(&uuid(*camera));
                create == want
            })
            .ok_or_else(bad)?;
        if std::mem::replace(&mut seen[camera.index()], true) {
            return Err(bad());
        }
        let count_bytes = payload(c, children[3])?;
        if count_bytes.len() != 9 || !count_bytes.starts_with(b"ktv\0\x28") {
            return Err(bad());
        }
        let count = u32::from_be_bytes(count_bytes[5..].try_into().map_err(|_| bad())?) as usize;
        if count == 0 || count > MAX_SESSION_KEYS - scene.session_key_count() {
            return Err(bad());
        }
        let data = payload(c, children[4])?;
        let prefix = format!("ktv[0:{}]\0\x20", count - 1);
        let data = data.strip_prefix(prefix.as_bytes()).ok_or_else(bad)?;
        if data.len() != count.checked_mul(16).ok_or_else(bad)? {
            return Err(bad());
        }
        let mut last = None;
        for pair in data.as_chunks::<16>().0 {
            let ticks = i64::from_be_bytes(pair[..8].try_into().map_err(|_| bad())?);
            if ticks % TICKS_PER_FRAME != 0 {
                return Err(bad());
            }
            let frame = i32::try_from(ticks / TICKS_PER_FRAME).map_err(|_| bad())?;
            let value = f64::from_be_bytes(pair[8..].try_into().map_err(|_| bad())?);
            if frame.unsigned_abs() > MAX_FRAME as u32
                || last.is_some_and(|last| frame <= last)
                || !value.is_finite()
                || !(2.5..=100000.).contains(&value)
            {
                return Err(bad());
            }
            scene
                .insert_decoded_camera_key(camera, frame, value)
                .map_err(|_| bad())?;
            last = Some(frame);
        }
    }
    scene.validate().map_err(|_| bad())?;
    if write::scene_file_unchecked(&scene)
        .map_err(|_| bad())?
        .as_slice()
        != c.source().as_ref()
    {
        return Err(bad());
    }
    Ok(Some(scene))
}
