//! Guard matrix: today's behavior of every `Operation` through both dispatch pipelines.
//!
//! Each cell runs one sample operation on a freshly built editor in one state, through either the
//! native pipeline (`apply_operation_guarded`, provenance Native) or the remote pipeline
//! (`remote_operation_guarded`), and records the result plus every observable side effect that
//! differs from the state's baseline. The text lives in `tests/snapshots/guard_matrix.txt`; it is
//! generated from the code, never edited by hand (`UPDATE_SNAPSHOTS=1`).
//!
//! Adding an `Operation` variant fails to compile in `expand`, which matches without a wildcard.
//! The new arm must list the samples that exercise that variant.

use super::*;
use crate::editor_operations::{
    FileDecision, ManipulationSpace, ManipulationValue, MoveDistance, RemoteRequest, SelectRule,
    ToolHandle, TransformInput,
};
use azteccraft::history::Command;
use azteccraft::history::{ManipulationFinish, ManipulationId};
use azteccraft::move_handles::ManipulatorSize;
use azteccraft::native_scene::{
    MaterialChoice, NativeScene, RotateAxis, ScaleHandle, SessionMaterialId, TransformTarget,
    WorldAxis,
};
use azteccraft::primitive_options::{Axis, PrimitiveOptions};
use std::fmt::Write as _;

#[allow(dead_code)]
mod material_wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}
#[allow(dead_code)]
mod material_fixture {
    use super::material_wire as wire;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_appearance.rs"
    ));
}
#[allow(dead_code)]
mod sited_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sites.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sited.rs"
    ));
}
mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pipeline {
    Native,
    Remote,
}

impl Pipeline {
    fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Remote => "remote",
        }
    }
}

/// Editor conditions. `StaleToken` exists only for the remote pipeline, which alone carries a
/// scene token; `NoInstall*` run only the three operations that thread `may_install`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Closing,
    Modal,
    LocalMove,
    RemoteMove,
    NameDraft,
    MaterialDraft,
    ChannelDraft,
    TextFocus,
    NativeInputPending,
    StaleToken,
    StaleLocalMove,
    StaleRemoteMove,
    StaleClosing,
    StaleModal,
    StaleNameDraft,
    StaleNativeInput,
    ReadOnlyAscii,
    BinaryDirty,
    SitedBinaryDirty,
    NoInstall,
    NoInstallModal,
}

const STATES: [State; 22] = [
    State::Idle,
    State::Closing,
    State::Modal,
    State::LocalMove,
    State::RemoteMove,
    State::NameDraft,
    State::MaterialDraft,
    State::ChannelDraft,
    State::TextFocus,
    State::NativeInputPending,
    State::StaleToken,
    State::StaleLocalMove,
    State::StaleRemoteMove,
    State::StaleClosing,
    State::StaleModal,
    State::StaleNameDraft,
    State::StaleNativeInput,
    State::ReadOnlyAscii,
    State::BinaryDirty,
    State::SitedBinaryDirty,
    State::NoInstall,
    State::NoInstallModal,
];

impl State {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Closing => "closing",
            Self::Modal => "modal",
            Self::LocalMove => "local_move",
            Self::RemoteMove => "remote_move",
            Self::NameDraft => "name_draft",
            Self::MaterialDraft => "material_draft",
            Self::ChannelDraft => "channel_draft",
            Self::TextFocus => "text_focus",
            Self::NativeInputPending => "native_input_pending",
            Self::StaleToken => "stale_token",
            Self::StaleLocalMove => "stale_token+local_move",
            Self::StaleRemoteMove => "stale_token+remote_move",
            Self::StaleClosing => "stale_token+closing",
            Self::StaleModal => "stale_token+modal",
            Self::StaleNameDraft => "stale_token+name_draft",
            Self::StaleNativeInput => "stale_token+native_input_pending",
            Self::ReadOnlyAscii => "read_only_ascii",
            Self::BinaryDirty => "binary_dirty",
            Self::SitedBinaryDirty => "sited_binary_dirty",
            Self::NoInstall => "no_install",
            Self::NoInstallModal => "no_install_modal",
        }
    }

    /// The underlying state for the stale-token combinations, which test that the token check
    /// runs before every other remote guard.
    fn stale_over(self) -> Option<State> {
        match self {
            Self::StaleToken => Some(Self::Idle),
            Self::StaleLocalMove => Some(Self::LocalMove),
            Self::StaleRemoteMove => Some(Self::RemoteMove),
            Self::StaleClosing => Some(Self::Closing),
            Self::StaleModal => Some(Self::Modal),
            Self::StaleNameDraft => Some(Self::NameDraft),
            Self::StaleNativeInput => Some(Self::NativeInputPending),
            _ => None,
        }
    }

    fn reachable(self, pipeline: Pipeline) -> bool {
        !(self.stale_over().is_some() && pipeline == Pipeline::Native)
    }
}

/// Mutation applied after the state is built and before the operation runs.
#[derive(Clone, Copy)]
enum Pre {
    None,
    FaceMode,
    VertexMode,
    EdgeMode,
    /// Adds a second node with different channels, reselects the first and enters Face mode.
    SecondNodeFaceMode,
    /// Adds a second node and selects both, the original node as the lead, so the cells start from
    /// an ordered multi-selection.
    MultiSelection,
    /// Makes the Attribute Editor the active dock panel, the only state that accepts a tab.
    AttributeEditorDock,
    /// Creates session material 1 (refused on sourced scenes, where the cell shows that).
    SessionMaterial,
}

struct Sample {
    label: &'static str,
    pre: Pre,
    install_probe: bool,
    /// Restricts the sample to these states; `None` means every reachable state.
    only: Option<&'static [State]>,
    operation: Operation,
}

fn sample(label: &'static str, operation: Operation) -> Sample {
    Sample {
        label,
        pre: Pre::None,
        install_probe: false,
        only: None,
        operation,
    }
}

struct Rig {
    editor: Editor,
    context: egui::Context,
    directory: tempfile::TempDir,
    token: Option<Uuid>,
    may_install: bool,
}

/// Identifiers the sample operations refer to, read from the built editor.
struct Targets {
    node: azteccraft::scene::NodeId,
    path: Vec<NativeId>,
    shape: NativeId,
    /// Second node and its shape when the scene has one; otherwise the first node's.
    other: (azteccraft::scene::NodeId, Vec<NativeId>, NativeId),
    move_id: ManipulationId,
    pending_id: Uuid,
    ascii: std::path::PathBuf,
    save_as: std::path::PathBuf,
}

impl Targets {
    fn transform_target(&self) -> TransformTarget {
        TransformTarget {
            node: self.node,
            path: self.path.clone(),
        }
    }

    fn other_face(&self) -> FaceTarget {
        let (node, path, shape) = self.other.clone();
        let mut path = path;
        path.push(shape);
        FaceTarget {
            transform: node,
            shape,
            path,
            face_index: 0,
        }
    }

    fn vertex(&self) -> VertexTarget {
        let mut path = self.path.clone();
        path.push(self.shape);
        VertexTarget {
            transform: self.node,
            shape: self.shape,
            path,
            vertex_index: 0,
        }
    }

    fn edge(&self) -> EdgeTarget {
        let mut path = self.path.clone();
        path.push(self.shape);
        EdgeTarget {
            transform: self.node,
            shape: self.shape,
            path,
            edge_index: 0,
        }
    }

    fn face(&self) -> FaceTarget {
        let mut path = self.path.clone();
        path.push(self.shape);
        FaceTarget {
            transform: self.node,
            shape: self.shape,
            path,
            face_index: 0,
        }
    }
}

fn targets(rig: &Rig) -> Targets {
    let editor = &rig.editor;
    let scene = editor.history.scene();
    let node = editor.selected().expect("every state selects a node");
    let shape = scene
        .shapes()
        .first()
        .map(|shape| shape.id())
        .or_else(|| scene.curves().first().map(|curve| curve.id()))
        .expect("every state has a shape or curve");
    let other = match (scene.nodes().get(1), scene.shapes().get(1)) {
        (Some(node), Some(shape)) => (node.id(), vec![node.transform_id()], shape.id()),
        _ => (node, editor.selected_path().to_vec(), shape),
    };
    Targets {
        node,
        path: editor.selected_path().to_vec(),
        shape,
        other,
        move_id: editor.history.active_manipulation().map_or_else(
            || ManipulationId::from_decimal("999").unwrap(),
            |active| active.id,
        ),
        pending_id: editor
            .files
            .pending_inspection()
            .map_or(Uuid::nil(), |pending| pending.id.parse().unwrap()),
        ascii: rig.directory.path().join("curve.ma"),
        save_as: rig.directory.path().join("saved.mb"),
    }
}

fn cube_editor() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let id = editor.selected().unwrap();
    editor
        .apply_operation(Operation::Rename {
            id,
            name: "renamed".into(),
        })
        .unwrap();
    editor.apply_operation(Operation::Undo).unwrap();
    editor
}

fn material_editor() -> Editor {
    let scene = azteccraft::native_scene::binary::decode(
        material_fixture::appearance_specimen_with_users(2).into(),
    )
    .unwrap();
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.set_selection_for_test(
        Some(editor.history.scene().nodes()[0].id()),
        vec![editor.history.scene().nodes()[0].transform_id()],
    );
    editor.sync_fields();
    editor
}

/// A dirty, writable Maya binary document: the only fixture whose Save paths reach the installer.
fn binary_editor(directory: &std::path::Path) -> Editor {
    dirty_binary_editor(directory, material_wire::specimen())
}

/// A dirty binary document whose `object` transform has no translate leaf but a recorded site, so
/// the edit that dirtied it makes Save insert the leaf (issue #19).
fn sited_binary_editor(directory: &std::path::Path) -> Editor {
    dirty_binary_editor(
        directory,
        sited_fixture::sited_scene(&material_wire::mesh_payload(&[0, 1, 0x6000_0002])),
    )
}

fn dirty_binary_editor(directory: &std::path::Path, bytes: Vec<u8>) -> Editor {
    let path = directory.join("original.mb");
    std::fs::write(&path, bytes).unwrap();
    let mut editor = Editor::default();
    editor
        .apply_operation(Operation::OpenScene { path })
        .unwrap();
    let node = editor.history.scene().nodes()[0].id();
    let path = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .clone();
    editor
        .apply_operation(Operation::Select {
            node: Some(node),
            instance_path: Some(path),
        })
        .unwrap();
    let mut transform = editor
        .transform_input(editor.history.scene().nodes()[0].transform())
        .unwrap();
    transform.translation[0] += 1.0;
    editor
        .apply_operation(Operation::SetTransform {
            id: node,
            transform,
        })
        .unwrap();
    editor
}

fn pending_confirmation(editor: &mut Editor, operation: Operation) {
    assert!(matches!(
        editor.apply_operation(operation).unwrap(),
        OperationReply::PendingConfirmation { .. }
    ));
}

fn build(state: State) -> Rig {
    if let Some(inner) = state.stale_over() {
        return Rig {
            token: Some(Uuid::nil()),
            ..build(inner)
        };
    }
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("curve.ma"), ascii_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let mut editor = match state {
        State::MaterialDraft => material_editor(),
        _ => cube_editor(),
    };
    let token = None;
    let mut may_install = true;
    match state {
        State::Idle => {}
        State::Closing => {
            pending_confirmation(&mut editor, Operation::RequestClose);
            let pending = editor.files.pending_inspection().unwrap();
            editor
                .apply_operation(Operation::ResolveFileAction {
                    pending_id: pending.id.parse().unwrap(),
                    decision: FileDecision::Discard,
                })
                .unwrap();
            assert!(editor.files.is_closing());
        }
        State::Modal => pending_confirmation(&mut editor, Operation::NewScene),
        State::NoInstallModal => {
            editor = binary_editor(directory.path());
            pending_confirmation(&mut editor, Operation::NewScene);
            may_install = false;
        }
        State::LocalMove | State::RemoteMove => {
            let provenance = if state == State::LocalMove {
                ManipulationProvenance::Native
            } else {
                ManipulationProvenance::Remote
            };
            editor.active_tool = ActiveTool::Move;
            let target = TransformTarget {
                node: editor.selected().unwrap(),
                path: editor.selected_path().to_vec(),
            };
            editor
                .apply_operation_guarded(
                    provenance,
                    Operation::BeginMove {
                        target,
                        axis: WorldAxis::X,
                    },
                    &|| true,
                )
                .unwrap();
            assert!(editor.history.active_manipulation().is_some());
        }
        State::NameDraft => editor.name = "draft name".into(),
        State::MaterialDraft => {
            editor.material_draft.as_mut().unwrap().text = "0.5".into();
            assert!(editor.has_draft());
        }
        State::ChannelDraft => {
            editor.channels[0] = "5".into();
            assert!(editor.has_channel_draft());
        }
        State::TextFocus => {
            context.memory_mut(|memory| memory.request_focus(name_edit_id()));
        }
        State::NativeInputPending => editor.native_input_pending = true,
        State::StaleToken
        | State::StaleLocalMove
        | State::StaleRemoteMove
        | State::StaleClosing
        | State::StaleModal
        | State::StaleNameDraft
        | State::StaleNativeInput => unreachable!("built from the underlying state"),
        State::ReadOnlyAscii => {
            editor = Editor::default();
            editor
                .apply_operation(Operation::OpenScene {
                    path: directory.path().join("curve.ma"),
                })
                .unwrap();
            let node = editor.history.scene().nodes()[0].id();
            let path = editor.history.scene().transform_paths().unwrap()[0]
                .path
                .clone();
            editor
                .apply_operation(Operation::Select {
                    node: Some(node),
                    instance_path: Some(path),
                })
                .unwrap();
            assert!(
                editor
                    .history
                    .scene()
                    .authoring_read_only_reason()
                    .is_some()
            );
        }
        State::BinaryDirty => editor = binary_editor(directory.path()),
        State::SitedBinaryDirty => editor = sited_binary_editor(directory.path()),
        State::NoInstall => {
            editor = binary_editor(directory.path());
            may_install = false;
        }
    }
    Rig {
        editor,
        context,
        directory,
        token,
        may_install,
    }
}

/// One entry per operation kind. `Operation` is not `Clone`, so each cell rebuilds its samples.
fn seeds(t: &Targets) -> Vec<Operation> {
    vec![
        Operation::SetActiveTool(ActiveTool::Move),
        Operation::SetToolSpace {
            tool: ActiveTool::Scale,
            space: ManipulationSpace::Object,
        },
        Operation::SetToolHandle(ToolHandle::RotateAxis(RotateAxis::Y)),
        Operation::SetManipulatorSize(ManipulatorSize::new(2.0).unwrap()),
        Operation::BeginMove {
            target: t.transform_target(),
            axis: WorldAxis::X,
        },
        Operation::UpdateMove {
            id: t.move_id,
            distance: MoveDistance::Centimeters(1.0),
        },
        Operation::BeginRotate {
            target: t.transform_target(),
            axis: RotateAxis::Y,
            space: ManipulationSpace::Object,
        },
        Operation::BeginScale {
            target: t.transform_target(),
            handle: ScaleHandle::X,
            space: ManipulationSpace::Object,
        },
        Operation::UpdateManipulation {
            id: t.move_id,
            value: ManipulationValue::Degrees(1.0),
        },
        Operation::FinishMove {
            id: t.move_id,
            finish: ManipulationFinish::Commit,
        },
        Operation::SetMaterialDiffuse {
            shape: t.shape,
            value: 0.25,
        },
        Operation::CreateSessionMaterial {
            color: [1.0, 0.0, 0.0],
        },
        Operation::AssignSessionMaterial {
            shape: t.shape,
            material: MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        },
        Operation::SetSessionMaterialColor {
            material: SessionMaterialId::from_index(1),
            color: [0.0, 1.0, 0.0],
        },
        Operation::Inspect,
        Operation::Create {
            primitive: Primitive::Plane,
            options: None,
            name: None,
        },
        Operation::Select {
            node: Some(t.node),
            instance_path: Some(t.path.clone()),
        },
        Operation::ModifySelection {
            change: SelectionChange::Add,
            node: t.node,
            instance_path: Some(t.path.clone()),
        },
        Operation::SelectByRule(SelectRule::All),
        Operation::SetSelectionMode(SelectionMode::Face),
        Operation::DeselectAll,
        Operation::SelectFace {
            target: Some(t.face()),
        },
        Operation::SelectVertex {
            target: Some(t.vertex()),
        },
        Operation::SelectEdge {
            target: Some(t.edge()),
        },
        Operation::NewScene,
        Operation::RequestClose,
        Operation::OpenScene {
            path: t.ascii.clone(),
        },
        Operation::SaveScene,
        Operation::SaveSceneAs {
            path: t.save_as.clone(),
        },
        Operation::ResolveFileAction {
            pending_id: t.pending_id,
            decision: FileDecision::Cancel,
        },
        Operation::Rename {
            id: t.node,
            name: "renamed".into(),
        },
        Operation::SetTransform {
            id: t.node,
            transform: TransformInput {
                translation: [1.0, 2.0, 3.0],
                rotation_degrees: [0.0; 3],
                scale: [1.0; 3],
            },
        },
        Operation::EditNode {
            id: t.node,
            name: None,
            translation: [Some(1.0), None, None],
            rotation_degrees: [None; 3],
            scale: [None; 3],
        },
        Operation::ExtrudeFace {
            target: t.face(),
            distance: 0.5,
        },
        Operation::Delete(t.node),
        Operation::DeleteSelection,
        Operation::Undo,
        Operation::Redo,
        Operation::FrameSelected {
            with_children: false,
        },
        Operation::FrameAll,
        Operation::SetGridVisible { visible: false },
        Operation::SetShadingMode(crate::editor_operations::ShadingMode::Wireframe),
        Operation::SetSidebarPanelVisible {
            panel: crate::editor_operations::SidebarPanel::ChannelBox,
            visible: false,
        },
        Operation::SelectAttributeEditorTab {
            tab: crate::editor_operations::AttributeEditorTab::Shape(t.shape),
        },
        Operation::Orbit {
            radians: [0.1, 0.0],
        },
        Operation::Pan {
            world_units: [1.0, 0.0],
        },
        Operation::Dolly { factor: 1.1 },
        // Differs from the rig's default camera so a Changed outcome is observable.
        Operation::SetCameraPose {
            target: [1.0, 0.5, -2.0],
            distance: 5.0,
            yaw: 0.3,
            pitch: 0.2,
        },
    ]
}

/// Samples for one operation kind. No wildcard arm: a new `Operation` variant must be added here.
fn expand(seed: Operation, t: &Targets) -> Vec<Sample> {
    match seed {
        Operation::SetActiveTool(_) => vec![
            sample(
                "set_active_tool(move)",
                Operation::SetActiveTool(ActiveTool::Move),
            ),
            sample(
                "set_active_tool(rotate)",
                Operation::SetActiveTool(ActiveTool::Rotate),
            ),
            sample(
                "set_active_tool(scale)",
                Operation::SetActiveTool(ActiveTool::Scale),
            ),
            sample(
                "set_active_tool(select)",
                Operation::SetActiveTool(ActiveTool::Select),
            ),
        ],
        Operation::SetToolSpace { .. } => vec![
            sample(
                "set_tool_space(rotate,world)",
                Operation::SetToolSpace {
                    tool: ActiveTool::Rotate,
                    space: ManipulationSpace::World,
                },
            ),
            sample(
                "set_tool_space(scale,object)",
                Operation::SetToolSpace {
                    tool: ActiveTool::Scale,
                    space: ManipulationSpace::Object,
                },
            ),
            sample(
                "set_tool_space(move,world)",
                Operation::SetToolSpace {
                    tool: ActiveTool::Move,
                    space: ManipulationSpace::World,
                },
            ),
        ],
        Operation::SetToolHandle(_) => vec![
            sample(
                "set_tool_handle(rotate,y)",
                Operation::SetToolHandle(ToolHandle::RotateAxis(RotateAxis::Y)),
            ),
            sample(
                "set_tool_handle(scale,uniform)",
                Operation::SetToolHandle(ToolHandle::ScaleHandle(ScaleHandle::Uniform)),
            ),
            sample(
                "set_tool_handle(scale,x)",
                Operation::SetToolHandle(ToolHandle::ScaleHandle(ScaleHandle::X)),
            ),
        ],
        Operation::SetManipulatorSize(_) => vec![
            sample(
                "set_manipulator_size(2)",
                Operation::SetManipulatorSize(ManipulatorSize::new(2.0).unwrap()),
            ),
            sample(
                "set_manipulator_size(1)",
                Operation::SetManipulatorSize(ManipulatorSize::default()),
            ),
        ],
        Operation::BeginMove { .. } => vec![
            sample(
                "begin_move",
                Operation::BeginMove {
                    target: t.transform_target(),
                    axis: WorldAxis::X,
                },
            ),
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "begin_move(face_mode)",
                    Operation::BeginMove {
                        target: t.transform_target(),
                        axis: WorldAxis::X,
                    },
                )
            },
        ],
        Operation::UpdateMove { .. } => vec![sample(
            "update_move",
            Operation::UpdateMove {
                id: t.move_id,
                distance: MoveDistance::Centimeters(1.0),
            },
        )],
        Operation::BeginRotate { .. } => vec![
            sample(
                "begin_rotate(object)",
                Operation::BeginRotate {
                    target: t.transform_target(),
                    axis: RotateAxis::Y,
                    space: ManipulationSpace::Object,
                },
            ),
            sample(
                "begin_rotate(world)",
                Operation::BeginRotate {
                    target: t.transform_target(),
                    axis: RotateAxis::Y,
                    space: ManipulationSpace::World,
                },
            ),
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "begin_rotate(face_mode)",
                    Operation::BeginRotate {
                        target: t.transform_target(),
                        axis: RotateAxis::Y,
                        space: ManipulationSpace::Object,
                    },
                )
            },
        ],
        Operation::BeginScale { .. } => vec![
            sample(
                "begin_scale(object)",
                Operation::BeginScale {
                    target: t.transform_target(),
                    handle: ScaleHandle::X,
                    space: ManipulationSpace::Object,
                },
            ),
            sample(
                "begin_scale(world)",
                Operation::BeginScale {
                    target: t.transform_target(),
                    handle: ScaleHandle::Uniform,
                    space: ManipulationSpace::World,
                },
            ),
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "begin_scale(face_mode)",
                    Operation::BeginScale {
                        target: t.transform_target(),
                        handle: ScaleHandle::X,
                        space: ManipulationSpace::Object,
                    },
                )
            },
        ],
        Operation::UpdateManipulation { .. } => vec![
            sample(
                "update_manipulation(degrees)",
                Operation::UpdateManipulation {
                    id: t.move_id,
                    value: ManipulationValue::Degrees(1.0),
                },
            ),
            sample(
                "update_manipulation(factor)",
                Operation::UpdateManipulation {
                    id: t.move_id,
                    value: ManipulationValue::Factor(2.0),
                },
            ),
        ],
        Operation::FinishMove { .. } => vec![sample(
            "finish_move(commit)",
            Operation::FinishMove {
                id: t.move_id,
                finish: ManipulationFinish::Commit,
            },
        )],
        Operation::SetMaterialDiffuse { .. } => vec![sample(
            "set_material_diffuse",
            Operation::SetMaterialDiffuse {
                shape: t.shape,
                value: 0.25,
            },
        )],
        Operation::CreateSessionMaterial { .. } => vec![sample(
            "create_session_material",
            Operation::CreateSessionMaterial {
                color: [1.0, 0.0, 0.0],
            },
        )],
        Operation::AssignSessionMaterial { .. } => vec![
            sample(
                "assign_session_material(new)",
                Operation::AssignSessionMaterial {
                    shape: t.shape,
                    material: MaterialChoice::New {
                        color: [1.0, 0.0, 0.0],
                    },
                },
            ),
            sample(
                "assign_session_material(initial)",
                Operation::AssignSessionMaterial {
                    shape: t.shape,
                    material: MaterialChoice::Existing(SessionMaterialId::from_index(0)),
                },
            ),
        ],
        Operation::SetSessionMaterialColor { .. } => vec![
            sample(
                "set_session_material_color(unknown)",
                Operation::SetSessionMaterialColor {
                    material: SessionMaterialId::from_index(1),
                    color: [0.0, 1.0, 0.0],
                },
            ),
            Sample {
                pre: Pre::SessionMaterial,
                ..sample(
                    "set_session_material_color(created)",
                    Operation::SetSessionMaterialColor {
                        material: SessionMaterialId::from_index(1),
                        color: [0.0, 1.0, 0.0],
                    },
                )
            },
        ],
        Operation::Inspect => vec![sample("inspect", Operation::Inspect)],
        Operation::Create { .. } => vec![
            sample(
                "create(plane)",
                Operation::Create {
                    primitive: Primitive::Plane,
                    options: None,
                    name: None,
                },
            ),
            sample(
                "create(sphere)",
                Operation::Create {
                    primitive: Primitive::Sphere,
                    options: None,
                    name: None,
                },
            ),
            sample(
                "create(cylinder)",
                Operation::Create {
                    primitive: Primitive::Cylinder,
                    options: None,
                    name: None,
                },
            ),
            sample(
                "create(cone)",
                Operation::Create {
                    primitive: Primitive::Cone,
                    options: None,
                    name: None,
                },
            ),
            sample(
                "create(cube,options)",
                Operation::Create {
                    primitive: Primitive::Cube,
                    options: Some(PrimitiveOptions::Cube {
                        width: 2.0,
                        height: 3.0,
                        depth: 4.0,
                        sub_width: 2,
                        sub_height: 2,
                        sub_depth: 2,
                        axis: Axis::Z,
                    }),
                    name: None,
                },
            ),
            sample(
                "create(sphere,invalid options)",
                Operation::Create {
                    primitive: Primitive::Sphere,
                    options: Some(PrimitiveOptions::Sphere {
                        radius: -1.0,
                        sub_axis: 20,
                        sub_height: 20,
                        axis: Axis::Y,
                    }),
                    name: None,
                },
            ),
            sample(
                "create(cone,mismatched options)",
                Operation::Create {
                    primitive: Primitive::Cone,
                    options: Some(PrimitiveOptions::default_for(Primitive::Cube)),
                    name: None,
                },
            ),
        ],
        Operation::Select { .. } => vec![
            sample(
                "select(node)",
                Operation::Select {
                    node: Some(t.node),
                    instance_path: Some(t.path.clone()),
                },
            ),
            sample(
                "select(none)",
                Operation::Select {
                    node: None,
                    instance_path: None,
                },
            ),
            sample(
                "select(none_with_path)",
                Operation::Select {
                    node: None,
                    instance_path: Some(t.path.clone()),
                },
            ),
        ],
        Operation::ModifySelection { .. } => {
            let modify = |change, node, path: &[NativeId]| Operation::ModifySelection {
                change,
                node,
                instance_path: Some(path.to_vec()),
            };
            let (other, other_path, _) = t.other.clone();
            // With Pre::MultiSelection `t.node` is the lead and `other` the second node.
            vec![
                sample(
                    "modify_selection(add)",
                    modify(SelectionChange::Add, t.node, &t.path),
                ),
                sample(
                    "modify_selection(toggle)",
                    modify(SelectionChange::Toggle, t.node, &t.path),
                ),
                sample(
                    "modify_selection(remove)",
                    modify(SelectionChange::Remove, t.node, &t.path),
                ),
                sample(
                    "modify_selection(unknown_path)",
                    modify(SelectionChange::Add, t.node, &[t.shape]),
                ),
                Sample {
                    pre: Pre::MultiSelection,
                    only: Some(&[State::Idle]),
                    ..sample(
                        "modify_selection(toggle_non_lead)",
                        modify(SelectionChange::Toggle, other, &other_path),
                    )
                },
                Sample {
                    pre: Pre::MultiSelection,
                    only: Some(&[State::Idle]),
                    ..sample(
                        "modify_selection(remove_lead)",
                        modify(SelectionChange::Remove, t.node, &t.path),
                    )
                },
                Sample {
                    pre: Pre::MultiSelection,
                    only: Some(&[State::Idle]),
                    ..sample(
                        "modify_selection(add_non_lead)",
                        modify(SelectionChange::Add, other, &other_path),
                    )
                },
            ]
        }
        Operation::SelectByRule(_) => vec![
            sample("select_all", Operation::SelectByRule(SelectRule::All)),
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "select_all(face_mode)",
                    Operation::SelectByRule(SelectRule::All),
                )
            },
            sample(
                "select_hierarchy",
                Operation::SelectByRule(SelectRule::Hierarchy),
            ),
            sample(
                "select_inverse",
                Operation::SelectByRule(SelectRule::Inverse),
            ),
            Sample {
                pre: Pre::MultiSelection,
                only: Some(&[State::Idle]),
                ..sample(
                    "select_inverse(multi_selection)",
                    Operation::SelectByRule(SelectRule::Inverse),
                )
            },
        ],
        Operation::SetSelectionMode(_) => vec![sample(
            "set_selection_mode(face)",
            Operation::SetSelectionMode(SelectionMode::Face),
        )],
        Operation::DeselectAll => vec![
            sample("deselect_all", Operation::DeselectAll),
            Sample {
                pre: Pre::FaceMode,
                ..sample("deselect_all(face_mode)", Operation::DeselectAll)
            },
        ],
        Operation::SelectFace { .. } => vec![
            sample(
                "select_face(object_mode)",
                Operation::SelectFace {
                    target: Some(t.face()),
                },
            ),
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "select_face(face_mode)",
                    Operation::SelectFace {
                        target: Some(t.face()),
                    },
                )
            },
            Sample {
                pre: Pre::SecondNodeFaceMode,
                only: Some(&[State::Idle]),
                ..sample(
                    "select_face(second_node)",
                    Operation::SelectFace {
                        target: Some(t.other_face()),
                    },
                )
            },
        ],
        Operation::SelectVertex { .. } => vec![
            sample(
                "select_vertex(object_mode)",
                Operation::SelectVertex {
                    target: Some(t.vertex()),
                },
            ),
            Sample {
                pre: Pre::VertexMode,
                ..sample(
                    "select_vertex(vertex_mode)",
                    Operation::SelectVertex {
                        target: Some(t.vertex()),
                    },
                )
            },
            Sample {
                pre: Pre::VertexMode,
                ..sample(
                    "select_vertex(vertex_mode_clear)",
                    Operation::SelectVertex { target: None },
                )
            },
            Sample {
                pre: Pre::FaceMode,
                ..sample(
                    "select_vertex(face_mode)",
                    Operation::SelectVertex {
                        target: Some(t.vertex()),
                    },
                )
            },
        ],
        Operation::SelectEdge { .. } => vec![
            sample(
                "select_edge(object_mode)",
                Operation::SelectEdge {
                    target: Some(t.edge()),
                },
            ),
            Sample {
                pre: Pre::EdgeMode,
                ..sample(
                    "select_edge(edge_mode)",
                    Operation::SelectEdge {
                        target: Some(t.edge()),
                    },
                )
            },
            Sample {
                pre: Pre::EdgeMode,
                ..sample(
                    "select_edge(edge_mode_clear)",
                    Operation::SelectEdge { target: None },
                )
            },
            Sample {
                pre: Pre::VertexMode,
                ..sample(
                    "select_edge(vertex_mode)",
                    Operation::SelectEdge {
                        target: Some(t.edge()),
                    },
                )
            },
        ],
        Operation::NewScene => vec![sample("new_scene", Operation::NewScene)],
        Operation::RequestClose => vec![sample("request_close", Operation::RequestClose)],
        Operation::OpenScene { .. } => vec![
            sample(
                "open_scene(ascii_curve)",
                Operation::OpenScene {
                    path: t.ascii.clone(),
                },
            ),
            sample(
                "open_scene(empty_path)",
                Operation::OpenScene {
                    path: std::path::PathBuf::new(),
                },
            ),
        ],
        Operation::SaveScene => vec![Sample {
            install_probe: true,
            ..sample("save_scene", Operation::SaveScene)
        }],
        Operation::SaveSceneAs { .. } => vec![Sample {
            install_probe: true,
            ..sample(
                "save_scene_as",
                Operation::SaveSceneAs {
                    path: t.save_as.clone(),
                },
            )
        }],
        Operation::ResolveFileAction { .. } => vec![
            sample(
                "resolve_file_action(cancel)",
                Operation::ResolveFileAction {
                    pending_id: t.pending_id,
                    decision: FileDecision::Cancel,
                },
            ),
            sample(
                "resolve_file_action(discard)",
                Operation::ResolveFileAction {
                    pending_id: t.pending_id,
                    decision: FileDecision::Discard,
                },
            ),
            Sample {
                install_probe: true,
                ..sample(
                    "resolve_file_action(save)",
                    Operation::ResolveFileAction {
                        pending_id: t.pending_id,
                        decision: FileDecision::Save,
                    },
                )
            },
        ],
        Operation::Rename { .. } => vec![sample(
            "rename",
            Operation::Rename {
                id: t.node,
                name: "renamed".into(),
            },
        )],
        Operation::SetTransform { .. } => vec![sample(
            "set_transform",
            Operation::SetTransform {
                id: t.node,
                transform: TransformInput {
                    translation: [1.0, 2.0, 3.0],
                    rotation_degrees: [0.0; 3],
                    scale: [1.0; 3],
                },
            },
        )],
        Operation::EditNode { .. } => vec![
            sample(
                "edit_node(translate_x)",
                Operation::EditNode {
                    id: t.node,
                    name: None,
                    translation: [Some(1.0), None, None],
                    rotation_degrees: [None; 3],
                    scale: [None; 3],
                },
            ),
            sample(
                "edit_node(empty_unknown_node)",
                Operation::EditNode {
                    id: azteccraft::scene::NodeId::from_decimal("999999").unwrap(),
                    name: None,
                    translation: [None; 3],
                    rotation_degrees: [None; 3],
                    scale: [None; 3],
                },
            ),
            sample(
                "edit_node(empty)",
                Operation::EditNode {
                    id: t.node,
                    name: None,
                    translation: [None; 3],
                    rotation_degrees: [None; 3],
                    scale: [None; 3],
                },
            ),
        ],
        Operation::ExtrudeFace { .. } => vec![sample(
            "extrude_face",
            Operation::ExtrudeFace {
                target: t.face(),
                distance: 0.5,
            },
        )],
        Operation::Delete(_) => vec![
            sample("delete", Operation::Delete(t.node)),
            Sample {
                pre: Pre::FaceMode,
                ..sample("delete(face_mode)", Operation::Delete(t.node))
            },
            Sample {
                pre: Pre::VertexMode,
                ..sample("delete(vertex_mode)", Operation::Delete(t.node))
            },
            Sample {
                pre: Pre::EdgeMode,
                ..sample("delete(edge_mode)", Operation::Delete(t.node))
            },
        ],
        Operation::DeleteSelection => vec![
            sample("delete_selection", Operation::DeleteSelection),
            Sample {
                pre: Pre::FaceMode,
                ..sample("delete_selection(face_mode)", Operation::DeleteSelection)
            },
            Sample {
                pre: Pre::VertexMode,
                ..sample("delete_selection(vertex_mode)", Operation::DeleteSelection)
            },
            Sample {
                pre: Pre::EdgeMode,
                ..sample("delete_selection(edge_mode)", Operation::DeleteSelection)
            },
            Sample {
                pre: Pre::MultiSelection,
                only: Some(&[State::Idle]),
                ..sample(
                    "delete_selection(multi_selection)",
                    Operation::DeleteSelection,
                )
            },
        ],
        Operation::Undo => vec![sample("undo", Operation::Undo)],
        Operation::Redo => vec![sample("redo", Operation::Redo)],
        Operation::FrameSelected { .. } => vec![
            sample(
                "frame_selected(parents)",
                Operation::FrameSelected {
                    with_children: false,
                },
            ),
            sample(
                "frame_selected(children)",
                Operation::FrameSelected {
                    with_children: true,
                },
            ),
        ],
        Operation::FrameAll => vec![sample("frame_all", Operation::FrameAll)],
        Operation::SetGridVisible { .. } => vec![sample(
            "set_grid_visible(false)",
            Operation::SetGridVisible { visible: false },
        )],
        Operation::SetShadingMode(_) => vec![sample(
            "set_shading_mode(wireframe)",
            Operation::SetShadingMode(crate::editor_operations::ShadingMode::Wireframe),
        )],
        Operation::SetSidebarPanelVisible { .. } => vec![
            sample(
                "set_sidebar_panel_visible(channel_box,false)",
                Operation::SetSidebarPanelVisible {
                    panel: crate::editor_operations::SidebarPanel::ChannelBox,
                    visible: false,
                },
            ),
            sample(
                "set_sidebar_panel_visible(attribute_editor,true)",
                Operation::SetSidebarPanelVisible {
                    panel: crate::editor_operations::SidebarPanel::AttributeEditor,
                    visible: true,
                },
            ),
        ],
        Operation::SelectAttributeEditorTab { .. } => vec![
            Sample {
                pre: Pre::AttributeEditorDock,
                ..sample(
                    "select_attribute_editor_tab(shape)",
                    Operation::SelectAttributeEditorTab {
                        tab: crate::editor_operations::AttributeEditorTab::Shape(t.shape),
                    },
                )
            },
            sample(
                "select_attribute_editor_tab(shape,panel_hidden)",
                Operation::SelectAttributeEditorTab {
                    tab: crate::editor_operations::AttributeEditorTab::Shape(t.shape),
                },
            ),
        ],
        Operation::Orbit { .. } => vec![sample(
            "orbit",
            Operation::Orbit {
                radians: [0.1, 0.0],
            },
        )],
        Operation::Pan { .. } => vec![sample(
            "pan",
            Operation::Pan {
                world_units: [1.0, 0.0],
            },
        )],
        Operation::Dolly { .. } => vec![sample("dolly", Operation::Dolly { factor: 1.1 })],
        Operation::SetCameraPose { .. } => vec![sample(
            "set_camera_pose",
            Operation::SetCameraPose {
                target: [1.0, 0.5, -2.0],
                distance: 5.0,
                yaw: 0.3,
                pitch: 0.2,
            },
        )],
    }
}

fn samples(rig: &Rig) -> Vec<Sample> {
    let t = targets(rig);
    seeds(&t)
        .into_iter()
        .flat_map(|seed| expand(seed, &t))
        .collect()
}

struct Observed {
    scene: NativeScene,
    fields: Vec<(&'static str, String)>,
}

fn observe(editor: &Editor, roots: &[&std::path::Path]) -> Observed {
    let scene = editor.history.scene();
    let selected = editor
        .selected()
        .and_then(|id| scene.node(id).ok())
        .map_or("-", |node| node.name());
    let active_manipulation = match editor.history.active_manipulation() {
        None => "none",
        Some(active) if active.provenance == ManipulationProvenance::Native => "native",
        Some(_) => "remote",
    };
    let pending = editor.files.pending_inspection();
    let material = editor.material_draft.as_ref().map_or_else(
        || "-".to_owned(),
        |draft| format!("text={} intent={}", draft.text, draft.intent),
    );
    let flags = |values: &[bool; 9]| {
        values
            .iter()
            .map(|v| if *v { '1' } else { '0' })
            .collect::<String>()
    };
    let fields = vec![
        ("token", editor.scene_token.to_string()),
        ("nodes", scene.nodes().len().to_string()),
        ("selected", selected.to_owned()),
        (
            "selected_path_len",
            editor.selected_path().len().to_string(),
        ),
        // Members before the lead, in order, so a cell that reorders or drops a member shows it.
        ("selection_rest", {
            let rest = editor
                .selection
                .members()
                .split_last()
                .map_or(&[][..], |(_, rest)| rest)
                .iter()
                .map(|member| {
                    let name = scene.node(member.node).map_or("?", |node| node.name());
                    format!("{name}@{}", member.path.len())
                })
                .collect::<Vec<_>>()
                .join(">");
            if rest.is_empty() {
                "-".to_owned()
            } else {
                rest
            }
        }),
        ("selection_mode", format!("{:?}", editor.selection_mode)),
        (
            "face_remembered",
            editor.remembered_face.is_some().to_string(),
        ),
        (
            "vertex_remembered",
            editor.remembered_vertex.is_some().to_string(),
        ),
        (
            "edge_remembered",
            editor.remembered_edge.is_some().to_string(),
        ),
        ("active_tool", format!("{:?}", editor.active_tool)),
        ("grid", editor.grid_visible.to_string()),
        ("shading", format!("{:?}", editor.shading)),
        (
            "sidebar_panels",
            format!(
                "channel_box={}",
                editor.sidebar_panel_visible(crate::editor_operations::SidebarPanel::ChannelBox)
            ),
        ),
        (
            "attribute_editor",
            format!(
                "panel={} tab={:?}",
                editor
                    .sidebar_panel_visible(crate::editor_operations::SidebarPanel::AttributeEditor),
                editor.attribute_editor_tab
            ),
        ),
        ("camera", format!("{:?}", editor.camera)),
        (
            "status",
            format!(
                "{:?}",
                super::snapshots::normalize_text(&editor.status, roots)
            ),
        ),
        ("status_is_error", editor.error.to_string()),
        ("active_move", active_manipulation.to_owned()),
        ("native_move_drag", editor.native_move.is_some().to_string()),
        (
            "pointer_consumed_frame",
            editor.move_consumed_frame.to_string(),
        ),
        (
            "pointer_suppress_primary",
            editor.move_suppress_primary.to_string(),
        ),
        ("has_draft", editor.has_draft().to_string()),
        ("has_channel_draft", editor.has_channel_draft().to_string()),
        ("name_intent", editor.name_intent.to_string()),
        ("material_draft", material),
        ("name_field", format!("{:?}", editor.name)),
        ("channel_fields", editor.channels.join(",")),
        ("channel_intents", flags(&editor.channel_intents)),
        (
            "session_materials",
            format!(
                "{:?}",
                scene
                    .session_materials()
                    .iter()
                    .skip(1)
                    .map(|material| material.color())
                    .collect::<Vec<_>>()
            ),
        ),
        ("modal", editor.files.is_modal().to_string()),
        ("closing", editor.files.is_closing().to_string()),
        (
            "pending_action",
            pending.map_or("-".to_owned(), |pending| format!("{:?}", pending.kind)),
        ),
        ("dirty", editor.is_dirty().to_string()),
        ("can_undo", editor.history.can_undo().to_string()),
        ("can_redo", editor.history.can_redo().to_string()),
        (
            "tool_settings",
            format!(
                "rotate_space={:?} scale_space={:?} rotate_axis={:?} scale_handle={:?}",
                editor.manipulation.space(ActiveTool::Rotate),
                editor.manipulation.space(ActiveTool::Scale),
                editor.manipulation.rotate_axis(),
                editor.manipulation.scale_handle()
            ),
        ),
        (
            "manipulator_size",
            editor.manipulation.manipulator_size().get().to_string(),
        ),
    ];
    Observed {
        scene: scene.clone(),
        fields,
    }
}

fn describe(result: &Result<OperationReply, OperationError>, roots: &[&std::path::Path]) -> String {
    match result {
        Ok(reply) => format!(
            "Ok({})",
            serde_json::to_value(reply).unwrap()["outcome"]
                .as_str()
                .unwrap()
        ),
        Err(error) => {
            let debug = format!("{error:?}");
            let variant: String = debug
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            format!(
                "Err({variant}: {})",
                super::snapshots::normalize_text(&error.to_string(), roots)
            )
        }
    }
}

fn add_second_node(editor: &mut Editor) {
    let first = editor.selected().unwrap();
    let first_path = editor.selected_path().to_vec();
    editor
        .apply_operation(Operation::Create {
            primitive: Primitive::Plane,
            options: None,
            name: None,
        })
        .unwrap();
    let plane = editor.selected().unwrap();
    editor
        .apply_operation(Operation::SetTransform {
            id: plane,
            transform: TransformInput {
                translation: [4.0, 5.0, 6.0],
                rotation_degrees: [0.0; 3],
                scale: [2.0; 3],
            },
        })
        .unwrap();
    editor
        .apply_operation(Operation::Select {
            node: Some(first),
            instance_path: Some(first_path),
        })
        .unwrap();
}

fn run_cell(state: State, pipeline: Pipeline, index: usize) -> (String, String) {
    let mut rig = build(state);
    let roots = [rig.directory.path()];
    let roots: Vec<std::path::PathBuf> = roots.iter().map(|path| path.to_path_buf()).collect();
    let root_refs: Vec<&std::path::Path> = roots.iter().map(|path| path.as_path()).collect();
    let pre = samples(&rig).swap_remove(index).pre;
    match pre {
        Pre::None => {}
        Pre::FaceMode => rig.editor.selection_mode = SelectionMode::Face,
        Pre::SessionMaterial => {
            let _ = rig.editor.history.execute(Command::CreateSessionMaterial {
                color: [1.0, 0.0, 0.0],
            });
        }
        Pre::VertexMode => rig.editor.selection_mode = SelectionMode::Vertex,
        Pre::EdgeMode => rig.editor.selection_mode = SelectionMode::Edge,
        Pre::AttributeEditorDock => {
            rig.editor.sidebar_dock = Some(crate::editor_operations::SidebarPanel::AttributeEditor)
        }
        Pre::SecondNodeFaceMode => {
            add_second_node(&mut rig.editor);
            rig.editor.selection_mode = SelectionMode::Face;
        }
        Pre::MultiSelection => {
            let first = rig.editor.selection.lead().cloned().unwrap();
            add_second_node(&mut rig.editor);
            let plane = rig
                .editor
                .history
                .scene()
                .transform_paths()
                .unwrap()
                .into_iter()
                .find(|target| target.node != first.node)
                .unwrap();
            rig.editor
                .set_selection_members_for_test(vec![plane, first]);
            rig.editor.sync_fields();
        }
    }
    // Rebuilt after the pre-step so targets can name nodes it created.
    let mut chosen = samples(&rig).swap_remove(index);
    let label = chosen.label;
    let before = observe(&rig.editor, &root_refs);
    let may_install = rig.may_install;
    let operation = std::mem::replace(&mut chosen.operation, Operation::Inspect);
    let result = match pipeline {
        Pipeline::Native => rig.editor.apply_operation_guarded(
            ManipulationProvenance::Native,
            operation,
            &move || may_install,
        ),
        Pipeline::Remote => {
            let expected = rig.token.unwrap_or(rig.editor.scene_token);
            rig.editor.remote_operation_guarded(
                RemoteRequest {
                    expected_scene_token: Some(expected),
                    operation,
                },
                &rig.context,
                &move || may_install,
            )
        }
    };
    let after = observe(&rig.editor, &root_refs);
    let mut effects = Vec::new();
    if before.scene != after.scene {
        effects.push("scene changed".to_owned());
    }
    for ((name, left), (_, right)) in before.fields.iter().zip(&after.fields) {
        if left != right {
            if *name == "token" {
                effects.push("token replaced".into());
            } else if *name == "camera" {
                effects.push("camera changed".into());
            } else {
                effects.push(format!("{name}: {left} -> {right}"));
            }
        }
    }
    let effects = if effects.is_empty() {
        "no state change".to_owned()
    } else {
        effects.join("; ")
    };
    (
        label.to_owned(),
        format!("{} | {effects}", describe(&result, &root_refs)),
    )
}

fn baseline_text(state: State) -> String {
    let rig = build(state);
    let roots = [rig.directory.path()];
    let observed = observe(&rig.editor, &roots);
    let mut lines = vec![String::new()];
    for (name, value) in observed
        .fields
        .iter()
        .filter(|(name, _)| !matches!(*name, "token" | "camera" | "selection_rest"))
    {
        let item = format!("{name}={value}");
        let line = lines.last_mut().unwrap();
        if line.len() + item.len() > 100 {
            lines.push(item);
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&item);
        }
    }
    lines
        .iter()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `file_writing` selects the cells that run the native writer (Save, Save As, Resolve(save)),
/// which is supported on Unix and Windows; every other cell is checked on all platforms.
fn matrix(file_writing: bool) -> (String, usize) {
    let mut text = String::new();
    text.push_str(
        &"# Guard matrix (generated by guard_matrix_tests.rs; regenerate with UPDATE_SNAPSHOTS=1)\n\
         # section: {}\n\
         # cell: operation => result | effects relative to the state's baseline\n\
         # native = apply_operation_guarded(Native), remote = remote_operation_guarded\n"
            .replace(
                "{}",
                if file_writing {
                    "cells that write files (Unix and Windows)"
                } else {
                    "all other cells (every platform)"
                },
            ),
    );
    let mut cells = 0;
    for state in STATES {
        for pipeline in [Pipeline::Native, Pipeline::Remote] {
            if !state.reachable(pipeline) {
                continue;
            }
            let rig = build(state);
            let all = samples(&rig);
            let selected: Vec<usize> = all
                .iter()
                .enumerate()
                .filter(|(_, sample)| {
                    sample.install_probe == file_writing
                        && sample.only.is_none_or(|only| only.contains(&state))
                })
                .map(|(index, _)| index)
                .collect();
            drop(rig);
            if selected.is_empty() {
                continue;
            }
            write!(
                text,
                "\n## {} / {}\n  baseline:\n{}\n",
                state.name(),
                pipeline.name(),
                baseline_text(state)
            )
            .unwrap();
            for index in selected {
                let (label, cell) = run_cell(state, pipeline, index);
                writeln!(text, "  {label} => {cell}").unwrap();
                cells += 1;
            }
        }
    }
    writeln!(text, "\ncells: {cells}").unwrap();
    (text, cells)
}

#[test]
fn guard_matrix_matches_committed_snapshot() {
    let (text, cells) = matrix(false);
    assert!(cells > 500, "matrix unexpectedly small: {cells}");
    super::snapshots::assert_snapshot("guard_matrix.txt", &text);
}

#[test]
fn guard_matrix_file_writing_cells_match_committed_snapshot() {
    // Save cells depend on the native writer, which is supported on Unix and Windows.
    if !cfg!(any(unix, windows)) {
        return;
    }
    let (text, cells) = matrix(true);
    assert!(
        cells > 10,
        "file-writing matrix unexpectedly small: {cells}"
    );
    super::snapshots::assert_snapshot("guard_matrix_file_writing.txt", &text);
}

#[test]
fn guard_matrix_is_deterministic() {
    for file_writing in [false, true] {
        assert_eq!(matrix(file_writing).0, matrix(file_writing).0);
    }
}

/// Native UI actions go through `native_operation`, which maps the shared pipeline's Face-mode
/// Delete refusal to the status line.
#[test]
fn native_operation_reports_face_mode_delete_refusal() {
    let mut editor = cube_editor();
    let id = editor.selected().unwrap();
    editor.selection_mode = SelectionMode::Face;
    let scene = editor.history.scene().clone();
    editor.native_operation(Operation::Delete(id));
    assert_eq!(editor.history.scene(), &scene);
    assert!(editor.error);
    assert_eq!(
        editor.status,
        "Face deletion is unsupported; switch to Object or Hierarchy selection"
    );
}
