#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::{NativeTransform, TransformAttributes};
use azteccraft::{
    history::{ManipulationError, ManipulationFinish, ManipulationId},
    move_handles::ManipulatorSize,
    native_scene::{
        EdgeTarget, FaceTarget, NativeError, NativeId, RotateAxis, RotateSpace, ScaleHandle,
        ScaleSpace, TransformTarget, VertexTarget, WorldAxis,
    },
    scene::{NodeId, Primitive},
    viewport::{InvalidCameraInput, InvalidCameraPose},
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

// schemars derives expand to unqualified `schemars::` paths; resolve them via rmcp's re-export.
#[cfg(feature = "mcp")]
use rmcp::schemars;

/// Sidebar panels the editor implements; the dock shows at most one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarPanel {
    ChannelBox,
    AttributeEditor,
}

/// An Attribute Editor tab. Each variant names its owner: the selected transform, or a direct
/// mesh shape (its own tab, or the material connected to it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeEditorTab {
    Transform(NodeId),
    Shape(NativeId),
    Material(NativeId),
}

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
    MayaAscii,
    LegacyAztec,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionMode {
    #[default]
    Object,
    Face,
    Hierarchy,
    Vertex,
    Edge,
}

impl SelectionMode {
    pub fn is_component(self) -> bool {
        matches!(self, Self::Vertex | Self::Edge | Self::Face)
    }
    pub fn component_noun(self) -> Option<&'static str> {
        match self {
            Self::Vertex => Some("Vertex"),
            Self::Edge => Some("Edge"),
            Self::Face => Some("Face"),
            Self::Object | Self::Hierarchy => None,
        }
    }
}
/// Session-only viewport shading, like grid visibility; never saved or dirtying.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ShadingMode {
    Wireframe,
    #[default]
    SmoothShadeAll,
}
#[cfg(any(test, feature = "mcp"))]
use uuid::Uuid;

#[cfg(feature = "mcp")]
pub const MAX_NAME_BYTES: usize = 4096;
#[cfg(any(test, feature = "mcp"))]
pub const MAX_RESULT_BYTES: usize = 512 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, Serialize)]
#[cfg_attr(feature = "mcp", derive(rmcp::schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ActiveTool {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
}

/// The space a Rotate or Scale handle works in. Gimbal, Parent, Normal, Custom and Component
/// are unsupported and have no variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManipulationSpace {
    Object,
    World,
}

impl ManipulationSpace {
    pub fn rotate(self) -> RotateSpace {
        match self {
            Self::Object => RotateSpace::Object,
            Self::World => RotateSpace::World,
        }
    }

    pub fn scale(self) -> ScaleSpace {
        match self {
            Self::Object => ScaleSpace::Object,
            Self::World => ScaleSpace::World,
        }
    }
}

/// A Rotate angle or a Scale factor. `Degrees` and `Factor` are what the history takes; `Angle`
/// is an absolute angle in the scene's declared angular unit and is converted to degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManipulationValue {
    Degrees(f64),
    Angle(f64),
    Factor(f64),
}

pub enum MoveDistance {
    Centimeters(f64),
    #[cfg(any(test, feature = "mcp"))]
    Declared(f64),
}

/// The persistent selection behind the Rotate axis and Scale handle controls. The sum type makes
/// a Rotate "uniform" or a Scale axis-only choice unrepresentable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolHandle {
    RotateAxis(RotateAxis),
    ScaleHandle(ScaleHandle),
}

pub enum Operation {
    SetActiveTool(ActiveTool),
    /// Sets the Axis Orientation of the Rotate or Scale tool settings; any other `tool` is an
    /// `InvalidInput` error.
    SetToolSpace {
        tool: ActiveTool,
        space: ManipulationSpace,
    },
    /// Sets the persistent Rotate axis or Scale handle that numeric entry uses.
    SetToolHandle(ToolHandle),
    /// Sets the global manipulator size, a session-only tool setting.
    SetManipulatorSize(ManipulatorSize),
    BeginMove {
        target: TransformTarget,
        axis: WorldAxis,
    },
    UpdateMove {
        id: ManipulationId,
        distance: MoveDistance,
    },
    BeginRotate {
        target: TransformTarget,
        axis: RotateAxis,
        space: ManipulationSpace,
    },
    BeginScale {
        target: TransformTarget,
        handle: ScaleHandle,
        space: ManipulationSpace,
    },
    UpdateManipulation {
        id: ManipulationId,
        value: ManipulationValue,
    },
    FinishMove {
        id: ManipulationId,
        finish: ManipulationFinish,
    },
    #[cfg(any(test, feature = "mcp"))]
    SetMaterialDiffuse {
        shape: NativeId,
        value: f64,
    },
    /// Ids returned by these three are valid only at the current history position.
    #[cfg(any(test, feature = "mcp"))]
    CreateSessionMaterial {
        color: [f64; 3],
    },
    #[cfg(any(test, feature = "mcp"))]
    AssignSessionMaterial {
        shape: NativeId,
        material: azteccraft::native_scene::MaterialChoice,
    },
    #[cfg(any(test, feature = "mcp"))]
    SetSessionMaterialColor {
        material: azteccraft::native_scene::SessionMaterialId,
        color: [f64; 3],
    },
    #[cfg(any(test, feature = "mcp"))]
    Inspect,
    Create {
        primitive: Primitive,
        options: Option<azteccraft::primitive_options::PrimitiveOptions>,
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
    SelectVertex {
        target: Option<VertexTarget>,
    },
    SelectEdge {
        target: Option<EdgeTarget>,
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
    FrameAll,
    SetGridVisible {
        visible: bool,
    },
    SetShadingMode(ShadingMode),
    SetSidebarPanelVisible {
        panel: SidebarPanel,
        visible: bool,
    },
    SelectAttributeEditorTab {
        tab: AttributeEditorTab,
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
    /// Absolute view, radians like `Orbit`. The lens and clip planes are untouched. Unchanged is
    /// judged numerically, so a pose differing only in zero signs replies Unchanged.
    SetCameraPose {
        target: [f32; 3],
        distance: f32,
        yaw: f32,
        pitch: f32,
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
    ManipulationStarted {
        manipulation_id: String,
    },
    /// Created or assigned session material; the id is valid only at this history position.
    #[cfg(any(test, feature = "mcp"))]
    SessionMaterial {
        material_id: String,
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
    pub manipulation: ManipulationInspection,
    pub nodes: Vec<NodeInspection>,
    pub shapes: Vec<ShapeInspection>,
    /// Session-only material table, empty on sourced scenes: entry 0 is `standardSurface1` on
    /// `initialShadingGroup`, modelled as a placeholder lambert colour (not a standardSurface),
    /// and later entries are lamberts. Ids are valid only at the current history position.
    pub session_materials: Vec<SessionMaterialInspection>,
    pub curves: Vec<CurveInspection>,
    pub retained_source_bytes: usize,
    pub retained_source_records: usize,
    pub read_only_reason: Option<String>,
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
    pub shading: ShadingMode,
    pub face_centres_hidden: bool,
    pub vertex_markers_hidden: bool,
    pub sidebar_panels: SidebarPanelsInspection,
    pub attribute_editor_tab: Option<AttributeEditorTabInspection>,
    pub depth_complete: bool,
    pub clipping_complete: bool,
    pub appearance_incomplete: bool,
    pub source_appearance: azteccraft::native_scene::AppearanceSummary,
    pub pending_action: Option<PendingInspection>,
    pub selected_node: Option<String>,
    pub selected_instance_path: Vec<String>,
    pub selection_mode: SelectionMode,
    pub remembered_face: Option<FaceSelectionTarget>,
    pub remembered_vertex: Option<VertexSelectionTarget>,
    pub remembered_edge: Option<EdgeSelectionTarget>,
    pub face_selection_policy: FaceSelectionPolicy,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub has_draft: bool,
    pub native_input_pending: bool,
    pub text_focused: bool,
    pub modal: bool,
    /// The native status line text, or a fixed notice when the text would carry a local path.
    pub status_message: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg(any(test, feature = "mcp"))]
pub struct VertexSelectionTarget {
    pub node_id: String,
    pub shape_id: String,
    #[cfg_attr(feature = "mcp", schemars(length(min = 2, max = 257)))]
    pub instance_path: Vec<String>,
    pub vertex_index: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[cfg(any(test, feature = "mcp"))]
pub struct EdgeSelectionTarget {
    pub node_id: String,
    pub shape_id: String,
    #[cfg_attr(feature = "mcp", schemars(length(min = 2, max = 257)))]
    pub instance_path: Vec<String>,
    pub edge_index: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg(any(test, feature = "mcp"))]
pub enum FaceSelectionPolicy {
    VisibleCentersOnly,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct ManipulationInspection {
    pub active_tool: ActiveTool,
    /// The Axis Orientation each tool uses for its next manipulation.
    pub rotate_space: ManipulationSpace,
    pub scale_space: ManipulationSpace,
    /// The persistent Rotate axis ("x", "y", "z") and Scale handle ("x", "y", "z", "uniform")
    /// that numeric entry uses, spelled as the tool arguments are.
    pub rotate_axis: &'static str,
    pub scale_handle: &'static str,
    /// The global manipulator size, as set.
    pub manipulator_size: f64,
    /// What each parameter of `update_manipulation` is measured in.
    pub units: ManipulationUnits,
    /// Whether a numeric manipulation of each kind could begin on the selection now.
    pub kinds: Vec<KindInspection>,
    pub active: Option<ActiveManipulationInspection>,
    /// Why the last update of the active manipulation was refused; the preview is unchanged.
    pub refusal: Option<String>,
    pub viewport: Option<MoveViewportInspection>,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct ManipulationUnits {
    pub distance: String,
    pub angle: String,
    pub factor: &'static str,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct KindInspection {
    pub kind: &'static str,
    pub available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct ActiveManipulationInspection {
    pub manipulation_id: String,
    pub kind: &'static str,
    pub node_id: String,
    pub instance_path: Vec<String>,
    pub space: &'static str,
    pub axis: Option<&'static str>,
    pub handle: Option<&'static str>,
    pub provenance: &'static str,
    /// `distance`, `angle` or `factor`: the parameter `update_manipulation` takes for this kind.
    pub parameter: &'static str,
    pub value: f64,
    pub unit: String,
    /// `translate`, `rotate` or `scale`: the three channels this manipulation edits.
    pub channels: &'static str,
    pub baseline_channels: [f64; 3],
    pub current_channels: [f64; 3],
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct MoveViewportInspection {
    pub provenance: &'static str,
    pub axes: Vec<MoveAxisInspection>,
    /// The active tool's enabled handle under the last native pointer: Move and Rotate `x|y|z`,
    /// Scale `x|y|z|uniform`; null for Select, no hit, or while a manipulation is active.
    pub pointer_hit: Option<&'static str>,
    /// `free_rotate`, `view_ring` or `scale_plane` when a drawn-but-unsupported handle is under
    /// the pointer and no enabled handle outranks it; null otherwise.
    pub pointer_disabled: Option<&'static str>,
    pub pointer_available: bool,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct MoveAxisInspection {
    pub axis: &'static str,
    pub visible: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct NodeInspection {
    pub id: String,
    pub name: String,
    pub transform_id: String,
    /// The node's own `.v`, not effective visibility; null when the source did not prove it.
    pub visible: Option<bool>,
    pub transform: TransformInput,
    pub authoring_transform: AuthoringTransformInspection,
    /// Read-only transform extras; present only when at least one differs from Maya's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform_attributes: Option<TransformAttributes>,
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
pub struct SidebarPanelsInspection {
    pub channel_box: bool,
    pub attribute_editor: bool,
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct AttributeEditorTabInspection {
    pub kind: &'static str,
    pub id: String,
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
    /// Session material id every instance of this shape shows; null on sourced scenes.
    pub session_material: Option<String>,
}
#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct SessionMaterialInspection {
    pub id: String,
    pub name: String,
    pub shading_group: String,
    pub color: [f64; 3],
    pub editable: bool,
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
pub struct CurveInspection {
    pub id: String,
    pub name: String,
    pub degree: u32,
    pub form: azteccraft::native_scene::CurveForm,
    pub control_vertices: usize,
    pub knots: usize,
    pub domain: [f64; 2],
    pub read_only: bool,
    pub draw_available: bool,
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
    pub yaw_degrees: f64,
    pub pitch_degrees: f64,
    pub fov_y_degrees: f64,
    pub near_clip: f32,
    pub far_clip: f32,
}

#[derive(Debug)]
pub enum OperationError {
    BusyEditing,
    BusyManipulation,
    Move(ManipulationError),
    #[cfg(any(test, feature = "mcp"))]
    StaleScene,
    #[cfg(any(test, feature = "mcp"))]
    TooLarge,
    /// A session-material operation failed in the core; the MCP layer reports a typed code.
    #[cfg(any(test, feature = "mcp"))]
    SessionMaterial(NativeError),
    NoSelection,
    /// Frame All on a scene without any mesh or curve instance.
    NothingToFrame,
    InvalidInput(String),
    Scene(NativeError),
    /// Path-free display or render failure (`ViewError`, `RenderError`).
    View(String),
    /// Text may carry a filesystem path (`DocumentError`, `ObjError`); never expose it remotely.
    File(String),
    NativeSave(azteccraft::document::NativeSaveError),
    AmbiguousSelection,
    StalePendingAction,
    Camera(InvalidCameraInput),
    /// A refused absolute pose; names the field.
    CameraPose(InvalidCameraPose),
    /// An update parameter that does not belong to the active manipulation's kind.
    KindMismatch {
        parameter: &'static str,
        active: &'static str,
    },
}

impl OperationError {
    pub(crate) fn from_history(error: NativeError) -> Self {
        match error {
            NativeError::BusyManipulation => Self::BusyManipulation,
            error => Self::Scene(error),
        }
    }
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BusyManipulation => f.write_str("Finish or cancel the active manipulation"),
            Self::Move(error) => error.fmt(f),
            Self::BusyEditing => f.write_str("The editor is busy with a text edit, dialog, close or pending input; try again when it finishes"),
            #[cfg(any(test, feature = "mcp"))]
            Self::StaleScene => f.write_str("Scene token is stale; inspect the current document"),
            #[cfg(any(test, feature = "mcp"))]
            Self::TooLarge => f.write_str("Scene inspection exceeds the prototype response limit"),
            #[cfg(any(test, feature = "mcp"))]
            Self::SessionMaterial(error) => error.fmt(f),
            Self::NoSelection => f.write_str("Select an object or face before framing"),
            Self::NothingToFrame => f.write_str("The scene has no objects to frame"),
            Self::InvalidInput(message) => f.write_str(message),
            Self::Scene(error) => error.fmt(f),
            Self::View(error) | Self::File(error) => f.write_str(error),
            Self::NativeSave(error) => error.fmt(f),
            Self::AmbiguousSelection => {
                f.write_str("object has multiple instances; supply an instance_path")
            }
            Self::StalePendingAction => {
                f.write_str("pending file action is absent or stale; inspect before deciding")
            }
            Self::Camera(error) => error.fmt(f),
            Self::CameraPose(error) => error.fmt(f),
            Self::KindMismatch { parameter, active } => write!(
                f,
                "{} {parameter} does not apply to the active {active} manipulation",
                if parameter.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "An"
                } else {
                    "A"
                }
            ),
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
/// Worst case of the serialized `transform_attributes` object plus its key, asserted by a test.
const NODE_TRANSFORM_ATTRIBUTES_MAX_BYTES: usize = 640;
#[cfg(any(test, feature = "mcp"))]
const NODE_VISIBLE_FIELD_MAX_BYTES: usize = "\"visible\":false,".len();
#[cfg(any(test, feature = "mcp"))]
pub const STATUS_MESSAGE_MAX_CHARS: usize = 512;
#[cfg(any(test, feature = "mcp"))]
pub const STATUS_MESSAGE_WITHHELD: &str = "File operation failed (details withheld)";
/// Six bytes per character covers JSON escaping of control characters.
#[cfg(any(test, feature = "mcp"))]
const STATUS_MESSAGE_MAX_BYTES: usize = STATUS_MESSAGE_MAX_CHARS * 6 + 32;
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MOVE_INSPECTION_MAX_BYTES: usize = 32 * 1024;
/// Nine f32 values (at most 15 JSON bytes each), three f64 degree values (at most 24) and the
/// field names: about 330 bytes in the worst case.
#[cfg(any(test, feature = "mcp"))]
const CAMERA_INSPECTION_MAX_BYTES: usize = 512;

#[cfg(any(test, feature = "mcp"))]
const SESSION_MATERIAL_INSPECTION_MAX_BYTES: usize = 192;

#[cfg(any(test, feature = "mcp"))]
pub fn check_inspection_size(
    scene: &azteccraft::native_scene::NativeScene,
) -> Result<(), OperationError> {
    let limit = MAX_RESULT_BYTES / 2;
    let mut estimated_bytes = APPEARANCE_SUMMARY_MAX_BYTES
        + MOVE_INSPECTION_MAX_BYTES
        + STATUS_MESSAGE_MAX_BYTES
        + CAMERA_INSPECTION_MAX_BYTES;
    for node in scene.nodes() {
        estimated_bytes = estimated_bytes
            .saturating_add(1024)
            .saturating_add(node.name().len().saturating_mul(6))
            .saturating_add(512)
            .saturating_add(NODE_VISIBLE_FIELD_MAX_BYTES);
        if !node.extras().is_default() {
            estimated_bytes = estimated_bytes.saturating_add(NODE_TRANSFORM_ATTRIBUTES_MAX_BYTES);
        }
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
    for curve in scene.curves() {
        estimated_bytes = estimated_bytes
            .saturating_add(512)
            .saturating_add(curve.name().len().saturating_mul(6));
        if estimated_bytes > limit {
            return Err(OperationError::TooLarge);
        }
    }
    estimated_bytes = estimated_bytes.saturating_add(
        scene
            .session_materials()
            .len()
            .saturating_mul(SESSION_MATERIAL_INSPECTION_MAX_BYTES),
    );
    estimated_bytes = estimated_bytes.saturating_add(scene.dag_edges().len().saturating_mul(24));
    if estimated_bytes > limit {
        return Err(OperationError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_session_material_table_stays_well_inside_the_inspection_budget() {
        let materials = azteccraft::native_scene::MAX_SESSION_MATERIALS;
        assert!(materials * SESSION_MATERIAL_INSPECTION_MAX_BYTES * 2 <= MAX_RESULT_BYTES / 2);
        let widest = SessionMaterialInspection {
            id: (materials - 1).to_string(),
            // Material numbers are u32 (adding past u32::MAX is a resource-limit refusal).
            name: format!("lambert{}", u32::MAX),
            shading_group: format!("lambert{}SG", u32::MAX),
            color: [-1.2345678901234567e-308; 3],
            editable: true,
        };
        assert!(
            serde_json::to_vec(&widest).unwrap().len() <= SESSION_MATERIAL_INSPECTION_MAX_BYTES
        );
    }
    use azteccraft::{native_scene::NativeScene, scene::Scene};

    #[test]
    fn kind_mismatch_uses_the_right_article() {
        for (parameter, expected) in [
            (
                "angle",
                "An angle does not apply to the active move manipulation",
            ),
            (
                "factor",
                "A factor does not apply to the active move manipulation",
            ),
            (
                "distance",
                "A distance does not apply to the active move manipulation",
            ),
        ] {
            let error = OperationError::KindMismatch {
                parameter,
                active: "move",
            };
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn curve_inspection_preflight_bounds_names_and_fixed_fields_before_cloning() {
        let curve = CurveInspection {
            id: u64::MAX.to_string(),
            name: String::new(),
            degree: u32::MAX,
            form: azteccraft::native_scene::CurveForm::Open,
            control_vertices: usize::MAX,
            knots: usize::MAX,
            domain: [-f64::MAX, f64::MAX],
            read_only: true,
            draw_available: true,
        };
        assert!(serde_json::to_vec(&curve).unwrap().len() <= 512);
        let mut source = String::from(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"root\";",
        );
        for index in 0..100 {
            source.push_str(&format!(" createNode nurbsCurve -n \"c{index}_{}\" -p \"root\"; setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 1 0 2 -1 0 3 0 0;","x".repeat(650)));
        }
        let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
        assert!(matches!(
            check_inspection_size(&scene),
            Err(OperationError::TooLarge)
        ));
    }

    #[test]
    fn inspection_preflight_reserves_bounded_move_metadata_before_cloning_scene() {
        let mut scene = NativeScene::default();
        for index in 0..66 {
            scene
                .create(Primitive::Cube, &format!("cube_{index:02}"))
                .unwrap();
        }
        assert!(matches!(
            check_inspection_size(&scene),
            Err(OperationError::TooLarge)
        ));
    }

    #[test]
    fn default_nodes_keep_the_base_inspection_capacity() {
        // Default nodes must not reserve bytes for transform extras: the first node count that
        // exceeds the budget is pinned, so a per-node reservation that grows shows up here.
        let mut scene = NativeScene::default();
        let mut first_too_large = None;
        for index in 0..200 {
            scene
                .create(Primitive::Cube, &format!("cube_{index:02}"))
                .unwrap();
            if check_inspection_size(&scene).is_err() {
                first_too_large = Some(index + 1);
                break;
            }
        }
        assert_eq!(first_too_large, Some(65));
    }

    #[test]
    fn node_visible_field_is_covered_by_the_preflight_estimate() {
        let node = |visible| NodeInspection {
            id: String::new(),
            name: String::new(),
            transform_id: String::new(),
            visible,
            transform: TransformInput {
                translation: [0.0; 3],
                rotation_degrees: [0.0; 3],
                scale: [0.0; 3],
            },
            authoring_transform: AuthoringTransformInspection {
                translation: [0.0; 3],
                rotation: [0.0; 3],
                scale: [0.0; 3],
            },
            transform_attributes: None,
            instance_paths: Vec::new(),
            shape_ids: Vec::new(),
        };
        let base = serde_json::to_vec(&node(None)).unwrap().len();
        for visible in [Some(true), Some(false)] {
            let with = serde_json::to_vec(&node(visible)).unwrap().len();
            assert!(with - base <= NODE_VISIBLE_FIELD_MAX_BYTES);
        }
        let worst = TransformAttributes {
            rotate_order: "zyx",
            rotate_axis_degrees: [-1.7976931348623157e308; 3],
            shear: [-1.7976931348623157e308; 3],
            rotate_pivot_translate: [-1.7976931348623157e308; 3],
            scale_pivot_translate: [-1.7976931348623157e308; 3],
            non_default: vec![
                "rotate_order",
                "rotate_axis",
                "shear",
                "rotate_pivot_translate",
                "scale_pivot_translate",
            ],
        };
        let size = serde_json::to_vec(&worst).unwrap().len() + "\"transform_attributes\":,".len();
        assert!(size <= NODE_TRANSFORM_ATTRIBUTES_MAX_BYTES, "{size}");
        assert!(
            String::from_utf8(serde_json::to_vec(&node(None)).unwrap())
                .unwrap()
                .contains("\"visible\":null")
        );
    }

    #[test]
    fn camera_inspection_preflight_bounds_the_longest_serialized_values() {
        let worst = -f32::MIN_POSITIVE * 1.000_000_1;
        let camera = CameraInspection {
            target: [worst; 3],
            eye: [f32::MIN; 3],
            distance: worst,
            yaw_degrees: f64::from(f32::MIN).to_degrees(),
            // Unreachable for a valid camera, but the longest f64 text.
            pitch_degrees: -f64::MIN_POSITIVE,
            fov_y_degrees: f64::from(worst).to_degrees(),
            near_clip: worst,
            far_clip: worst,
        };
        assert!(serde_json::to_vec(&camera).unwrap().len() <= CAMERA_INSPECTION_MAX_BYTES);
    }

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

    #[test]
    fn history_busy_manipulation_keeps_its_busy_error_class() {
        assert!(matches!(
            OperationError::from_history(NativeError::BusyManipulation),
            OperationError::BusyManipulation
        ));
        assert!(matches!(
            OperationError::from_history(NativeError::ResourceLimit),
            OperationError::Scene(NativeError::ResourceLimit)
        ));
    }
}
