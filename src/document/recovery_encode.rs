//! The bytes of a recovery file: the scene encoded exactly as Save would encode it, with no
//! destination, no file access and no change to the scene.

use crate::native_scene::{
    NativeScene,
    binary::{self, AuthoredRefusalCode},
};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecoveryEncodeError {
    /// A scene with no binary encoder: a read-only imported ASCII scene, or on Windows any
    /// authored scene.
    Encode(binary::BinaryError),
    /// A scene authored from scratch that the Maya Binary writer refuses; nothing was encoded.
    Unwritable(AuthoredRefusalCode),
}

impl fmt::Display for RecoveryEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => error.fmt(f),
            Self::Unwritable(code) => write!(f, "cannot encode this scene for recovery: {code}"),
        }
    }
}
impl std::error::Error for RecoveryEncodeError {}

/// Encode `scene` for a recovery file. Pure: the same scene gives the same bytes.
pub fn encode_recovery(scene: &NativeScene) -> Result<Vec<u8>, RecoveryEncodeError> {
    #[cfg(unix)]
    if scene.authored_document_id().is_some() {
        return binary::encode_authored(scene)
            .map_err(|refusal| RecoveryEncodeError::Unwritable((&refusal).into()));
    }
    binary::encode(scene).map_err(RecoveryEncodeError::Encode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Primitive, Scene};

    fn cube() -> NativeScene {
        let mut legacy = Scene::default();
        legacy.create(Primitive::Cube, "cube").unwrap();
        NativeScene::from_legacy(legacy).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn an_authored_scene_encodes_exactly_as_save_does_without_changing_the_scene() {
        for scene in [cube(), NativeScene::default()] {
            let before = scene.clone();
            let bytes = encode_recovery(&scene).unwrap();
            assert_eq!(bytes, binary::encode_authored(&scene).unwrap());
            assert_eq!(bytes, encode_recovery(&scene).unwrap());
            assert!(bytes.len() > 16);
            assert_eq!(scene, before);
        }
    }

    #[cfg(not(unix))]
    #[test]
    fn an_authored_scene_has_no_encoder_off_unix_and_the_error_is_typed() {
        let error = encode_recovery(&cube()).unwrap_err();
        assert!(matches!(error, RecoveryEncodeError::Encode(_)));
        assert!(!error.to_string().is_empty());
    }
}
