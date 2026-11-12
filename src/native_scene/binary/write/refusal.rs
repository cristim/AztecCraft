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
    Keys,
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
    SessionUvs,
    TooLarge,
    MissingUuid,
    SessionMaterials,
    SelfCheckMismatch,
}

impl From<&AuthoredBinaryRefusal> for AuthoredRefusalCode {
    fn from(refusal: &AuthoredBinaryRefusal) -> Self {
        use AuthoredIneligible as I;
        match refusal {
            AuthoredBinaryRefusal::Ineligible(ineligible) => match ineligible {
                I::SourcedScene => Self::SourcedScene,
                I::Curves => Self::Curves,
                I::Keys => Self::Keys,
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
                I::SessionUvs => Self::SessionUvs,
            },
            AuthoredBinaryRefusal::TooLarge => Self::TooLarge,
            AuthoredBinaryRefusal::MissingUuid => Self::MissingUuid,
            AuthoredBinaryRefusal::SessionMaterials => Self::SessionMaterials,
            AuthoredBinaryRefusal::SelfCheckMismatch => Self::SelfCheckMismatch,
        }
    }
}

impl AuthoredRefusalCode {
    /// The stable snake_case name MCP clients receive as `error.reason`; never reworded.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourcedScene => "sourced_scene",
            Self::Curves => "curves",
            Self::Keys => "keys",
            Self::InvalidScene => "invalid_scene",
            Self::UnsupportedUnits => "unsupported_units",
            Self::NonAuthoredHierarchy => "non_authored_hierarchy",
            Self::InvalidName => "invalid_name",
            Self::ReservedName => "reserved_name",
            Self::NameCollision => "name_collision",
            Self::EmptyMesh => "empty_mesh",
            Self::NonDefaultPivot => "non_default_pivot",
            Self::TransformExtras => "transform_extras",
            Self::TransformRestriction => "transform_restriction",
            Self::UnsupportedTransformEffect => "unsupported_transform_effect",
            Self::TransformVisibility => "transform_visibility",
            Self::ShapeRestriction => "shape_restriction",
            Self::ShapeVisibility => "shape_visibility",
            Self::Double3Points => "double3_points",
            Self::PointOffsets => "point_offsets",
            Self::EdgeSmoothing => "edge_smoothing",
            Self::FaceHoles => "face_holes",
            Self::UnsupportedMeshChannels => "unsupported_mesh_channels",
            Self::SessionUvs => "session_uvs",
            Self::TooLarge => "too_large",
            Self::MissingUuid => "missing_uuid",
            Self::SessionMaterials => "session_materials",
            Self::SelfCheckMismatch => "self_check_mismatch",
        }
    }
}

impl fmt::Display for AuthoredRefusalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SourcedScene => "the scene retains a parsed source and is not newly authored",
            Self::Curves => "curves cannot be written to a new .mb",
            Self::Keys => "session keys are not saved",
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
            Self::SessionUvs => "session UVs are not saved; use Delete UVs or Undo to save",
            Self::TooLarge => "the scene is too large to write and verify as a new .mb",
            Self::MissingUuid => "a node has no identity to write",
            Self::SessionMaterials => {
                "the scene has session materials, which a new .mb cannot carry"
            }
            Self::SelfCheckMismatch => "the written file did not read back as the scene",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_names_are_stable_snake_case() {
        assert_eq!(AuthoredRefusalCode::Keys.as_str(), "keys");
        assert_eq!(AuthoredRefusalCode::SessionUvs.as_str(), "session_uvs");
        assert_eq!(
            AuthoredRefusalCode::Double3Points.as_str(),
            "double3_points"
        );
    }

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
                AuthoredBinaryRefusal::SessionMaterials,
                AuthoredRefusalCode::SessionMaterials,
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
