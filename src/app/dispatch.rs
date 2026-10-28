use super::*;
use azteccraft::history::{ManipulationFinish, ManipulationKindState};

/// Status-line feedback planned for one operation; see [`Editor::status_feedback`].
struct StatusFeedback {
    status: Option<&'static str>,
    step_label: Option<String>,
    activated: bool,
    /// Report `status` whatever the reply, as a native drag release or Esc does.
    fixed: bool,
}

impl Editor {
    /// Rejects component selection outside its mode, and unbounded target paths, before any mutation.
    fn require_component_selection(
        &self,
        mode: SelectionMode,
        path_len: Option<usize>,
    ) -> Result<(), OperationError> {
        let Some(noun) = mode.component_noun() else {
            return Err(OperationError::InvalidInput(
                "Selection mode has no component selection".into(),
            ));
        };
        if self.selection_mode != mode {
            return Err(OperationError::InvalidInput(format!(
                "{noun} selection requires {noun} mode"
            )));
        }
        if let Some(len) = path_len
            && !(2..=azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1).contains(&len)
        {
            return Err(OperationError::InvalidInput(format!(
                "{noun} selection requires a bounded full shape path"
            )));
        }
        Ok(())
    }
    /// Selects the instance that owns a validated component target (path ends with the shape).
    fn select_component_owner(
        &mut self,
        transform: NodeId,
        target_path: &[NativeId],
    ) -> Result<(), OperationError> {
        let owner = target_path.split_last().map_or(&[][..], |(_, owner)| owner);
        let mut path = Vec::new();
        path.try_reserve_exact(owner.len()).map_err(|_| {
            OperationError::Scene(azteccraft::native_scene::NativeError::ResourceLimit)
        })?;
        path.extend_from_slice(owner);
        self.selection.replace(TransformTarget {
            node: transform,
            path,
        });
        self.sync_fields();
        Ok(())
    }
    pub(super) fn native_operation(&mut self, operation: Operation) {
        let feedback = self.status_feedback(&operation);
        let result = self.apply_operation(operation);
        self.record_feedback(feedback, &result);
    }
    /// What the status line reports for an operation, decided before it runs (Undo/Redo labels
    /// change once it does). Native and remote callers share it, as Maya's Command Line feedback
    /// reports a command's result whatever issued it.
    fn status_feedback(&self, operation: &Operation) -> StatusFeedback {
        let activated = matches!(
            *operation,
            Operation::SetActiveTool(ActiveTool::Rotate | ActiveTool::Scale)
        );
        let step_label = match *operation {
            Operation::Undo => self.history.undo_label(),
            Operation::Redo => self.history.redo_label(),
            _ => None,
        };
        let status = match *operation {
            Operation::Select { .. }
            | Operation::ModifySelection { .. }
            | Operation::SelectByRule(_)
            | Operation::DeselectAll
            | Operation::SelectFace { .. }
            | Operation::SelectVertex { .. }
            | Operation::SelectEdge { .. }
            | Operation::SetSelectionMode(_)
            | Operation::FrameSelected { .. }
            | Operation::FrameAll
            | Operation::SetActiveTool(_)
            | Operation::SetToolSpace { .. }
            | Operation::SetToolHandle(_)
            | Operation::SetManipulatorSize(_)
            | Operation::SetGridVisible { .. }
            | Operation::SetShadingMode(_)
            | Operation::SetSidebarPanelVisible { .. }
            | Operation::SelectAttributeEditorTab { .. } => None,
            Operation::Orbit { .. }
            | Operation::Pan { .. }
            | Operation::Dolly { .. }
            | Operation::SetCameraPose { .. } => None,
            // Native drags report their own live preview, which a remote preview does not drive.
            Operation::BeginMove { .. }
            | Operation::BeginRotate { .. }
            | Operation::BeginScale { .. }
            | Operation::UpdateMove { .. }
            | Operation::UpdateManipulation { .. } => None,
            Operation::FinishMove {
                finish: ManipulationFinish::Cancel,
                ..
            } => Some("Edit cancelled"),
            Operation::FinishMove { .. } => Some(
                match self.history.active_manipulation().map(|active| active.kind) {
                    Some(ManipulationKindState::Rotate { .. }) => "Rotate committed",
                    Some(ManipulationKindState::Scale { .. }) => "Scale committed",
                    _ => "Move committed",
                },
            ),
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetMaterialDiffuse { .. } => Some("Material data edited"),
            Operation::Undo => Some("Undo"),
            Operation::Redo => Some("Redo"),
            Operation::OpenScene { .. } | Operation::NewScene => Some("Scene ready"),
            Operation::RequestClose => None,
            Operation::SaveScene
            | Operation::SaveSceneAs { .. }
            | Operation::ResolveFileAction { .. } => None,
            _ => Some("Scene edited"),
        };
        StatusFeedback {
            status,
            step_label,
            activated,
            fixed: matches!(*operation, Operation::FinishMove { .. }),
        }
    }
    fn record_feedback(
        &mut self,
        feedback: StatusFeedback,
        result: &Result<OperationReply, OperationError>,
    ) {
        let StatusFeedback {
            status,
            step_label,
            activated,
            fixed,
        } = feedback;
        match result {
            Ok(reply) => {
                if let (Some(status), true) = (status, fixed) {
                    self.status = status.into();
                    self.error = false;
                } else if let Some(status) = status {
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
                    if let (OperationReply::Changed, Some(label)) = (reply, step_label) {
                        self.status = format!("{status} {label}");
                    }
                    self.error = false;
                }
                if activated {
                    self.status = self.tool_help();
                    self.error = false;
                }
            }
            Err(error) => {
                self.set_error_status(error);
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
        if let Operation::Inspect { page } = &request.operation {
            return Ok(OperationReply::Scene {
                inspection: Box::new(self.inspect_page(focused, page.as_ref())?),
            });
        }
        if request.expected_scene_token != Some(self.scene_token) {
            return Err(OperationError::StaleScene);
        }
        if let Some(active) = self.history.active_manipulation()
            && (active.provenance != ManipulationProvenance::Remote
                || !matches!(
                    request.operation,
                    Operation::UpdateMove { .. }
                        | Operation::UpdateManipulation { .. }
                        | Operation::FinishMove { .. }
                ))
        {
            return Err(OperationError::BusyManipulation);
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
        // A remote success is reported on the status line like a native one. Refusals and
        // failures go back to the caller only, preserving the native editor baseline.
        let feedback = self.status_feedback(&request.operation);
        let result = self.apply_operation_guarded(
            ManipulationProvenance::Remote,
            request.operation,
            may_install,
        );
        if result.is_ok() {
            self.record_feedback(feedback, &result);
        }
        result
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn transform_input(
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
    pub(super) fn apply_operation(
        &mut self,
        operation: Operation,
    ) -> Result<OperationReply, OperationError> {
        self.apply_operation_guarded(ManipulationProvenance::Native, operation, &|| true)
    }
    pub(super) fn apply_operation_guarded(
        &mut self,
        provenance: ManipulationProvenance,
        operation: Operation,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(operation, Operation::Delete(_) | Operation::DeleteSelection)
        {
            return Err(OperationError::InvalidInput(format!(
                "{noun} deletion is unsupported; switch to Object or Hierarchy selection"
            )));
        }
        self.guard_move_dispatch(&operation, provenance)?;
        if self.files.is_closing() {
            return Err(OperationError::BusyEditing);
        }
        if matches!(
            operation,
            Operation::SetGridVisible { .. }
                | Operation::SetShadingMode(_)
                | Operation::SetSidebarPanelVisible { .. }
                | Operation::SelectAttributeEditorTab { .. }
        ) && self.files.is_modal()
        {
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
        if self.files.is_modal()
            && matches!(
                operation,
                Operation::SetActiveTool(_)
                    | Operation::SetToolSpace { .. }
                    | Operation::SetToolHandle(_)
                    | Operation::SetManipulatorSize(_)
                    | Operation::BeginMove { .. }
                    | Operation::BeginRotate { .. }
                    | Operation::BeginScale { .. }
                    | Operation::UpdateMove { .. }
                    | Operation::UpdateManipulation { .. }
                    | Operation::FinishMove { .. }
                    | Operation::Select { .. }
                    | Operation::ModifySelection { .. }
                    | Operation::SelectByRule(_)
                    | Operation::DeselectAll
                    | Operation::SetSelectionMode(_)
                    | Operation::SelectFace { .. }
                    | Operation::SelectVertex { .. }
                    | Operation::SelectEdge { .. }
            )
        {
            return Err(OperationError::BusyEditing);
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
            Operation::ModifySelection {
                node,
                instance_path,
                ..
            } => {
                if self.selection_mode.is_component() {
                    return Err(OperationError::InvalidInput(
                        "Selection changes need Object or Hierarchy selection mode".into(),
                    ));
                }
                Some(self.selection_path(*node, instance_path.clone())?)
            }
            Operation::SelectByRule(_) if self.selection_mode.is_component() => {
                return Err(OperationError::InvalidInput(
                    "Selection changes need Object or Hierarchy selection mode".into(),
                ));
            }
            _ => None,
        };
        match &operation {
            Operation::SelectFace { target } => {
                self.require_component_selection(
                    SelectionMode::Face,
                    target.as_ref().map(|target| target.path.len()),
                )?;
                if let Some(target) = target {
                    self.history
                        .scene()
                        .resolve_face_target(target)
                        .map_err(OperationError::Scene)?;
                }
            }
            Operation::SelectVertex { target } => {
                self.require_component_selection(
                    SelectionMode::Vertex,
                    target.as_ref().map(|target| target.path.len()),
                )?;
                if let Some(target) = target {
                    self.history
                        .scene()
                        .resolve_vertex_target(target)
                        .map_err(OperationError::Scene)?;
                }
            }
            Operation::SelectEdge { target } => {
                self.require_component_selection(
                    SelectionMode::Edge,
                    target.as_ref().map(|target| target.path.len()),
                )?;
                if let Some(target) = target {
                    self.history
                        .scene()
                        .resolve_edge_target(target)
                        .map_err(OperationError::Scene)?;
                }
            }
            _ => {}
        }
        if !matches!(
            operation,
            Operation::UpdateMove { .. }
                | Operation::UpdateManipulation { .. }
                | Operation::FinishMove { .. }
        ) && !self.commit_pending()
        {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        let command = match operation {
            Operation::SetActiveTool(tool) => return self.set_active_tool(tool),
            Operation::SetToolSpace { tool, space } => return self.set_tool_space(tool, space),
            Operation::SetToolHandle(handle) => return Ok(self.set_tool_handle(handle)),
            Operation::SetManipulatorSize(size) => return Ok(self.set_manipulator_size(size)),
            Operation::BeginMove { target, axis } => {
                return self.begin_editor_move(&target, axis, provenance);
            }
            Operation::UpdateMove { id, distance } => {
                return self.update_editor_move(id, distance, provenance);
            }
            Operation::BeginRotate {
                target,
                axis,
                space,
            } => {
                return self.begin_editor_rotate(&target, axis, space, provenance);
            }
            Operation::BeginScale {
                target,
                handle,
                space,
            } => {
                return self.begin_editor_scale(&target, handle, space, provenance);
            }
            Operation::UpdateManipulation { id, value } => {
                return self.update_editor_manipulation(id, value, provenance);
            }
            Operation::FinishMove { id, finish } => {
                return self.finish_editor_move(id, finish, provenance);
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetMaterialDiffuse { shape, value } => {
                Command::SetMaterialDiffuse { shape, value }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::CreateSessionMaterial { color } => Command::CreateSessionMaterial { color },
            #[cfg(any(test, feature = "mcp"))]
            Operation::AssignSessionMaterial { shape, material } => {
                Command::AssignSessionMaterial { shape, material }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetSessionMaterialColor { material, color } => {
                Command::SetSessionMaterialColor { material, color }
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
            Operation::SetShadingMode(mode) => {
                let changed = self.shading != mode;
                self.shading = mode;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SetSidebarPanelVisible { panel, visible } => {
                let before = self.sidebar_dock;
                if visible {
                    self.sidebar_dock = Some(panel);
                } else if before == Some(panel) {
                    self.sidebar_dock = None;
                }
                if before != self.sidebar_dock
                    && self.sidebar_dock == Some(SidebarPanel::AttributeEditor)
                {
                    self.bind_material_shape_to_tab();
                }
                return Ok(if before != self.sidebar_dock {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectAttributeEditorTab { tab } => {
                return self.select_attribute_editor_tab(tab);
            }
            Operation::Create {
                primitive,
                options,
                name,
            } => match name {
                Some(name) => Command::Create {
                    primitive,
                    options,
                    name,
                },
                None => Command::CreateDefaultNamed { primitive, options },
            },
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

                let new_transform = NativeTransform {
                    translation: transform.translation,
                    rotation,
                    scale: transform.scale,
                };

                Command::SetTransform {
                    id,
                    transform: new_transform,
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
            Operation::DeleteSelection => {
                if self.selection.is_empty() {
                    return Err(OperationError::NoSelection);
                }
                Command::RemoveMany(self.selection.members().iter().map(|t| t.node).collect())
            }
            Operation::SetSelectionMode(mode) => {
                if mode.is_component() && self.selection.len() > 1 {
                    return Err(OperationError::MultipleSelection);
                }
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
                    self.select_component_owner(target.transform, &target.path)?;
                }
                self.remembered_face = target;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectVertex { target } => {
                let changed = self.remembered_vertex != target;
                if let Some(target) = &target {
                    self.select_component_owner(target.transform, &target.path)?;
                }
                self.remembered_vertex = target;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectEdge { target } => {
                let changed = self.remembered_edge != target;
                if let Some(target) = &target {
                    self.select_component_owner(target.transform, &target.path)?;
                }
                self.remembered_edge = target;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::DeselectAll => {
                // Maya's SelectNone runs changeSelectMode -component, then -object, then
                // select -cl, so it also leaves component and hierarchy modes.
                let (selected, current_path) = self.selection_presentation();
                let changed = selected.is_some()
                    || !current_path.is_empty()
                    || self.selection_mode != SelectionMode::Object
                    || self.remembered_face.is_some()
                    || self.remembered_vertex.is_some()
                    || self.remembered_edge.is_some();
                self.clear_remembered_components();
                self.selection_mode = SelectionMode::Object;
                self.selection.clear();
                self.sync_fields();
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::Select { node, .. } => {
                let path = selected_path.unwrap_or_default();
                let (selected, current_path) = self.selection_presentation();
                let changed = selected != node || current_path != path || self.selection.len() > 1;
                if changed {
                    self.clear_remembered_components();
                }
                match node {
                    Some(node) => self.selection.replace(TransformTarget { node, path }),
                    None => self.selection.clear(),
                }
                self.sync_fields();
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectByRule(rule) => return self.select_by_rule(rule),
            Operation::ModifySelection { change, node, .. } => {
                let target = TransformTarget {
                    node,
                    path: selected_path.unwrap_or_default(),
                };
                let full = |error: azteccraft::selection::SelectionFull| {
                    OperationError::InvalidInput(error.to_string())
                };
                let changed = match change {
                    SelectionChange::Add => {
                        let leads = self.selection.lead() == Some(&target);
                        self.selection.add(target).map_err(full)?;
                        !leads
                    }
                    SelectionChange::Toggle => {
                        self.selection.toggle(target).map_err(full)?;
                        true
                    }
                    SelectionChange::Remove => self.selection.remove(&target),
                };
                if changed {
                    self.clear_remembered_components();
                    self.sync_fields();
                }
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
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::FrameSelected { with_children } => {
                let component = matches!(
                    self.selection_mode,
                    SelectionMode::Face | SelectionMode::Vertex | SelectionMode::Edge
                );
                if component {
                    let scene = self.history.scene();
                    match self.selection_mode {
                        SelectionMode::Face => self
                            .remembered_face
                            .as_ref()
                            .map(|target| scene.resolve_face_target(target)),
                        SelectionMode::Vertex => self
                            .remembered_vertex
                            .as_ref()
                            .map(|target| scene.resolve_vertex_target(target)),
                        _ => self
                            .remembered_edge
                            .as_ref()
                            .map(|target| scene.resolve_edge_target(target)),
                    }
                    .ok_or(OperationError::NoSelection)?
                    .map_err(OperationError::Scene)?;
                }
                if !component && self.selection.is_empty() {
                    // The 2027 View page: Frame Selection with nothing selected acts like Frame All.
                    return self.frame_everything();
                }
                if !component {
                    let before = self.camera.clone();
                    self.frame_members(if with_children {
                        FrameChildren::Include
                    } else {
                        FrameChildren::Exclude
                    })?;
                    return Ok(if self.camera == before {
                        OperationReply::Unchanged
                    } else {
                        OperationReply::Changed
                    });
                }
                let view = ViewScene::from_native(self.history.scene(), 256 * 1024 * 1024)
                    .map_err(|error| OperationError::View(error.to_string()))?;
                let before = self.camera.clone();
                match self.selection_mode {
                    SelectionMode::Face => self
                        .remembered_face
                        .as_ref()
                        .map(|target| self.camera.frame_face(&view, target, self.aspect)),
                    SelectionMode::Vertex => self
                        .remembered_vertex
                        .as_ref()
                        .map(|target| self.camera.frame_vertex(&view, target, self.aspect)),
                    _ => self
                        .remembered_edge
                        .as_ref()
                        .map(|target| self.camera.frame_edge(&view, target, self.aspect)),
                }
                .ok_or(OperationError::NoSelection)?
                .map_err(OperationError::Camera)?;
                return Ok(if self.camera == before {
                    OperationReply::Unchanged
                } else {
                    OperationReply::Changed
                });
            }
            Operation::SetCameraPose {
                target,
                distance,
                yaw,
                pitch,
            } => {
                let before = self.camera.clone();
                self.camera
                    .set_pose(target.into(), distance, yaw, pitch)
                    .map_err(OperationError::CameraPose)?;
                return Ok(if self.camera == before {
                    OperationReply::Unchanged
                } else {
                    OperationReply::Changed
                });
            }
            Operation::FrameAll => return self.frame_everything(),
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
            Operation::Inspect { page } => {
                return Ok(OperationReply::Scene {
                    inspection: Box::new(self.inspect_page(false, page.as_ref())?),
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
        let session_material_command = matches!(
            command,
            Command::CreateSessionMaterial { .. }
                | Command::AssignSessionMaterial { .. }
                | Command::SetSessionMaterialColor { .. }
        );
        let outcome = self.history.execute(command).map_err(|error| {
            #[cfg(any(test, feature = "mcp"))]
            if session_material_command
                && error != azteccraft::native_scene::NativeError::BusyManipulation
            {
                return OperationError::SessionMaterial(error);
            }
            let _ = session_material_command;
            OperationError::from_history(error)
        })?;
        if let EditOutcome::Created(id) = outcome {
            self.clear_remembered_components();
            let transform = self
                .history
                .scene()
                .node(id)
                .map_err(OperationError::Scene)?
                .transform_id();
            self.selection.replace(TransformTarget {
                node: id,
                path: vec![transform],
            });
        }
        self.sync_fields();
        Ok(match outcome {
            EditOutcome::Created(id) => OperationReply::Created {
                node_id: id.to_decimal(),
            },
            #[cfg(any(test, feature = "mcp"))]
            EditOutcome::SessionMaterial(material) => OperationReply::SessionMaterial {
                material_id: material.index().to_string(),
            },
            #[cfg(not(any(test, feature = "mcp")))]
            EditOutcome::SessionMaterial(_) => OperationReply::Changed,
            EditOutcome::Changed => OperationReply::Changed,
            EditOutcome::Unchanged => OperationReply::Unchanged,
        })
    }
    pub(super) fn has_draft(&self) -> bool {
        self.material_draft
            .as_ref()
            .is_some_and(MaterialDraft::changed)
            || self.has_channel_draft()
    }
    pub(super) fn has_channel_draft(&self) -> bool {
        self.selection_presentation()
            .0
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
    pub(super) fn commit_pending(&mut self) -> bool {
        if self.history.active_manipulation().is_some() {
            self.status = OperationError::BusyManipulation.to_string();
            self.error = true;
            return false;
        }
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
        let Some((id, _)) = self.lead_target().ok() else {
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
}
