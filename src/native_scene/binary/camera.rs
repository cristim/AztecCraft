//! Built-in camera DBLE records. The authored SLCT/enum profile is PROVISIONAL (#458).
use super::{BinaryError, ErrorKind, fail};
use crate::{
    camera_optics::CameraOptics,
    maya_binary::{
        container::Container,
        semantic::{Data, WireFacts},
    },
    native_scene::{
        NativeScene, SceneCamera,
        camera::{CameraBinding, OPTICAL_ATTRS},
    },
};

fn unsupported() -> BinaryError {
    fail(ErrorKind::Unsupported, 0)
}
fn values(optics: CameraOptics) -> [f64; 18] {
    let mut out = [0.0; 18];
    out[..16].copy_from_slice(&optics.values());
    out[16] = optics.film_fit as u8 as f64;
    out[17] = optics.film_roll_order as u8 as f64;
    out
}
fn field(key: &[u8]) -> Option<usize> {
    OPTICAL_ATTRS
        .iter()
        .position(|v| v.as_bytes() == key)
        .or(match key {
            b"ff" => Some(16),
            b"fro" => Some(17),
            _ => None,
        })
}
fn name<'a>(facts: &'a WireFacts<'_>, data: &'a Data) -> Option<&'a [u8]> {
    match data {
        Data::Create(v) => Some(&facts.container.source()[v.name.clone()]),
        Data::Selection(v) => Some(&facts.container.source()[v.clone()]),
        _ => None,
    }
}
fn target(facts: &WireFacts<'_>, data: &Data) -> Option<SceneCamera> {
    let n = name(facts, data)?;
    let n = n.strip_prefix(b":").unwrap_or(n);
    SceneCamera::from_shape_name(std::str::from_utf8(n).ok()?)
}

pub(super) fn decode(
    scene: &mut NativeScene,
    container: &Container,
    facts: &WireFacts<'_>,
) -> Result<(), BinaryError> {
    let mut groups = Vec::new();
    groups
        .try_reserve_exact(container.chunks().len())
        .map_err(|_| super::resource())?;
    groups.resize(container.chunks().len(), None);
    for record in &facts.records {
        let Some(camera) = target(facts, &record.data) else {
            continue;
        };
        let chunk = container.chunk(record.group).ok_or_else(unsupported)?;
        match &record.data {
            Data::Create(create) if chunk.form() == Some(*b"DCAM") => {
                let parent = create.parent.as_ref().ok_or_else(unsupported)?;
                let parent = &container.source()[parent.clone()];
                let expected = camera
                    .shape_name()
                    .strip_suffix("Shape")
                    .ok_or_else(unsupported)?;
                if parent.strip_prefix(b":").unwrap_or(parent) != expected.as_bytes() {
                    return Err(unsupported());
                }
            }
            Data::Selection(_) if chunk.form() == Some(*b"SLCT") => {}
            _ => continue,
        }
        *groups.get_mut(record.group).ok_or_else(unsupported)? = Some(camera);
        scene.cameras[camera.index()].get_or_insert_with(CameraOptics::node_default);
        scene.camera_bindings[camera.index()].get_or_insert_with(CameraBinding::default);
    }
    // All potentially input-sized work is linear in the already bounded container/record count.
    for record in &facts.records {
        let Some(camera) = groups.get(record.group).copied().flatten() else {
            continue;
        };
        if let Data::Double(double) = &record.data {
            let key = &container.source()[double.key.clone()];
            if let Some(index) = field(key) {
                if !matches!(double.kind, 0x20 | 0x24) || double.numeric.len() != 8 {
                    return Err(unsupported());
                }
                let binding = scene.camera_bindings[camera.index()]
                    .as_mut()
                    .ok_or_else(unsupported)?;
                if binding.words[index]
                    .replace(double.numeric.clone())
                    .is_some()
                {
                    return Err(unsupported());
                }
                let word = container
                    .source()
                    .get(double.numeric.clone())
                    .ok_or_else(unsupported)?;
                let value = f64::from_be_bytes(word.try_into().map_err(|_| unsupported())?);
                if !value.is_finite() {
                    return Err(unsupported());
                }
                let optics = scene.cameras[camera.index()]
                    .as_mut()
                    .ok_or_else(unsupported)?;
                if index < 16 {
                    optics.set_scalar(index, value).map_err(|_| unsupported())?;
                } else {
                    if value.fract() != 0.0 || !(0.0..=3.0).contains(&value) {
                        return Err(unsupported());
                    }
                    if index == 16 {
                        optics.set_fit(value as i32)
                    } else {
                        optics.set_roll_order(value as i32)
                    }
                    .map_err(|_| unsupported())?;
                }
            }
        }
    }
    for chunk in container.chunks() {
        let Some(camera) = chunk
            .parent()
            .and_then(|p| groups.get(p))
            .copied()
            .flatten()
        else {
            continue;
        };
        if chunk.form().is_some() {
            scene.camera_bindings[camera.index()]
                .as_mut()
                .ok_or_else(unsupported)?
                .blocked = true;
            continue;
        }
        let range = chunk.payload_range();
        let bytes = container.source().get(range).ok_or_else(unsupported)?;
        let key = bytes.split(|b| *b == 0).next().unwrap_or_default();
        let passive = matches!(
            key,
            b"v" | b"rnd" | b"coi" | b"imn" | b"den" | b"man" | b"hc" | b"tp" | b"ai_translator"
        );
        let known = matches!(&chunk.tag(), b"CREA" | b"SLCT")
            || (chunk.tag() == *b"DBLE" && (field(key).is_some() || passive))
            || (matches!(&chunk.tag(), b"STR " | b"DBL3") && passive);
        if !known {
            scene.camera_bindings[camera.index()]
                .as_mut()
                .ok_or_else(unsupported)?
                .blocked = true;
        }
    }
    for record in &facts.records {
        if let Data::Connection(connection) = &record.data {
            for endpoint in [&connection.source, &connection.destination] {
                let bytes = &container.source()[endpoint.clone()];
                let owner = bytes.split(|b| *b == b'.').next().unwrap_or_default();
                let owner = owner.rsplit(|b| *b == b'|').next().unwrap_or(owner);
                let owner = owner.strip_prefix(b":").unwrap_or(owner);
                if let Ok(name) = std::str::from_utf8(owner)
                    && let Some(camera) = SceneCamera::from_shape_name(name)
                    && let Some(binding) = scene.camera_bindings[camera.index()].as_mut()
                {
                    binding.blocked = true;
                }
            }
        }
    }
    for camera in SceneCamera::ALL {
        if let (Some(optics), Some(binding)) = (
            scene.cameras[camera.index()],
            scene.camera_bindings[camera.index()].as_mut(),
        ) {
            binding.original = optics;
        }
    }
    Ok(())
}

pub(super) fn patch(scene: &NativeScene, output: &mut [u8]) -> Result<(), BinaryError> {
    for camera in SceneCamera::ALL {
        let Some(optics) = scene.cameras[camera.index()] else {
            continue;
        };
        let binding = scene.camera_bindings[camera.index()]
            .as_ref()
            .ok_or_else(unsupported)?;
        for (index, (now, old)) in values(optics)
            .into_iter()
            .zip(values(binding.original))
            .enumerate()
        {
            if now.to_bits() == old.to_bits() {
                continue;
            }
            if binding.blocked || !scene.removed.is_empty() {
                return Err(unsupported());
            }
            let range = binding.words[index].as_ref().ok_or_else(unsupported)?;
            if range.len() != 8 {
                return Err(unsupported());
            }
            let bytes = output.get_mut(range.clone()).ok_or_else(unsupported)?;
            if bytes != old.to_be_bytes() {
                return Err(unsupported());
            }
            bytes.copy_from_slice(&now.to_be_bytes());
        }
    }
    Ok(())
}

pub(super) fn authored(scene: &NativeScene) -> Result<Vec<Vec<u8>>, super::write::chunk::TooLarge> {
    use super::write::chunk::{Align, group, leaf};
    let mut result = Vec::new();
    for camera in SceneCamera::ALL {
        let Some(optics) = scene.cameras[camera.index()] else {
            continue;
        };
        let mut children = vec![leaf(
            *b"SLCT",
            0xa800_0000,
            format!(":{}", camera.shape_name()).as_bytes(),
            Align::Eight,
        )?];
        for (i, value) in values(optics).into_iter().enumerate() {
            let key = if i < 16 {
                OPTICAL_ATTRS[i]
            } else if i == 16 {
                "ff"
            } else {
                "fro"
            };
            let mut payload = key.as_bytes().to_vec();
            payload.extend_from_slice(&[0, 0x20]);
            payload.extend_from_slice(&value.to_be_bytes());
            children.push(leaf(*b"DBLE", 0xa800_0000, &payload, Align::Eight)?);
        }
        result.push(group(*b"SLCT", 0xa800_0000, &children, Align::Four)?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        camera_optics::{FilmFit, FilmRollOrder},
        native_scene::binary,
        scene::Primitive,
    };
    use std::sync::Arc;
    fn optics() -> CameraOptics {
        CameraOptics {
            focal_length_mm: 45.123456789012345,
            aperture_in: [1.6234567890123456, 0.8123456789012345],
            film_fit: FilmFit::Overscan,
            film_offset_in: [-0.0, 0.125],
            film_fit_offset_in: 0.03,
            lens_squeeze_ratio: 2.0,
            overscan: 1.2,
            camera_scale: 0.75,
            pre_scale: 1.1,
            post_scale: 0.9,
            film_translate: [0.1, -0.2],
            film_roll_radians: 0.5123456789012345,
            film_roll_pivot: [0.2, 0.1],
            film_roll_order: FilmRollOrder::TranslateRotate,
        }
    }
    fn authored(mesh: bool) -> (NativeScene, Vec<u8>) {
        let mut scene = NativeScene::default();
        if mesh {
            scene.create(Primitive::Cube, "cube1").unwrap();
        }
        for camera in SceneCamera::ALL {
            scene.set_camera_optics(camera, optics()).unwrap();
        }
        let bytes = binary::encode_authored(&scene).unwrap();
        (scene, bytes)
    }
    #[test]
    fn authored_empty_and_mesh_cameras_have_native_words_and_exact_readback() {
        for mesh in [false, true] {
            let (scene, bytes) = authored(mesh);
            let parsed = binary::decode(Arc::from(bytes.clone())).unwrap();
            assert_eq!(parsed.cameras, scene.cameras);
            // Independent record oracle: locate raw DBLE payloads, parse key/type/BE word.
            let container =
                crate::maya_binary::container::inspect(Arc::from(bytes.as_slice())).unwrap();
            let mut focal_words = 0;
            let mut fit_words = 0;
            for chunk in container.chunks() {
                if chunk.tag() != *b"DBLE" {
                    continue;
                }
                let p = &bytes[chunk.payload_range()];
                if p.starts_with(b"fl\0 ") {
                    assert_eq!(&p[4..], &optics().focal_length_mm.to_be_bytes());
                    focal_words += 1;
                }
                if p.starts_with(b"ff\0 ") {
                    assert_eq!(&p[4..], &3.0f64.to_be_bytes());
                    fit_words += 1;
                }
            }
            assert_eq!(focal_words, 4);
            assert_eq!(fit_words, 4);
            assert_eq!(binary::encode(&parsed).unwrap(), bytes);
        }
    }
    #[test]
    fn source_scalar_edits_change_only_the_proven_word_repeatably() {
        let (_, bytes) = authored(true);
        let mut scene = binary::decode(Arc::from(bytes.clone())).unwrap();
        let span = scene.camera_bindings[0].as_ref().unwrap().words[0]
            .clone()
            .unwrap();
        let edited = CameraOptics {
            focal_length_mm: 85.98765432109876,
            ..scene.camera_optics(SceneCamera::Persp)
        };
        scene.set_camera_optics(SceneCamera::Persp, edited).unwrap();
        let output = binary::encode(&scene).unwrap();
        assert_eq!(&output[..span.start], &bytes[..span.start]);
        assert_eq!(&output[span.end..], &bytes[span.end..]);
        assert_eq!(&output[span.clone()], &edited.focal_length_mm.to_be_bytes());
        assert_eq!(binary::encode(&scene).unwrap(), output);
        let parsed = binary::decode(Arc::from(output)).unwrap();
        assert_eq!(parsed.camera_optics(SceneCamera::Persp), edited);
        scene
            .set_camera_optics(SceneCamera::Persp, optics())
            .unwrap();
        assert_eq!(binary::encode(&scene).unwrap(), bytes);
    }
    #[test]
    fn missing_scalar_and_blocked_metadata_refuse_without_mutation() {
        let (_, bytes) = authored(false);
        let mut scene = binary::decode(Arc::from(bytes)).unwrap();
        scene.camera_bindings[0].as_mut().unwrap().words[0] = None;
        let baseline = scene.clone();
        let edited = CameraOptics {
            focal_length_mm: 100.0,
            ..optics()
        };
        assert!(scene.set_camera_optics(SceneCamera::Persp, edited).is_err());
        assert_eq!(scene, baseline);
        let (_, bytes) = authored(false);
        let mut scene = binary::decode(Arc::from(bytes)).unwrap();
        scene.camera_bindings[0].as_mut().unwrap().blocked = true;
        let baseline = scene.clone();
        assert!(scene.set_camera_optics(SceneCamera::Persp, edited).is_err());
        assert_eq!(scene, baseline);
    }

    fn append_group(mut bytes: Vec<u8>, extra: Vec<u8>) -> Vec<u8> {
        let old = u64::from_be_bytes(bytes[8..16].try_into().unwrap());
        bytes[8..16].copy_from_slice(&(old + extra.len() as u64).to_be_bytes());
        bytes.extend(extra);
        bytes
    }
    fn selected(children: Vec<Vec<u8>>) -> Vec<u8> {
        use super::super::write::chunk::{Align, group, leaf};
        let mut all = vec![leaf(*b"SLCT", 0xa8000000, b":perspShape", Align::Eight).unwrap()];
        all.extend(children);
        group(*b"SLCT", 0xa8000000, &all, Align::Four).unwrap()
    }
    fn scalar(key: &str, value: f64) -> Vec<u8> {
        use super::super::write::chunk::{Align, leaf};
        let mut p = key.as_bytes().to_vec();
        p.extend([0, 0x20]);
        p.extend(value.to_be_bytes());
        leaf(*b"DBLE", 0xa8000000, &p, Align::Eight).unwrap()
    }
    #[test]
    fn real_wire_duplicates_missing_fields_unknown_groups_and_drivers_are_guarded() {
        use super::super::write::chunk::{Align, group, leaf};
        let (_, base) = authored(false);
        let duplicate = append_group(base.clone(), selected(vec![scalar("fl", 40.0)]));
        assert!(binary::decode(Arc::from(duplicate)).is_err());
        let empty = binary::encode_authored(&NativeScene::default()).unwrap();
        let partial = append_group(empty, selected(vec![scalar("fl", 50.0)]));
        let mut scene = binary::decode(Arc::from(partial)).unwrap();
        let baseline = scene.clone();
        let edited = CameraOptics {
            overscan: 2.0,
            ..scene.camera_optics(SceneCamera::Persp)
        };
        assert!(scene.set_camera_optics(SceneCamera::Persp, edited).is_err());
        assert_eq!(scene, baseline);
        for extra in [
            selected(vec![
                leaf(*b"STR ", 0, b"unknown\0 \0", Align::Eight).unwrap(),
            ]),
            selected(vec![group(*b"HEAD", 0, &[], Align::Eight).unwrap()]),
            selected(vec![scalar("o", 1.0)]),
            selected(vec![scalar("ow", 37.0)]),
        ] {
            let bytes = append_group(base.clone(), extra);
            let mut scene = binary::decode(Arc::from(bytes.clone())).unwrap();
            let baseline = scene.clone();
            let edit = CameraOptics {
                focal_length_mm: 85.0,
                ..scene.camera_optics(SceneCamera::Persp)
            };
            assert!(scene.set_camera_optics(SceneCamera::Persp, edit).is_err());
            assert_eq!(scene, baseline);
            assert_eq!(binary::encode(&scene).unwrap(), bytes);
        }
        let payload = b"\0:time1.outTime\0:perspShape.fl\0";
        let connection = leaf(*b"CWFL", 0, payload, Align::Eight).unwrap();
        let conn = group(*b"CONN", 0, &[connection], Align::Four).unwrap();
        let mut cons = group(*b"CONS", 0, &[conn], Align::Four).unwrap();
        cons[..4].copy_from_slice(b"LIS8");
        let mut scene = binary::decode(Arc::from(append_group(base, cons))).unwrap();
        let baseline = scene.clone();
        let edit = CameraOptics {
            focal_length_mm: 90.0,
            ..scene.camera_optics(SceneCamera::Persp)
        };
        assert!(scene.set_camera_optics(SceneCamera::Persp, edit).is_err());
        assert_eq!(scene, baseline);
    }
    #[test]
    fn corrupted_enum_and_scalar_fail_closed() {
        let (_, bytes) = authored(false);
        let scene = binary::decode(Arc::from(bytes.clone())).unwrap();
        for (index, value) in [(0, f64::NAN), (16, 4.0), (16, 0.5), (17, 2.0)] {
            let range = scene.camera_bindings[0].as_ref().unwrap().words[index]
                .clone()
                .unwrap();
            let mut bad = bytes.clone();
            bad[range].copy_from_slice(&value.to_be_bytes());
            assert!(binary::decode(Arc::from(bad)).is_err());
        }
    }
}
