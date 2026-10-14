#[cfg(any(test, feature = "mcp"))]
use azteccraft::scene::Mesh;
use azteccraft::{
    scene::{NodeId, Primitive, SceneError, Transform},
    viewport::InvalidCameraInput,
};
use serde::Serialize;
use std::fmt;
#[cfg(any(test, feature = "mcp"))]
use uuid::Uuid;

#[cfg(feature = "mcp")]
pub const MAX_NAME_BYTES: usize = 4096;
#[cfg(any(test, feature = "mcp"))]
pub const MAX_RESULT_BYTES: usize = 512 * 1024;

#[cfg(any(test, feature = "mcp"))]
pub const TOOL_NAMES: [&str; 12] = [
    "inspect_scene",
    "create_primitive",
    "select_node",
    "rename_node",
    "set_transform",
    "delete_node",
    "undo",
    "redo",
    "frame_selected",
    "orbit_camera",
    "pan_camera",
    "dolly_camera",
];

pub enum Operation {
    #[cfg(any(test, feature = "mcp"))]
    Inspect,
    Create {
        primitive: Primitive,
        name: Option<String>,
    },
    Select(Option<NodeId>),
    Rename {
        id: NodeId,
        name: String,
    },
    SetTransform {
        id: NodeId,
        transform: Transform,
    },
    ExtrudeFace {
        id: NodeId,
        face_index: usize,
        distance: f32,
    },
    Delete(NodeId),
    Undo,
    Redo,
    FrameSelected,
    #[cfg(any(test, feature = "mcp"))]
    Orbit {
        radians: [f32; 2],
    },
    #[cfg(any(test, feature = "mcp"))]
    Pan {
        world_units: [f32; 2],
    },
    #[cfg(any(test, feature = "mcp"))]
    Dolly {
        factor: f32,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OperationReply {
    #[cfg(any(test, feature = "mcp"))]
    Scene {
        inspection: SceneInspection,
    },
    Created {
        node_id: String,
    },
    Changed,
    Unchanged,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct SceneInspection {
    pub scene_token: String,
    pub nodes: Vec<NodeInspection>,
    pub selected_node: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub has_draft: bool,
    pub native_input_pending: bool,
    pub text_focused: bool,
    pub modal: bool,
    pub camera: CameraInspection,
}

#[derive(Debug, Serialize)]
#[cfg(any(test, feature = "mcp"))]
pub struct NodeInspection {
    pub id: String,
    pub name: String,
    pub transform: Transform,
    pub mesh: Mesh,
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
    #[cfg(any(test, feature = "mcp"))]
    BusyEditing,
    #[cfg(any(test, feature = "mcp"))]
    StaleScene,
    #[cfg(any(test, feature = "mcp"))]
    TooLarge,
    NoSelection,
    InvalidInput(String),
    Scene(SceneError),
    Camera(InvalidCameraInput),
}

impl fmt::Display for OperationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(any(test, feature = "mcp"))]
            Self::BusyEditing => f.write_str(
                "Process native input and finish the text edit or dialog before remote mutation",
            ),
            #[cfg(any(test, feature = "mcp"))]
            Self::StaleScene => f.write_str("Scene token is stale; inspect the current document"),
            #[cfg(any(test, feature = "mcp"))]
            Self::TooLarge => f.write_str("Scene inspection exceeds the prototype response limit"),
            Self::NoSelection => f.write_str("Select an object before framing"),
            Self::InvalidInput(message) => f.write_str(message),
            Self::Scene(error) => error.fmt(f),
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
pub fn check_inspection_size(scene: &azteccraft::scene::Scene) -> Result<(), OperationError> {
    let limit = MAX_RESULT_BYTES / 2;
    let mut estimated_bytes = 0_usize;
    for node in scene.nodes() {
        estimated_bytes = estimated_bytes
            .saturating_add(1024)
            .saturating_add(node.name().len())
            .saturating_add(node.mesh().vertices().len().saturating_mul(12))
            .saturating_add(
                node.mesh()
                    .faces()
                    .len()
                    .saturating_mul(4 * size_of::<usize>()),
            );
        if estimated_bytes > limit {
            return Err(OperationError::TooLarge);
        }
    }
    Ok(())
}
