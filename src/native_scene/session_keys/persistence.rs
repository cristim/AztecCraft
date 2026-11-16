//! Bounded native focal curves. Authored profiles require exact whole-file readback.
use super::*;
use crate::native_scene::SceneCamera;
use std::fmt::Write;

pub(in crate::native_scene) fn curve_name(camera: SceneCamera) -> String {
    format!("aztec_{}_focalLength", camera.shape_name())
}

impl NativeScene {
    pub(in crate::native_scene) fn focal_ascii(&self) -> Result<Vec<u8>, NativeError> {
        self.check_focal_curve_names()?;
        let mut out = String::new();
        out.try_reserve(
            self.session_key_count()
                .checked_mul(64)
                .and_then(|n| n.checked_add(2048))
                .ok_or(NativeError::ResourceLimit)?,
        )
        .map_err(|_| NativeError::ResourceLimit)?;
        for camera in SceneCamera::ALL {
            let curve = &self.session_keys.camera_curves[camera.index()];
            if curve.is_empty() {
                continue;
            }
            let name = curve_name(camera);
            writeln!(out, "createNode animCurveTU -n \"{name}\";\n\tsetAttr \".tan\" 2;\n\tsetAttr \".wgt\" no;\n\tsetAttr -s {} \".ktv[0:{}]\"", curve.len(), curve.len()-1).map_err(|_|NativeError::ResourceLimit)?;
            for (frame, value) in curve {
                writeln!(out, "\t{frame} {value}").map_err(|_| NativeError::ResourceLimit)?;
            }
            writeln!(
                out,
                ";\nconnectAttr \"{name}.o\" \":{}.fl\";",
                camera.shape_name()
            )
            .map_err(|_| NativeError::ResourceLimit)?;
        }
        Ok(out.into_bytes())
    }
    pub(in crate::native_scene) fn parse_focal_ascii(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), NativeError> {
        let text = std::str::from_utf8(bytes).map_err(|_| NativeError::InvalidSessionKeys)?;
        let mut lines = text.lines();
        let mut seen = [false; 4];
        while let Some(line) = lines.next() {
            let camera = SceneCamera::ALL
                .into_iter()
                .find(|camera| {
                    line == format!("createNode animCurveTU -n \"{}\";", curve_name(*camera))
                })
                .ok_or(NativeError::InvalidSessionKeys)?;
            if std::mem::replace(&mut seen[camera.index()], true) {
                return Err(NativeError::InvalidSessionKeys);
            }
            if lines.next() != Some("\tsetAttr \".tan\" 2;")
                || lines.next() != Some("\tsetAttr \".wgt\" no;")
            {
                return Err(NativeError::InvalidSessionKeys);
            }
            let header = lines.next().ok_or(NativeError::InvalidSessionKeys)?;
            let count = header
                .strip_prefix("\tsetAttr -s ")
                .and_then(|s| s.split_once(' '))
                .and_then(|(s, _)| s.parse::<usize>().ok())
                .filter(|n| *n > 0 && *n <= MAX_SESSION_KEYS - self.session_keys.len())
                .ok_or(NativeError::KeyLimit)?;
            if header != format!("\tsetAttr -s {count} \".ktv[0:{}]\"", count - 1) {
                return Err(NativeError::InvalidSessionKeys);
            }
            let mut last = None;
            for _ in 0..count {
                let line = lines.next().ok_or(NativeError::InvalidSessionKeys)?;
                let (frame, value) = line
                    .trim()
                    .split_once(' ')
                    .ok_or(NativeError::InvalidSessionKeys)?;
                let frame = frame
                    .parse::<i32>()
                    .map_err(|_| NativeError::InvalidFrame)?;
                let value = value.parse::<f64>().map_err(|_| NativeError::NonFinite)?;
                check_frame(frame)?;
                if last.is_some_and(|last| frame <= last)
                    || !value.is_finite()
                    || !(2.5..=100000.0).contains(&value)
                {
                    return Err(NativeError::InvalidSessionKeys);
                }
                self.session_keys.camera_curves[camera.index()].insert(frame, value);
                last = Some(frame);
            }
            if lines.next() != Some(";")
                || lines.next()
                    != Some(
                        format!(
                            "connectAttr \"{}.o\" \":{}.fl\";",
                            curve_name(camera),
                            camera.shape_name()
                        )
                        .as_str(),
                    )
            {
                return Err(NativeError::InvalidSessionKeys);
            }
        }
        Ok(())
    }
    pub(in crate::native_scene) fn check_focal_curve_names(&self) -> Result<(), NativeError> {
        for camera in SceneCamera::ALL {
            if self.camera_focal_keys(camera).next().is_none() {
                continue;
            }
            let name = curve_name(camera);
            if self.nodes.iter().any(|n| n.name == name)
                || self.shapes.iter().any(|s| s.name == name)
                || self
                    .session_materials
                    .materials
                    .iter()
                    .any(|m| m.name == name)
                || self.session_materials.groups.iter().any(|g| g.name == name)
            {
                return Err(NativeError::InvalidSessionKeys);
            }
        }
        Ok(())
    }
    pub(in crate::native_scene) fn has_transform_keys(&self) -> bool {
        !self.session_keys.curves.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_scene::{ascii, binary};
    fn animated() -> NativeScene {
        let mut s = NativeScene::default();
        s.create(crate::scene::Primitive::Cube, "cube").unwrap();
        for camera in SceneCamera::ALL {
            for (frame, value) in [(-10, 35.123456789012345), (11, 75.123_456_789_012_35)] {
                s.set_camera_focal_key(camera, frame, value, 6).unwrap();
            }
        }
        s
    }
    #[test]
    fn native_focal_curves_roundtrip_both_formats_exactly() {
        let scene = animated();
        let ma = ascii::write::emit_geometry_prefix(&scene).unwrap();
        assert!(
            std::str::from_utf8(&ma)
                .unwrap()
                .contains("createNode animCurveTU")
        );
        let ascii::AsciiDecoded::Authored(decoded) =
            ascii::decode_document(ma.clone().into(), true).unwrap()
        else {
            panic!("authored");
        };
        let decoded = decoded.into_scene();
        assert_eq!(decoded.session_keys, scene.session_keys);
        assert_eq!(ascii::write::emit_geometry_prefix(&decoded).unwrap(), ma);
        let mb = binary::encode_authored(&scene).unwrap();
        assert!(mb.windows(4).any(|b| b == b"PCTU"));
        let decoded = binary::decode(mb.clone().into()).unwrap();
        assert!(!decoded.has_source());
        assert_eq!(decoded.session_keys, scene.session_keys);
        assert_eq!(binary::encode_authored(&decoded).unwrap(), mb);
        for camera in SceneCamera::ALL {
            assert_eq!(
                decoded.camera_focal_at(camera, 11).unwrap().to_bits(),
                75.123_456_789_012_35_f64.to_bits()
            );
        }
    }
    #[test]
    fn native_focal_curves_reject_malformed_counts_connections_and_unknown_bytes() {
        let scene = animated();
        let ma = String::from_utf8(ascii::write::emit_geometry_prefix(&scene).unwrap()).unwrap();
        for bad in [
            ma.replace(".tan\" 2", ".tan\" 9"),
            ma.replace(".wgt\" no", ".wgt\" yes"),
            ma.replace("-s 2", "-s 65537"),
            ma.replace(":perspShape.fl", ":topShape.fl"),
            format!("{ma}// unknown tail\n"),
        ] {
            assert!(ascii::decode_document(bad.into_bytes().into(), true).is_err());
        }
        let mb = binary::encode_authored(&scene).unwrap();
        for needle in [
            b"ktv\0\x28".as_slice(),
            b"tan\0\x20",
            b":perspShape.fl",
            b"ktv[0:1]\0\x20",
        ] {
            let mut bad = mb.clone();
            let offset = bad.windows(needle.len()).position(|b| b == needle).unwrap();
            bad[offset + needle.len() - 1] ^= 1;
            assert!(binary::decode(bad.into()).is_err());
        }
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    use crate::native_scene::{ascii, binary};
    #[test]
    fn curve_names_colliding_with_authored_names_refuse_both_writers() {
        let mut scene = NativeScene::default();
        scene
            .create(
                crate::scene::Primitive::Cube,
                &curve_name(SceneCamera::Persp),
            )
            .unwrap();
        scene
            .set_camera_focal_key(SceneCamera::Persp, 1, 50., 1)
            .unwrap();
        let before = scene.clone();
        assert!(ascii::write::emit_geometry_prefix(&scene).is_err());
        assert!(binary::encode_authored(&scene).is_err());
        assert_eq!(scene, before);
    }
    #[test]
    fn quoted_or_commented_curve_text_does_not_change_retained_ascii_admission() {
        for text in [
            "// createNode animCurveTU -n ",
            "fileInfo \"note\" \"createNode animCurveTU -n example\";",
        ] {
            let bytes = format!(
                "//Maya ASCII 2027 scene\nrequires maya \"2027\";\ncurrentUnit -l centimeter -a degree -t film;\n{text}\n"
            );
            assert!(matches!(
                ascii::decode_document(bytes.into_bytes().into(), true).unwrap(),
                ascii::AsciiDecoded::Retained(_)
            ));
        }
    }
}

#[cfg(test)]
mod raw_profile_tests {
    use super::*;
    #[test]
    fn independent_native_curve_words_match_declared_profile() {
        let mut scene = NativeScene::default();
        scene.create(crate::scene::Primitive::Cube, "cube").unwrap();
        scene
            .set_camera_focal_key(SceneCamera::Persp, 1, 50., 1)
            .unwrap();
        let bytes = crate::native_scene::binary::encode_authored(&scene).unwrap();
        // Independent literal oracle: ktv[0:0], kind0x20, frame1=5880000 ticks, focal50.
        let mut expected = b"ktv[0:0]\0\x20".to_vec();
        expected.extend_from_slice(&[0, 0, 0, 0, 0, 0x59, 0xb8, 0xc0]);
        expected.extend_from_slice(&[0x40, 0x49, 0, 0, 0, 0, 0, 0]);
        assert!(bytes.windows(expected.len()).any(|v| v == expected));
        let connection = b"\0aztec_perspShape_focalLength.o\0:perspShape.fl\0";
        assert!(bytes.windows(connection.len()).any(|v| v == connection));
        let ascii = scene.focal_ascii().unwrap();
        assert_eq!(ascii,b"createNode animCurveTU -n \"aztec_perspShape_focalLength\";\n\tsetAttr \".tan\" 2;\n\tsetAttr \".wgt\" no;\n\tsetAttr -s 1 \".ktv[0:0]\"\n\t1 50\n;\nconnectAttr \"aztec_perspShape_focalLength.o\" \":perspShape.fl\";\n");
    }
}
