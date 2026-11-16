//! Built-in Maya camera shapes. Optical state belongs to the scene, never to a bookmark.
use super::{NativeError, NativeScene};
use crate::camera_optics::{CameraOptics, FilmFit, FilmRollOrder, OpticalProjection};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CameraBinding {
    pub original: CameraOptics,
    pub words: [Option<std::ops::Range<usize>>; 18],
    pub blocked: bool,
}
impl Default for CameraBinding {
    fn default() -> Self {
        Self {
            original: CameraOptics::node_default(),
            words: Default::default(),
            blocked: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneCamera {
    Persp,
    Top,
    Front,
    Side,
}
impl SceneCamera {
    pub const ALL: [Self; 4] = [Self::Persp, Self::Top, Self::Front, Self::Side];
    pub fn shape_name(self) -> &'static str {
        match self {
            Self::Persp => "perspShape",
            Self::Top => "topShape",
            Self::Front => "frontShape",
            Self::Side => "sideShape",
        }
    }
    pub fn from_shape_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.shape_name() == name)
    }
    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// Scalar order used by the native codecs. The two enums have separate integer representations.
pub(crate) const OPTICAL_ATTRS: [&str; 16] = [
    "fl", "hfa", "vfa", "hfo", "vfo", "ffo", "lsr", "ovr", "cs", "psc", "ptsc", "fth", "ftv",
    "frv", "hrp", "vrp",
];

impl CameraOptics {
    pub(crate) fn set_scalar(&mut self, index: usize, value: f64) -> Result<(), NativeError> {
        let field = match index {
            0 => &mut self.focal_length_mm,
            1 => &mut self.aperture_in[0],
            2 => &mut self.aperture_in[1],
            3 => &mut self.film_offset_in[0],
            4 => &mut self.film_offset_in[1],
            5 => &mut self.film_fit_offset_in,
            6 => &mut self.lens_squeeze_ratio,
            7 => &mut self.overscan,
            8 => &mut self.camera_scale,
            9 => &mut self.pre_scale,
            10 => &mut self.post_scale,
            11 => &mut self.film_translate[0],
            12 => &mut self.film_translate[1],
            13 => &mut self.film_roll_radians,
            14 => &mut self.film_roll_pivot[0],
            15 => &mut self.film_roll_pivot[1],
            _ => return Err(NativeError::InvalidIdentity),
        };
        *field = value;
        Ok(())
    }
    pub(crate) fn set_fit(&mut self, value: i32) -> Result<(), NativeError> {
        self.film_fit = match value {
            0 => FilmFit::Fill,
            1 => FilmFit::Horizontal,
            2 => FilmFit::Vertical,
            3 => FilmFit::Overscan,
            _ => return Err(NativeError::UnsupportedEdit),
        };
        Ok(())
    }
    pub(crate) fn set_roll_order(&mut self, value: i32) -> Result<(), NativeError> {
        self.film_roll_order = match value {
            0 => FilmRollOrder::RotateTranslate,
            1 => FilmRollOrder::TranslateRotate,
            _ => return Err(NativeError::UnsupportedEdit),
        };
        Ok(())
    }
}
impl NativeScene {
    pub fn camera_optics(&self, camera: SceneCamera) -> CameraOptics {
        self.cameras[camera.index()].unwrap_or_else(CameraOptics::node_default)
    }
    pub fn set_camera_optics(
        &mut self,
        camera: SceneCamera,
        optics: CameraOptics,
    ) -> Result<(), NativeError> {
        validate_optics(optics)?;
        if let Some(source) = &self.source {
            if source.format != super::source::SourceFormat::Binary {
                return Err(NativeError::UnsupportedEdit);
            }
            let binding = self.camera_bindings[camera.index()]
                .as_ref()
                .ok_or(NativeError::UnsupportedEdit)?;
            if binding.blocked || !self.removed.is_empty() {
                return Err(NativeError::UnsupportedEdit);
            }
            let current = self.cameras[camera.index()].ok_or(NativeError::UnsupportedEdit)?;
            for (i, (a, b)) in current
                .values()
                .into_iter()
                .zip(optics.values())
                .enumerate()
            {
                if a.to_bits() != b.to_bits() && binding.words[i].is_none() {
                    return Err(NativeError::UnsupportedEdit);
                }
            }
            if (current.film_fit != optics.film_fit && binding.words[16].is_none())
                || (current.film_roll_order != optics.film_roll_order
                    && binding.words[17].is_none())
            {
                return Err(NativeError::UnsupportedEdit);
            }
            self.cameras[camera.index()] = Some(optics);
            return Ok(());
        }
        // Implicit built-in defaults carry no authored statement. Reverting removes the override.
        self.cameras[camera.index()] = (optics != CameraOptics::node_default()).then_some(optics);
        Ok(())
    }
    pub(super) fn validate_cameras(&self) -> Result<(), NativeError> {
        for optics in self.cameras.iter().flatten() {
            validate_optics(*optics)?;
        }
        Ok(())
    }
    pub(crate) fn camera_ascii(&self) -> Result<Vec<u8>, NativeError> {
        use std::fmt::Write;
        let mut text = String::new();
        for camera in SceneCamera::ALL {
            let Some(optics) = self.cameras[camera.index()] else {
                continue;
            };
            validate_optics(optics)?;
            writeln!(text, "select -ne :{};", camera.shape_name())
                .map_err(|_| NativeError::ResourceLimit)?;
            for (i, (attr, value)) in OPTICAL_ATTRS.iter().zip(optics.values()).enumerate() {
                let unit = if i == 13 { "rad" } else { "" };
                writeln!(text, "\tsetAttr \".{attr}\" {value}{unit};")
                    .map_err(|_| NativeError::ResourceLimit)?;
            }
            writeln!(text, "\tsetAttr \".ff\" {};", optics.film_fit as u8)
                .map_err(|_| NativeError::ResourceLimit)?;
            writeln!(text, "\tsetAttr \".fro\" {};", optics.film_roll_order as u8)
                .map_err(|_| NativeError::ResourceLimit)?;
        }
        Ok(text.into_bytes())
    }
}
fn validate_optics(optics: CameraOptics) -> Result<(), NativeError> {
    optics
        .validate()
        .and_then(|()| {
            optics
                .projection(1.0, 0.1, 10000.0, OpticalProjection::Perspective)
                .map(|_| ())
        })
        .map_err(|_| NativeError::UnsupportedView("invalid or unrepresentable camera optics"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        history::{Command, History},
        native_scene::ascii,
        scene::Primitive,
    };
    use std::sync::Arc;
    #[test]
    fn camera_data_is_authoritative_undoable_and_authored_ascii_round_trips() {
        let mut scene = NativeScene::default();
        scene.create(Primitive::Cube, "cube1").unwrap();
        let original = scene.clone();
        let optics = CameraOptics {
            focal_length_mm: 35.123456789012345,
            film_offset_in: [-0.0, 0.125],
            film_roll_radians: 0.5,
            ..CameraOptics::default()
        };
        let mut history = History::new(scene);
        history
            .execute(Command::SetCameraOptics {
                camera: SceneCamera::Persp,
                optics,
            })
            .unwrap();
        assert_eq!(history.scene().camera_optics(SceneCamera::Persp), optics);
        history.undo();
        assert_eq!(history.scene(), &original);
        history.redo();
        let bytes = ascii::write::emit_geometry_prefix(history.scene()).unwrap();
        let decoded = ascii::decode_document(Arc::from(bytes), true).unwrap();
        let ascii::AsciiDecoded::Authored(decoded) = decoded else {
            panic!("authored camera lost");
        };
        assert_eq!(decoded.scene().camera_optics(SceneCamera::Persp), optics);
        assert_eq!(decoded.scene().nodes().len(), 1);
        history
            .execute(Command::SetCameraOptics {
                camera: SceneCamera::Persp,
                optics: CameraOptics::node_default(),
            })
            .unwrap();
        assert_eq!(history.scene(), &original);
    }
    #[test]
    fn empty_camera_documents_and_all_camera_identities_round_trip() {
        let mut scene = NativeScene::default();
        for (i, camera) in SceneCamera::ALL.into_iter().enumerate() {
            let optics = CameraOptics {
                focal_length_mm: 40.123456789012345 + i as f64,
                film_roll_radians: f64::from_bits(0x3fe0000000000001),
                ..CameraOptics::default()
            };
            scene.set_camera_optics(camera, optics).unwrap();
        }
        let bytes = scene.camera_ascii().unwrap();
        assert!(!bytes.is_empty());
        let bytes = ascii::write::emit_geometry_prefix(&scene).unwrap();
        let ascii::AsciiDecoded::Authored(decoded) =
            ascii::decode_document(Arc::from(bytes), true).unwrap()
        else {
            panic!("not authored");
        };
        for camera in SceneCamera::ALL {
            assert_eq!(
                scene.camera_optics(camera),
                decoded.scene().camera_optics(camera)
            );
        }
        assert!(super::super::binary::encode_authored(&scene).is_ok());
        assert!(decoded.scene().nodes().is_empty());
    }
    #[test]
    fn implicit_builtin_fit_matches_native_node_default_on_initial_and_reset_output() {
        let mut scene = NativeScene::default();
        assert_eq!(
            scene.camera_optics(SceneCamera::Persp).film_fit,
            FilmFit::Horizontal
        );
        let baseline = scene.clone();
        scene
            .set_camera_optics(SceneCamera::Persp, CameraOptics::default())
            .unwrap();
        let output = String::from_utf8(scene.camera_ascii().unwrap()).unwrap();
        assert!(output.contains("setAttr \".ff\" 0;"));
        scene
            .set_camera_optics(SceneCamera::Persp, CameraOptics::node_default())
            .unwrap();
        assert!(scene.camera_ascii().unwrap().is_empty());
        assert_eq!(scene, baseline);
    }
    #[test]
    fn malformed_duplicate_and_unknown_camera_inputs_are_refused() {
        let prefix = "//Maya ASCII 2026 scene\nrequires maya \"2026\"; currentUnit -l centimeter -a degree -t film; ";
        for body in [
            "select -ne :unknownShape; setAttr \".fl\" 35;",
            "select -ne :perspShape; setAttr \".fl\" 35; setAttr \".fl\" 40;",
            "select -ne :perspShape; setAttr \".ff\" 4;",
            "select -ne :perspShape; setAttr \".fro\" -1;",
            "select -ne :perspShape; setAttr \".lsr\" 0;",
            "select -ne :perspShape; setAttr \".frv\" nan;",
            "select -ne :perspShape; setAttr \".frv\" 3turn;",
            "select -ne :perspShape; setAttr \".fl\" 1e300;",
            "select -ne :perspShape; setAttr \".shakeEnabled\" 1;",
        ] {
            assert!(
                ascii::decode(Arc::from(format!("{prefix}{body}").into_bytes())).is_err(),
                "{body}"
            );
        }
    }
    #[test]
    fn invalid_and_sourced_edits_preserve_baseline() {
        let mut scene = NativeScene::default();
        let before = scene.clone();
        let invalid = CameraOptics {
            focal_length_mm: f64::NAN,
            ..CameraOptics::default()
        };
        assert!(
            scene
                .set_camera_optics(SceneCamera::Persp, invalid)
                .is_err()
        );
        assert_eq!(scene, before);
        let mut sourced=ascii::decode(Arc::from(b"//Maya ASCII 2026 scene\nrequires maya \"2026\"; currentUnit -l centimeter -a degree -t film; select -ne :perspShape; setAttr \".fl\" 45;".as_slice())).unwrap();
        assert_eq!(
            sourced.camera_optics(SceneCamera::Persp).film_fit,
            FilmFit::Horizontal
        );
        let before = sourced.clone();
        assert!(
            sourced
                .set_camera_optics(SceneCamera::Persp, CameraOptics::default())
                .is_err()
        );
        assert_eq!(sourced, before);
    }
}
