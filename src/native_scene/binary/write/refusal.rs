//! The public, `Copy` face of an authored-write refusal. `AuthoredBinaryRefusal` carries a
//! `NativeError` that is not `Copy`, so save errors carry this payload-free code instead.

use super::AuthoredBinaryRefusal;
use crate::native_scene::authored::AuthoredIneligible;
use std::fmt;

/// Why a scene cannot be written as a new Maya Binary file. Nothing is written when one is
/// returned, and none of them embeds a path or scene text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AuthoredRefusalCode {
    SourcedScene,
    Curves,
    InvalidScene,
    UnsupportedUnits,
    NonAuthoredHierarchy,
    InvalidName,
    ReservedName,
    NameCollision,
    EmptyMesh,
    NonDefaultPivot,
    TransformExtras,
    TransformRestriction,
    UnsupportedTransformEffect,
    TransformVisibility,
    ShapeRestriction,
    ShapeVisibility,
    Double3Points,
    PointOffsets,
    EdgeSmoothing,
    FaceHoles,
    UnsupportedMeshChannels,
    TooLarge,
    MissingUuid,
    MaterialAssigned,
    SelfCheckMismatch,
}

impl From<&AuthoredBinaryRefusal> for AuthoredRefusalCode {
    fn from(refusal: &AuthoredBinaryRefusal) -> Self {
        use AuthoredIneligible as I;
        match refusal {
            AuthoredBinaryRefusal::Ineligible(ineligible) => match ineligible {
                I::SourcedScene => Self::SourcedScene,
                I::Curves => Self::Curves,
                I::InvalidScene(_) => Self::InvalidScene,
                I::UnsupportedUnits => Self::UnsupportedUnits,
                I::NonAuthoredHierarchy => Self::NonAuthoredHierarchy,
                I::InvalidName => Self::InvalidName,
                I::ReservedName => Self::ReservedName,
                I::NameCollision => Self::NameCollision,
                I::EmptyMesh => Self::EmptyMesh,
                I::NonDefaultPivot => Self::NonDefaultPivot,
                I::TransformExtras => Self::TransformExtras,
                I::TransformRestriction => Self::TransformRestriction,
                I::UnsupportedTransformEffect => Self::UnsupportedTransformEffect,
                I::TransformVisibility => Self::TransformVisibility,
                I::ShapeRestriction => Self::ShapeRestriction,
                I::ShapeVisibility => Self::ShapeVisibility,
                I::Double3Points => Self::Double3Points,
                I::PointOffsets => Self::PointOffsets,
                I::EdgeSmoothing => Self::EdgeSmoothing,
                I::FaceHoles => Self::FaceHoles,
                I::UnsupportedMeshChannels => Self::UnsupportedMeshChannels,
            },
            AuthoredBinaryRefusal::TooLarge => Self::TooLarge,
            AuthoredBinaryRefusal::MissingUuid => Self::MissingUuid,
            AuthoredBinaryRefusal::MaterialAssigned => Self::MaterialAssigned,
            AuthoredBinaryRefusal::SelfCheckMismatch => Self::SelfCheckMismatch,
        }
    }
}

impl fmt::Display for AuthoredRefusalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourcedScene => "the scene retains a parsed source and is not newly authored",
            Self::Curves => "curves cannot be written to a new .mb",
            Self::InvalidScene => "the scene is not valid",
            Self::UnsupportedUnits => "only centimetre, degree and film units can be written",
            Self::NonAuthoredHierarchy => {
                "only a flat list of transforms, each over at most one mesh, can be written"
            }
            Self::InvalidName => "a node name is not a valid Maya name",
            Self::ReservedName => "a node name is reserved for a default Maya scene node",
            Self::NameCollision => "two nodes share a name",
            Self::EmptyMesh => "a mesh has no geometry",
            Self::NonDefaultPivot => "a transform has a non-zero pivot",
            Self::TransformExtras => "a transform carries extra attributes",
            Self::TransformRestriction => "a transform has restricted channels",
            Self::UnsupportedTransformEffect => "a transform has an unsupported effect",
            Self::TransformVisibility => "a transform has non-default visibility",
            Self::ShapeRestriction => "a shape has restricted attributes",
            Self::ShapeVisibility => "a shape has non-default visibility",
            Self::Double3Points => "a mesh stores double-precision points",
            Self::PointOffsets => "a mesh has point offsets",
            Self::EdgeSmoothing => "a mesh has edge smoothing that a new .mb cannot carry",
            Self::FaceHoles => "a mesh has face holes",
            Self::UnsupportedMeshChannels => "a mesh has channels a new .mb cannot carry",
            Self::TooLarge => "the scene is too large for a new .mb",
            Self::MissingUuid => "a node has no identity to write",
            Self::MaterialAssigned => {
                "a shape is assigned a non-initial shading group, which a new .mb cannot carry"
            }
            Self::SelfCheckMismatch => "the written file did not read back as the scene",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_maps_to_its_code_without_a_payload() {
        let invalid = AuthoredBinaryRefusal::Ineligible(AuthoredIneligible::InvalidScene(
            crate::native_scene::NativeError::ResourceLimit,
        ));
        assert_eq!(
            AuthoredRefusalCode::from(&invalid),
            AuthoredRefusalCode::InvalidScene
        );
        for (refusal, code) in [
            (
                AuthoredBinaryRefusal::Ineligible(AuthoredIneligible::Curves),
                AuthoredRefusalCode::Curves,
            ),
            (
                AuthoredBinaryRefusal::MaterialAssigned,
                AuthoredRefusalCode::MaterialAssigned,
            ),
            (
                AuthoredBinaryRefusal::TooLarge,
                AuthoredRefusalCode::TooLarge,
            ),
            (
                AuthoredBinaryRefusal::MissingUuid,
                AuthoredRefusalCode::MissingUuid,
            ),
            (
                AuthoredBinaryRefusal::SelfCheckMismatch,
                AuthoredRefusalCode::SelfCheckMismatch,
            ),
        ] {
            assert_eq!(AuthoredRefusalCode::from(&refusal), code);
            assert!(!code.to_string().is_empty());
        }
        let error = crate::document::NativeSaveError::Unwritable(AuthoredRefusalCode::Curves);
        assert_eq!(
            error.to_string(),
            "cannot write this scene as a new .mb: curves cannot be written to a new .mb"
        );
    }
}
