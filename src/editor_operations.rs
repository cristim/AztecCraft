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

/// Which objects `Operation::SelectByRule` selects, after Maya's Select menu rows. All selects
/// every transform instance, Hierarchy adds the descendants of the selection (the lead stays the
/// lead) and Inverse swaps selected and unselected. In Hierarchy selection mode All and Inverse
/// see only the roots and Hierarchy changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectRule {
    All,
    Hierarchy,
    Inverse,
}

/// How `Operation::ModifySelection` changes the object selection. Add appends an absent instance
/// as the lead, or moves a present one to the end; Toggle flips membership; Remove drops a member
/// and leaves an absent instance alone. Removing the lead promotes the previous member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionChange {
    Add,
    Toggle,
    Remove,
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
/// Early reject for a tool reply, before it is turned into a `serde_json::Value`, and the limit of
/// the inspection preflight. The `CallToolResult` carries the reply twice (the structured value
/// and a compact text copy that escapes every `"` and `\`, about 1.1 times the reply for
/// `inspect_scene`), so a reply near this cap makes a result of about 2.1 times its size. That
/// result is the authoritative bound: it must stay within `MAX_RESULT_BYTES`, and the 1 MiB
/// transport frame is the result plus about 100 bytes of envelope. This cap only bounds the
/// allocation made before the exact check, so it never admits more than that check would.
#[cfg(any(test, feature = "mcp"))]
pub const MAX_REPLY_BYTES: usize = MAX_RESULT_BYTES / 2;

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
    /// Adds, toggles or removes one object instance in the ordered object selection; the last
    /// member is the lead. Object and Hierarchy modes only.
    ModifySelection {
        change: SelectionChange,
        node: NodeId,
        instance_path: Option<Vec<NativeId>>,
    },
    /// Replaces the object selection by a rule (Select > All, Hierarchy, Inverse). Object and Hierarchy modes only.
    SelectByRule(SelectRule),
    SetSelectionMode(SelectionMode),
    /// Maya's Select > Deselect All (`SelectNone`, Alt+D): clears the object and component
    /// selection and returns to Object selection mode.
    DeselectAll,
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
    /// Deletes one node by id; only MCP `delete_node` and tests build it, the UI deletes through
    /// `DeleteSelection`.
    #[cfg_attr(not(any(test, feature = "mcp")), allow(dead_code))]
    Delete(NodeId),
    /// Edit > Delete on the whole object selection: every selected node is removed as one undo
    /// step, or none is. Object and Hierarchy modes only.
    DeleteSelection,
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
    /// The lead of the ordered object selection (`selection`'s last member), if any.
    pub selected_node: Option<String>,
    pub selected_instance_path: Vec<String>,
    /// Selected object instances in selection order; the last is the lead.
    pub selection: Vec<SelectionMemberInspection>,
    pub selection_count: usize,
    pub selection_mode: SelectionMode,
    pub remembered_face: Option<FaceSelectionTarget>,
    pub remembered_vertex: Option<VertexSelectionTarget>,
    pub remembered_edge: Option<EdgeSelectionTarget>,
    pub face_selection_policy: FaceSelectionPolicy,
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
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
    /// Why a press at the pointer would be refused, in the text native hover shows after
    /// `{space} {handle} {tool}: ` (a handle) or `{tool} unavailable: ` (the whole tool). Non-null
    /// with a `pointer_hit` means that handle's press is refused; non-null with a null
    /// `pointer_hit` means Rotate or Scale is refused at every pixel (for example a mirrored
    /// frame). Null for Move (its refusals stay in `kinds` and `axes`), Select, disabled handles,
    /// an accepted handle, no selection, an unavailable pointer and an active manipulation.
    pub pointer_refusal: Option<String>,
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
pub struct SelectionMemberInspection {
    pub node: String,
    pub instance_path: Vec<String>,
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
    /// An action that needs exactly one object ran while several were selected.
    MultipleSelection,
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
            Self::MultipleSelection => f.write_str("Several objects are selected; select one object"),
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
pub const STATUS_MESSAGE_MAX_CHARS: usize = 512;
#[cfg(any(test, feature = "mcp"))]
pub const STATUS_MESSAGE_WITHHELD: &str = "File operation failed (details withheld)";
#[cfg(test)]
pub(crate) const MOVE_INSPECTION_MAX_BYTES: usize = 32 * 1024;
#[cfg(test)]
const SESSION_MATERIAL_INSPECTION_MAX_BYTES: usize = 192;

// The inspection preflight is a guaranteed minimum: each constant below is the serialized length
// of the smallest value the DTO can take, so the sum never exceeds what `inspect_scene` writes and
// can never refuse a scene that would fit. It only has to bound the clone memory (a hostile scene
// can carry huge names); `tool_reply` measures the real reply. Per-field minima: `null` and `true`
// are 4 bytes (`false` is 5), floats `0.0` are 3, `[]` is 2, an `Option<String>` is `""` (2), a
// decimal id is `"1"` (3), and every key counts with its quotes and colon. Commas and fields
// skipped with `skip_serializing_if` count 0. Names add `len()` unescaped, which JSON can only
// lengthen.
#[cfg(any(test, feature = "mcp"))]
macro_rules! min_appearance_summary {
    () => {
        concat!(
            r#"{"state":"unbound","shapes":0,"checked_source_bindings":0,"unavailable":0,"unbound":0,"#,
            r#""issues":{"missing_membership":true,"overlapping_membership":true,"#,
            r#""unsupported_shader":true,"unsupported_graph":true,"ambiguous_instance":true,"#,
            r#""unsupported_settings":true,"uv_mismatch":true,"malformed_data":true,"#,
            r#""evaluation_unavailable":true,"quadratic_filtering_unproved":true,"#,
            r#""color_processor_unavailable":true}}"#
        )
    };
}
#[cfg(any(test, feature = "mcp"))]
const MIN_NODE: &str = concat!(
    r#"{"id":"1","name":"","transform_id":"1","visible":null,"#,
    r#""transform":{"translation":[0.0,0.0,0.0],"rotation_degrees":[0.0,0.0,0.0],"scale":[0.0,0.0,0.0]},"#,
    r#""authoring_transform":{"translation":[0.0,0.0,0.0],"rotation":[0.0,0.0,0.0],"scale":[0.0,0.0,0.0]},"#,
    r#""instance_paths":[],"shape_ids":[]}"#
);
#[cfg(any(test, feature = "mcp"))]
const MIN_SHAPE: &str = concat!(
    r#"{"id":"1","name":"","points":0,"edges":0,"faces":0,"smoothing_known":true,"#,
    r#""source_appearance":"#,
    min_appearance_summary!(),
    r#","#,
    r#""material":null,"session_material":"0"}"#
);
#[cfg(any(test, feature = "mcp"))]
const MIN_CURVE: &str = concat!(
    r#"{"id":"1","name":"","degree":0,"form":"open","control_vertices":0,"knots":0,"#,
    r#""domain":[0.0,0.0],"read_only":true,"draw_available":true}"#
);
#[cfg(any(test, feature = "mcp"))]
const MIN_SESSION_MATERIAL: &str =
    r#"{"id":"0","name":"","shading_group":"","color":[0.0,0.0,0.0],"editable":true}"#;
/// One path element in an `instance_path` array: `"1",`. A non-empty array of L elements costs
/// 4L + 1 bytes (the last element has no comma), so a member or target already counted with `[]`
/// adds 4L - 1; a node's `instance_paths` entry is a whole array and is charged 4L.
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_PATH_ELEMENT: usize = r#""1","#.len();
/// A `null` that a present remembered target replaces.
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_NULL: usize = "null".len();
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_SELECTION_MEMBER: &str = r#"{"node":"1","instance_path":[]}"#;
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_FACE_TARGET: &str =
    r#"{"node_id":"1","shape_id":"1","instance_path":[],"face_index":0}"#;
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_VERTEX_TARGET: &str =
    r#"{"node_id":"1","shape_id":"1","instance_path":[],"vertex_index":0}"#;
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_EDGE_TARGET: &str =
    r#"{"node_id":"1","shape_id":"1","instance_path":[],"edge_index":0}"#;
/// Everything in `SceneInspection` except the four lists; nested fixed objects at their minimum.
#[cfg(any(test, feature = "mcp"))]
pub(crate) const MIN_FIXED: &str = concat!(
    r#"{"scene_token":"00000000-0000-0000-0000-000000000000","#,
    r#""manipulation":{"active_tool":"","rotate_space":"","scale_space":"","rotate_axis":"","#,
    r#""scale_handle":"","manipulator_size":0.0,"units":{"distance":"","angle":"","factor":""},"#,
    r#""kinds":[],"active":null,"refusal":null,"viewport":null},"#,
    r#""nodes":[],"shapes":[],"session_materials":[],"curves":[],"#,
    r#""retained_source_bytes":0,"retained_source_records":0,"read_only_reason":"","#,
    r#""units":{"linear":"","angular":"","time":""},"save_available":true,"save_as_available":true,"#,
    r#""save_reconciliation_required":true,"document_origin":"","has_file":true,"closing":true,"#,
    r#""viewport_backend":"","viewport_error":null,"grid_visible":true,"shading":"","#,
    r#""face_centres_hidden":true,"vertex_markers_hidden":true,"#,
    r#""sidebar_panels":{"channel_box":true,"attribute_editor":true},"attribute_editor_tab":null,"#,
    r#""depth_complete":true,"clipping_complete":true,"appearance_incomplete":true,"#,
    r#""source_appearance":"#,
    min_appearance_summary!(),
    r#","#,
    r#""pending_action":null,"selected_node":"","selected_instance_path":[],"selection":[],"#,
    r#""selection_count":0,"selection_mode":"","remembered_face":null,"remembered_vertex":null,"#,
    r#""remembered_edge":null,"face_selection_policy":"","can_undo":true,"can_redo":true,"#,
    r#""undo_label":"","redo_label":"","dirty":true,"has_draft":true,"native_input_pending":true,"#,
    r#""text_focused":true,"modal":true,"status_message":"","#,
    r#""camera":{"target":[0.0,0.0,0.0],"eye":[0.0,0.0,0.0],"distance":0.0,"yaw_degrees":0.0,"#,
    r#""pitch_degrees":0.0,"fov_y_degrees":0.0,"near_clip":0.0,"far_clip":0.0}}"#
);

/// Guaranteed lower bound of the serialized `SceneInspection` for the scene alone: the fixed part
/// plus the minimum of every node, shape, curve and session material and each name's length.
#[cfg(any(test, feature = "mcp"))]
pub fn inspection_lower_bound(scene: &azteccraft::native_scene::NativeScene) -> usize {
    let nodes = scene.nodes().iter().fold(0usize, |sum, node| {
        sum.saturating_add(MIN_NODE.len())
            .saturating_add(node.name().len())
    });
    let shapes = scene.shapes().iter().fold(0usize, |sum, shape| {
        sum.saturating_add(MIN_SHAPE.len())
            .saturating_add(shape.name().len())
    });
    let curves = scene.curves().iter().fold(0usize, |sum, curve| {
        sum.saturating_add(MIN_CURVE.len())
            .saturating_add(curve.name().len())
    });
    // Sourced scenes (appearance incomplete) report an empty table.
    let materials = if scene.appearance_incomplete() {
        0
    } else {
        scene
            .session_materials()
            .len()
            .saturating_mul(MIN_SESSION_MATERIAL.len())
    };
    [nodes, shapes, curves, materials]
        .into_iter()
        .fold(MIN_FIXED.len(), usize::saturating_add)
}

/// Refuses before any clone when even the guaranteed minimum exceeds the reply ceiling.
#[cfg(any(test, feature = "mcp"))]
pub fn check_inspection_size(
    scene: &azteccraft::native_scene::NativeScene,
) -> Result<(), OperationError> {
    if inspection_lower_bound(scene) > MAX_REPLY_BYTES {
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
        assert!(materials * SESSION_MATERIAL_INSPECTION_MAX_BYTES * 2 <= MAX_REPLY_BYTES);
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

    fn zeros() -> [f64; 3] {
        [0.0; 3]
    }

    #[test]
    fn minimum_constants_are_the_smallest_serialized_dtos() {
        use azteccraft::native_scene::{
            AppearanceIssues, AppearanceState, AppearanceSummary, CurveForm,
        };
        let summary = AppearanceSummary {
            state: AppearanceState::Unbound,
            shapes: 0,
            checked_source_bindings: 0,
            unavailable: 0,
            unbound: 0,
            issues: AppearanceIssues {
                missing_membership: true,
                overlapping_membership: true,
                unsupported_shader: true,
                unsupported_graph: true,
                ambiguous_instance: true,
                unsupported_settings: true,
                uv_mismatch: true,
                malformed_data: true,
                evaluation_unavailable: true,
                quadratic_filtering_unproved: true,
                color_processor_unavailable: true,
            },
        };
        let node = NodeInspection {
            id: "1".into(),
            name: String::new(),
            transform_id: "1".into(),
            visible: None,
            transform: TransformInput {
                translation: zeros(),
                rotation_degrees: zeros(),
                scale: zeros(),
            },
            authoring_transform: AuthoringTransformInspection {
                translation: zeros(),
                rotation: zeros(),
                scale: zeros(),
            },
            transform_attributes: None,
            instance_paths: Vec::new(),
            shape_ids: Vec::new(),
        };
        assert_eq!(serde_json::to_string(&node).unwrap(), MIN_NODE);
        let shape = ShapeInspection {
            id: "1".into(),
            name: String::new(),
            points: 0,
            edges: 0,
            faces: 0,
            smoothing_known: true,
            source_appearance: summary,
            material: None,
            session_material: Some("0".into()),
        };
        assert_eq!(serde_json::to_string(&shape).unwrap(), MIN_SHAPE);
        let curve = CurveInspection {
            id: "1".into(),
            name: String::new(),
            degree: 0,
            form: CurveForm::Open,
            control_vertices: 0,
            knots: 0,
            domain: [0.0; 2],
            read_only: true,
            draw_available: true,
        };
        assert_eq!(serde_json::to_string(&curve).unwrap(), MIN_CURVE);
        let material = SessionMaterialInspection {
            id: "0".into(),
            name: String::new(),
            shading_group: String::new(),
            color: zeros(),
            editable: true,
        };
        assert_eq!(
            serde_json::to_string(&material).unwrap(),
            MIN_SESSION_MATERIAL
        );
    }

    #[test]
    fn path_minimum_constants_are_the_smallest_serialized_dtos() {
        let member = SelectionMemberInspection {
            node: "1".into(),
            instance_path: Vec::new(),
        };
        assert_eq!(
            serde_json::to_string(&member).unwrap(),
            MIN_SELECTION_MEMBER
        );
        let face = FaceSelectionTarget {
            node_id: "1".into(),
            shape_id: "1".into(),
            instance_path: Vec::new(),
            face_index: 0,
        };
        assert_eq!(serde_json::to_string(&face).unwrap(), MIN_FACE_TARGET);
        let vertex = VertexSelectionTarget {
            node_id: "1".into(),
            shape_id: "1".into(),
            instance_path: Vec::new(),
            vertex_index: 0,
        };
        assert_eq!(serde_json::to_string(&vertex).unwrap(), MIN_VERTEX_TARGET);
        let edge = EdgeSelectionTarget {
            node_id: "1".into(),
            shape_id: "1".into(),
            instance_path: Vec::new(),
            edge_index: 0,
        };
        assert_eq!(serde_json::to_string(&edge).unwrap(), MIN_EDGE_TARGET);
        let path = SelectionMemberInspection {
            node: "1".into(),
            instance_path: vec!["1".into(); 5],
        };
        let with = serde_json::to_string(&path).unwrap().len();
        assert_eq!(with - MIN_SELECTION_MEMBER.len(), 5 * MIN_PATH_ELEMENT - 1);
    }

    fn cubes(count: usize) -> NativeScene {
        let mut scene = NativeScene::default();
        for index in 0..count {
            scene
                .create(Primitive::Cube, &format!("cube_{index:03}"))
                .unwrap();
        }
        scene
    }

    fn named_legacy(name: &str) -> NativeScene {
        let mut legacy = Scene::default();
        legacy.create(Primitive::Cube, name).unwrap();
        NativeScene::from_legacy(legacy).unwrap()
    }

    #[test]
    fn the_preflight_accepts_scenes_the_old_worst_case_estimate_refused() {
        // The old estimate refused from 66 cubes although 120 serialize to about 105 KB.
        for count in [65, 66, 120, 140] {
            assert!(check_inspection_size(&cubes(count)).is_ok(), "{count}");
        }
    }

    #[test]
    fn the_inspection_capacity_follows_the_minimum_sizes_and_the_limit() {
        // Pinned from the limit and the constants, not as a second literal.
        let per_cube = MIN_NODE.len() + MIN_SHAPE.len() + "cube_000".len() + "cube_000Shape".len();
        let fixed = MIN_FIXED.len() + MIN_SESSION_MATERIAL.len();
        let capacity = (MAX_REPLY_BYTES - fixed) / per_cube;
        assert!(check_inspection_size(&cubes(capacity)).is_ok());
        assert!(matches!(
            check_inspection_size(&cubes(capacity + 1)),
            Err(OperationError::TooLarge)
        ));
    }

    #[test]
    fn the_preflight_refuses_names_whose_minimum_exceeds_the_limit_before_cloning() {
        for length in [300_000, MAX_RESULT_BYTES] {
            assert!(matches!(
                check_inspection_size(&named_legacy(&"\"".repeat(length))),
                Err(OperationError::TooLarge)
            ));
        }
        // 50 000 quotes serialize to about 100 KB, which fits the ceiling.
        assert!(check_inspection_size(&named_legacy(&"\"".repeat(50_000))).is_ok());
    }

    #[test]
    fn the_preflight_refuses_many_long_curve_names_before_cloning() {
        let mut source = String::from(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"root\";",
        );
        for index in 0..100 {
            source.push_str(&format!(" createNode nurbsCurve -n \"c{index}_{}\" -p \"root\"; setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 1 0 2 -1 0 3 0 0;","x".repeat(3000)));
        }
        let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
        assert!(matches!(
            check_inspection_size(&scene),
            Err(OperationError::TooLarge)
        ));
    }

    #[test]
    fn the_preflight_charges_node_and_shape_names_separately() {
        // Each list alone stays under the limit; only the sum of both refuses.
        let mut legacy = Scene::default();
        let name = "s".repeat(3500);
        for index in 0..40 {
            legacy
                .create(Primitive::Cube, &format!("{index:02}{name}"))
                .unwrap();
        }
        let scene = NativeScene::from_legacy(legacy).unwrap();
        assert!(matches!(
            check_inspection_size(&scene),
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
