#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{
    CameraInspection, FaceSelectionPolicy, FaceSelectionTarget, NodeInspection, RemoteRequest,
    SceneInspection, ShapeInspection, TransformInput, UnitsInspection,
};
use crate::editor_operations::{Operation, OperationError, OperationReply, SelectionMode};
#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::{LinearUnit, NativeTransform};
use azteccraft::{
    history::{Command, EditOutcome, History},
    native_scene::{AngularUnit, FaceTarget, NativeId},
    scene::{NodeId, Primitive},
    view_scene::ViewScene,
    viewport::Camera,
};
use eframe::egui::{self, Color32, Key, RichText};
#[cfg(test)]
use glam::Vec3;
use uuid::Uuid;

#[path = "file_ui.rs"]
mod files;

struct MaterialDraft {
    scene_token: Uuid,
    shape: NativeId,
    original: f64,
    text: String,
    intent: bool,
    focus: egui::Id,
}

impl MaterialDraft {
    fn changed(&self) -> bool {
        self.intent || self.text != self.original.to_string()
    }
}

#[derive(Clone, Copy)]
enum SelectionModeIntent {
    Toggle,
    Face,
}

pub struct Editor {
    #[cfg(feature = "mcp")]
    mcp: Option<crate::mcp::UiBridge>,
    scene_token: Uuid,
    history: History,
    selected: Option<NodeId>,
    selected_path: Vec<NativeId>,
    selection_mode: SelectionMode,
    remembered_face: Option<FaceTarget>,
    camera: Camera,
    grid_visible: bool,
    renderer: Option<crate::viewport_renderer::ViewportRenderer>,
    viewport_error: Option<crate::viewport_renderer::ViewportError>,
    name: String,
    channels: [String; 9],
    channel_intents: [bool; 9],
    name_intent: bool,
    material_shape: Option<NativeId>,
    material_draft: Option<MaterialDraft>,
    native_input_pending: bool,
    status: String,
    error: bool,
    aspect: f32,
    files: files::FileState,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            #[cfg(feature = "mcp")]
            mcp: None,
            scene_token: Uuid::new_v4(),
            history: History::default(),
            selected: None,
            selected_path: Vec::new(),
            selection_mode: SelectionMode::default(),
            remembered_face: None,
            camera: Camera::default(),
            grid_visible: true,
            renderer: None,
            viewport_error: None,
            name: String::new(),
            channels: Default::default(),
            channel_intents: [false; 9],
            name_intent: false,
            material_shape: None,
            material_draft: None,
            native_input_pending: false,
            status: "Ready | Create a cube or plane to start".into(),
            error: false,
            aspect: 1.0,
            files: files::FileState::default(),
        }
    }
}

impl Editor {
    #[cfg(any(test, feature = "mcp"))]
    fn current_viewport_error(&self) -> Option<crate::viewport_renderer::ViewportError> {
        use crate::viewport_renderer::ViewportError;
        match &self.renderer {
            None => Some(ViewportError::Uninitialized),
            Some(renderer) if renderer.terminal_error().is_some() => {
                Some(ViewportError::BackendFailed)
            }
            Some(_) => self.viewport_error,
        }
    }
    #[cfg(feature = "mcp")]
    pub(crate) fn start_mcp(&mut self, context: &egui::Context) -> std::io::Result<()> {
        self.mcp = Some(crate::mcp::UiBridge::start(context.clone())?);
        Ok(())
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, crate::viewport_renderer::RenderError> {
        let renderer = crate::viewport_renderer::ViewportRenderer::initialize(cc)?;
        let mut style = egui::Style {
            visuals: egui::Visuals::dark(),
            ..Default::default()
        };
        style.visuals.panel_fill = Color32::from_gray(48);
        style.visuals.window_fill = Color32::from_gray(52);
        style.visuals.selection.bg_fill = Color32::from_rgb(58, 92, 120);
        style.spacing.item_spacing = egui::vec2(6.0, 5.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style);
        Ok(Self {
            renderer: Some(renderer),
            ..Self::default()
        })
    }

    fn sync_fields(&mut self) {
        self.material_draft = None;
        self.channel_intents = [false; 9];
        self.name_intent = false;
        let Some(node) = self
            .selected
            .and_then(|id| self.history.scene().node(id).ok())
        else {
            self.selected = None;
            self.selected_path.clear();
            self.remembered_face = None;
            self.name.clear();
            self.channels = Default::default();
            self.material_shape = None;
            return;
        };
        if self.remembered_face.as_ref().is_some_and(|target| {
            Some(target.transform) != self.selected
                || target
                    .path
                    .split_last()
                    .is_none_or(|(_, path)| path != self.selected_path)
                || self.history.scene().resolve_face_target(target).is_err()
        }) {
            self.remembered_face = None;
        }
        self.name = node.name().into();
        let values = node.transform().values();
        self.channels = values.map(|value| value.to_string());
        let transform = node.transform_id();
        let direct = |shape| {
            self.history.scene().dag_edges().iter().any(|edge| {
                edge.parent() == Some(transform)
                    && edge.child() == shape
                    && self.history.scene().shape(shape).is_ok()
            })
        };
        if !self.material_shape.is_some_and(direct) {
            let mut shapes = self
                .history
                .scene()
                .dag_edges()
                .iter()
                .filter(|edge| edge.parent() == Some(transform))
                .filter_map(|edge| self.history.scene().shape(edge.child()).ok())
                .map(|shape| shape.id());
            self.material_shape = shapes.next().filter(|_| shapes.next().is_none());
        }
        if let Some(shape) = self.material_shape
            && let Ok(value) = self.history.scene().material_for_shape(shape)
        {
            self.material_draft = Some(MaterialDraft {
                scene_token: self.scene_token,
                shape,
                original: value.diffuse,
                text: value.diffuse.to_string(),
                intent: false,
                focus: material_edit_id(),
            });
        }
    }

    #[cfg(test)]
    fn select(&mut self, selected: Option<NodeId>) {
        self.native_operation(Operation::Select {
            node: selected,
            instance_path: None,
        });
    }

    fn create(&mut self, primitive: Primitive) {
        self.native_operation(Operation::Create {
            primitive,
            name: None,
        });
    }

    fn history_step(&mut self, redo: bool) {
        self.native_operation(if redo {
            Operation::Redo
        } else {
            Operation::Undo
        });
    }

    fn frame_selected(&mut self) {
        self.native_operation(Operation::FrameSelected);
    }

    fn native_operation(&mut self, operation: Operation) {
        if self.selection_mode == SelectionMode::Face && matches!(operation, Operation::Delete(_)) {
            self.status = "Face deletion is unsupported; switch to Object selection".into();
            self.error = true;
            return;
        }
        let status = match operation {
            Operation::Select { .. }
            | Operation::SelectFace { .. }
            | Operation::SetSelectionMode(_)
            | Operation::FrameSelected
            | Operation::SetGridVisible { .. } => None,
            Operation::Orbit { .. } | Operation::Pan { .. } | Operation::Dolly { .. } => None,
            Operation::Undo => Some("Undo"),
            Operation::Redo => Some("Redo"),
            Operation::OpenScene { .. } | Operation::NewScene => Some("Scene ready"),
            Operation::RequestClose => None,
            Operation::SaveScene
            | Operation::SaveSceneAs { .. }
            | Operation::ResolveFileAction { .. } => None,
            _ => Some("Scene edited"),
        };
        match self.apply_operation(operation) {
            Ok(reply) => {
                if let Some(status) = status {
                    self.status = match reply {
                        OperationReply::Unchanged => "No change",
                        OperationReply::DocumentApplied { .. } => "Scene ready",
                        OperationReply::PendingConfirmation { .. } => {
                            "Resolve unsaved changes to continue"
                        }
                        OperationReply::Cancelled { .. } => "File action cancelled",
                        _ => status,
                    }
                    .into();
                    self.error = false;
                }
            }
            Err(error) => {
                self.status = error.to_string();
                self.error = true;
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn remote_operation(
        &mut self,
        request: RemoteRequest,
        ctx: &egui::Context,
    ) -> Result<OperationReply, OperationError> {
        self.remote_operation_guarded(request, ctx, &|| true)
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(crate) fn remote_operation_guarded(
        &mut self,
        request: RemoteRequest,
        ctx: &egui::Context,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        let focused = text_edit_focused(ctx);
        if matches!(request.operation, Operation::Inspect) {
            return Ok(OperationReply::Scene {
                inspection: Box::new(self.inspect(focused)?),
            });
        }
        if request.expected_scene_token != Some(self.scene_token) {
            return Err(OperationError::StaleScene);
        }
        if self.files.is_closing()
            || (self.files.is_modal()
                && !matches!(request.operation, Operation::ResolveFileAction { .. }))
            || focused
            || self.has_draft()
            || self.native_input_pending
        {
            return Err(OperationError::BusyEditing);
        }
        self.apply_operation_guarded(request.operation, may_install)
    }

    #[cfg(any(test, feature = "mcp"))]
    fn transform_input(
        &self,
        transform: NativeTransform,
    ) -> Result<TransformInput, OperationError> {
        let rotation_degrees = match self.history.scene().units().angular {
            AngularUnit::Degree => transform.rotation,
            AngularUnit::Radian => transform.rotation.map(f64::to_degrees),
            AngularUnit::Unsupported(_) => {
                return Err(OperationError::InvalidInput(
                    "Declared angular unit is unsupported".into(),
                ));
            }
        };
        if rotation_degrees.iter().any(|v| !v.is_finite()) {
            return Err(OperationError::InvalidInput(
                "Rotation cannot be represented in degrees".into(),
            ));
        }
        Ok(TransformInput {
            translation: transform.translation,
            rotation_degrees,
            scale: transform.scale,
        })
    }

    #[cfg(any(test, feature = "mcp"))]
    fn inspect(&self, text_focused: bool) -> Result<SceneInspection, OperationError> {
        crate::editor_operations::check_inspection_size(self.history.scene())?;
        let paths = self
            .history
            .scene()
            .transform_paths()
            .map_err(OperationError::Scene)?;
        let path_bytes = paths
            .iter()
            .try_fold(0usize, |sum, target| {
                sum.checked_add(target.path.len().checked_mul(24)?)
                    .and_then(|sum| sum.checked_add(64))
            })
            .ok_or(OperationError::TooLarge)?;
        let face_bytes = self
            .remembered_face
            .as_ref()
            .map_or(Some(0), |target| {
                if target.path.len() > azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1 {
                    return None;
                }
                target.path.len().checked_mul(24)?.checked_add(128)
            })
            .ok_or(OperationError::TooLarge)?;
        if path_bytes
            .checked_add(face_bytes)
            .is_none_or(|bytes| bytes > crate::editor_operations::MAX_RESULT_BYTES / 2)
        {
            return Err(OperationError::TooLarge);
        }
        let units = self.history.scene().units();
        let linear = match &units.linear {
            LinearUnit::Centimeter => "cm",
            LinearUnit::Millimeter => "mm",
            LinearUnit::Meter => "m",
            LinearUnit::Inch => "in",
            LinearUnit::Foot => "ft",
            LinearUnit::Unsupported(value) => value,
        };
        let angular = match &units.angular {
            AngularUnit::Degree => "deg",
            AngularUnit::Radian => "rad",
            AngularUnit::Unsupported(value) => value,
        };
        if linear
            .len()
            .saturating_add(angular.len())
            .saturating_add(units.time.len())
            > 4096
        {
            return Err(OperationError::TooLarge);
        }
        let nodes = self
            .history
            .scene()
            .nodes()
            .iter()
            .map(|node| {
                Ok(NodeInspection {
                    id: node.id().to_decimal(),
                    transform_id: node.transform_id().to_decimal(),
                    name: node.name().into(),
                    transform: self.transform_input(node.transform())?,
                    authoring_transform: node.transform().into(),
                    shape_ids: self
                        .history
                        .scene()
                        .dag_edges()
                        .iter()
                        .filter(|edge| edge.parent() == Some(node.transform_id()))
                        .filter_map(|edge| {
                            self.history
                                .scene()
                                .shape(edge.child())
                                .ok()
                                .map(|shape| shape.id().to_decimal())
                        })
                        .collect(),
                    instance_paths: paths
                        .iter()
                        .filter(|target| target.node == node.id())
                        .map(|target| target.path.iter().map(|id| id.to_decimal()).collect())
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, OperationError>>()?;
        let viewport_error = self.current_viewport_error();
        let gpu_healthy = self.renderer.is_some()
            && viewport_error != Some(crate::viewport_renderer::ViewportError::BackendFailed);
        Ok(SceneInspection {
            scene_token: self.scene_token.to_string(),
            nodes,
            shapes: self
                .history
                .scene()
                .shapes()
                .iter()
                .map(|shape| {
                    Ok(ShapeInspection {
                        id: shape.id().to_decimal(),
                        name: shape.name().into(),
                        points: shape.mesh().point_count(),
                        edges: shape.mesh().edges().len(),
                        faces: shape.mesh().faces().len(),
                        smoothing_known: shape
                            .mesh()
                            .edges()
                            .iter()
                            .all(|edge| edge.smooth().is_some()),
                        source_appearance: self
                            .history
                            .scene()
                            .shape_appearance_readiness(shape.id())
                            .map_err(OperationError::Scene)?,
                        material: match self.history.scene().material_for_shape(shape.id()) {
                            Ok(value) => Some(crate::editor_operations::MaterialInspection {
                                shader: match value.shader {
                                    azteccraft::native_scene::MaterialShader::Phong => "phong",
                                },
                                diffuse: value.diffuse,
                                shared_users: value.shared_users,
                                editing_available: value.editing_available,
                                rendering_available: value.rendering_available,
                            }),
                            Err(azteccraft::native_scene::NativeError::UnsupportedEdit) => None,
                            Err(error) => return Err(OperationError::Scene(error)),
                        },
                    })
                })
                .collect::<Result<Vec<_>, OperationError>>()?,
            units: UnitsInspection {
                linear: linear.into(),
                angular: angular.into(),
                time: units.time.clone(),
            },
            save_available: self.files.save_available(),
            save_as_available: self.files.save_as_available(),
            save_reconciliation_required: self.files.reconciliation_required(),
            document_origin: self.files.document_origin(),
            has_file: self.files.has_file(),
            closing: self.files.is_closing(),
            viewport_backend: if self.renderer.is_some() {
                "wgpu_geometry_preview"
            } else {
                "uninitialized"
            },
            viewport_error,
            grid_visible: self.grid_visible,
            depth_complete: gpu_healthy,
            clipping_complete: gpu_healthy,
            appearance_incomplete: self.history.scene().appearance_incomplete(),
            source_appearance: self.history.scene().appearance_readiness(),
            pending_action: self.files.pending_inspection(),
            selected_node: self.selected.map(NodeId::to_decimal),
            selected_instance_path: self
                .selected_path
                .iter()
                .map(|id| id.to_decimal())
                .collect(),
            selection_mode: self.selection_mode,
            remembered_face: self
                .remembered_face
                .as_ref()
                .map(|target| FaceSelectionTarget {
                    node_id: target.transform.to_decimal(),
                    shape_id: target.shape.to_decimal(),
                    instance_path: target.path.iter().map(|id| id.to_decimal()).collect(),
                    face_index: target.face_index,
                }),
            face_selection_policy: FaceSelectionPolicy::VisibleCentersOnly,
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            dirty: self.is_dirty(),
            has_draft: self.has_draft(),
            native_input_pending: self.native_input_pending,
            text_focused,
            modal: self.files.is_modal(),
            camera: CameraInspection {
                target: self.camera.target().to_array(),
                eye: self.camera.eye().to_array(),
                distance: self.camera.distance(),
            },
        })
    }

    fn selection_path(
        &self,
        id: NodeId,
        provided: Option<Vec<NativeId>>,
    ) -> Result<Vec<NativeId>, OperationError> {
        self.history
            .scene()
            .node(id)
            .map_err(OperationError::Scene)?;
        if let Some(path) = provided {
            if path.len() > azteccraft::native_scene::MAX_TRANSFORM_DEPTH {
                return Err(OperationError::InvalidInput(
                    "Instance path is too deep".into(),
                ));
            }
            self.history
                .scene()
                .resolve_transform_path(id, &path)
                .map_err(OperationError::Scene)?;
            return Ok(path);
        }
        let mut matches = self
            .history
            .scene()
            .transform_paths()
            .map_err(OperationError::Scene)?
            .into_iter()
            .filter(|target| target.node == id);
        let first = matches
            .next()
            .ok_or_else(|| OperationError::InvalidInput("Object has no instance path".into()))?;
        if matches.next().is_some() {
            return Err(OperationError::AmbiguousSelection);
        }
        Ok(first.path)
    }

    fn apply_operation(&mut self, operation: Operation) -> Result<OperationReply, OperationError> {
        self.apply_operation_guarded(operation, &|| true)
    }

    fn apply_operation_guarded(
        &mut self,
        operation: Operation,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        if self.files.is_closing() {
            return Err(OperationError::BusyEditing);
        }
        if matches!(operation, Operation::SetGridVisible { .. }) && self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        match &operation {
            Operation::OpenScene { path } | Operation::SaveSceneAs { path } => {
                let text = path.to_str().ok_or_else(|| {
                    OperationError::InvalidInput("Scene path must be UTF-8".into())
                })?;
                if text.is_empty() || text.len() > 4096 || text.contains('\0') {
                    return Err(OperationError::InvalidInput(
                        "Scene path must contain 1 to 4096 bytes and no NUL".into(),
                    ));
                }
                if matches!(operation, Operation::OpenScene { .. }) {
                    return self.open_candidate(path);
                }
                if self.files.is_modal() {
                    return Err(OperationError::BusyEditing);
                }
                return self
                    .save_document(azteccraft::document::SaveDestination::As(path), may_install);
            }
            Operation::SaveScene => {
                if self.files.is_modal() {
                    return Err(OperationError::BusyEditing);
                }
                return self
                    .save_document(azteccraft::document::SaveDestination::Active, may_install);
            }
            Operation::NewScene => return self.new_document(),
            Operation::RequestClose => return self.close_document(),
            Operation::ResolveFileAction {
                pending_id,
                decision,
            } => return self.resolve_file_action_guarded(*pending_id, *decision, may_install),
            _ => {}
        }
        let selected_path = match &operation {
            Operation::Select {
                node: Some(id),
                instance_path,
            } => Some(self.selection_path(*id, instance_path.clone())?),
            Operation::Select {
                node: None,
                instance_path: Some(_),
            } => {
                return Err(OperationError::InvalidInput(
                    "Clearing selection cannot include an instance path".into(),
                ));
            }
            _ => None,
        };
        if let Operation::SelectFace { target } = &operation {
            if self.selection_mode != SelectionMode::Face {
                return Err(OperationError::InvalidInput(
                    "Face selection requires Face mode".into(),
                ));
            }
            if let Some(target) = target {
                if !(2..=azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1)
                    .contains(&target.path.len())
                {
                    return Err(OperationError::InvalidInput(
                        "Face selection requires a bounded full shape path".into(),
                    ));
                }
                self.history
                    .scene()
                    .resolve_face_target(target)
                    .map_err(OperationError::Scene)?;
            }
        }
        if !self.commit_pending() {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        let command = match operation {
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetMaterialDiffuse { shape, value } => {
                Command::SetMaterialDiffuse { shape, value }
            }
            Operation::SetGridVisible { visible } => {
                let changed = self.grid_visible != visible;
                self.grid_visible = visible;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::Create { primitive, name } => {
                let name = name.unwrap_or_else(|| {
                    let base = match primitive {
                        Primitive::Cube => "pCube",
                        Primitive::Plane => "pPlane",
                    };
                    let mut suffix = 1;
                    while self
                        .history
                        .scene()
                        .nodes()
                        .iter()
                        .any(|node| node.name() == format!("{base}{suffix}"))
                    {
                        suffix += 1;
                    }
                    format!("{base}{suffix}")
                });
                Command::Create { primitive, name }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::Rename { id, name } => Command::Rename { id, name },
            #[cfg(any(test, feature = "mcp"))]
            Operation::EditNode {
                id,
                name,
                translation,
                rotation_degrees,
                scale,
            } => {
                if name.is_none()
                    && translation
                        .iter()
                        .chain(&rotation_degrees)
                        .chain(&scale)
                        .all(Option::is_none)
                {
                    self.history
                        .scene()
                        .node(id)
                        .map_err(OperationError::Scene)?;
                    return Ok(OperationReply::Unchanged);
                }
                let mut channels = [None; 9];
                channels[..3].copy_from_slice(&translation);
                channels[6..].copy_from_slice(&scale);
                for (index, value) in rotation_degrees.into_iter().enumerate() {
                    if let Some(value) = value {
                        let value = match self.history.scene().units().angular {
                            AngularUnit::Degree => value,
                            AngularUnit::Radian => value.to_radians(),
                            AngularUnit::Unsupported(_) => {
                                return Err(OperationError::InvalidInput(
                                    "Declared angular unit is unsupported".into(),
                                ));
                            }
                        };
                        if !value.is_finite() {
                            return Err(OperationError::InvalidInput(
                                "Rotation must be finite".into(),
                            ));
                        }
                        channels[index + 3] = Some(value);
                    }
                }
                Command::EditNode { id, name, channels }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetTransform { id, transform } => {
                let rotation = match self.history.scene().units().angular {
                    AngularUnit::Degree => transform.rotation_degrees,
                    AngularUnit::Radian => transform.rotation_degrees.map(f64::to_radians),
                    AngularUnit::Unsupported(_) => {
                        return Err(OperationError::InvalidInput(
                            "Declared angular unit is unsupported".into(),
                        ));
                    }
                };
                Command::SetTransform {
                    id,
                    transform: NativeTransform {
                        translation: transform.translation,
                        rotation,
                        scale: transform.scale,
                    },
                }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::ExtrudeFace { target, distance } => {
                if !(2..=azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1)
                    .contains(&target.path.len())
                    || !distance.is_finite()
                {
                    return Err(OperationError::InvalidInput(
                        "Extrusion requires a bounded full shape path and finite distance".into(),
                    ));
                }
                self.history
                    .scene()
                    .resolve_face_target(&target)
                    .map_err(OperationError::Scene)?;
                Command::ExtrudeFace {
                    shape: target.shape,
                    face_index: target.face_index,
                    distance,
                }
            }
            Operation::Delete(id) => Command::Remove(id),
            Operation::SetSelectionMode(mode) => {
                let changed = self.selection_mode != mode;
                self.selection_mode = mode;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectFace { target } => {
                let changed = self.remembered_face != target;
                if let Some(target) = &target {
                    let mut path = Vec::new();
                    path.try_reserve_exact(target.path.len() - 1).map_err(|_| {
                        OperationError::Scene(azteccraft::native_scene::NativeError::ResourceLimit)
                    })?;
                    path.extend_from_slice(&target.path[..target.path.len() - 1]);
                    self.selected = Some(target.transform);
                    self.selected_path = path;
                    self.sync_fields();
                }
                self.remembered_face = target;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::Select { node, .. } => {
                let path = selected_path.unwrap_or_default();
                let changed = self.selected != node || self.selected_path != path;
                if changed {
                    self.remembered_face = None;
                }
                self.selected = node;
                self.selected_path = path;
                self.sync_fields();
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::Undo | Operation::Redo => {
                let changed = if matches!(operation, Operation::Redo) {
                    self.history.redo()
                } else {
                    self.history.undo()
                };
                self.sync_fields();
                if let Some(id) = self.selected
                    && self
                        .history
                        .scene()
                        .resolve_transform_path(id, &self.selected_path)
                        .is_err()
                {
                    self.selected = None;
                    self.selected_path.clear();
                    self.sync_fields();
                }
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::FrameSelected => {
                let path = if self.selection_mode == SelectionMode::Object {
                    let id = self.selected.ok_or(OperationError::NoSelection)?;
                    Some(self.selection_path(
                        id,
                        (!self.selected_path.is_empty()).then(|| self.selected_path.clone()),
                    )?)
                } else {
                    let target = self
                        .remembered_face
                        .as_ref()
                        .ok_or(OperationError::NoSelection)?;
                    self.history
                        .scene()
                        .resolve_face_target(target)
                        .map_err(OperationError::Scene)?;
                    None
                };
                let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
                    .map_err(|error| OperationError::File(error.to_string()))?;
                let before = self.camera.clone();
                if let Some(path) = path {
                    self.camera.frame_view(&view, Some(&path), self.aspect)
                } else {
                    self.camera.frame_face(
                        &view,
                        self.remembered_face
                            .as_ref()
                            .ok_or(OperationError::NoSelection)?,
                        self.aspect,
                    )
                }
                .map_err(OperationError::Camera)?;
                return Ok(if self.camera == before {
                    OperationReply::Unchanged
                } else {
                    OperationReply::Changed
                });
            }
            Operation::Orbit { .. } | Operation::Pan { .. } | Operation::Dolly { .. } => {
                let before = self.camera.clone();
                match operation {
                    Operation::Orbit { radians } => self.camera.orbit(radians.into()),
                    Operation::Pan { world_units } => self.camera.pan(world_units.into()),
                    Operation::Dolly { factor } => self.camera.dolly(factor),
                    _ => unreachable!(),
                }
                .map_err(OperationError::Camera)?;
                return Ok(if self.camera == before {
                    OperationReply::Unchanged
                } else {
                    OperationReply::Changed
                });
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::Inspect => {
                return Ok(OperationReply::Scene {
                    inspection: Box::new(self.inspect(false)?),
                });
            }
            Operation::OpenScene { .. }
            | Operation::SaveScene
            | Operation::SaveSceneAs { .. }
            | Operation::NewScene
            | Operation::RequestClose
            | Operation::ResolveFileAction { .. } => {
                return Err(OperationError::InvalidInput(
                    "File action was not dispatched".into(),
                ));
            }
        };
        let outcome = self
            .history
            .execute(command)
            .map_err(OperationError::Scene)?;
        if let EditOutcome::Created(id) = outcome {
            self.remembered_face = None;
            self.selected = Some(id);
            self.selected_path = vec![
                self.history
                    .scene()
                    .node(id)
                    .map_err(OperationError::Scene)?
                    .transform_id(),
            ];
        }
        self.sync_fields();
        Ok(match outcome {
            EditOutcome::Created(id) => OperationReply::Created {
                node_id: id.to_decimal(),
            },
            EditOutcome::Changed => OperationReply::Changed,
            EditOutcome::Unchanged => OperationReply::Unchanged,
        })
    }

    fn has_draft(&self) -> bool {
        self.material_draft
            .as_ref()
            .is_some_and(MaterialDraft::changed)
            || self.has_channel_draft()
    }

    fn has_channel_draft(&self) -> bool {
        self.selected
            .and_then(|id| self.history.scene().node(id).ok())
            .is_some_and(|node| {
                let values = node.transform().values();
                self.name_intent
                    || self.channel_intents.iter().any(|v| *v)
                    || node.name() != self.name
                    || values
                        .iter()
                        .zip(&self.channels)
                        .any(|(value, draft)| draft != &value.to_string())
            })
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || egui::Popup::is_any_open(ctx) || self.files.is_modal()
        {
            return;
        }
        let (undo, redo, frame, save, save_as) = ctx.input(|i| {
            (
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.is_none() && i.key_pressed(Key::F),
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::S),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::S),
            )
        });
        if undo {
            self.history_step(false);
        }
        if redo {
            self.history_step(true);
        }
        if frame {
            self.frame_selected();
        }
        if save && self.files.save_available() {
            self.native_operation(Operation::SaveScene);
        }
        if save_as && self.files.save_as_available() {
            self.save_as_dialog();
        }
    }

    fn commit_pending(&mut self) -> bool {
        if self
            .material_draft
            .as_ref()
            .is_some_and(MaterialDraft::changed)
        {
            if self.has_channel_draft() {
                self.status = "Finish the channel edit before editing material data".into();
                self.error = true;
                return false;
            }
            let result = (|| {
                let draft = self.material_draft.as_ref().unwrap();
                if draft.scene_token != self.scene_token {
                    return Err("Material draft belongs to a replaced document".to_string());
                }
                self.history
                    .scene()
                    .material_for_shape(draft.shape)
                    .map_err(|error| error.to_string())?;
                let value = draft
                    .text
                    .parse::<f64>()
                    .map_err(|_| "Enter a valid diffuse number".to_string())?;
                if !value.is_finite() || value < 0.0 {
                    return Err("Diffuse must be finite and nonnegative".to_string());
                }
                self.history
                    .execute(Command::SetMaterialDiffuse {
                        shape: draft.shape,
                        value,
                    })
                    .map_err(|error| error.to_string())
            })();
            return match result {
                Ok(outcome) => {
                    self.sync_fields();
                    self.status = if outcome == EditOutcome::Unchanged {
                        "No change"
                    } else {
                        "Material data edited"
                    }
                    .into();
                    self.error = false;
                    true
                }
                Err(error) => {
                    self.status = error;
                    self.error = true;
                    false
                }
            };
        }
        let Some(id) = self.selected else {
            return true;
        };
        if !self.has_draft() {
            return true;
        }
        let result = || -> Result<Command, String> {
            let node = self.history.scene().node(id).map_err(|e| e.to_string())?;
            let original = node.transform().values();
            let mut channels = [None; 9];
            for index in 0..9 {
                if self.channel_intents[index]
                    || self.channels[index] != original[index].to_string()
                {
                    channels[index] =
                        Some(self.channels[index].parse::<f64>().map_err(|_| {
                            "Enter a valid number in the edited channel".to_string()
                        })?);
                }
            }
            Ok(Command::EditNode {
                id,
                name: (self.name_intent || node.name() != self.name).then(|| self.name.clone()),
                channels,
            })
        };
        let outcome = result().and_then(|command| {
            self.history
                .execute(command)
                .map(|outcome| outcome != EditOutcome::Unchanged)
                .map_err(|e| e.to_string())
        });
        match outcome {
            Ok(changed) => {
                self.sync_fields();
                if changed {
                    self.status = "Scene edited".into();
                    self.error = false;
                }
                true
            }
            Err(error) => {
                self.status = error;
                self.error = true;
                false
            }
        }
    }

    fn cancel_on_escape(&mut self, ctx: &egui::Context) {
        // egui clears text focus before this pass, so cancellation cannot rely on has_focus.
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.sync_fields();
            self.status = "Edit cancelled".into();
            self.error = false;
        }
    }

    fn menus(
        &mut self,
        ui: &mut egui::Ui,
        selection_intent: &mut Option<SelectionModeIntent>,
        delete_intent: &mut bool,
    ) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New scene").clicked() {
                    ui.close();
                    self.request_new();
                }
                if ui.button("Open...").clicked() {
                    ui.close();
                    self.open_dialog();
                }
                if ui
                    .add_enabled(self.files.save_available(), egui::Button::new("Save"))
                    .clicked()
                {
                    ui.close();
                    self.native_operation(Operation::SaveScene);
                }
                if ui
                    .add_enabled(
                        self.files.save_as_available(),
                        egui::Button::new("Save As..."),
                    )
                    .clicked()
                {
                    ui.close();
                    self.save_as_dialog();
                }
                unsupported(ui, "Export...");
                ui.separator();
                if ui.button("Close").clicked() {
                    ui.close();
                    self.request_close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new("Undo   Ctrl/Cmd Z"),
                    )
                    .clicked()
                {
                    self.history_step(false);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new("Redo   Ctrl/Cmd Shift Z"),
                    )
                    .clicked()
                {
                    self.history_step(true);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.selected.is_some() && self.selection_mode == SelectionMode::Object,
                        egui::Button::new("Delete"),
                    )
                    .on_disabled_hover_text(
                        "Object selection required; face deletion is unsupported",
                    )
                    .clicked()
                {
                    *delete_intent = true;
                    ui.close();
                }
            });
            ui.menu_button("Create", |ui| {
                if ui.button("Polygon cube").clicked() {
                    self.create(Primitive::Cube);
                    ui.close();
                }
                if ui.button("Polygon plane").clicked() {
                    self.create(Primitive::Plane);
                    ui.close();
                }
                unsupported(ui, "Other primitives");
            });
            ui.menu_button("Select", |ui| {
                if ui.button("Object / Component   F8").clicked() {
                    selection_intent.get_or_insert(SelectionModeIntent::Toggle);
                    ui.close();
                }
                ui.menu_button("Components", |ui| {
                    if ui
                        .selectable_label(self.selection_mode == SelectionMode::Face, "Face   F11")
                        .clicked()
                    {
                        *selection_intent = Some(SelectionModeIntent::Face);
                        ui.close();
                    }
                    unsupported(ui, "Vertices / edges / UVs");
                });
            });
            for label in ["Modify", "Mesh", "UV", "Windows", "Render"] {
                ui.menu_button(label, |ui| {
                    unsupported(ui, "Not implemented yet");
                });
            }
            ui.menu_button("Help", |ui| {
                ui.label("Local editor prototype");
                ui.label("Maya parity has not been verified.");
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("Modeling").strong());
            ui.separator();
            if crate::tool_icons::button(ui, "Cube", crate::tool_icons::Icon::Cube, true).clicked()
            {
                self.create(Primitive::Cube);
            }
            if crate::tool_icons::button(ui, "Plane", crate::tool_icons::Icon::Plane, true)
                .clicked()
            {
                self.create(Primitive::Plane);
            }
            ui.separator();
            if crate::tool_icons::button(
                ui,
                "Undo",
                crate::tool_icons::Icon::Undo,
                self.history.can_undo(),
            )
            .clicked()
            {
                self.history_step(false);
            }
            if crate::tool_icons::button(
                ui,
                "Redo",
                crate::tool_icons::Icon::Redo,
                self.history.can_redo(),
            )
            .clicked()
            {
                self.history_step(true);
            }
            if crate::tool_icons::button(
                ui,
                "Frame selected  F",
                crate::tool_icons::Icon::Frame,
                self.selected.is_some(),
            )
            .clicked()
            {
                self.frame_selected();
            }
            ui.separator();
            ui.add_enabled(false, egui::Button::new("Extrude"))
                .on_disabled_hover_text(
                    "Extrude construction history and manipulator workflow are unsupported",
                );
            unsupported(ui, "Bevel");
            unsupported(ui, "Materials");
        });
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Outliner").strong());
        ui.separator();
        ui.weak("Scene root");
        let paths = match self.history.scene().transform_paths() {
            Ok(paths) => paths,
            Err(error) => {
                ui.colored_label(Color32::LIGHT_RED, error.to_string());
                return;
            }
        };
        let mut choice = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for target in paths {
                let Ok(node) = self.history.scene().node(target.node) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.add_space((target.path.len() - 1) as f32 * 12.0);
                    if ui
                        .selectable_label(
                            self.selected == Some(target.node) && self.selected_path == target.path,
                            format!("◇  {}", node.name()),
                        )
                        .clicked()
                    {
                        choice = Some(target);
                    }
                });
            }
        });
        if let Some(target) = choice {
            self.native_operation(Operation::Select {
                node: Some(target.node),
                instance_path: Some(target.path),
            });
        }
        if self.history.scene().nodes().is_empty() {
            ui.weak("No objects. Create a primitive above.");
        }
    }

    fn appearance_scope(
        &self,
    ) -> Result<
        (&'static str, azteccraft::native_scene::AppearanceSummary),
        azteccraft::native_scene::NativeError,
    > {
        if let Some(id) = self.selected
            && let Some(summary) = self.history.scene().transform_appearance_readiness(id)?
        {
            return Ok(("Direct shapes", summary));
        }
        Ok(("Scene totals", self.history.scene().appearance_readiness()))
    }

    fn appearance_summary(&self, ui: &mut egui::Ui) {
        let Ok((scope, summary)) = self.appearance_scope() else {
            ui.weak("Appearance source state unavailable");
            return;
        };
        ui.label(egui::RichText::new(format!("{scope} ({})", summary.shapes)).small());
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!(
                    "Source bindings: {} checked, {} unavailable, {} unbound",
                    summary.checked_source_bindings, summary.unavailable, summary.unbound
                ))
                .small(),
            )
            .wrap(),
        );
        let issues = summary.issues;
        let reasons = [
            (issues.missing_membership, "missing membership"),
            (issues.overlapping_membership, "overlapping membership"),
            (issues.unsupported_shader, "unsupported shader"),
            (issues.unsupported_graph, "unsupported graph"),
            (issues.ambiguous_instance, "ambiguous instance"),
            (issues.unsupported_settings, "unsupported settings"),
            (issues.uv_mismatch, "UV mismatch"),
            (issues.malformed_data, "malformed data"),
        ]
        .into_iter()
        .filter_map(|(present, label)| present.then_some(label))
        .collect::<Vec<_>>()
        .join(", ");
        if !reasons.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(reasons).small()).wrap());
        }
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "Evaluation unavailable; filtering unproved; no color processor.",
                )
                .weak()
                .small(),
            )
            .wrap(),
        );
    }

    fn channels(&mut self, ui: &mut egui::Ui, delete_intent: &mut bool) {
        ui.label(RichText::new("Channel Box").strong());
        self.appearance_summary(ui);
        ui.separator();
        if self.selected.is_none() {
            ui.weak("Select an object to edit its channels.");
            return;
        }
        ui.label("Object name");
        let response = ui.add(egui::TextEdit::singleline(&mut self.name).id(name_edit_id()));
        self.name_intent |= response.changed();
        let mut apply = response.lost_focus();
        ui.separator();
        egui::Grid::new("transform_channels")
            .num_columns(2)
            .spacing([8.0, 5.0])
            .show(ui, |ui| {
                for (index, (label, value)) in [
                    "Translate X",
                    "Translate Y",
                    "Translate Z",
                    "Rotate X",
                    "Rotate Y",
                    "Rotate Z",
                    "Scale X",
                    "Scale Y",
                    "Scale Z",
                ]
                .iter()
                .zip(&mut self.channels)
                .enumerate()
                {
                    ui.label(*label);
                    let response = ui.add(
                        egui::TextEdit::singleline(value)
                            .id(channel_edit_id(index))
                            .desired_width(100.0),
                    );
                    self.channel_intents[index] |= response.changed();
                    apply |= response.lost_focus();
                    ui.end_row();
                }
            });
        let angular = match &self.history.scene().units().angular {
            AngularUnit::Degree => "degrees",
            AngularUnit::Radian => "radians",
            AngularUnit::Unsupported(unit) => unit,
        };
        ui.weak(format!("Rotation in {angular} | XYZ"));
        if apply {
            self.commit_pending();
        }
        self.material_controls(ui);
        ui.separator();
        if ui
            .add_enabled(
                self.selection_mode == SelectionMode::Object,
                egui::Button::new("Delete selected"),
            )
            .on_disabled_hover_text("Face deletion is unsupported; switch to Object selection")
            .clicked()
        {
            *delete_intent = true;
        }
        ui.separator();
        ui.weak("Inputs / construction history");
        unsupported(ui, "Not implemented yet");
    }

    fn material_controls(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.label(RichText::new("Materials").strong());
        ui.label("Common Material Attributes");
        let Some(transform) = self
            .selected
            .and_then(|id| self.history.scene().node(id).ok())
            .map(|node| node.transform_id())
        else {
            return;
        };
        let scene = self.history.scene();
        let shapes = || {
            scene
                .dag_edges()
                .iter()
                .filter(|edge| edge.parent() == Some(transform))
                .filter_map(|edge| scene.shape(edge.child()).ok())
        };
        let mut direct = shapes();
        if direct.next().is_none() {
            ui.weak("No direct mesh shape. Select a mesh transform.");
            return;
        }
        if direct.next().is_some() {
            let mut choice = self.material_shape;
            egui::ComboBox::from_id_salt("material_shape")
                .selected_text(
                    shapes()
                        .find(|shape| Some(shape.id()) == choice)
                        .map(|shape| shape.name())
                        .unwrap_or("Choose direct shape"),
                )
                .show_ui(ui, |ui| {
                    for shape in shapes() {
                        ui.selectable_value(&mut choice, Some(shape.id()), shape.name());
                    }
                });
            if choice != self.material_shape && self.commit_pending() {
                self.material_shape = choice;
                self.sync_fields();
            }
        }
        let Some(shape) = self.material_shape else {
            return;
        };
        let Ok(material) = self.history.scene().material_for_shape(shape) else {
            ui.weak("Material data editing unavailable for this shape.");
            return;
        };
        ui.label(format!(
            "Phong | {} shared shape users",
            material.shared_users
        ));
        ui.add_enabled(false, egui::Button::new("Color (connected)"));
        ui.weak("Material rendering unavailable");
        let channel_draft = self.has_channel_draft();
        let Some(draft) = self.material_draft.as_mut() else {
            return;
        };
        let mut apply = false;
        ui.add_enabled_ui(material.editing_available && !channel_draft, |ui| {
            ui.horizontal(|ui| {
                ui.label("Diffuse");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut draft.text)
                        .id(draft.focus)
                        .desired_width(110.0),
                );
                draft.intent |= response.changed();
                apply |= response.lost_focus()
                    || (response.has_focus() && ui.input(|input| input.key_pressed(Key::Enter)));
            });
            let mut interaction = draft
                .text
                .parse::<f64>()
                .unwrap_or(draft.original)
                .clamp(0.0, 1.0);
            let response = ui.add(egui::Slider::new(&mut interaction, 0.0..=1.0).show_value(false));
            if response.changed() {
                draft.text = interaction.to_string();
                draft.intent = true;
            }
            apply |= response.drag_stopped();
        });
        if apply {
            self.commit_pending();
        }
    }
}

impl eframe::App for Editor {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let pointer_held = ctx.input(|input| input.pointer.any_down())
            || input
                .events
                .iter()
                .any(|event| matches!(event, egui::Event::PointerButton { pressed: true, .. }));
        self.native_input_pending |= input.events.iter().any(|event| match event {
            egui::Event::PointerMoved(_) | egui::Event::MouseMoved(_) => pointer_held,
            egui::Event::Key { pressed, .. } => *pressed,
            egui::Event::Copy
            | egui::Event::Screenshot { .. }
            | egui::Event::ModifiersChanged(_)
            | egui::Event::WindowFocused(_)
            | egui::Event::PointerGone => false,
            egui::Event::Cut
            | egui::Event::Paste(_)
            | egui::Event::Text(_)
            | egui::Event::PointerButton { .. }
            | egui::Event::Zoom(_)
            | egui::Event::Rotate(_)
            | egui::Event::Ime(_)
            | egui::Event::Touch { .. }
            | egui::Event::MouseWheel { .. }
            | egui::Event::AccessKitActionRequest(_) => true,
        });
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().visible()) == Some(false) {
            self.handle_close(ctx);
            #[cfg(feature = "mcp")]
            self.process_mcp(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

impl Editor {
    fn render(&mut self, ui: &mut egui::Ui) {
        let frame_token = self.scene_token;
        let frame_admitted = !self.files.is_modal() && !self.files.is_closing();
        let keyboard_admitted = frame_admitted && !egui::Popup::is_any_open(ui.ctx());
        let (face, toggle, delete) = ui.input(|input| {
            let pressed = |key| {
                input.events.iter().any(|event| {
                    matches!(event,
                egui::Event::Key { key: event_key, pressed: true, modifiers, .. }
                if *event_key == key && modifiers.is_none())
                })
            };
            (pressed(Key::F11), pressed(Key::F8), pressed(Key::Delete))
        });
        let mut selection_intent = if !keyboard_admitted {
            None
        } else if face {
            Some(SelectionModeIntent::Face)
        } else if toggle {
            Some(SelectionModeIntent::Toggle)
        } else {
            None
        };
        let mut delete_intent = keyboard_admitted && delete;
        self.handle_close(ui.ctx());
        if !self.files.is_modal() {
            self.cancel_on_escape(ui.ctx());
        }
        self.shortcuts(ui.ctx());
        egui::Panel::top("menu_shelf").show(ui, |ui| {
            self.menus(ui, &mut selection_intent, &mut delete_intent)
        });
        egui::Panel::bottom("timeline_status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Time slider");
                for label in ["|<", "<", "Play", ">", ">|"] {
                    unsupported(ui, label);
                }
                ui.weak("Animation not implemented");
            });
            ui.separator();
            ui.colored_label(
                if self.error {
                    Color32::from_rgb(255, 160, 130)
                } else {
                    Color32::LIGHT_GRAY
                },
                &self.status,
            );
        });
        egui::Panel::left("toolbox")
            .exact_size(crate::tool_icons::toolbox_width(ui))
            .resizable(false)
            .show(ui, |ui| {
                ui.label("Tools");
                ui.separator();
                crate::tool_icons::select_indicator(ui);
                for (label, icon) in [
                    ("Move", crate::tool_icons::Icon::Move),
                    ("Rotate", crate::tool_icons::Icon::Rotate),
                    ("Scale", crate::tool_icons::Icon::Scale),
                ] {
                    crate::tool_icons::button(ui, label, icon, false)
                        .on_disabled_hover_text("Not implemented in this build");
                }
                unsupported(ui, "Snap");
            });
        egui::Panel::left("outliner")
            .default_size(200.0)
            .size_range(150.0..=350.0)
            .show(ui, |ui| self.outliner(ui));
        egui::Panel::right("channels")
            .default_size(255.0)
            .size_range(230.0..=400.0)
            .show(ui, |ui| {
                ui.add_enabled_ui(!self.files.is_closing() && !self.files.is_modal(), |ui| {
                    self.channels(ui, &mut delete_intent)
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            self.viewport(
                ui,
                frame_token,
                selection_intent,
                delete_intent,
                frame_admitted,
            )
        });
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(self.document_title()));
        self.native_input_pending = false;
        #[cfg(feature = "mcp")]
        self.process_mcp(ui.ctx());
    }

    fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        frame_token: Uuid,
        selection_intent: Option<SelectionModeIntent>,
        delete_intent: bool,
        frame_admitted: bool,
    ) {
        let mut grid_requested = false;
        ui.horizontal(|ui| {
            ui.label("persp");
            ui.label(match self.selection_mode {
                SelectionMode::Object => "Object selection",
                SelectionMode::Face => "Face selection | visible centers only",
            });
            ui.separator();
            grid_requested = ui
                .add_enabled(
                    !self.files.is_modal() && !self.files.is_closing(),
                    egui::Button::new("Grid").selected(self.grid_visible),
                )
                .clicked();
            ui.separator();
            ui.weak("Alt + left: orbit | Middle: pan | Right / wheel: dolly");
        });
        let captured = crate::viewport_ui::capture(ui, &self.camera);
        if let Ok(Some(input)) = &captured {
            self.aspect = input.aspect();
        }
        let admitted = frame_admitted && !self.files.is_modal() && !self.files.is_closing();
        self.file_modal(ui.ctx());
        if grid_requested && self.scene_token == frame_token {
            self.native_operation(Operation::SetGridVisible {
                visible: !self.grid_visible,
            });
        }
        let result = (|| -> Result<(), OperationError> {
            let Some(mut input) = captured.map_err(|error| {
                self.viewport_error = Some(crate::viewport_renderer::ViewportError::CameraInput);
                OperationError::Camera(error)
            })?
            else {
                return Ok(());
            };
            let navigation = input.take_navigation();
            let current_document = self.scene_token == frame_token;
            let can_select = admitted
                && current_document
                && !self.files.is_modal()
                && !self.files.is_closing()
                && !text_edit_focused(ui.ctx())
                && !egui::Popup::is_any_open(ui.ctx());
            let clickable = input.click_ndc().filter(|_| can_select);
            let selection_intent = selection_intent.filter(|_| can_select);
            let delete_intent = delete_intent && can_select;
            let selection_refusal = input.unsupported_selection().filter(|_| can_select);
            let navigation = if current_document {
                navigation
            } else {
                Vec::new()
            };
            if !self.files.is_modal()
                && (!navigation.is_empty()
                    || clickable.is_some()
                    || selection_intent.is_some()
                    || delete_intent)
            {
                if !self.commit_pending() {
                    return Err(OperationError::InvalidInput(self.status.clone()));
                }
                for operation in navigation {
                    self.apply_operation(operation)?;
                }
            }
            if let Some(intent) = selection_intent {
                let mode = match intent {
                    SelectionModeIntent::Face => SelectionMode::Face,
                    SelectionModeIntent::Toggle => match self.selection_mode {
                        SelectionMode::Object => SelectionMode::Face,
                        SelectionMode::Face => SelectionMode::Object,
                    },
                };
                self.apply_operation(Operation::SetSelectionMode(mode))?;
            }
            if delete_intent && let Some(id) = self.selected {
                self.native_operation(Operation::Delete(id));
            }
            if let Some(refusal) = selection_refusal {
                return Err(OperationError::InvalidInput(refusal.message().into()));
            }
            let view =
                ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024).map_err(|e| {
                    if matches!(e, azteccraft::view_scene::ViewError::ResourceLimit) {
                        self.viewport_error =
                            Some(crate::viewport_renderer::ViewportError::ResourceLimit);
                    }
                    OperationError::File(e.to_string())
                })?;
            if !self.files.is_modal()
                && let Some(ndc) = clickable
            {
                match self.selection_mode {
                    SelectionMode::Object => {
                        let hit = self
                            .camera
                            .pick_view(&view, ndc, self.aspect)
                            .map_err(|e| OperationError::File(e.to_string()))?;
                        let (node, instance_path) = if let Some(hit) = hit {
                            let mut prefix = hit.path;
                            prefix.pop();
                            (Some(hit.transform), Some(prefix))
                        } else {
                            (None, None)
                        };
                        self.apply_operation(Operation::Select {
                            node,
                            instance_path,
                        })?;
                    }
                    SelectionMode::Face => {
                        let target = input
                            .pick_face_center(
                                &self.camera,
                                &view,
                                self.selected
                                    .map(|owner| (owner, self.selected_path.as_slice())),
                            )
                            .map_err(|e| OperationError::File(e.to_string()))?;
                        self.apply_operation(Operation::SelectFace { target })?;
                    }
                }
            }
            let painted = input.paint(
                self.renderer.as_ref(),
                &self.camera,
                &view,
                match self.selection_mode {
                    SelectionMode::Object => crate::viewport_renderer::SelectionFeedback::Object(
                        self.selected.map(|_| self.selected_path.as_slice()),
                    ),
                    SelectionMode::Face => crate::viewport_renderer::SelectionFeedback::Face {
                        active: self
                            .selected
                            .map(|owner| (owner, self.selected_path.as_slice())),
                        selected: self.remembered_face.as_ref(),
                    },
                },
                self.grid_visible,
            );
            self.viewport_error = painted.as_ref().err().map(|error| error.category());
            match painted {
                Ok(()) | Err(crate::viewport_renderer::RenderError::Uninitialized) => Ok(()),
                Err(error) => Err(OperationError::File(error.to_string())),
            }
        })();
        if let Err(error) = result {
            if matches!(error, OperationError::Camera(_)) {
                self.viewport_error = Some(crate::viewport_renderer::ViewportError::CameraInput);
            }
            self.status = error.to_string();
            self.error = true;
            if self
                .renderer
                .as_ref()
                .is_some_and(|r| r.terminal_error().is_some())
            {
                ui.painter().text(
                    ui.max_rect().left_top() + egui::vec2(12.0, 32.0),
                    egui::Align2::LEFT_TOP,
                    "Viewport GPU failed: backend_failed",
                    egui::FontId::proportional(12.0),
                    Color32::from_rgb(255, 160, 130),
                );
            }
        }
    }

    #[cfg(feature = "mcp")]
    fn process_mcp(&mut self, ctx: &egui::Context) {
        if let Some(mut bridge) = self.mcp.take() {
            if !self.native_input_pending && bridge.take_close_request() {
                self.request_close();
                ctx.request_repaint();
            }
            bridge.dispatch(self, ctx);
            self.mcp = Some(bridge);
        }
    }
}

fn unsupported(ui: &mut egui::Ui, label: &str) {
    ui.add_enabled(false, egui::Button::new(label))
        .on_disabled_hover_text("Not implemented in this build");
}

fn text_edit_focused(ctx: &egui::Context) -> bool {
    ctx.memory(|memory| {
        memory.focused().is_some_and(|id| {
            id == name_edit_id()
                || id == material_edit_id()
                || (0..9).any(|index| id == channel_edit_id(index))
        })
    })
}

fn name_edit_id() -> egui::Id {
    egui::Id::new("node_name")
}

fn channel_edit_id(index: usize) -> egui::Id {
    egui::Id::new(("channel", index))
}

fn material_edit_id() -> egui::Id {
    egui::Id::new("material_diffuse")
}

#[cfg(test)]
mod tests {
    use super::*;

    mod material_wire {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_binary.rs"
        ));
    }
    mod material_fixture {
        use super::material_wire as wire;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_appearance.rs"
        ));
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
        editor.selected = Some(editor.history.scene().nodes()[0].id());
        editor.selected_path = vec![editor.history.scene().nodes()[0].transform_id()];
        editor.sync_fields();
        editor
    }

    fn independent_material_editor() -> Editor {
        let mut bytes = material_fixture::appearance_specimen_with_users(2);
        let link = b"shader.oc\0surface2.ss\0";
        let offset = bytes
            .windows(link.len())
            .position(|word| word == link)
            .unwrap();
        bytes[offset..offset + 6].copy_from_slice(b"shadeB");
        let container = azteccraft::maya_binary::container::inspect(bytes.clone().into()).unwrap();
        let shader = container
            .chunks()
            .iter()
            .find(|chunk| chunk.form() == Some(*b"RPHO"))
            .unwrap();
        let mut second = bytes[shader.header_range().start..shader.padding_range().end].to_vec();
        let offset = second
            .windows(7)
            .position(|word| word == b"shader\0")
            .unwrap();
        second[offset..offset + 6].copy_from_slice(b"shadeB");
        let mut children = Vec::new();
        for (index, chunk) in container
            .chunks()
            .iter()
            .enumerate()
            .filter(|(_, chunk)| chunk.parent() == Some(0))
        {
            if chunk.form() == Some(*b"CONS") {
                let connection = container
                    .chunks()
                    .iter()
                    .find(|child| child.parent() == Some(index) && child.form() == Some(*b"CONN"))
                    .unwrap();
                let mut links = bytes
                    [connection.payload_range().start + 4..connection.payload_range().end]
                    .to_vec();
                links.extend(material_wire::chunk(
                    b"CWFL",
                    b"\0texture.oc\0shadeB.c\0",
                    8,
                ));
                children.extend(material_wire::group(
                    b"CONS",
                    &material_wire::group(b"CONN", &links, 4),
                    4,
                ));
            } else {
                children.extend_from_slice(
                    &bytes[chunk.header_range().start..chunk.padding_range().end],
                );
            }
        }
        children.extend(second);
        let scene = azteccraft::native_scene::binary::decode(
            material_wire::group(b"Maya", &children, 1).into(),
        )
        .unwrap();
        let mut editor = Editor {
            history: History::new(scene),
            ..Editor::default()
        };
        editor.selected = Some(editor.history.scene().nodes()[0].id());
        editor.sync_fields();
        editor
    }

    #[test]
    fn material_selection_commits_only_original_independent_shader() {
        let mut editor = independent_material_editor();
        let first = editor.material_shape.unwrap();
        let second = editor.history.scene().shapes()[1].id();
        let other = editor.history.scene().nodes()[1].id();
        let untouched = editor
            .history
            .scene()
            .material_for_shape(second)
            .unwrap()
            .diffuse
            .to_bits();
        editor.material_draft.as_mut().unwrap().text = "0.125".into();
        editor.select(Some(other));
        assert_eq!(editor.material_shape, Some(second));
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(first)
                .unwrap()
                .diffuse,
            0.125
        );
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(second)
                .unwrap()
                .diffuse
                .to_bits(),
            untouched
        );
        editor.history_step(false);
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(first)
                .unwrap()
                .diffuse
                .to_bits(),
            untouched
        );
        editor.history_step(true);
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(first)
                .unwrap()
                .diffuse,
            0.125
        );
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(second)
                .unwrap()
                .diffuse
                .to_bits(),
            untouched
        );
    }

    #[test]
    fn material_real_text_enter_and_escape_preserve_precision_in_one_history_entry() {
        let mut editor = material_editor();
        let shape = editor.material_shape.unwrap();
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(material_edit_id()));
        frame(&mut editor, &ctx, vec![]);
        let command = egui::Modifiers::COMMAND;
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(command),
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
                egui::Event::Text("1.0000000000000002".into()),
                key(Key::Enter),
            ],
        );
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(shape)
                .unwrap()
                .diffuse
                .to_bits(),
            1.0000000000000002f64.to_bits()
        );
        assert!(!editor.has_draft());
        assert!(editor.history.undo());
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.history.can_undo());
        editor.sync_fields();
        ctx.memory_mut(|memory| memory.request_focus(material_edit_id()));
        frame(&mut editor, &ctx, vec![]);
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(command),
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
                egui::Event::Text("0.125".into()),
            ],
        );
        assert!(editor.has_draft());
        frame(&mut editor, &ctx, vec![key(Key::Escape)]);
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.has_draft());
    }

    #[test]
    fn material_failed_and_cancelled_open_preserve_draft_and_success_resets_reused_shape_choice() {
        let mut editor = material_editor();
        editor.material_draft.as_mut().unwrap().text = "0.125".into();
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let directory = tempfile::tempdir().unwrap();
        assert!(
            editor
                .open_candidate(&directory.path().join("missing.mb"))
                .is_err()
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.material_draft.as_ref().unwrap().text, "0.125");
        editor.sync_fields();
        let source = directory.path().join("replacement.mb");
        std::fs::write(&source, material_wire::specimen()).unwrap();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor.open_candidate(&source).unwrap()
        else {
            panic!("original nonempty document is dirty");
        };
        let original_shape = editor.material_shape;
        editor
            .resolve_file_action(
                Uuid::parse_str(&pending_action_id).unwrap(),
                crate::editor_operations::FileDecision::Cancel,
            )
            .unwrap();
        assert_eq!(editor.material_shape, original_shape);
        assert_eq!(editor.scene_token, token);
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor.open_candidate(&source).unwrap()
        else {
            panic!("confirmation required");
        };
        editor
            .resolve_file_action(
                Uuid::parse_str(&pending_action_id).unwrap(),
                crate::editor_operations::FileDecision::Discard,
            )
            .unwrap();
        assert_ne!(editor.scene_token, token);
        assert!(editor.material_shape.is_none());
        assert!(editor.material_draft.is_none());
        assert!(editor.commit_pending());
    }

    #[test]
    fn material_draft_old_target_shared_history_and_escape_use_authoritative_state() {
        let mut editor = material_editor();
        let shape = editor.material_shape.unwrap();
        let original = editor
            .history
            .scene()
            .material_for_shape(shape)
            .unwrap()
            .diffuse;
        editor.material_draft.as_mut().unwrap().text = "1.2345678901234567".into();
        editor.material_draft.as_mut().unwrap().intent = true;
        let other = editor.history.scene().nodes()[1].id();
        editor.select(Some(other));
        assert_eq!(editor.selected, Some(other));
        let value = editor.history.scene().material_for_shape(shape).unwrap();
        assert_eq!(value.diffuse.to_bits(), 1.2345678901234567f64.to_bits());
        assert_eq!(value.shared_users, 2);
        for candidate in editor.history.scene().shapes() {
            assert_eq!(
                editor
                    .history
                    .scene()
                    .material_for_shape(candidate.id())
                    .unwrap()
                    .diffuse
                    .to_bits(),
                value.diffuse.to_bits()
            );
        }
        editor.history_step(false);
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(shape)
                .unwrap()
                .diffuse
                .to_bits(),
            original.to_bits()
        );
        editor.history_step(true);
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(shape)
                .unwrap()
                .diffuse
                .to_bits(),
            value.diffuse.to_bits()
        );
        let before = editor.history.scene().clone();
        editor.material_draft.as_mut().unwrap().text = "0.25".into();
        let context = egui::Context::default();
        let mut frame = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |ui| editor.cancel_on_escape(ui.ctx()),
        );
        frame.textures_delta.clear();
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.has_draft());
        assert!(
            editor
                .history
                .scene()
                .clone()
                .set_material_diffuse(shape, 0.5)
                .is_ok()
        );
        assert!(editor.history.execute(Command::Remove(other)).is_err());
    }

    #[test]
    fn material_paint_preserves_unrounded_words_and_focus_or_drafts_block_remote_mutation() {
        let context = egui::Context::default();
        let mut editor = material_editor();
        let shape = editor.material_shape.unwrap();
        for value in [1.2345678901234567, -0.0, 4.5] {
            editor
                .history
                .execute(Command::SetMaterialDiffuse { shape, value })
                .unwrap();
            editor.sync_fields();
            let before = editor.history.scene().clone();
            for _ in 0..2 {
                let mut frame =
                    context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
                frame.textures_delta.clear();
            }
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(
                editor.material_draft.as_ref().unwrap().text,
                value.to_string()
            );
            assert!(!editor.has_draft());
        }
        context.memory_mut(|memory| memory.request_focus(material_edit_id()));
        let token = editor.scene_token;
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(token),
                    operation: Operation::SetMaterialDiffuse { shape, value: 0.25 }
                },
                &context
            ),
            Err(OperationError::BusyEditing)
        ));
        context.memory_mut(|memory| memory.surrender_focus(material_edit_id()));
        editor.material_draft.as_mut().unwrap().text = "0.125".into();
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(token),
                    operation: Operation::Undo
                },
                &context
            ),
            Err(OperationError::BusyEditing)
        ));
        let before = editor.history.scene().clone();
        editor.material_draft.as_mut().unwrap().scene_token = Uuid::new_v4();
        assert!(!editor.commit_pending());
        assert_eq!(editor.history.scene(), &before);
    }

    #[test]
    fn material_invalid_or_simultaneous_channel_draft_never_partially_commits() {
        let mut editor = material_editor();
        let before = editor.history.scene().clone();
        editor.material_draft.as_mut().unwrap().text = "NaN".into();
        assert!(!editor.commit_pending());
        assert_eq!(editor.history.scene(), &before);
        editor.material_draft.as_mut().unwrap().text = "0.2".into();
        editor.channels[0] = "2".into();
        assert!(!editor.commit_pending());
        assert_eq!(editor.history.scene(), &before);
        editor.sync_fields();
        assert!(!editor.has_draft());
    }

    #[test]
    fn material_new_document_commits_the_original_target_then_clears_its_draft() {
        let mut editor = material_editor();
        let token = editor.scene_token;
        let shape = editor.material_shape.unwrap();
        editor.material_draft.as_mut().unwrap().text = "0.125".into();
        let reply = editor.new_document().unwrap();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = reply
        else {
            panic!("edited material must require confirmation");
        };
        assert_eq!(
            editor
                .history
                .scene()
                .material_for_shape(shape)
                .unwrap()
                .diffuse,
            0.125
        );
        assert!(!editor.has_draft());
        editor
            .resolve_file_action(
                Uuid::parse_str(&pending_action_id).unwrap(),
                crate::editor_operations::FileDecision::Discard,
            )
            .unwrap();
        assert_ne!(editor.scene_token, token);
        assert!(editor.material_draft.is_none());
        assert!(editor.material_shape.is_none());
        assert!(editor.history.scene().shapes().is_empty());
        assert!(editor.commit_pending());
    }

    #[test]
    fn material_negative_source_word_is_not_changed_by_native_paint() {
        let mut bytes = material_fixture::appearance_specimen();
        let marker = [b'd', b'c', 0, 0x24];
        let offset = bytes
            .windows(marker.len())
            .position(|value| value == marker)
            .unwrap()
            + marker.len();
        let value = -1.2345678901234567f64;
        bytes[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
        let scene = azteccraft::native_scene::binary::decode(bytes.into()).unwrap();
        let mut editor = Editor {
            history: History::new(scene),
            ..Editor::default()
        };
        editor.selected = Some(editor.history.scene().nodes()[0].id());
        editor.sync_fields();
        let before = editor.history.scene().clone();
        let context = egui::Context::default();
        for _ in 0..2 {
            let mut frame =
                context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
            frame.textures_delta.clear();
        }
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(
            editor.material_draft.as_ref().unwrap().text,
            value.to_string()
        );
        assert!(!editor.has_draft());
    }

    #[test]
    fn material_multiple_direct_shapes_require_an_explicit_choice_without_guessing() {
        let extra = material_wire::group(
            b"DMSH",
            &[
                material_wire::creation(b"secondShape", Some(b"object")),
                material_wire::chunk(
                    b"MESH",
                    &material_wire::mesh_payload(&[0, 1, 0x6000_0002]),
                    8,
                ),
            ]
            .concat(),
            4,
        );
        let bytes = material_fixture::appearance_specimen_with_fields(material_fixture::Fields {
            groups: &extra,
            ..Default::default()
        });
        let scene = azteccraft::native_scene::binary::decode(bytes.into()).unwrap();
        let mut editor = Editor {
            history: History::new(scene),
            ..Editor::default()
        };
        editor.selected = Some(editor.history.scene().nodes()[0].id());
        editor.sync_fields();
        assert!(editor.material_shape.is_none());
        assert!(editor.material_draft.is_none());
        let context = egui::Context::default();
        let before = editor.history.scene().clone();
        let mut frame =
            context.run_ui(egui::RawInput::default(), |ui| editor.material_controls(ui));
        frame.textures_delta.clear();
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.material_shape.is_none());
        editor.material_shape = Some(editor.history.scene().shapes()[0].id());
        editor.sync_fields();
        assert!(editor.material_draft.is_some());
    }

    fn selected_face(editor: &Editor, face_index: usize) -> FaceTarget {
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        let instance = view
            .instances()
            .iter()
            .find(|instance| Some(instance.transform()) == editor.selected)
            .unwrap();
        FaceTarget {
            transform: instance.transform(),
            shape: instance.shape(),
            path: instance.path().to_vec(),
            face_index,
        }
    }

    #[test]
    fn face_lifecycle_revalidates_numeric_edits_topology_history_and_object_deletion() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let cap = selected_face(&editor, 0);
        remote(
            &mut editor,
            Operation::SetSelectionMode(SelectionMode::Face),
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(cap.clone()),
            },
        )
        .unwrap();
        editor.channels[0] = "2".into();
        editor.native_operation(Operation::FrameSelected);
        assert!(!editor.error);
        assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            2.0
        );
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
        remote(&mut editor, Operation::Redo).unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
        remote(
            &mut editor,
            Operation::ExtrudeFace {
                target: cap.clone(),
                distance: 0.25,
            },
        )
        .unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&cap));
        let side = FaceTarget {
            face_index: 6,
            ..cap
        };
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(side.clone()),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(editor.remembered_face, None);
        assert!(editor.history.can_redo());
        remote(&mut editor, Operation::Redo).unwrap();
        assert_eq!(editor.remembered_face, None);
        remote(&mut editor, Operation::SelectFace { target: Some(side) }).unwrap();
        remote(&mut editor, Operation::Delete(id)).unwrap();
        assert_eq!(editor.selected, None);
        assert!(editor.selected_path.is_empty());
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        remote(&mut editor, Operation::Undo).unwrap();
        assert!(editor.history.scene().node(id).is_ok());
        assert_eq!(editor.remembered_face, None);
    }

    #[test]
    fn face_frame_uses_selected_corners_even_when_center_is_not_pickable() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.aspect = 4.0 / 3.0;
        let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
        let instance = &view.instances()[0];
        let (face, target) = instance
            .faces()
            .iter()
            .find_map(|face| {
                let center = face.center(instance.world_vertices()).unwrap();
                let ndc = editor.camera.project(center, editor.aspect).unwrap()?;
                let pointer = egui::vec2((ndc.x + 1.0) * 400.0, (1.0 - ndc.y) * 300.0);
                let target = FaceTarget {
                    transform: instance.transform(),
                    shape: instance.shape(),
                    path: instance.path().to_vec(),
                    face_index: face.source_index(),
                };
                let hit = editor
                    .camera
                    .pick_face_center(
                        &view,
                        Some((instance.transform(), &editor.selected_path)),
                        glam::Vec2::new(pointer.x, pointer.y),
                        [800, 600],
                        1.0,
                    )
                    .unwrap();
                (hit.as_ref() != Some(&target)).then_some((face, target))
            })
            .expect("cube has an occluded center");
        let corners: Vec<_> = face
            .corners()
            .iter()
            .map(|&index| instance.world_vertices()[index])
            .collect();
        let mut expected = editor.camera.clone();
        expected.frame_points(&corners, editor.aspect).unwrap();
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        remote(
            &mut editor,
            Operation::SetSelectionMode(SelectionMode::Face),
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target.clone()),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::FrameSelected).unwrap();
        assert_eq!(editor.camera, expected);
        assert_eq!(editor.remembered_face.as_ref(), Some(&target));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.scene_token, token);
        remote(&mut editor, Operation::SelectFace { target: None }).unwrap();
        let camera = editor.camera.clone();
        assert!(matches!(
            remote(&mut editor, Operation::FrameSelected),
            Err(OperationError::NoSelection)
        ));
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.history.scene(), &scene);
        editor.name = "valid_human_rename".into();
        assert!(matches!(
            remote(&mut editor, Operation::FrameSelected),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.name, "valid_human_rename");
        editor.native_operation(Operation::FrameSelected);
        assert!(editor.error);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.remembered_face, None);
        assert_eq!(
            editor
                .history
                .scene()
                .node(target.transform)
                .unwrap()
                .name(),
            "valid_human_rename"
        );
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(editor.history.scene(), &scene);
    }

    #[test]
    fn face_mode_preserves_the_remembered_target_without_editing_scene_or_redo() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let target = selected_face(&editor, 4);
        remote(
            &mut editor,
            Operation::Rename {
                id,
                name: "temporary".into(),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        let scene = editor.history.scene().clone();
        let token = editor.scene_token;
        let camera = editor.camera.clone();
        let dirty = editor.inspect(false).unwrap().dirty;
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        remote(
            &mut editor,
            Operation::SetSelectionMode(SelectionMode::Face),
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target.clone()),
            },
        )
        .unwrap();
        for mode in [SelectionMode::Object, SelectionMode::Face] {
            remote(&mut editor, Operation::SetSelectionMode(mode)).unwrap();
            assert_eq!(editor.remembered_face.as_ref(), Some(&target));
            assert_eq!(editor.selected, Some(id));
            assert_eq!(editor.selected_path, target.path[..target.path.len() - 1]);
        }
        remote(
            &mut editor,
            Operation::Select {
                node: Some(id),
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(editor.remembered_face.as_ref(), Some(&target));
        remote(&mut editor, Operation::SelectFace { target: None }).unwrap();
        assert_eq!(editor.selected, Some(id));
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.can_redo());
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.inspect(false).unwrap().dirty, dirty);
    }

    #[test]
    fn explicit_face_selection_selects_its_owner_and_object_changes_clear_memory() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let target = selected_face(&editor, 5);
        editor.create(Primitive::Plane);
        let other = editor.selected.unwrap();
        let second_shape = selected_face(&editor, 0);
        remote(
            &mut editor,
            Operation::Rename {
                id: other,
                name: "temporary".into(),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        remote(
            &mut editor,
            Operation::SetSelectionMode(SelectionMode::Face),
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target.clone()),
            },
        )
        .unwrap();
        assert_eq!(editor.selected, Some(target.transform));
        assert_eq!(editor.selected_path, target.path[..target.path.len() - 1]);
        assert_eq!(editor.remembered_face.as_ref(), Some(&target));
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(second_shape.clone()),
            },
        )
        .unwrap();
        assert_eq!(editor.selected, Some(other));
        assert_eq!(editor.remembered_face.as_ref(), Some(&second_shape));
        assert_eq!(
            editor.selected_path,
            second_shape.path[..second_shape.path.len() - 1]
        );
        let baseline = (
            editor.history.scene().clone(),
            editor.selected,
            editor.selected_path.clone(),
            editor.selection_mode,
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token,
            editor.name.clone(),
            editor.channels.clone(),
            editor.inspect(false).unwrap().dirty,
        );
        assert!(editor.history.can_redo());
        assert!(matches!(
            remote(
                &mut editor,
                Operation::SelectFace {
                    target: Some(second_shape)
                }
            )
            .unwrap(),
            OperationReply::Unchanged
        ));
        assert_eq!(
            (
                editor.history.scene().clone(),
                editor.selected,
                editor.selected_path.clone(),
                editor.selection_mode,
                editor.remembered_face.clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.name.clone(),
                editor.channels.clone(),
                editor.inspect(false).unwrap().dirty,
            ),
            baseline
        );
        assert!(editor.history.can_redo());
        remote(
            &mut editor,
            Operation::Select {
                node: Some(target.transform),
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(editor.remembered_face, None);
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target),
            },
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::Select {
                node: None,
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.selected, None);
    }

    #[test]
    fn invalid_face_targets_and_wrong_mode_preserve_the_selection_baseline() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let target = selected_face(&editor, 1);
        editor.name = "valid_human_draft".into();
        assert!(
            editor
                .apply_operation(Operation::SelectFace {
                    target: Some(target.clone())
                })
                .is_err()
        );
        assert!(editor.has_draft());
        assert_eq!(editor.remembered_face, None);
        editor.sync_fields();
        editor.create(Primitive::Plane);
        let other = selected_face(&editor, 0);
        remote(
            &mut editor,
            Operation::SetSelectionMode(SelectionMode::Face),
        )
        .unwrap();
        remote(
            &mut editor,
            Operation::SelectFace {
                target: Some(target.clone()),
            },
        )
        .unwrap();
        let scene = editor.history.scene().clone();
        let before = (
            editor.selected,
            editor.selected_path.clone(),
            editor.selection_mode,
            editor.remembered_face.clone(),
            editor.name.clone(),
            editor.channels.clone(),
            editor.scene_token,
            editor.camera.clone(),
        );
        let mut invalid = Vec::new();
        let mut bad = target.clone();
        bad.path = other.path;
        invalid.push(bad);
        let mut bad = target.clone();
        bad.transform = other.transform;
        invalid.push(bad);
        let mut bad = target.clone();
        bad.shape = other.shape;
        invalid.push(bad);
        let mut bad = target.clone();
        bad.face_index = usize::MAX;
        invalid.push(bad);
        let mut bad = target.clone();
        bad.path.clear();
        invalid.push(bad);
        let mut bad = target.clone();
        bad.path = vec![target.shape; azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 2];
        invalid.push(bad);
        let mut bad = target.clone();
        bad.transform = NodeId::from_decimal("18446744073709551615").unwrap();
        invalid.push(bad);
        let mut bad = target;
        bad.shape = NativeId::from_decimal("18446744073709551615").unwrap();
        invalid.push(bad);
        for target in invalid {
            assert!(
                remote(
                    &mut editor,
                    Operation::SelectFace {
                        target: Some(target)
                    }
                )
                .is_err()
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(
                (
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.selection_mode,
                    editor.remembered_face.clone(),
                    editor.name.clone(),
                    editor.channels.clone(),
                    editor.scene_token,
                    editor.camera.clone(),
                ),
                before
            );
        }
    }

    #[test]
    fn channel_box_appearance_summary_precedes_controls_and_preserves_human_state() {
        for width in [240.0, 420.0] {
            let context = egui::Context::default();
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            editor.name = "pending human draft".into();
            context.memory_mut(|memory| memory.request_focus(name_edit_id()));
            let scene = editor.history.scene().clone();
            let selected = editor.selected;
            let path = editor.selected_path.clone();
            let token = editor.scene_token;
            let mut output = None;
            for _ in 0..2 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 720.0),
                    )),
                    ..Default::default()
                };
                let mut delete_intent = false;
                let mut frame = context.run_ui(input, |ui| editor.channels(ui, &mut delete_intent));
                frame.textures_delta.clear();
                output = Some(frame);
            }
            let output = output.unwrap();
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((text.galley.job.text.as_str(), text.pos.y)),
                    _ => None,
                })
                .collect();
            let summary = labels
                .iter()
                .find(|(text, _)| *text == "Direct shapes (1)")
                .unwrap();
            let controls = labels
                .iter()
                .find(|(text, _)| *text == "Object name")
                .unwrap();
            assert!(summary.1 < controls.1);
            assert!(
                labels
                    .iter()
                    .any(|(text, _)| text.contains("Evaluation unavailable"))
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.selected, selected);
            assert_eq!(editor.selected_path, path);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.name, "pending human draft");
            assert!(context.memory(|memory| memory.has_focus(name_edit_id())));
        }
    }

    #[test]
    fn camera_nonfinite_operations_and_native_draft_failure_preserve_their_baselines() {
        let context = egui::Context::default();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let camera = editor.camera.clone();
        let before = editor.history.scene().clone();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for operation in [
                Operation::Orbit {
                    radians: [value, 0.0],
                },
                Operation::Orbit {
                    radians: [0.0, value],
                },
                Operation::Pan {
                    world_units: [value, 0.0],
                },
                Operation::Pan {
                    world_units: [0.0, value],
                },
                Operation::Dolly { factor: value },
            ] {
                assert!(matches!(
                    editor.remote_operation(
                        RemoteRequest {
                            expected_scene_token: Some(editor.scene_token),
                            operation,
                        },
                        &context
                    ),
                    Err(OperationError::Camera(_))
                ));
                assert_eq!(editor.camera, camera);
                assert_eq!(editor.history.scene(), &before);
            }
        }
        editor.name = "committed_before_navigation".into();
        editor.native_operation(Operation::Dolly { factor: -1.0 });
        assert_eq!(editor.camera, camera);
        assert_eq!(
            editor.history.scene().nodes()[0].name(),
            "committed_before_navigation"
        );
        assert!(!editor.has_draft());
        assert!(editor.error);
        editor.history_step(false);
        assert_eq!(editor.history.scene(), &before);
        editor.native_operation(Operation::Dolly { factor: 0.5 });
        assert_ne!(editor.status, "Scene edited");
        assert!(editor.history.can_redo());
    }

    #[test]
    fn native_hidden_close_checks_pending_input_without_mcp() {
        for pending in [false, true] {
            let context = egui::Context::default();
            let mut editor = Editor::default();
            let mut input = egui::RawInput::default();
            if pending {
                input.events.push(egui::Event::Text("pending".into()));
            }
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.minimized = Some(true);
            viewport.events.push(egui::ViewportEvent::Close);
            eframe::App::raw_input_hook(&mut editor, &context, &mut input);
            let mut native_frame = eframe::Frame::_new_kittest();
            let output = context.run_logic(&input, |ctx| {
                eframe::App::logic(&mut editor, ctx, &mut native_frame)
            });
            let commands = &output.viewport_commands[&egui::ViewportId::ROOT];
            assert!(
                commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
            );
            assert_eq!(
                commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Close)),
                !pending
            );
        }
    }

    fn remote(editor: &mut Editor, operation: Operation) -> Result<OperationReply, OperationError> {
        editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(editor.scene_token),
                operation,
            },
            &egui::Context::default(),
        )
    }

    #[test]
    fn grid_setter_is_idempotent_and_preserves_scene_history_and_editor_baseline() {
        let mut editor = Editor::default();
        assert!(editor.grid_visible);
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        remote(
            &mut editor,
            Operation::Rename {
                id,
                name: "renamed".into(),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        assert!(editor.history.can_redo());
        let scene = editor.history.scene().clone();
        let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
        let fields = (editor.name.clone(), editor.channels.clone());
        let status = (editor.status.clone(), editor.error);
        for (visible, changed) in [(true, false), (false, true), (false, false), (true, true)] {
            let reply = remote(&mut editor, Operation::SetGridVisible { visible }).unwrap();
            assert_eq!(matches!(reply, OperationReply::Changed), changed);
            assert_eq!(matches!(reply, OperationReply::Unchanged), !changed);
            let mut actual = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
            assert_eq!(actual["grid_visible"], visible);
            actual["grid_visible"] = serde_json::Value::Bool(true);
            assert_eq!(actual, before);
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!((editor.name.clone(), editor.channels.clone()), fields);
            assert_eq!((editor.status.clone(), editor.error), status);
        }
    }

    #[test]
    fn grid_native_draft_policy_and_remote_draft_refusal_share_the_typed_setter() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let scene = editor.history.scene().clone();
        editor.channels[0] = "invalid".into();
        assert!(matches!(
            editor.apply_operation(Operation::SetGridVisible { visible: false }),
            Err(OperationError::InvalidInput(_))
        ));
        assert!(editor.grid_visible);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.channels[0], "invalid");
        for draft in ["2", "invalid"] {
            editor.channels[0] = draft.into();
            assert!(matches!(
                remote(&mut editor, Operation::SetGridVisible { visible: false }),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.channels[0], draft);
            assert!(editor.grid_visible);
        }
        editor.channels[0] = "2".into();
        editor.native_operation(Operation::SetGridVisible { visible: false });
        assert!(!editor.grid_visible);
        assert!(!editor.has_draft());
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            2.0
        );
        editor.history_step(false);
        assert_eq!(editor.history.scene(), &scene);
        assert!(!editor.grid_visible);
    }

    #[test]
    fn grid_native_modal_and_close_refuse_before_committing_drafts() {
        for closing in [false, true] {
            let mut editor = Editor::default();
            if closing {
                editor.close_document().unwrap();
            } else {
                editor.create(Primitive::Cube);
                editor.new_document().unwrap();
            }
            editor.name = "pending draft".into();
            let before = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
            let scene = editor.history.scene().clone();
            assert!(matches!(
                editor.apply_operation(Operation::SetGridVisible { visible: false }),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(
                serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
                before
            );
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.name, "pending draft");
        }
    }

    #[test]
    fn grid_session_setting_survives_successful_and_canceled_new_and_open() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("empty.aztec");
        azteccraft::document::save_new(&path, &azteccraft::scene::Scene::default()).unwrap();
        let mut editor = Editor::default();
        remote(&mut editor, Operation::SetGridVisible { visible: false }).unwrap();
        for open in [false, true] {
            let operation = if open {
                Operation::OpenScene { path: path.clone() }
            } else {
                Operation::NewScene
            };
            assert!(matches!(
                remote(&mut editor, operation).unwrap(),
                OperationReply::DocumentApplied { .. }
            ));
            assert!(!editor.grid_visible);
            editor.create(Primitive::Cube);
            let scene = editor.history.scene().clone();
            let token = editor.scene_token;
            let operation = if open {
                Operation::OpenScene { path: path.clone() }
            } else {
                Operation::NewScene
            };
            let OperationReply::PendingConfirmation {
                pending_action_id, ..
            } = remote(&mut editor, operation).unwrap()
            else {
                panic!("expected unsaved confirmation");
            };
            remote(
                &mut editor,
                Operation::ResolveFileAction {
                    pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                    decision: crate::editor_operations::FileDecision::Cancel,
                },
            )
            .unwrap();
            assert!(!editor.grid_visible);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.history.scene(), &scene);
            editor.history_step(false);
        }
    }

    #[test]
    fn sparse_nonfinite_intents_preserve_scene_fields_and_redo() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let before = editor.history.scene().clone();
        remote(
            &mut editor,
            Operation::Rename {
                id,
                name: "temporary".into(),
            },
        )
        .unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        let fields = (editor.name.clone(), editor.channels.clone());
        for group in 0..3 {
            let mut components = [[None; 3]; 3];
            components[group][0] = Some(f64::NAN);
            assert!(
                remote(
                    &mut editor,
                    Operation::EditNode {
                        id,
                        name: Some("must_not_apply".into()),
                        translation: components[0],
                        rotation_degrees: components[1],
                        scale: components[2]
                    }
                )
                .is_err()
            );
            assert_eq!(editor.history.scene(), &before);
            assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
            assert!(editor.history.can_redo());
        }
    }

    #[test]
    fn remote_operations_share_native_scene_selection_history_and_camera() {
        let mut editor = Editor::default();
        assert_eq!(crate::editor_operations::TOOL_NAMES.len(), 24);
        let OperationReply::Created { node_id } = remote(
            &mut editor,
            Operation::Create {
                primitive: Primitive::Cube,
                name: Some("remote_cube".into()),
            },
        )
        .unwrap() else {
            panic!("expected created object")
        };
        let id = NodeId::from_decimal(&node_id).unwrap();
        assert_eq!(editor.selected, Some(id));
        remote(
            &mut editor,
            Operation::Rename {
                id,
                name: "edited".into(),
            },
        )
        .unwrap();
        let transform = NativeTransform {
            translation: [4.0, 2.0, 1.0],
            ..Default::default()
        };
        let input = editor.transform_input(transform).unwrap();
        assert!(matches!(
            remote(
                &mut editor,
                Operation::SetTransform {
                    id,
                    transform: input
                }
            )
            .unwrap(),
            OperationReply::Changed
        ));
        assert!(matches!(
            remote(
                &mut editor,
                Operation::SetTransform {
                    id,
                    transform: input
                }
            )
            .unwrap(),
            OperationReply::Unchanged
        ));
        remote(&mut editor, Operation::FrameSelected).unwrap();
        assert!(editor.camera.target().distance(Vec3::new(4.0, 2.0, 1.0)) < 0.01);
        remote(
            &mut editor,
            Operation::Select {
                node: None,
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(editor.selected, None);
        remote(
            &mut editor,
            Operation::Select {
                node: Some(id),
                instance_path: None,
            },
        )
        .unwrap();
        assert_eq!(editor.name, "edited");
        editor.history_step(false);
        assert_eq!(
            editor.history.scene().node(id).unwrap().transform(),
            NativeTransform::default()
        );
        remote(&mut editor, Operation::Redo).unwrap();
        assert_eq!(
            editor.history.scene().node(id).unwrap().transform(),
            transform
        );
        editor.channels[0] = "6".into();
        editor.create(Primitive::Plane);
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            6.0
        );
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection")
        };
        assert_eq!(inspection.nodes.len(), 2);
        assert_eq!(inspection.nodes[0].id, node_id);
        assert_eq!(inspection.nodes[0].transform.translation[0], 6.0);
        assert!(inspection.dirty && inspection.can_undo);
        assert!(!inspection.has_draft);
        remote(&mut editor, Operation::Delete(id)).unwrap();
        remote(&mut editor, Operation::Undo).unwrap();
        assert_eq!(editor.history.scene().node(id).unwrap().name(), "edited");
    }

    #[test]
    fn remote_busy_and_stale_denials_preserve_human_state() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let before = editor.history.scene().clone();
        let camera = editor.camera.eye();
        for draft in ["2", "nan"] {
            editor.channels[0] = draft.into();
            assert!(matches!(
                remote(&mut editor, Operation::Delete(id)),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(editor.history.scene(), &before);
            assert_eq!(editor.channels[0], draft);
            assert_eq!(editor.selected, Some(id));
            assert_eq!(editor.camera.eye(), camera);
            assert!(editor.history.can_undo());
            assert!(!editor.history.can_redo());
        }
        let old = Uuid::new_v4();
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(old),
                    operation: Operation::Undo,
                },
                &egui::Context::default()
            ),
            Err(OperationError::StaleScene)
        ));
        assert_eq!(editor.channels[0], "nan");
        editor.sync_fields();
        let ctx = egui::Context::default();
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("node_name")));
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(editor.scene_token),
                    operation: Operation::Delete(id),
                },
                &ctx
            ),
            Err(OperationError::BusyEditing)
        ));
        editor.request_new();
        assert!(matches!(
            remote(&mut editor, Operation::Undo),
            Err(OperationError::BusyEditing)
        ));
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection")
        };
        assert!(inspection.modal);
        assert_eq!(editor.history.scene(), &before);
    }

    #[test]
    fn remote_errors_and_noop_do_not_change_selection_or_history() {
        let mut editor = Editor::default();
        assert!(matches!(
            remote(&mut editor, Operation::Undo).unwrap(),
            OperationReply::Unchanged
        ));
        assert!(matches!(
            remote(&mut editor, Operation::FrameSelected),
            Err(OperationError::NoSelection)
        ));
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let before = editor.history.scene().clone();
        let missing = NodeId::from_decimal("999").unwrap();
        assert!(matches!(
            remote(
                &mut editor,
                Operation::Select {
                    node: Some(missing),
                    instance_path: None
                }
            ),
            Err(OperationError::Scene(_))
        ));
        assert!(matches!(
            remote(
                &mut editor,
                Operation::Rename {
                    id,
                    name: "".into()
                }
            ),
            Err(OperationError::Scene(_))
        ));
        let transform = TransformInput {
            scale: [f64::INFINITY; 3],
            translation: [0.0; 3],
            rotation_degrees: [0.0; 3],
        };
        assert!(matches!(
            remote(&mut editor, Operation::SetTransform { id, transform }),
            Err(OperationError::Scene(_))
        ));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.selected, Some(id));
        editor.history_step(false);
        assert!(editor.history.scene().nodes().is_empty());
    }

    #[test]
    fn remote_guard_distinguishes_rendered_buttons_from_name_and_channel_text_focus() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            editor.render(ui);
            ui.button("Focusable native control").request_focus();
        });
        output.textures_delta.clear();
        assert!(ctx.memory(|memory| memory.focused().is_some()));
        let OperationReply::Scene { inspection } = editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: None,
                    operation: Operation::Inspect,
                },
                &ctx,
            )
            .unwrap()
        else {
            panic!("expected inspection")
        };
        assert!(!inspection.text_focused);
        editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(editor.scene_token),
                    operation: Operation::Rename {
                        id,
                        name: "permitted".into(),
                    },
                },
                &ctx,
            )
            .unwrap();
        assert_eq!(editor.history.scene().node(id).unwrap().name(), "permitted");
        let before = editor.history.scene().clone();
        for text_id in [name_edit_id(), channel_edit_id(0)] {
            ctx.memory_mut(|memory| memory.request_focus(text_id));
            frame(&mut editor, &ctx, vec![]);
            assert_eq!(ctx.memory(|memory| memory.focused()), Some(text_id));
            let OperationReply::Scene { inspection } = editor
                .remote_operation(
                    RemoteRequest {
                        expected_scene_token: None,
                        operation: Operation::Inspect,
                    },
                    &ctx,
                )
                .unwrap()
            else {
                panic!("expected inspection")
            };
            assert!(inspection.text_focused);
            assert!(matches!(
                editor.remote_operation(
                    RemoteRequest {
                        expected_scene_token: Some(editor.scene_token),
                        operation: Operation::Undo,
                    },
                    &ctx
                ),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(editor.history.scene(), &before);
        }
    }

    #[test]
    fn typed_extrusion_uses_native_history_and_preserves_pending_edits_on_error() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let shape = editor.history.scene().shapes()[0].id();
        editor.channels[0] = "2".into();
        let target = FaceTarget {
            transform: id,
            shape,
            path: vec![
                editor.history.scene().node(id).unwrap().transform_id(),
                shape,
            ],
            face_index: 0,
        };
        editor.native_operation(Operation::ExtrudeFace {
            target: target.clone(),
            distance: 0.0,
        });
        assert!(!editor.error);
        assert_eq!(
            editor
                .history
                .scene()
                .shape(shape)
                .unwrap()
                .mesh()
                .point_count(),
            12
        );
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            2.0
        );
        editor.history_step(false);
        assert_eq!(
            editor
                .history
                .scene()
                .shape(shape)
                .unwrap()
                .mesh()
                .point_count(),
            8
        );
        let before = editor.history.scene().clone();
        editor.channels[0] = "nan".into();
        editor.native_operation(Operation::ExtrudeFace {
            target: target.clone(),
            distance: 1.0,
        });
        assert!(editor.error);
        assert_eq!(editor.channels[0], "nan");
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        editor.sync_fields();
        editor.native_operation(Operation::ExtrudeFace {
            target: target.clone(),
            distance: f64::NAN,
        });
        assert!(editor.error);
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        editor.native_operation(Operation::ExtrudeFace {
            target: FaceTarget {
                face_index: 99,
                ..target
            },
            distance: 1.0,
        });
        assert!(editor.error);
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        editor.history_step(true);
        assert_eq!(
            editor
                .history
                .scene()
                .shape(shape)
                .unwrap()
                .mesh()
                .point_count(),
            12
        );
    }

    fn key(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn frame(
        editor: &mut Editor,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| editor.render(ui),
        );
        output.textures_delta.clear();
        output
    }

    fn face_viewport_rect(output: &egui::FullOutput) -> egui::Rect {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_gray(31) => Some(rect.rect),
                _ => None,
            })
            .max_by(|a, b| a.area().total_cmp(&b.area()))
            .unwrap()
    }

    fn face_text_position(output: &egui::FullOutput, text: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text_shape) if text_shape.galley.job.text == text => {
                    Some(text_shape.pos + text_shape.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing actual text: {text}; rendered labels: {:?}",
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                )
            })
    }

    fn face_pointer_click(
        editor: &mut Editor,
        ctx: &egui::Context,
        position: egui::Pos2,
        modifiers: egui::Modifiers,
        release_keys: Vec<egui::Event>,
    ) -> egui::FullOutput {
        frame(
            editor,
            ctx,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(position),
            ],
        );
        for pressed in [true, false] {
            let mut events = vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers,
                },
            ];
            if !pressed {
                events.extend(release_keys.iter().cloned());
            }
            frame(editor, ctx, events);
        }
        frame(editor, ctx, vec![])
    }

    #[test]
    fn face_native_keys_coalesce_before_same_frame_click_and_preserve_authoring_state() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.camera =
            Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![]);
        let center = face_viewport_rect(&output).center();
        face_pointer_click(
            &mut editor,
            &ctx,
            center,
            egui::Modifiers::NONE,
            vec![key(Key::F8), key(Key::F11), key(Key::F11)],
        );
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 1);
        let target = editor.remembered_face.clone();
        frame(&mut editor, &ctx, vec![key(Key::F8)]);
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.remembered_face, target);
        frame(&mut editor, &ctx, vec![key(Key::F11)]);
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.remembered_face, target);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        assert!(editor.is_dirty());
        editor.history_step(false);
        assert!(editor.history.scene().nodes().is_empty());
    }

    #[test]
    fn face_native_modified_click_and_delete_never_fall_back_to_object_mutation() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.camera =
            Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![key(Key::F11)]);
        let rect = face_viewport_rect(&output);
        face_pointer_click(
            &mut editor,
            &ctx,
            rect.center(),
            egui::Modifiers::NONE,
            vec![],
        );
        let baseline = (
            editor.history.scene().clone(),
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token,
        );
        for modifiers in [egui::Modifiers::SHIFT, egui::Modifiers::CTRL] {
            face_pointer_click(
                &mut editor,
                &ctx,
                rect.left_top() + egui::vec2(20.0, 20.0),
                modifiers,
                vec![],
            );
            assert!(editor.error && editor.status.contains("unsupported"));
            assert_eq!(
                (
                    editor.history.scene().clone(),
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone(),
                    editor.camera.clone(),
                    editor.scene_token
                ),
                baseline
            );
        }
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                key(Key::Delete),
            ],
        );
        assert!(editor.error && editor.status.contains("Face deletion is unsupported"));
        assert_eq!(editor.history.scene(), &baseline.0);
        assert_eq!(editor.remembered_face, baseline.3);
        face_pointer_click(
            &mut editor,
            &ctx,
            rect.left_top() + egui::vec2(20.0, 20.0),
            egui::Modifiers::NONE,
            vec![],
        );
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.selected, baseline.1);
        assert_eq!(editor.selected_path, baseline.2);
        editor
            .remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(editor.scene_token),
                    operation: Operation::Delete(baseline.1.unwrap()),
                },
                &ctx,
            )
            .unwrap();
        assert!(editor.history.scene().nodes().is_empty());
    }

    #[test]
    fn face_native_mode_waits_for_current_text_and_rejects_invalid_drafts() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(name_edit_id()));
        frame(&mut editor, &ctx, vec![]);
        frame(
            &mut editor,
            &ctx,
            vec![egui::Event::Text("draft".into()), key(Key::F11)],
        );
        assert!(editor.has_draft());
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.history.scene(), &before);
        frame(&mut editor, &ctx, vec![key(Key::Escape)]);
        editor.channels[0] = "nan".into();
        editor.channel_intents[0] = true;
        frame(&mut editor, &ctx, vec![key(Key::F11)]);
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.channels[0], "nan");
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.error);
    }

    #[test]
    fn face_native_closing_editor_refuses_mode_intents_without_selection_changes() {
        let mut editor = Editor::default();
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        editor.close_document().unwrap();
        assert!(editor.files.is_closing());
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![key(Key::F11)]);
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert_eq!(editor.selected, None);
        assert_eq!(editor.remembered_face, None);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
    }

    #[test]
    fn face_native_pending_mode_prevents_keyboard_and_channel_object_delete() {
        for button in [false, true] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let before = editor.history.scene().clone();
            let token = editor.scene_token;
            let ctx = egui::Context::default();
            let output = frame(&mut editor, &ctx, vec![]);
            if button {
                let position = face_text_position(&output, "Delete selected");
                face_pointer_click(
                    &mut editor,
                    &ctx,
                    position,
                    egui::Modifiers::NONE,
                    vec![key(Key::F11)],
                );
            } else {
                frame(&mut editor, &ctx, vec![key(Key::F11), key(Key::Delete)]);
            }
            assert_eq!(editor.selection_mode, SelectionMode::Face);
            assert_eq!(editor.history.scene(), &before, "button={button}");
            assert_eq!(editor.scene_token, token);
            assert!(editor.error && editor.status.contains("Face deletion is unsupported"));
        }
    }

    #[test]
    fn face_native_edit_menu_delete_keeps_face_mode_unsupported_and_object_mode_working() {
        for face_mode in [false, true] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let before = editor.history.scene().clone();
            let ctx = egui::Context::default();
            let output = frame(
                &mut editor,
                &ctx,
                if face_mode {
                    vec![key(Key::F11)]
                } else {
                    vec![]
                },
            );
            let edit = face_text_position(&output, "Edit");
            let output = face_pointer_click(&mut editor, &ctx, edit, egui::Modifiers::NONE, vec![]);
            let delete = face_text_position(&output, "Delete");
            let selection = (
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.camera.clone(),
                editor.scene_token,
            );
            face_pointer_click(&mut editor, &ctx, delete, egui::Modifiers::NONE, vec![]);
            if face_mode {
                assert_eq!(editor.history.scene(), &before);
                assert_eq!(
                    (
                        editor.selection_mode,
                        editor.selected,
                        editor.selected_path.clone(),
                        editor.remembered_face.clone(),
                        editor.camera.clone(),
                        editor.scene_token
                    ),
                    selection
                );
            } else {
                assert!(editor.history.scene().nodes().is_empty());
                editor.history_step(false);
                assert_eq!(editor.history.scene(), &before);
            }
        }
    }

    #[test]
    fn face_native_event_modifiers_survive_later_modifier_changes() {
        for (event_modifiers, final_modifiers, expected) in [
            (
                egui::Modifiers::SHIFT,
                egui::Modifiers::NONE,
                SelectionMode::Object,
            ),
            (
                egui::Modifiers::NONE,
                egui::Modifiers::SHIFT,
                SelectionMode::Face,
            ),
        ] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let before = editor.history.scene().clone();
            let ctx = egui::Context::default();
            frame(&mut editor, &ctx, vec![]);
            frame(
                &mut editor,
                &ctx,
                vec![
                    egui::Event::Key {
                        key: Key::F11,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: event_modifiers,
                    },
                    egui::Event::ModifiersChanged(final_modifiers),
                ],
            );
            assert_eq!(editor.selection_mode, expected);
            assert_eq!(editor.history.scene(), &before);
        }
    }

    #[test]
    fn face_native_modal_cancel_does_not_replay_blocked_mode_intent() {
        for (button, open) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut editor = Editor::default();
            editor.create(Primitive::Cube);
            let ctx = egui::Context::default();
            frame(&mut editor, &ctx, vec![]);
            let directory = tempfile::tempdir().unwrap();
            if open {
                let path = directory.path().join("replacement.aztec");
                let mut candidate = azteccraft::scene::Scene::default();
                candidate.create(Primitive::Plane, "replacement").unwrap();
                azteccraft::document::save_new(&path, &candidate).unwrap();
                editor
                    .apply_operation(Operation::OpenScene { path })
                    .unwrap();
            } else {
                editor.new_document().unwrap();
            }
            frame(&mut editor, &ctx, vec![]);
            let output = frame(&mut editor, &ctx, vec![]);
            assert!(editor.files.is_modal());
            let baseline = (
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.history.scene().clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.is_dirty(),
                editor.history.can_undo(),
                editor.history.can_redo(),
            );
            let draft = (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents,
            );
            if button {
                let cancel = face_text_position(&output, "Cancel");
                face_pointer_click(
                    &mut editor,
                    &ctx,
                    cancel,
                    egui::Modifiers::NONE,
                    vec![key(Key::F11)],
                );
            } else {
                frame(&mut editor, &ctx, vec![key(Key::Escape), key(Key::F11)]);
            }
            assert!(!editor.files.is_modal());
            assert_eq!(
                (
                    editor.selection_mode,
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone(),
                    editor.history.scene().clone(),
                    editor.camera.clone(),
                    editor.scene_token,
                    editor.is_dirty(),
                    editor.history.can_undo(),
                    editor.history.can_redo()
                ),
                baseline
            );
            assert_eq!(
                (
                    editor.name.clone(),
                    editor.channels.clone(),
                    editor.name_intent,
                    editor.channel_intents
                ),
                draft
            );
        }
    }

    #[test]
    fn face_native_ambiguous_primary_frame_preserves_selection_scene_and_draft() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.camera =
            Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![key(Key::F11)]);
        let point = face_viewport_rect(&output).center();
        face_pointer_click(&mut editor, &ctx, point, egui::Modifiers::NONE, vec![]);
        assert_eq!(editor.remembered_face.as_ref().unwrap().face_index, 1);
        editor.native_operation(Operation::Rename {
            id: editor.selected.unwrap(),
            name: "redo-payload".into(),
        });
        editor.history_step(false);
        assert!(editor.history.can_redo());
        editor.name = "pending-human-name".into();
        editor.name_intent = true;
        let baseline = (
            editor.selection_mode,
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
            editor.history.scene().clone(),
            editor.camera.clone(),
            editor.scene_token,
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.is_dirty(),
        );
        let draft = (
            editor.name.clone(),
            editor.channels.clone(),
            editor.name_intent,
            editor.channel_intents,
        );
        let end = point + egui::vec2(36.0, 0.0);
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::SHIFT),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::SHIFT,
                },
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(end),
                egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(editor.error && editor.status.contains("Ambiguous coalesced primary input"));
        assert_eq!(
            (
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.history.scene().clone(),
                editor.camera.clone(),
                editor.scene_token,
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.is_dirty()
            ),
            baseline
        );
        assert_eq!(
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents
            ),
            draft
        );
    }

    #[test]
    fn face_native_menu_explicit_face_beats_toggle_and_modal_replacement_drops_intent() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![]);
        let select = face_text_position(&output, "Select");
        let output = face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
        let components = face_text_position(&output, "Components");
        let output =
            face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
        let face = face_text_position(&output, "Face   F11");
        face_pointer_click(
            &mut editor,
            &ctx,
            face,
            egui::Modifiers::NONE,
            vec![key(Key::F8)],
        );
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.history.scene(), &before);
        editor
            .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
            .unwrap();
        editor.new_document().unwrap();
        let token = editor.scene_token;
        frame(&mut editor, &ctx, vec![key(Key::F11)]);
        assert!(editor.files.is_modal());
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        let output = frame(&mut editor, &ctx, vec![]);
        let discard = face_text_position(&output, "Discard");
        face_pointer_click(
            &mut editor,
            &ctx,
            discard,
            egui::Modifiers::NONE,
            vec![key(Key::F11)],
        );
        assert_ne!(editor.scene_token, token);
        assert!(!editor.files.is_modal());
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert!(editor.history.scene().nodes().is_empty());
    }

    #[test]
    fn face_native_popup_intercepts_combined_menu_activation_and_viewport_click() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.camera =
            Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap();
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        let output = frame(&mut editor, &ctx, vec![]);
        let center = face_viewport_rect(&output).center();
        let select = face_text_position(&output, "Select");
        let output = face_pointer_click(&mut editor, &ctx, select, egui::Modifiers::NONE, vec![]);
        let components = face_text_position(&output, "Components");
        let output =
            face_pointer_click(&mut editor, &ctx, components, egui::Modifiers::NONE, vec![]);
        let face = face_text_position(&output, "Face   F11");
        let mut focused = false;
        for _ in 0..32 {
            frame(&mut editor, &ctx, vec![key(Key::Tab)]);
            focused = ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .is_some_and(|response| response.rect.contains(face));
            if focused {
                break;
            }
        }
        assert!(
            focused,
            "actual menu Face item must receive keyboard focus before activation"
        );
        assert_eq!(editor.selection_mode, SelectionMode::Object);
        assert!(egui::Popup::is_any_open(&ctx));
        let selection = (
            editor.selection_mode,
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
            editor.camera.clone(),
            editor.scene_token,
        );
        let history = (
            editor.history.can_undo(),
            editor.history.can_redo(),
            editor.is_dirty(),
        );
        let draft = (
            editor.name.clone(),
            editor.channels.clone(),
            editor.name_intent,
            editor.channel_intents,
        );
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::NONE),
                key(Key::Enter),
                key(Key::F8),
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(
            (
                editor.selection_mode,
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone(),
                editor.camera.clone(),
                editor.scene_token
            ),
            selection
        );
        assert_eq!(
            (
                editor.history.can_undo(),
                editor.history.can_redo(),
                editor.is_dirty()
            ),
            history
        );
        assert_eq!(
            (
                editor.name.clone(),
                editor.channels.clone(),
                editor.name_intent,
                editor.channel_intents
            ),
            draft
        );
        assert_eq!(editor.history.scene(), &before);
    }

    #[test]
    fn real_escape_event_cancels_channel_and_name_after_egui_clears_focus() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(("channel", 0_usize))));
        frame(&mut editor, &ctx, vec![]);
        editor.channels[0] = "3".into();
        frame(&mut editor, &ctx, vec![key(Key::Escape)]);
        assert_eq!(editor.channels[0], "0");
        assert_eq!(editor.history.scene(), &before);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("node_name")));
        frame(&mut editor, &ctx, vec![]);
        editor.name = "discarded".into();
        frame(&mut editor, &ctx, vec![key(Key::Escape)]);
        assert_eq!(editor.history.scene().node(id).unwrap().name(), "pCube1");
        editor.history_step(false);
        assert!(editor.history.scene().nodes().is_empty());
    }

    #[test]
    fn real_grid_button_dispatches_current_state_and_disables_for_modal_and_close() {
        for blocked in [0, 1, 2] {
            let context = egui::Context::default();
            let mut editor = Editor {
                renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
                ..Editor::default()
            };
            if blocked == 1 {
                editor.create(Primitive::Cube);
                editor.new_document().unwrap();
            } else if blocked == 2 {
                editor.close_document().unwrap();
            }
            let scene = editor.history.scene().clone();
            let token = editor.scene_token;
            let mut position = None;
            let mut run = |editor: &mut Editor, events| {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(720.0, 480.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| editor.viewport(ui, token, None, false, true),
                );
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.job.text == "Grid"
                    {
                        position = Some(text.pos + text.galley.size() * 0.5);
                    }
                }
                output.textures_delta.clear();
                position
            };
            let point = run(&mut editor, vec![]).unwrap();
            for expected in [blocked != 0, true] {
                run(&mut editor, vec![egui::Event::PointerMoved(point)]);
                for pressed in [true, false] {
                    run(
                        &mut editor,
                        vec![egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        }],
                    );
                }
                assert_eq!(editor.grid_visible, expected);
                assert_eq!(editor.inspect(false).unwrap().grid_visible, expected);
                assert_eq!(editor.scene_token, token);
                assert_eq!(editor.history.scene(), &scene);
            }
        }
    }

    #[test]
    fn real_channel_text_keeps_f64_precision_and_untouched_slots_in_one_undo() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        editor
            .history
            .execute(Command::SetTransform {
                id,
                transform: NativeTransform {
                    translation: [0.0, 1.0000000000000004, 0.0],
                    ..Default::default()
                },
            })
            .unwrap();
        editor.sync_fields();
        let before = editor.history.scene().clone();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(channel_edit_id(0)));
        frame(&mut editor, &ctx, vec![]);
        let command = egui::Modifiers::COMMAND;
        frame(
            &mut editor,
            &ctx,
            vec![
                egui::Event::ModifiersChanged(command),
                egui::Event::Key {
                    key: Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: command,
                },
                egui::Event::Text("1.0000000000000002".into()),
                key(Key::Enter),
            ],
        );
        let translation = editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation;
        assert_eq!(translation[0].to_bits(), 1.0000000000000002_f64.to_bits());
        assert_eq!(translation[1].to_bits(), 1.0000000000000004_f64.to_bits());
        assert!(editor.history.undo());
        assert_eq!(editor.history.scene(), &before);
    }

    #[test]
    fn viewport_click_commits_draft_before_picking_its_new_geometry() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        editor.camera = Camera::default();
        let ctx = egui::Context::default();
        let mut viewport_rect = egui::Rect::NOTHING;
        let run = |editor: &mut Editor, events, rect: &mut egui::Rect| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.viewport(ui, editor.scene_token, None, false, true);
                    *rect = ui.min_rect();
                },
            );
            output.textures_delta.clear();
        };
        run(&mut editor, vec![], &mut viewport_rect);
        let new_center = glam::Vec3::new(2.0, 0.0, 0.0);
        let ndc = editor
            .camera
            .project(new_center, editor.aspect)
            .unwrap()
            .unwrap();
        let width = viewport_rect.width();
        let height = width / editor.aspect;
        let position = egui::pos2(
            viewport_rect.left() + (ndc.x + 1.0) * 0.5 * width,
            viewport_rect.bottom() - (ndc.y + 1.0) * 0.5 * height,
        );
        run(
            &mut editor,
            vec![egui::Event::PointerMoved(position)],
            &mut viewport_rect,
        );
        editor.channels[0] = "2".into();
        editor.channel_intents[0] = true;
        for pressed in [true, false] {
            run(
                &mut editor,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
                &mut viewport_rect,
            );
        }
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation[0],
            2.0
        );
        assert_eq!(
            editor.selected,
            Some(id),
            "fresh geometry must be hit at the new location"
        );
        assert!(!editor.has_draft());
    }

    #[test]
    fn enter_commits_once_and_selection_commits_draft_to_previous_object() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let old = editor.selected.unwrap();
        editor.create(Primitive::Plane);
        let next = editor.selected.unwrap();
        editor.select(Some(old));
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(("channel", 0_usize))));
        frame(&mut editor, &ctx, vec![]);
        editor.channels[0] = "2".into();
        frame(&mut editor, &ctx, vec![key(Key::Enter)]);
        assert_eq!(
            editor
                .history
                .scene()
                .node(old)
                .unwrap()
                .transform()
                .translation[0],
            2.0
        );
        editor.history_step(false);
        assert_eq!(
            editor
                .history
                .scene()
                .node(old)
                .unwrap()
                .transform()
                .translation[0],
            0.0
        );
        editor.channels[0] = "4".into();
        editor.select(Some(next));
        assert_eq!(
            editor
                .history
                .scene()
                .node(old)
                .unwrap()
                .transform()
                .translation[0],
            4.0
        );
        assert_eq!(
            editor.history.scene().node(next).unwrap().transform(),
            NativeTransform::default()
        );
        assert_eq!(editor.channels[0], "0");
    }

    #[test]
    fn creation_commits_old_draft_and_invalid_draft_blocks_selection_and_actions() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let old = editor.selected.unwrap();
        editor.channels[1] = "5".into();
        editor.create(Primitive::Plane);
        let next = editor.selected.unwrap();
        assert_eq!(
            editor
                .history
                .scene()
                .node(old)
                .unwrap()
                .transform()
                .translation[1],
            5.0
        );
        assert_eq!(
            editor.history.scene().node(next).unwrap().transform(),
            NativeTransform::default()
        );
        editor.channels[0] = "nan".into();
        let before = editor.history.scene().clone();
        editor.select(Some(old));
        assert_eq!(editor.selected, Some(next));
        editor.create(Primitive::Cube);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.channels[0], "nan");
        assert!(editor.error);
    }

    #[test]
    fn create_allocates_name_after_committing_pending_rename() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let old = editor.selected.unwrap();
        editor.name = "pCube2".into();
        editor.create(Primitive::Cube);
        let created = editor.selected.unwrap();
        assert_ne!(old, created);
        assert_eq!(editor.history.scene().node(old).unwrap().name(), "pCube2");
        assert_eq!(
            editor.history.scene().node(created).unwrap().name(),
            "pCube1"
        );
    }
}

#[cfg(all(test, feature = "mcp"))]
#[path = "mcp_editor_tests.rs"]
mod mcp_editor_tests;

#[cfg(test)]
#[path = "gpu_viewport_tests.rs"]
mod gpu_viewport_tests;
