#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{
    CameraInspection, NodeInspection, RemoteRequest, SceneInspection, ShapeInspection,
    TransformInput, UnitsInspection,
};
use crate::editor_operations::{Operation, OperationError, OperationReply};
#[cfg(test)]
use azteccraft::native_scene::FaceTarget;
#[cfg(any(test, feature = "mcp"))]
use azteccraft::native_scene::{LinearUnit, NativeTransform};
use azteccraft::{
    history::{Command, EditOutcome, History},
    native_scene::{AngularUnit, NativeId},
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

pub struct Editor {
    #[cfg(feature = "mcp")]
    mcp: Option<crate::mcp::UiBridge>,
    scene_token: Uuid,
    history: History,
    selected: Option<NodeId>,
    selected_path: Vec<NativeId>,
    camera: Camera,
    renderer: Option<crate::viewport_renderer::ViewportRenderer>,
    viewport_error: Option<crate::viewport_renderer::ViewportError>,
    name: String,
    channels: [String; 9],
    channel_intents: [bool; 9],
    name_intent: bool,
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
            camera: Camera::default(),
            renderer: None,
            viewport_error: None,
            name: String::new(),
            channels: Default::default(),
            channel_intents: [false; 9],
            name_intent: false,
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
        self.channel_intents = [false; 9];
        self.name_intent = false;
        let Some(node) = self
            .selected
            .and_then(|id| self.history.scene().node(id).ok())
        else {
            self.selected = None;
            self.selected_path.clear();
            self.name.clear();
            self.channels = Default::default();
            return;
        };
        self.name = node.name().into();
        let values = node.transform().values();
        self.channels = values.map(|value| value.to_string());
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
        let status = match operation {
            Operation::Select { .. } | Operation::FrameSelected => None,
            Operation::Orbit { .. } | Operation::Pan { .. } | Operation::Dolly { .. } => None,
            Operation::Undo => Some("Undo"),
            Operation::Redo => Some("Redo"),
            Operation::OpenScene { .. } | Operation::NewScene => Some("Scene ready"),
            Operation::RequestClose => None,
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

    #[cfg(any(test, feature = "mcp"))]
    pub(crate) fn remote_operation(
        &mut self,
        request: RemoteRequest,
        ctx: &egui::Context,
    ) -> Result<OperationReply, OperationError> {
        let focused = ctx.memory(|memory| {
            memory.focused().is_some_and(|id| {
                id == name_edit_id() || (0..9).any(|index| id == channel_edit_id(index))
            })
        });
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
        self.apply_operation(request.operation)
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
        if path_bytes > crate::editor_operations::MAX_RESULT_BYTES / 2 {
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
                            let id = edge.child();
                            (self.history.scene().shape(id).is_ok()
                                || self
                                    .history
                                    .scene()
                                    .curves()
                                    .iter()
                                    .any(|curve| curve.id() == id))
                            .then(|| id.to_decimal())
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
        let prepared_view = (!self.history.scene().curves().is_empty())
            .then(|| ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024));
        let gpu_healthy = self.renderer.is_some()
            && viewport_error != Some(crate::viewport_renderer::ViewportError::BackendFailed);
        Ok(SceneInspection {
            scene_token: self.scene_token.to_string(),
            nodes,
            document_origin: self.files.document_origin(),
            retained_source_bytes: self.history.scene().retained_source_bytes(),
            retained_source_records: self.history.scene().retained_source_records(),
            read_only_reason: self
                .history
                .scene()
                .authoring_read_only_reason()
                .map(|reason| reason.to_string()),
            curves: self
                .history
                .scene()
                .curves()
                .iter()
                .map(|curve| {
                    Ok(crate::editor_operations::CurveInspection {
                        id: curve.id().to_decimal(),
                        name: curve.name().into(),
                        degree: curve.degree(),
                        form: curve.form(),
                        control_vertices: curve.control_vertices().len(),
                        knots: curve.knots().len(),
                        domain: curve.domain().map_err(OperationError::Scene)?,
                        read_only: self.history.scene().authoring_read_only_reason().is_some(),
                        draw_available: prepared_view.as_ref().is_some_and(|prepared| {
                            prepared.as_ref().is_ok_and(|view| {
                                view.curve_instances()
                                    .iter()
                                    .any(|instance| instance.shape() == curve.id())
                            })
                        }),
                    })
                })
                .collect::<Result<Vec<_>, OperationError>>()?,
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
                    })
                })
                .collect::<Result<Vec<_>, OperationError>>()?,
            units: UnitsInspection {
                linear: linear.into(),
                angular: angular.into(),
                time: units.time.clone(),
            },
            save_available: false,
            closing: self.files.is_closing(),
            viewport_backend: if self.renderer.is_some() {
                "wgpu_geometry_preview"
            } else {
                "uninitialized"
            },
            viewport_error,
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
        if self.files.is_closing() {
            return Err(OperationError::BusyEditing);
        }
        match &operation {
            Operation::OpenScene { path } => {
                let text = path.to_str().ok_or_else(|| {
                    OperationError::InvalidInput("Scene path must be UTF-8".into())
                })?;
                if text.is_empty() || text.len() > 4096 || text.contains('\0') {
                    return Err(OperationError::InvalidInput(
                        "Scene path must contain 1 to 4096 bytes and no NUL".into(),
                    ));
                }
                return self.open_candidate(path);
            }
            Operation::NewScene => return self.new_document(),
            Operation::RequestClose => return self.close_document(),
            Operation::ResolveFileAction {
                pending_id,
                decision,
            } => return self.resolve_file_action(*pending_id, *decision),
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
        if !self.commit_pending() {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        let command = match operation {
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
            Operation::Select { node, .. } => {
                let path = selected_path.unwrap_or_default();
                let changed = self.selected != node || self.selected_path != path;
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
                let id = self.selected.ok_or(OperationError::NoSelection)?;
                let path = self.selection_path(
                    id,
                    (!self.selected_path.is_empty()).then(|| self.selected_path.clone()),
                )?;
                let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
                    .map_err(|error| OperationError::File(error.to_string()))?;
                let before = self.camera.clone();
                self.camera
                    .frame_view(&view, Some(&path), self.aspect)
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
        let (undo, redo, delete, frame) = ctx.input(|i| {
            (
                i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(Key::Z),
                i.modifiers.is_none() && i.key_pressed(Key::Delete),
                i.modifiers.is_none() && i.key_pressed(Key::F),
            )
        });
        if undo {
            self.history_step(false);
        }
        if redo {
            self.history_step(true);
        }
        if delete && let Some(id) = self.selected {
            self.native_operation(Operation::Delete(id));
        }
        if frame {
            self.frame_selected();
        }
    }

    fn commit_pending(&mut self) -> bool {
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

    fn menus(&mut self, ui: &mut egui::Ui) {
        let read_only_reason = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string());
        let authoring = read_only_reason.is_none();
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
                if let Some(reason) = read_only_reason.as_deref() {
                    ui.add_enabled(false, egui::Button::new("Save As..."))
                        .on_disabled_hover_text(reason);
                } else {
                    unsupported(ui, "Save As...");
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
                        authoring && self.selected.is_some(),
                        egui::Button::new("Delete"),
                    )
                    .clicked()
                {
                    if let Some(id) = self.selected {
                        self.native_operation(Operation::Delete(id));
                    }
                    ui.close();
                }
            });
            ui.menu_button("Create", |ui| {
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon cube"))
                    .clicked()
                {
                    self.create(Primitive::Cube);
                    ui.close();
                }
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon plane"))
                    .clicked()
                {
                    self.create(Primitive::Plane);
                    ui.close();
                }
                unsupported(ui, "Other primitives");
            });
            for label in ["Select", "Modify", "Mesh", "UV", "Windows", "Render"] {
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
            if crate::tool_icons::button(ui, "Cube", crate::tool_icons::Icon::Cube, authoring)
                .clicked()
            {
                self.create(Primitive::Cube);
            }
            if crate::tool_icons::button(ui, "Plane", crate::tool_icons::Icon::Plane, authoring)
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
            unsupported(ui, "Extrude");
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

    fn channels(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Channel Box").strong());
        let read_only_reason = self.history.scene().authoring_read_only_reason();
        let authoring = read_only_reason.is_none();
        if let Some(reason) = read_only_reason {
            ui.weak(reason.to_string());
        }
        if !self.history.scene().curves().is_empty() {
            ui.weak("Curves: select in the Outliner and frame with F. Viewport curve picking and CV editing are unavailable.");
        }
        self.appearance_summary(ui);
        ui.separator();
        let Some(id) = self.selected else {
            ui.weak("Select an object to edit its channels.");
            return;
        };
        ui.label("Object name");
        let response = ui.add_enabled(
            authoring,
            egui::TextEdit::singleline(&mut self.name)
                .interactive(authoring)
                .id(name_edit_id()),
        );
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
                    let response = ui.add_enabled(
                        authoring,
                        egui::TextEdit::singleline(value)
                            .interactive(authoring)
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
        ui.separator();
        if ui
            .add_enabled(authoring, egui::Button::new("Delete selected"))
            .clicked()
        {
            self.native_operation(Operation::Delete(id));
        }
        ui.separator();
        ui.weak("Inputs / construction history");
        unsupported(ui, "Not implemented yet");
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
        self.handle_close(ui.ctx());
        if !self.files.is_modal() {
            self.cancel_on_escape(ui.ctx());
        }
        self.shortcuts(ui.ctx());
        egui::Panel::top("menu_shelf").show(ui, |ui| self.menus(ui));
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
                    self.channels(ui)
                });
            });
        egui::CentralPanel::default().show(ui, |ui| self.viewport(ui, frame_token));
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(self.document_title()));
        self.native_input_pending = false;
        #[cfg(feature = "mcp")]
        self.process_mcp(ui.ctx());
    }

    fn viewport(&mut self, ui: &mut egui::Ui, frame_token: Uuid) {
        ui.horizontal(|ui| {
            ui.label("persp");
            ui.separator();
            ui.weak("Alt + left: orbit | Middle: pan | Right / wheel: dolly");
        });
        let captured = crate::viewport_ui::capture(ui, &self.camera);
        if let Ok(Some(input)) = &captured {
            self.aspect = input.aspect();
        }
        self.file_modal(ui.ctx());
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
            let clickable = input.click_ndc().filter(|_| current_document);
            let navigation = if current_document {
                navigation
            } else {
                Vec::new()
            };
            if !self.files.is_modal() && (!navigation.is_empty() || clickable.is_some()) {
                if !self.commit_pending() {
                    return Err(OperationError::InvalidInput(self.status.clone()));
                }
                for operation in navigation {
                    self.apply_operation(operation)?;
                }
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
            let painted = input.paint(
                self.renderer.as_ref(),
                &self.camera,
                &view,
                self.selected.map(|_| self.selected_path.as_slice()),
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

fn name_edit_id() -> egui::Id {
    egui::Id::new("node_name")
}

fn channel_edit_id(index: usize) -> egui::Id {
    egui::Id::new(("channel", index))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod ascii_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_ascii.rs"
        ));
    }

    #[test]
    fn ascii_inspection_reports_canonical_curves_without_private_arrays_or_gpu_claims() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.ma");
        let source = ascii_fixture::specimen();
        std::fs::write(&path, &source).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let inspection = editor.inspect(false).unwrap();
        assert_eq!(
            inspection.document_origin,
            crate::editor_operations::DocumentOrigin::MayaAscii
        );
        assert_eq!(inspection.retained_source_bytes, source.len());
        assert_eq!(inspection.retained_source_records, 10);
        assert!(inspection.read_only_reason.unwrap().contains("read-only"));
        assert!(inspection.shapes.is_empty());
        assert_eq!(inspection.curves.len(), 1);
        let curve = &inspection.curves[0];
        assert_eq!(
            curve.id,
            editor.history.scene().curves()[0].id().to_decimal()
        );
        assert_eq!(
            inspection.nodes[0].shape_ids.as_slice(),
            std::slice::from_ref(&curve.id)
        );
        assert_eq!(curve.degree, 3);
        assert_eq!(curve.control_vertices, 4);
        assert_eq!(curve.knots, 6);
        assert_eq!(curve.domain, [0.0, 1.0]);
        assert!(curve.read_only && curve.draw_available);
        assert_eq!(inspection.viewport_backend, "uninitialized");
        assert!(!inspection.depth_complete && !inspection.clipping_complete);
        let value = serde_json::to_value(curve).unwrap();
        assert_eq!(value["form"], "open");
        assert!(value["control_vertices"].is_number() && value["knots"].is_number());
        assert!(!serde_json::to_string(&value).unwrap().contains("script"));

        for source in [
            String::from_utf8(source.clone())
                .unwrap()
                .replace("setAttr \".t\"", "setAttr \".v\" no; setAttr \".t\""),
            String::from_utf8(source)
                .unwrap()
                .replace("12 0 0", "100000000 0 0"),
        ] {
            let scene =
                azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
            let mut editor = Editor {
                history: History::new(scene),
                ..Default::default()
            };
            editor.viewport_error = None;
            let inspection = editor.inspect(false).unwrap();
            assert_eq!(inspection.curves.len(), 1);
            assert!(!inspection.curves[0].draw_available);
            assert_eq!(inspection.viewport_backend, "uninitialized");
            assert!(!inspection.depth_complete && !inspection.clipping_complete);
        }
    }

    #[test]
    fn ascii_channel_controls_are_disabled_and_show_the_shared_core_reason() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.ma");
        std::fs::write(&path, ascii_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let id = editor.history.scene().nodes()[0].id();
        editor.select(Some(id));
        let before = editor.history.scene().clone();
        let fields = (editor.name.clone(), editor.channels.clone());
        for text_id in [name_edit_id(), channel_edit_id(0)] {
            let context = egui::Context::default();
            context.memory_mut(|memory| memory.request_focus(text_id));
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420.0, 900.0),
                    )),
                    events: vec![
                        egui::Event::Text("must not become a draft".into()),
                        key(Key::Enter),
                    ],
                    ..Default::default()
                },
                |ui| editor.channels(ui),
            );
            output.textures_delta.clear();
            let reason = editor
                .history
                .scene()
                .authoring_read_only_reason()
                .unwrap()
                .to_string();
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text.contains(&reason))));
            assert_eq!((&editor.name, &editor.channels), (&fields.0, &fields.1));
            assert!(!context.memory(|memory| memory.has_focus(text_id)));
            assert!(!editor.has_draft());
            assert_eq!(editor.history.scene(), &before);
        }
    }

    #[test]
    fn ascii_outliner_selection_and_frame_use_actual_curve_owner_without_authoring() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.ma");
        std::fs::write(&path, ascii_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path })
            .unwrap();
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| editor.outliner(ui));
        output.textures_delta.clear();
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text.contains("◇  curve") => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        for pressed in [true, false] {
            let mut output = context.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| editor.outliner(ui),
            );
            output.textures_delta.clear();
        }
        assert_eq!(
            editor.selected,
            Some(editor.history.scene().nodes()[0].id())
        );
        editor
            .apply_operation(Operation::Pan {
                world_units: [20.0, 10.0],
            })
            .unwrap();
        editor.apply_operation(Operation::FrameSelected).unwrap();
        assert_eq!(editor.camera.target(), glam::Vec3::new(13.5, 0.0, 0.0));
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        assert!(!editor.is_dirty() && !editor.history.can_undo());
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
                let mut frame = context.run_ui(input, |ui| editor.channels(ui));
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
        assert_eq!(crate::editor_operations::TOOL_NAMES.len(), 18);
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

    fn frame(editor: &mut Editor, ctx: &egui::Context, events: Vec<egui::Event>) {
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
                    editor.viewport(ui, editor.scene_token);
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
