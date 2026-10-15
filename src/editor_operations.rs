#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::NativeTransform;
use azteccraft::{
    native_scene::{FaceTarget, NativeError, NativeId},
    scene::{NodeId, Primitive},
    viewport::InvalidCameraInput,
};
#[cfg(feature = "mcp")]
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct TransformInput {
    pub translation: [f64; 3],
    pub rotation_degrees: [f64; 3],
    pub scale: [f64; 3],
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileActionKind {
    New,
    Open,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub enum FileDecision {
    Cancel,
    Discard,
    Save,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingActionStatus {
    None,
    Retained,
    SourceChanged,
}

#[cfg(any(test, feature = "mcp"))]
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentOrigin {
    Untitled,
    MayaBinary,
    LegacyAztec,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Object,
    Face,
}
#[cfg(any(test, feature = "mcp"))]
use uuid::Uuid;

#[cfg(feature = "mcp")]
pub const MAX_NAME_BYTES: usize = 4096;
#[cfg(any(test, feature = "mcp"))]
pub const MAX_RESULT_BYTES: usize = 512 * 1024;

#[cfg(any(test, feature = "mcp"))]
pub const TOOL_NAMES: [&str; 24] = [
    "inspect_scene",
    "create_primitive",
    "select_node",
    "set_selection_mode",
    "select_face",
    "rename_node",
    "set_transform",
    "edit_node",
    "extrude_face",
    "set_material_diffuse",
    "delete_node",
    "undo",
    "redo",
    "frame_selected",
    "set_grid_visibility",
    "orbit_camera",
    "pan_camera",
    "dolly_camera",
    "open_scene",
    "save_scene",
    "save_scene_as",
    "new_scene",
    "resolve_file_action",
    "request_close",
];

pub enum Operation {
    #[cfg(any(test, feature = "mcp"))]
    SetMaterialDiffuse {
        shape: NativeId,
        value: f64,
    },
    #[cfg(any(test, feature = "mcp"))]
    Inspect,
    Create {
        primitive: Primitive,
        name: Option<String>,
    },
    Select {
        node: Option<NodeId>,
        instance_path: Option<Vec<NativeId>>,
    },
    SetSelectionMode(SelectionMode),
    SelectFace {
        target: Option<FaceTarget>,
    },
    NewScene,
    RequestClose,
    OpenScene {
        path: PathBuf,
    },
    SaveScene,
    SaveSceneAs {
        path: PathBuf,
    },
    ResolveFileAction {
        pending_id: uuid::Uuid,
        decision: FileDecision,
    },
    #[cfg(any(test, feature = "mcp"))]
    Rename {
        id: NodeId,
        name: String,
    },
    #[cfg(any(test, feature = "mcp"))]
    SetTransform {
        id: NodeId,
        transform: TransformInput,
    },
    #[cfg(any(test, feature = "mcp"))]
    EditNode {
        id: NodeId,
        name: Option<String>,
        translation: [Option<f64>; 3],
        rotation_degrees: [Option<f64>; 3],
        scale: [Option<f64>; 3],
    },
    #[cfg(any(test, feature = "mcp"))]
    ExtrudeFace {
        target: FaceTarget,
        distance: f64,
    },
    Delete(NodeId),
    Undo,
    Redo,
    FrameSelected,
    SetGridVisible {
        visible: bool,
    },
    Orbit {
        radians: [f32; 2],
    },
    Pan {
        world_units: [f32; 2],
    },
    Dolly {
        factor: f32,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OperationReply {
    #[cfg(any(test, feature = "mcp"))]
    Scene {
        inspection: Box<SceneInspection>,
    },
    Created {
        node_id: String,
    },
    Changed,
    Unchanged,
    DocumentApplied {
        scene_token: String,
    },
    PendingConfirmation {
        scene_token: String,
        pending_action_id: String,
        kind: FileActionKind,
    },
    Cancelled {
        scene_token: String,
    },
    CloseRequested {
        scene_token: String,
    },
    Saved {
        scene_token: String,
        dirty: bool,
        pending_action_status: PendingActionStatus,
    },
    SaveUncertain {
        scene_token: String,
        dirty: bool,
        pending_action_status: PendingActionStatus,
        message: String,
    },
    SaveReconciled {
        scene_token: String,
        dirty: bool,
        pending_action_status: PendingActionStatus,
    },
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct SceneInspection {
    pub scene_token: String,
    pub nodes: Vec<NodeInspection>,
    pub shapes: Vec<ShapeInspection>,
    pub units: UnitsInspection,
    pub save_available: bool,
    pub save_as_available: bool,
    pub save_reconciliation_required: bool,
    pub document_origin: DocumentOrigin,
    pub has_file: bool,
    pub closing: bool,
    pub viewport_backend: &'static str,
    pub viewport_error: Option<crate::viewport_renderer::ViewportError>,
    pub grid_visible: bool,
    pub depth_complete: bool,
    pub clipping_complete: bool,
    pub appearance_incomplete: bool,
    pub source_appearance: azteccraft::native_scene::AppearanceSummary,
    pub pending_action: Option<PendingInspection>,
    pub selected_node: Option<String>,
    pub selected_instance_path: Vec<String>,
    pub selection_mode: SelectionMode,
    pub remembered_face: Option<FaceSelectionTarget>,
    pub face_selection_policy: FaceSelectionPolicy,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub has_draft: bool,
    pub native_input_pending: bool,
    pub text_focused: bool,
    pub modal: bool,
    pub camera: CameraInspection,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg(any(test, feature = "mcp"))]
pub struct FaceSelectionTarget {
    pub node_id: String,
    pub shape_id: String,
    #[cfg_attr(feature = "mcp", schemars(length(min = 2, max = 257)))]
    pub instance_path: Vec<String>,
    pub face_index: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg(any(test, feature = "mcp"))]
pub enum FaceSelectionPolicy {
    VisibleCentersOnly,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct NodeInspection {
    pub id: String,
    pub name: String,
    pub transform_id: String,
    pub transform: TransformInput,
    pub authoring_transform: AuthoringTransformInspection,
    pub instance_paths: Vec<Vec<String>>,
    pub shape_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct AuthoringTransformInspection {
    pub translation: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}
#[cfg(any(test, feature = "mcp"))]
impl From<NativeTransform> for AuthoringTransformInspection {
    fn from(value: NativeTransform) -> Self {
        Self {
            translation: value.translation,
            rotation: value.rotation,
            scale: value.scale,
        }
    }
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct UnitsInspection {
    pub linear: String,
    pub angular: String,
    pub time: String,
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct ShapeInspection {
    pub id: String,
    pub name: String,
    pub points: usize,
    pub edges: usize,
    pub faces: usize,
    pub smoothing_known: bool,
    pub source_appearance: azteccraft::native_scene::AppearanceSummary,
    pub material: Option<MaterialInspection>,
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct MaterialInspection {
    pub shader: &'static str,
    pub diffuse: f64,
    pub shared_users: usize,
    pub editing_available: bool,
    pub rendering_available: bool,
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct PendingInspection {
    pub id: String,
    pub kind: FileActionKind,
    pub source_changed: bool,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct CameraInspection {
    pub target: [f32; 3],
    pub eye: [f32; 3],
    pub distance: f32,
}

#[derive(Debug)]
pub enum OperationError {
    BusyEditing,
    #[cfg(any(test, feature = "mcp"))]
    StaleScene,
    #[cfg(any(test, feature = "mcp"))]
    TooLarge,
    NoSelection,
    InvalidInput(String),
    Scene(NativeError),
    File(String),
    NativeSave(azteccraft::document::NativeSaveError),
    AmbiguousSelection,
    StalePendingAction,
    Camera(InvalidCameraInput),
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BusyEditing => f.write_str(
                "Process native input and finish the text edit or dialog before remote mutation",
            ),
            #[cfg(any(test, feature = "mcp"))]
            Self::StaleScene => f.write_str("Scene token is stale; inspect the current document"),
            #[cfg(any(test, feature = "mcp"))]
            Self::TooLarge => f.write_str("Scene inspection exceeds the prototype response limit"),
            Self::NoSelection => f.write_str("Select an object or face before framing"),
            Self::InvalidInput(message) => f.write_str(message),
            Self::Scene(error) => error.fmt(f),
            Self::File(error) => f.write_str(error),
            Self::NativeSave(error) => error.fmt(f),
            Self::AmbiguousSelection => {
                f.write_str("object has multiple instances; supply an instance_path")
            }
            Self::StalePendingAction => {
                f.write_str("pending file action is absent or stale; inspect before deciding")
            }
            Self::Camera(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for OperationError {}

#[cfg(any(test, feature = "mcp"))]
pub struct RemoteRequest {
    pub expected_scene_token: Option<Uuid>,
    pub operation: Operation,
}

#[cfg(any(test, feature = "mcp"))]
const APPEARANCE_SUMMARY_MAX_BYTES: usize = 1024;

#[cfg(any(test, feature = "mcp"))]
pub fn check_inspection_size(
    scene: &azteccraft::native_scene::NativeScene,
) -> Result<(), OperationError> {
    let limit = MAX_RESULT_BYTES / 2;
    let mut estimated_bytes = APPEARANCE_SUMMARY_MAX_BYTES;
    for node in scene.nodes() {
        estimated_bytes = estimated_bytes
            .saturating_add(1024)
            .saturating_add(node.name().len().saturating_mul(6))
            .saturating_add(512);
        if estimated_bytes > limit {
            return Err(OperationError::TooLarge);
        }
    }
    for shape in scene.shapes() {
        estimated_bytes = estimated_bytes
            .saturating_add(512)
            .saturating_add(256)
            .saturating_add(APPEARANCE_SUMMARY_MAX_BYTES)
            .saturating_add(shape.name().len().saturating_mul(6));
        if estimated_bytes > limit {
            return Err(OperationError::TooLarge);
        }
    }
    estimated_bytes = estimated_bytes.saturating_add(scene.dag_edges().len().saturating_mul(24));
    if estimated_bytes > limit {
        return Err(OperationError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use azteccraft::{native_scene::NativeScene, scene::Scene};

    #[test]
    fn appearance_summary_preflight_bounds_the_largest_fixed_dto() {
        use azteccraft::native_scene::{AppearanceIssues, AppearanceState, AppearanceSummary};
        let summary = AppearanceSummary {
            state: AppearanceState::CheckedViewerSource,
            shapes: usize::MAX,
            checked_source_bindings: usize::MAX,
            unavailable: usize::MAX,
            unbound: usize::MAX,
            issues: AppearanceIssues::default(),
        };
        assert!(serde_json::to_vec(&summary).unwrap().len() <= APPEARANCE_SUMMARY_MAX_BYTES);
    }

    #[test]
    fn inspection_preflight_accounts_for_escaped_legacy_names_before_cloning() {
        let mut legacy = Scene::default();
        legacy
            .create(Primitive::Cube, &"\"".repeat(50_000))
            .unwrap();
        let native = NativeScene::from_legacy(legacy).unwrap();
        assert!(matches!(
            check_inspection_size(&native),
            Err(OperationError::TooLarge)
        ));
    }
}
