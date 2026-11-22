use super::*;
#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::NameScope;
use crate::editor_operations::{
    ComponentKind, ComponentMoveSpace, MAX_COMPONENT_INDICES, MAX_MOVED_VERTICES,
};
use azteccraft::history::{ManipulationFinish, ManipulationKindState};
use azteccraft::native_scene::NativeTransformNode;

/// Status-line feedback planned for one operation; see [`Editor::status_feedback`].
struct StatusFeedback {
    status: Option<&'static str>,
    step_label: Option<String>,
    activated: bool,
    /// Report `status` whatever the reply, as a native drag release or Esc does.
    fixed: bool,
}

/// Why Create Joints is unavailable in a component mode, natively and over MCP.
pub(super) const JOINT_COMPONENT_REASON: &str =
    "Joints are created in Object or Hierarchy selection mode";

/// Radians for a rotation request in degrees. A request equal to the stored angle in degrees keeps
/// the stored radians, because `to_degrees` then `to_radians` can land 1 ulp away.
#[cfg(any(test, feature = "mcp"))]
pub(super) fn radians_keeping_stored(stored: f64, degrees: f64) -> f64 {
    if stored.to_degrees() == degrees {
        stored
    } else {
        degrees.to_radians()
    }
}

/// The selection holds `MAX_TRANSFORM_PATHS` objects, so selecting every imported root cannot fail.
const _: () =
    assert!(azteccraft::obj::import::MAX_OBJECTS <= azteccraft::native_scene::MAX_TRANSFORM_PATHS);

/// The pivot refusals that carry their own MCP code (`NonFinite` is an overflow of the request's
/// numbers); every other failure stays `Scene`.
fn is_pivot_refusal(error: &azteccraft::native_scene::NativeError) -> bool {
    use azteccraft::native_scene::NativeError as E;
    matches!(
        error,
        E::ReadOnlyFormat
            | E::UnsupportedEdit
            | E::AbsentPivotLeaf { .. }
            | E::InsertionAfterDelete
            | E::PivotNeedsTranslateCorrection
            | E::PivotOnKeyedNode
            | E::NonFinite
    )
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
    /// Validates a component-set request before any mutation: the kind's mode is active, the
    /// owner is the one selected object's shape (this never switches the object selection),
    /// the request is bounded, and every index exists in the owner's mesh.
    fn check_component_request(
        &self,
        kind: ComponentKind,
        owner: &azteccraft::selection::ComponentOwner,
        indices: &[usize],
    ) -> Result<(), OperationError> {
        self.require_component_selection(kind.mode(), Some(owner.path.len()))?;
        if indices.len() > MAX_COMPONENT_INDICES {
            return Err(OperationError::InvalidInput(format!(
                "A component selection request holds at most {MAX_COMPONENT_INDICES} indices"
            )));
        }
        let (selected, selected_path) = self.single_target()?;
        if owner.transform != selected
            || owner
                .path
                .split_last()
                .is_none_or(|(_, parent)| parent != selected_path)
        {
            return Err(OperationError::InvalidInput(
                "Component selection must name a shape of the selected object".into(),
            ));
        }
        let counts = self
            .history
            .scene()
            .component_counts(owner.transform, owner.shape, &owner.path)
            .map_err(OperationError::Scene)?;
        let (limit, missing): (usize, fn(usize) -> azteccraft::scene::SceneError) = match kind {
            ComponentKind::Face => (counts.faces, azteccraft::scene::SceneError::MissingFace),
            ComponentKind::Vertex => (
                counts.vertices,
                azteccraft::scene::SceneError::MissingVertex,
            ),
            ComponentKind::Edge => (counts.edges, azteccraft::scene::SceneError::MissingEdge),
        };
        match indices.iter().find(|index| **index >= limit) {
            Some(bad) => Err(OperationError::Scene(missing(*bad).into())),
            None => Ok(()),
        }
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
        let _ = self.native_operation_result(operation);
    }
    /// `native_operation` for callers that must tell success from refusal.
    pub(super) fn native_operation_result(
        &mut self,
        operation: Operation,
    ) -> Result<OperationReply, OperationError> {
        let feedback = self.status_feedback(&operation);
        let result = self.apply_operation(operation);
        self.record_feedback(feedback, &result);
        result
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
            Operation::Undo => self.history.undo_label_after_chunk(),
            Operation::Redo => self.history.redo_label_after_chunk(),
            _ => None,
        };
        let status = match *operation {
            Operation::Select { .. }
            | Operation::ModifySelection { .. }
            | Operation::SelectByRule(_)
            | Operation::SelectInRect { .. }
            | Operation::DeselectAll
            | Operation::SelectFace { .. }
            | Operation::SelectVertex { .. }
            | Operation::SelectEdge { .. }
            | Operation::ModifyComponentSelection { .. }
            | Operation::SetSelectionMode(_)
            | Operation::FrameSelected { .. }
            | Operation::FrameAll { .. }
            | Operation::SetActiveTool(_)
            | Operation::SetToolSpace { .. }
            | Operation::SetMoveSpace { .. }
            | Operation::SetToolHandle(_)
            | Operation::SetManipulatorSize(_)
            | Operation::SetRotateStep { .. }
            | Operation::SetScaleStep { .. }
            | Operation::SetUndoQueue(_)
            | Operation::SetUndoEnabled { .. }
            | Operation::OpenUndoChunk { .. }
            | Operation::CloseUndoChunk
            | Operation::SetGridVisible { .. }
            | Operation::SetSnapToGrid { .. }
            | Operation::SetShadingMode(_)
            | Operation::SetView(_)
            | Operation::SetMenuSet { .. }
            | Operation::SetSidebarPanelVisible { .. }
            | Operation::SelectAttributeEditorTab { .. }
            | Operation::SetCurrentTime { .. }
            | Operation::SetPlaybackRange { .. }
            | Operation::SetAnimationRange { .. }
            | Operation::Play
            | Operation::Stop => None,
            // Without a line of its own a success would leave an earlier refusal on screen.
            Operation::SetImagePlane(_) => Some("Image plane set"),
            Operation::RemoveImagePlane => Some("Image plane removed"),
            Operation::SetKeyframe { .. } => Some("Key set"),
            Operation::DeleteKeys { .. } => Some("Keys deleted"),
            Operation::EulerFilter { .. } => Some("Euler filter applied"),
            Operation::SetInfinity { .. } => Some("Infinity set"),
            Operation::ReduceMesh { .. } => Some("Mesh reduced"),
            Operation::Orbit { .. }
            | Operation::Pan { .. }
            | Operation::Dolly { .. }
            | Operation::BoxDolly { .. }
            | Operation::UndoView
            | Operation::RedoView
            | Operation::DefaultView
            | Operation::SetCameraPose { .. }
            | Operation::SetCameraFilmBack(_)
            | Operation::CreateCameraBookmark { .. }
            | Operation::RenameCameraBookmark { .. }
            | Operation::DeleteCameraBookmark { .. }
            | Operation::ApplyCameraBookmark { .. } => None,
            #[cfg(any(test, feature = "mcp"))]
            Operation::ListCameraBookmarks
            | Operation::RecoveryStatus
            | Operation::InspectLimits => None,
            #[cfg(any(test, feature = "mcp"))]
            Operation::AutosaveNow => None,
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
            | Operation::ExportObj { .. }
            | Operation::ResolveFileAction { .. } => None,
            #[cfg(any(test, feature = "mcp"))]
            Operation::ExportGltf { .. } | Operation::ExportUsd { .. } => None,
            Operation::Playblast(_) | Operation::CancelPlayblast => None,
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
            Ok(OperationReply::Exported {
                objects,
                vertices,
                faces,
            }) => {
                self.status = format!(
                    "Exported {objects} object(s), {vertices} vertices and {faces} faces to OBJ"
                );
                self.error = false;
            }
            Ok(OperationReply::Imported {
                objects,
                vertices,
                faces,
                from_maya,
                uv_objects,
                uv_dropped_objects,
                ..
            }) => {
                let dropped = if *from_maya {
                    String::new()
                } else {
                    features::obj_import::obj_import_note(*uv_objects, *uv_dropped_objects)
                };
                self.status = format!(
                    "Imported {objects} object(s), {vertices} vertices and {faces} faces{dropped}"
                );
                self.error = false;
            }
            #[cfg(any(test, feature = "mcp"))]
            Ok(OperationReply::UsdExported {
                objects,
                vertices,
                faces,
            }) => {
                self.status = format!(
                    "Exported {objects} object(s), {vertices} vertices and {faces} faces to USD"
                );
                self.error = false;
            }
            #[cfg(any(test, feature = "mcp"))]
            Ok(OperationReply::GltfExported {
                objects,
                vertices,
                triangles,
            }) => {
                self.status = format!(
                    "Exported {objects} object(s), {vertices} vertices and {triangles} triangles to glTF"
                );
                self.error = false;
            }
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
        if matches!(result, Ok(OperationReply::DocumentApplied { .. })) {
            let blocked = self.history.scene().blocked_camera_names();
            if !blocked.is_empty() {
                self.status = format!(
                    "{} - camera {} film back is kept in the file but not applied (default lens shown)",
                    self.status,
                    blocked.join(", ")
                );
            }
        }
        if let Some((step, error)) = self.refusal_after_change.take() {
            self.report_refusal_after_change(step, &error);
        }
    }
    /// The step happened; only the pose re-evaluation was refused, so the line must not read as
    /// a failed Undo or Redo.
    pub(in crate::app) fn report_refusal_after_change(
        &mut self,
        step: &str,
        error: &OperationError,
    ) {
        self.set_error_text(
            format!("{step} done; pose not re-evaluated: {error}"),
            error,
        );
        self.error = true;
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
        if let Operation::InspectKeys { node, cursor } = &request.operation {
            return Ok(OperationReply::Keys {
                inspection: Box::new(self.inspect_keys(*node, cursor.as_ref())?),
            });
        }
        if matches!(request.operation, Operation::ListCameraBookmarks) {
            return Ok(self.list_camera_bookmarks());
        }
        if matches!(request.operation, Operation::RecoveryStatus) {
            return self.recovery_status();
        }
        #[cfg(any(test, feature = "mcp"))]
        if matches!(request.operation, Operation::InspectLimits) {
            return Ok(OperationReply::SceneLimits {
                limits: azteccraft::scene_limits::scene_limits(),
                usage: self.history.scene().usage(),
            });
        }
        if let Operation::InspectUvs { shape, cursor } = &request.operation {
            return Ok(OperationReply::Uvs {
                inspection: Box::new(self.inspect_uvs(*shape, cursor.as_ref())?),
            });
        }
        if let Operation::InspectVertices { shape, cursor } = &request.operation {
            return Ok(OperationReply::Vertices {
                inspection: Box::new(self.inspect_vertices(*shape, cursor.as_ref())?),
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
            || self.marking_menu.is_active()
            || self.native_camera_drag
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
    fn rotation_radians(&self, id: NodeId, axis: usize, degrees: f64) -> f64 {
        match self.history.scene().node(id) {
            Ok(node) => radians_keeping_stored(node.transform().rotation[axis], degrees),
            Err(_) => degrees.to_radians(),
        }
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
    /// Runs `operation` and, for Edit > Repeat, records it or replays the recorded one.
    /// PROVISIONAL: Maya's Repeat semantics (which commands count, behaviour after selection
    /// changes, undoability) are unverified; only Delete is recorded and replay is a fresh
    /// guarded dispatch against the current selection.
    pub(super) fn apply_operation_guarded(
        &mut self,
        provenance: ManipulationProvenance,
        operation: Operation,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        let operation = if matches!(operation, Operation::RepeatLast) {
            self.last_repeatable
                .as_ref()
                .and_then(Operation::repeatable)
                .ok_or(OperationError::NothingToRepeat)?
        } else {
            operation
        };
        let record = Operation::repeatable(&operation);
        let reply = self.apply_operation_unrecorded(provenance, operation, may_install)?;
        if matches!(reply, OperationReply::Changed) && record.is_some() {
            self.last_repeatable = record;
        }
        Ok(reply)
    }
    fn apply_operation_unrecorded(
        &mut self,
        provenance: ManipulationProvenance,
        operation: Operation,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        let before = features::camera::changes_camera(&operation).then(|| self.camera.clone());
        let grouped = provenance == ManipulationProvenance::Native
            && self.native_camera_drag
            && matches!(
                operation,
                Operation::Orbit { .. } | Operation::Pan { .. } | Operation::Dolly { .. }
            );
        let result = self.dispatch_operation_guarded(provenance, operation, may_install);
        if result.is_ok()
            && let Some(before) = before
        {
            if self.camera != before {
                self.view_history.record(before, grouped);
            } else if !grouped {
                self.view_history.end_gesture();
            }
        }
        result
    }

    fn camera_reply(&self, before: &Camera) -> OperationReply {
        if self.camera == *before {
            OperationReply::Unchanged
        } else {
            OperationReply::Changed
        }
    }

    fn dispatch_operation_guarded(
        &mut self,
        provenance: ManipulationProvenance,
        operation: Operation,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        self.refusal_after_change = None;
        if features::timeline::stops_playback(&operation) {
            self.stop_playback();
        }
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(operation, Operation::Delete(_) | Operation::DeleteSelection)
        {
            return Err(OperationError::InvalidInput(format!(
                "{noun} deletion is unsupported; switch to Object or Hierarchy selection"
            )));
        }
        if self.selection_mode.is_component() && matches!(operation, Operation::CreateJoint { .. })
        {
            return Err(OperationError::InvalidInput(JOINT_COMPONENT_REASON.into()));
        }
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(
                operation,
                Operation::Group { .. } | Operation::Ungroup { .. }
            )
        {
            return Err(OperationError::InvalidInput(format!(
                "Group and Ungroup on {noun} selection are unsupported; switch to Object or Hierarchy selection"
            )));
        }
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(operation, Operation::ResetPivots { .. })
        {
            return Err(OperationError::InvalidInput(format!(
                "Pivot reset on {noun} selection is unsupported; switch to Object or Hierarchy selection"
            )));
        }
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(
                operation,
                Operation::ResetTransformations { .. }
                    | Operation::FreezeTransformations { .. }
                    | Operation::MatchTransformations { .. }
            )
        {
            return Err(OperationError::InvalidInput(format!(
                "Reset, Freeze and Match Transformations on {noun} selection are unsupported; switch to Object or Hierarchy selection"
            )));
        }
        #[cfg(any(test, feature = "mcp"))]
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(
                operation,
                Operation::PrefixHierarchyNames { .. } | Operation::SearchReplaceNames { .. }
            )
        {
            return Err(OperationError::InvalidInput(format!(
                "Prefix Hierarchy Names and Search and Replace Names on {noun} selection are unsupported; switch to Object or Hierarchy selection"
            )));
        }
        #[cfg(any(test, feature = "mcp"))]
        if let Some(noun) = self.selection_mode.component_noun()
            && matches!(operation, Operation::AlignObjects { .. })
        {
            return Err(OperationError::InvalidInput(format!(
                "Align Objects on {noun} selection is unsupported; switch to Object or Hierarchy selection"
            )));
        }
        self.guard_move_dispatch(&operation, provenance)?;
        if self.files.is_closing() {
            return Err(OperationError::BusyEditing);
        }
        if matches!(
            operation,
            Operation::UndoView
                | Operation::RedoView
                | Operation::DefaultView
                | Operation::BoxDolly { .. }
                | Operation::SetGridVisible { .. }
                | Operation::SetSnapToGrid { .. }
                | Operation::SetShadingMode(_)
                | Operation::SetView(_)
                | Operation::SetMenuSet { .. }
                | Operation::SetSidebarPanelVisible { .. }
                | Operation::SelectAttributeEditorTab { .. }
                | Operation::SetCameraFilmBack(_)
                | Operation::CreateCameraBookmark { .. }
                | Operation::RenameCameraBookmark { .. }
                | Operation::DeleteCameraBookmark { .. }
        ) && self.files.is_modal()
        {
            return Err(OperationError::BusyEditing);
        }
        if let Operation::ExportObj { path, .. } = &operation {
            super::features::obj_export::validate_export_path(path)?;
        }
        if let Operation::ImportObj { path } = &operation {
            super::features::obj_import::validate_import_path(path)?;
            if self.files.is_modal() {
                return Err(OperationError::BusyEditing);
            }
        }
        // The name rules (length, NUL, extension) run before a pending draft is committed, like the
        // OBJ import path check, so a refused name never touches the draft.
        if let Operation::SetImagePlane(edit) = &operation
            && let Some(name) = &edit.image_name
        {
            azteccraft::image_plane::ops::check_image_name(name)
                .map_err(OperationError::ImagePlane)?;
        }
        #[cfg(any(test, feature = "mcp"))]
        if let Operation::ExportUsd { path, .. } = &operation {
            super::features::usd_export::validate_export_path(path)?;
        }
        #[cfg(any(test, feature = "mcp"))]
        if let Operation::ExportGltf { path, .. } = &operation {
            super::features::gltf_export::validate_export_path(path)?;
        }
        if matches!(operation, Operation::Playblast(_)) && self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        #[cfg(any(test, feature = "mcp"))]
        if let Operation::ImportMaya { path, .. } = &operation {
            super::features::maya_import::validate_maya_import_path(path)?;
            if self.files.is_modal() {
                return Err(OperationError::BusyEditing);
            }
        }
        match &operation {
            Operation::OpenScene { path } | Operation::SaveSceneAs { path } => {
                super::files::validate_scene_path(path)?;
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
            #[cfg(any(test, feature = "mcp"))]
            Operation::AutosaveNow => {
                if self.files.is_modal() {
                    return Err(OperationError::BusyEditing);
                }
                return self.autosave_now();
            }
            Operation::NewScene => return self.new_document(),
            Operation::RequestClose => return self.close_document(),
            Operation::ResolveFileAction {
                pending_id,
                decision,
            } => {
                return self.resolve_file_action_guarded(
                    *pending_id,
                    decision.clone(),
                    may_install,
                );
            }
            _ => {}
        }
        if self.files.is_modal()
            && matches!(
                operation,
                Operation::SetActiveTool(_)
                    | Operation::SetToolSpace { .. }
                    | Operation::SetMoveSpace { .. }
                    | Operation::SetToolHandle(_)
                    | Operation::SetManipulatorSize(_)
                    | Operation::SetRotateStep { .. }
                    | Operation::SetScaleStep { .. }
                    | Operation::BeginMove { .. }
                    | Operation::BeginRotate { .. }
                    | Operation::BeginScale { .. }
                    | Operation::UpdateMove { .. }
                    | Operation::UpdateManipulation { .. }
                    | Operation::FinishMove { .. }
                    | Operation::Select { .. }
                    | Operation::ModifySelection { .. }
                    | Operation::SelectByRule(_)
                    | Operation::SelectInRect { .. }
                    | Operation::DeselectAll
                    | Operation::SetSelectionMode(_)
                    | Operation::SelectFace { .. }
                    | Operation::SelectVertex { .. }
                    | Operation::SelectEdge { .. }
                    | Operation::ModifyComponentSelection { .. }
                    | Operation::SetCurrentTime { .. }
                    | Operation::SetPlaybackRange { .. }
                    | Operation::SetAnimationRange { .. }
                    | Operation::Play
                    | Operation::SetKeyframe { .. }
                    | Operation::DeleteKeys { .. }
                    | Operation::EulerFilter { .. }
                    | Operation::SetInfinity { .. }
                    | Operation::Create { .. }
                    | Operation::Delete(_)
                    | Operation::DeleteSelection
                    | Operation::ExtrudeFace { .. }
                    | Operation::MoveVertices { .. }
                    | Operation::ProjectUvPlanar { .. }
                    | Operation::DeleteUvs { .. }
                    | Operation::ReduceMesh { .. }
                    | Operation::CreateJoint { .. }
                    | Operation::Undo
                    | Operation::Redo
                    | Operation::SetUndoQueue(_)
                    | Operation::SetUndoEnabled { .. }
                    | Operation::OpenUndoChunk { .. }
                    | Operation::CloseUndoChunk
                    | Operation::SetPivots { .. }
                    | Operation::ResetPivots { .. }
                    | Operation::ResetTransformations { .. }
                    | Operation::FreezeTransformations { .. }
                    | Operation::MatchTransformations { .. }
                    | Operation::Group { .. }
                    | Operation::Ungroup { .. }
                    | Operation::AssignSessionMaterial { .. }
            )
        {
            return Err(OperationError::BusyEditing);
        }
        #[cfg(any(test, feature = "mcp"))]
        if self.files.is_modal()
            && matches!(
                operation,
                Operation::Rename { .. }
                    | Operation::PrefixHierarchyNames { .. }
                    | Operation::SearchReplaceNames { .. }
                    | Operation::AlignObjects { .. }
                    | Operation::SetTransform { .. }
                    | Operation::EditNode { .. }
                    | Operation::Reparent { .. }
                    | Operation::SetMaterialDiffuse { .. }
                    | Operation::CreateSessionMaterial { .. }
                    | Operation::SetSessionMaterialColor { .. }
                    | Operation::SetSessionMaterialAttribute { .. }
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
            Operation::SelectInRect { .. } if self.selection_mode.is_component() => {
                return Err(OperationError::InvalidInput(
                    "Marquee selection needs Object or Hierarchy selection mode".into(),
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
            Operation::ModifyComponentSelection {
                kind,
                owner,
                indices,
                ..
            } => self.check_component_request(*kind, owner, indices)?,
            Operation::SetKeyframe {
                nodes,
                time,
                channels,
            } => self.check_key_request(nodes.as_deref(), *time, *channels)?,
            Operation::DeleteKeys {
                nodes,
                time,
                channels,
            } => self.check_key_request(nodes.as_deref(), *time, *channels)?,
            Operation::EulerFilter { nodes, channels } => {
                self.check_key_request(nodes.as_deref(), None, *channels)?;
                azteccraft::native_scene::check_euler_channels(*channels)
                    .map_err(OperationError::Scene)?;
            }
            Operation::SetInfinity {
                nodes,
                channels,
                pre,
                post,
            } => {
                if pre.is_none() && post.is_none() {
                    return Err(OperationError::InvalidInput(
                        "set_infinity needs pre, post or both".into(),
                    ));
                }
                self.check_key_request(nodes.as_deref(), None, *channels)?;
            }
            Operation::SetAnimationRange { start, end } => {
                self.check_animation_range(*start, *end)?;
            }
            Operation::SetPlaybackRange { start, end } => {
                self.check_playback_range(*start, *end)?;
            }
            _ => {}
        }
        // A time change and Play refuse a draft instead of committing it; Stop leaves it alone.
        if !matches!(
            operation,
            Operation::UpdateMove { .. }
                | Operation::UpdateManipulation { .. }
                | Operation::FinishMove { .. }
                | Operation::SetCurrentTime { .. }
                | Operation::Play
                | Operation::Stop
        ) && !self.commit_pending()
        {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        let mut import_totals = features::obj_import::ImportTotals::default();
        #[cfg_attr(not(any(test, feature = "mcp")), allow(unused_mut))]
        let mut from_maya = false;
        let command = match operation {
            #[cfg(any(test, feature = "mcp"))]
            Operation::ImportMaya { path, prefix } => {
                let (command, totals) = self.plan_maya_file_import(&path, prefix)?;
                import_totals = totals;
                from_maya = true;
                command
            }
            Operation::ImportObj { path } => {
                let (command, totals) = self.plan_obj_import(&path)?;
                import_totals = totals;
                command
            }
            Operation::SetActiveTool(tool) => return self.set_active_tool(tool),
            Operation::SetToolSpace { tool, space } => return self.set_tool_space(tool, space),
            Operation::SetMoveSpace { space } => return Ok(self.set_move_space(space)),
            Operation::SetToolHandle(handle) => return Ok(self.set_tool_handle(handle)),
            Operation::SetManipulatorSize(size) => return Ok(self.set_manipulator_size(size)),
            Operation::SetRotateStep { degrees } => return self.set_rotate_step(degrees),
            Operation::SetScaleStep { step } => return self.set_scale_step(step),
            Operation::BeginMove {
                target,
                axis,
                expected_selection_generation,
            } => {
                return self.begin_editor_move(
                    &target,
                    axis,
                    provenance,
                    expected_selection_generation,
                );
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
                expected_selection_generation,
            } => {
                return self.begin_editor_scale(
                    &target,
                    handle,
                    space,
                    provenance,
                    expected_selection_generation,
                );
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
            Operation::AssignSessionMaterial { shapes, material } => {
                Command::AssignSessionMaterial { shapes, material }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetSessionMaterialColor { material, color } => {
                Command::SetSessionMaterialColor { material, color }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SetSessionMaterialAttribute {
                material,
                attribute,
                rgb,
            } => Command::SetSessionMaterialAttribute {
                material,
                attribute,
                rgb,
            },
            Operation::SetCurrentTime { time } => return self.set_current_time(time),
            Operation::SetAnimationRange { start, end } => {
                return self.set_animation_range(start, end);
            }
            Operation::SetPlaybackRange { start, end } => {
                return self.set_playback_range(start, end);
            }
            Operation::Play => return self.play(),
            Operation::Stop => return Ok(self.stop()),
            Operation::SetKeyframe {
                nodes,
                time,
                channels,
            } => self.set_key_command(nodes, time, channels)?,
            Operation::DeleteKeys {
                nodes,
                time,
                channels,
            } => self.delete_keys_command(nodes, time, channels)?,
            Operation::EulerFilter { nodes, channels } => {
                self.euler_filter_command(nodes, channels)
            }
            Operation::SetInfinity {
                nodes,
                channels,
                pre,
                post,
            } => self.set_infinity_command(nodes, channels, pre, post),
            Operation::ProjectUvPlanar { shape, axis } => Command::ProjectUvPlanar { shape, axis },
            Operation::DeleteUvs { shape } => Command::DeleteUvs { shape },
            Operation::ReduceMesh {
                shape,
                target,
                faces,
            } => Command::ReduceMesh {
                shape,
                target,
                faces,
            },
            Operation::CreateCameraBookmark { name } => {
                return self.create_camera_bookmark(&name);
            }
            Operation::RenameCameraBookmark { name, new_name } => {
                return self.rename_camera_bookmark(&name, &new_name);
            }
            Operation::DeleteCameraBookmark { name } => {
                return self.delete_camera_bookmark(&name);
            }
            Operation::ApplyCameraBookmark { name } => {
                return self.apply_camera_bookmark(&name);
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::ListCameraBookmarks => return Ok(self.list_camera_bookmarks()),
            #[cfg(any(test, feature = "mcp"))]
            Operation::RecoveryStatus => return self.recovery_status(),
            #[cfg(any(test, feature = "mcp"))]
            Operation::InspectLimits => {
                return Ok(OperationReply::SceneLimits {
                    limits: azteccraft::scene_limits::scene_limits(),
                    usage: self.history.scene().usage(),
                });
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
            Operation::SetSnapToGrid { enabled } => {
                let changed = self.snap_to_grid != enabled;
                self.snap_to_grid = enabled;
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
            Operation::SetView(preset) => {
                let before = self.camera.clone();
                match preset {
                    ViewPreset::Persp => self.camera.show_perspective(),
                    ViewPreset::Top => self.show_view(OrthographicView::Top)?,
                    ViewPreset::Front => self.show_view(OrthographicView::Front)?,
                    ViewPreset::Side => self.show_view(OrthographicView::Side)?,
                }
                return Ok(self.camera_reply(&before));
            }
            Operation::SetMenuSet { set } => {
                let changed = self.menu_set != set;
                self.menu_set = set;
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
                create_uvs,
            } => match name {
                Some(name) => Command::Create {
                    primitive,
                    options,
                    name,
                    create_uvs,
                },
                None => Command::CreateDefaultNamed {
                    primitive,
                    options,
                    create_uvs,
                },
            },
            #[cfg(any(test, feature = "mcp"))]
            Operation::Rename { id, name } => Command::Rename { id, name },
            #[cfg(any(test, feature = "mcp"))]
            Operation::Reparent { id, parent, keep } => Command::Reparent { id, parent, keep },
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
                            AngularUnit::Radian => self.rotation_radians(id, index, value),
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
                    AngularUnit::Radian => std::array::from_fn(|axis| {
                        self.rotation_radians(id, axis, transform.rotation_degrees[axis])
                    }),
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
            Operation::MoveVertices {
                target,
                delta,
                space,
            } => {
                if !(2..=azteccraft::native_scene::MAX_TRANSFORM_DEPTH + 1)
                    .contains(&target.path.len())
                    || target.vertex_indices.is_empty()
                    || target.vertex_indices.len() > MAX_MOVED_VERTICES
                    || delta.iter().any(|v| !v.is_finite())
                {
                    return Err(OperationError::InvalidInput(format!(
                        "Moving vertices requires a bounded full shape path, 1 to {MAX_MOVED_VERTICES} vertex indices and a finite delta"
                    )));
                }
                self.history
                    .scene()
                    .resolve_vertex_set(&target)
                    .map_err(OperationError::Scene)?;
                let scene = self.history.scene();
                let declared_cm = scene
                    .declared_vector_to_cm(delta)
                    .map_err(OperationError::Scene)?;
                let delta_cm = match space {
                    ComponentMoveSpace::Object => declared_cm,
                    ComponentMoveSpace::World => {
                        let (_, owner_path) =
                            target.path.split_last().ok_or(OperationError::Scene(
                                azteccraft::native_scene::NativeError::InvalidInstancePath,
                            ))?;
                        let owner = TransformTarget {
                            node: target.transform,
                            path: owner_path.to_vec(),
                        };
                        scene
                            .component_move_frame(&owner)
                            .and_then(|frame| {
                                frame.local_delta_cm(glam::DVec3::from_array(declared_cm))
                            })
                            .map_err(OperationError::Scene)?
                    }
                };
                Command::MoveVertices {
                    shape: target.shape,
                    vertices: target.vertex_indices,
                    delta_cm,
                }
            }
            Operation::CreateJoint {
                parent,
                position,
                relative,
                radius,
                name,
            } => Command::CreateJoint(azteccraft::native_scene::JointRequest {
                parent,
                position,
                relative,
                radius,
                name,
            }),
            Operation::Delete(id) => Command::Remove(id),
            // Replaced by the recorded operation before dispatch; never reaches here.
            Operation::RepeatLast => return Err(OperationError::NothingToRepeat),
            Operation::DeleteSelection => {
                if self.selection.is_empty() {
                    return Err(OperationError::NoSelection);
                }
                Command::RemoveMany(self.selection.members().iter().map(|t| t.node).collect())
            }
            Operation::SetPivots {
                id,
                rotate_pivot,
                scale_pivot,
                preserve,
            } => {
                if rotate_pivot.is_none() && scale_pivot.is_none() {
                    self.history
                        .scene()
                        .node(id)
                        .map_err(OperationError::Scene)?;
                    return Ok(OperationReply::Unchanged);
                }
                Command::SetPivots {
                    id,
                    rotate_pivot,
                    scale_pivot,
                    preserve,
                }
            }
            Operation::ResetPivots {
                mode,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::ResetPivots { ids, mode }
            }
            Operation::ResetTransformations {
                channels,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::ResetTransformations { ids, channels }
            }
            Operation::FreezeTransformations {
                channels,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::FreezeTransformations { ids, channels }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::AlignObjects {
                options,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::AlignObjects { ids, options }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::PrefixHierarchyNames {
                prefix,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::PrefixHierarchyNames { ids, prefix }
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::SearchReplaceNames {
                scope,
                search,
                replace,
                match_case,
                expected_selection_generation,
            } => {
                let ids = if scope == NameScope::All {
                    self.check_generation(expected_selection_generation)?;
                    self.history
                        .scene()
                        .nodes()
                        .iter()
                        .map(|n| n.id())
                        .collect()
                } else {
                    self.selected_object_ids(expected_selection_generation, provenance)?
                };
                Command::SearchReplaceNames {
                    ids,
                    hierarchy: scope == NameScope::Hierarchy,
                    search,
                    replace,
                    match_case,
                }
            }
            Operation::MatchTransformations {
                channels,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::MatchTransformations { ids, channels }
            }
            Operation::Group {
                options,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::Group { ids, options }
            }
            Operation::Ungroup {
                options,
                expected_selection_generation,
            } => {
                let ids = self.selected_object_ids(expected_selection_generation, provenance)?;
                Command::Ungroup { ids, options }
            }
            Operation::SetSelectionMode(mode) => {
                if mode.is_component() && self.selection.len() > 1 {
                    return Err(OperationError::MultipleSelection);
                }
                let changed = self.set_selection_mode_field(mode);
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectFace { target } => {
                if let Some(target) = &target {
                    self.select_component_owner(target.transform, &target.path)?;
                }
                let changed = self.set_remembered_face(target);
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectVertex { target } => {
                if let Some(target) = &target {
                    self.select_component_owner(target.transform, &target.path)?;
                }
                let changed = self.set_remembered_vertex(target);
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SelectEdge { target } => {
                if let Some(target) = &target {
                    self.select_component_owner(target.transform, &target.path)?;
                }
                let changed = self.set_remembered_edge(target);
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::ModifyComponentSelection {
                kind,
                mode,
                owner,
                indices,
            } => {
                let changed = self.modify_component_set(kind, |set| match mode {
                    MarqueeMode::Replace => set.replace_with(&owner, indices),
                    MarqueeMode::Add => set.extend_absent(&owner, indices),
                    MarqueeMode::Remove => set.remove_all(&owner, indices),
                    MarqueeMode::Toggle => set.toggle_all(&owner, indices),
                });
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
                    || self.remembered_face().is_some()
                    || self.remembered_vertex().is_some()
                    || self.remembered_edge().is_some();
                self.clear_remembered_components();
                self.set_selection_mode_field(SelectionMode::Object);
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
            Operation::SelectInRect { rect, mode } => return self.select_in_rect(rect, mode),
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
            Operation::SetUndoQueue(queue) => {
                let changed = self.history.undo_queue() != queue
                    || matches!(queue, UndoQueue::Finite(size) if self.history.undo_len() > size);
                self.history
                    .set_undo_queue(queue)
                    .map_err(OperationError::from_history)?;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SetUndoEnabled { enabled, flush } => {
                let changed = self.history.undo_enabled() != enabled
                    || (flush && (self.history.can_undo() || self.history.can_redo()));
                self.history
                    .set_undo_enabled(enabled, flush)
                    .map_err(OperationError::from_history)?;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::OpenUndoChunk { name } => {
                self.history
                    .open_chunk(&name)
                    .map_err(OperationError::from_history)?;
                return Ok(OperationReply::Changed);
            }
            Operation::CloseUndoChunk => {
                return Ok(
                    match self
                        .history
                        .close_chunk()
                        .map_err(OperationError::from_history)?
                    {
                        EditOutcome::Unchanged => OperationReply::Unchanged,
                        _ => OperationReply::Changed,
                    },
                );
            }
            Operation::Undo | Operation::Redo => {
                let changed = if matches!(operation, Operation::Redo) {
                    self.history.redo()
                } else {
                    self.history.undo()
                };
                // A snapshot taken at another time holds that time's pose, so it is re-evaluated;
                // one taken at the current time is the recorded state, held edits included.
                let evaluated = if changed && self.history.pose_frame() != self.timeline.current {
                    self.history.apply_evaluated(self.timeline.current)
                } else {
                    Ok(false)
                };
                self.sync_fields();
                // History has already moved, so a refused re-evaluation is reported in the status
                // line, like a playback tick, and the reply stays truthful about the change.
                // `record_feedback` re-applies it after its own success text.
                if let Err(error) = evaluated {
                    let step = if matches!(operation, Operation::Redo) {
                        "Redo"
                    } else {
                        "Undo"
                    };
                    let error = OperationError::from_history(error);
                    self.report_refusal_after_change(step, &error);
                    self.refusal_after_change = Some((step, error));
                }
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::FrameSelected {
                with_children,
                fit_factor,
            } => {
                let original = self.begin_fit(fit_factor)?;
                let result = (|| -> Result<OperationReply, OperationError> {
                    let component = matches!(
                        self.selection_mode,
                        SelectionMode::Face | SelectionMode::Vertex | SelectionMode::Edge
                    );
                    if component {
                        let scene = self.history.scene();
                        match self.selection_mode {
                            SelectionMode::Face => self
                                .remembered_face()
                                .as_ref()
                                .map(|target| scene.resolve_face_target(target)),
                            SelectionMode::Vertex => self
                                .remembered_vertex()
                                .as_ref()
                                .map(|target| scene.resolve_vertex_target(target)),
                            _ => self
                                .remembered_edge()
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
                    // The whole set, not just its lead; the clone ends the borrow of `self`.
                    let (set, aspect) = (
                        self.component_set(match self.selection_mode {
                            SelectionMode::Face => ComponentKind::Face,
                            SelectionMode::Vertex => ComponentKind::Vertex,
                            _ => ComponentKind::Edge,
                        })
                        .clone(),
                        self.aspect,
                    );
                    match self.selection_mode {
                        SelectionMode::Face => self.camera.frame_faces(&view, &set, aspect),
                        SelectionMode::Vertex => self.camera.frame_vertices(&view, &set, aspect),
                        _ => self.camera.frame_edges(&view, &set, aspect),
                    }
                    .map_err(OperationError::Camera)?;
                    Ok(if self.camera == before {
                        OperationReply::Unchanged
                    } else {
                        OperationReply::Changed
                    })
                })();
                return self.end_fit(original, result);
            }
            Operation::ExportObj {
                path,
                selection_only,
            } => return self.export_obj(&path, selection_only, may_install),
            #[cfg(any(test, feature = "mcp"))]
            Operation::ExportGltf {
                path,
                selection_only,
            } => return self.export_gltf(&path, selection_only, may_install),
            #[cfg(any(test, feature = "mcp"))]
            Operation::ExportUsd {
                path,
                selection_only,
            } => return self.export_usd(&path, selection_only, may_install),
            Operation::Playblast(request) => return self.start_playblast(request),
            Operation::CancelPlayblast => return Ok(self.cancel_playblast()),

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
                return Ok(self.camera_reply(&before));
            }
            Operation::SetCameraFilmBack(edit) => {
                let changed = self
                    .camera
                    .edit_film_back(edit)
                    .map_err(OperationError::FilmBack)?;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::SetImagePlane(edit) => {
                let change =
                    azteccraft::image_plane::ops::set_image_plane(&mut self.image_plane, &edit)
                        .map_err(OperationError::ImagePlane)?;
                return Ok(plate_reply(change));
            }
            Operation::RemoveImagePlane => {
                return Ok(plate_reply(
                    azteccraft::image_plane::ops::remove_image_plane(&mut self.image_plane),
                ));
            }
            Operation::FrameAll { fit_factor } => {
                let original = self.begin_fit(fit_factor)?;
                let result = self.frame_everything();
                return self.end_fit(original, result);
            }
            Operation::UndoView => return Ok(self.view_history.step(&mut self.camera, false)),
            Operation::RedoView => return Ok(self.view_history.step(&mut self.camera, true)),
            Operation::DefaultView => {
                let default = Camera::default();
                let mut next = self.camera.clone();
                next.show_perspective();
                next.set_pose(
                    default.target(),
                    default.distance(),
                    default.yaw(),
                    default.pitch(),
                )
                .map_err(OperationError::CameraPose)?;
                let changed = self.camera != next;
                self.camera = next;
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            Operation::BoxDolly { rect } => {
                let before = self.camera.clone();
                self.camera
                    .box_dolly(rect, self.aspect)
                    .map_err(OperationError::Camera)?;
                return Ok(self.camera_reply(&before));
            }
            Operation::Orbit { .. } if self.camera.projection() == Projection::Orthographic => {
                return Err(OperationError::InvalidInput(
                    "orthographic views cannot be orbited; switch to Persp first".into(),
                ));
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
                return Ok(self.camera_reply(&before));
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::Inspect { page } => {
                return Ok(OperationReply::Scene {
                    inspection: Box::new(self.inspect_page(false, page.as_ref())?),
                });
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::InspectKeys { node, cursor } => {
                return Ok(OperationReply::Keys {
                    inspection: Box::new(self.inspect_keys(node, cursor.as_ref())?),
                });
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::InspectUvs { shape, cursor } => {
                return Ok(OperationReply::Uvs {
                    inspection: Box::new(self.inspect_uvs(shape, cursor.as_ref())?),
                });
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::InspectVertices { shape, cursor } => {
                return Ok(OperationReply::Vertices {
                    inspection: Box::new(self.inspect_vertices(shape, cursor.as_ref())?),
                });
            }
            #[cfg(any(test, feature = "mcp"))]
            Operation::AutosaveNow => {
                return Err(OperationError::InvalidInput(
                    "Autosave was not dispatched".into(),
                ));
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
                | Command::SetSessionMaterialAttribute { .. }
        );
        let reparent_command = matches!(
            command,
            Command::Reparent { .. } | Command::Group { .. } | Command::Ungroup { .. }
        );
        let transform_command = matches!(
            command,
            Command::ResetTransformations { .. }
                | Command::FreezeTransformations { .. }
                | Command::MatchTransformations { .. }
                | Command::AlignObjects { .. }
        );
        let naming_command = matches!(
            command,
            Command::PrefixHierarchyNames { .. } | Command::SearchReplaceNames { .. }
        );
        let key_command = matches!(
            command,
            Command::SetKeys { .. }
                | Command::DeleteKeys { .. }
                | Command::EulerFilter { .. }
                | Command::SetInfinity { .. }
        );
        let reduce_command = matches!(command, Command::ReduceMesh { .. });
        let pivot_command = matches!(
            command,
            Command::SetPivots { .. } | Command::ResetPivots { .. }
        );
        let outcome = self.history.execute(command).map_err(|error| {
            if pivot_command && is_pivot_refusal(&error) {
                return OperationError::Pivot(error);
            }
            if key_command
                && let Some(refusal) = crate::editor_operations::KeysRefusal::from_native(&error)
            {
                return OperationError::Keys(refusal);
            }
            if transform_command && error != azteccraft::native_scene::NativeError::BusyManipulation
            {
                return OperationError::Transform(error);
            }
            if naming_command && error != azteccraft::native_scene::NativeError::BusyManipulation {
                return OperationError::Naming(error);
            }
            if reparent_command && error != azteccraft::native_scene::NativeError::BusyManipulation
            {
                return OperationError::Reparent(error);
            }
            if session_material_command
                && error != azteccraft::native_scene::NativeError::BusyManipulation
            {
                return OperationError::SessionMaterial(error);
            }
            OperationError::from_history(error)
        })?;
        match &outcome {
            EditOutcome::Created(id) => self.after_nodes_created(&[*id])?,
            EditOutcome::Imported(ids) | EditOutcome::Nodes(ids) => {
                self.after_nodes_created(ids)?
            }
            EditOutcome::CreatedJoint(target) => {
                self.clear_remembered_components();
                self.selection.replace(target.clone());
            }
            _ if reduce_command => self.clear_remembered_components(),
            _ => {}
        }
        self.sync_fields();
        Ok(match outcome {
            EditOutcome::Created(id) => OperationReply::Created {
                node_id: id.to_decimal(),
            },
            EditOutcome::Imported(ids) => OperationReply::Imported {
                objects: ids.len(),
                vertices: import_totals.vertices,
                faces: import_totals.faces,
                ignored_records: import_totals.ignored_records,
                uv_objects: import_totals.uv_objects,
                uv_dropped_objects: import_totals.uv_dropped_objects,
                node_ids: ids.iter().map(|id| id.to_decimal()).collect(),
                from_maya,
            },
            EditOutcome::Nodes(ids) => OperationReply::Nodes {
                node_ids: ids.iter().map(|id| id.to_decimal()).collect(),
            },
            EditOutcome::CreatedJoint(target) => OperationReply::Created {
                node_id: target.node.to_decimal(),
            },
            EditOutcome::SessionMaterial(material) => OperationReply::SessionMaterial {
                material_id: material.index().to_string(),
            },
            EditOutcome::Changed => OperationReply::Changed,
            EditOutcome::Unchanged => OperationReply::Unchanged,
        })
    }
    /// Makes the given nodes the object selection, in order with the last as lead, and forgets
    /// remembered component selections. Shared by Create, OBJ import, Group and Ungroup. The
    /// paths are root-only; `sync_fields` follows a nested node to its one real path.
    fn after_nodes_created(&mut self, ids: &[NodeId]) -> Result<(), OperationError> {
        self.clear_remembered_components();
        let mut targets = Vec::new();
        for id in ids {
            let transform = self
                .history
                .scene()
                .node(*id)
                .map_err(OperationError::Scene)?
                .transform_id();
            targets.push(TransformTarget {
                node: *id,
                path: vec![transform],
            });
        }
        match <[TransformTarget; 1]>::try_from(targets) {
            Ok([only]) => self.selection.replace(only),
            // Cannot be full: an import adds at most MAX_OBJECTS roots (see the assertion below).
            Err(targets) => self
                .selection
                .replace_with(targets)
                .map_err(|error| OperationError::InvalidInput(error.to_string()))?,
        }
        Ok(())
    }
    pub(super) fn has_draft(&self) -> bool {
        self.material_draft
            .as_ref()
            .is_some_and(MaterialDraft::changed)
            || self
                .session_color_draft
                .as_ref()
                .is_some_and(SessionColorDraft::changed)
            || self.has_channel_draft()
    }
    /// A name, channel or pivot draft. Pivot drafts share the channel guards, so the remote
    /// `BusyEditing` check and `has_draft` need no pivot case of their own.
    pub(super) fn has_channel_draft(&self) -> bool {
        self.has_transform_draft() || self.has_pivot_draft()
    }
    fn has_transform_draft(&self) -> bool {
        self.selected_node().is_some_and(|node| {
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
    fn has_pivot_draft(&self) -> bool {
        self.selected_node().is_some_and(|node| {
            self.pivot_intents.iter().any(|v| *v)
                || pivot_values(node)
                    .iter()
                    .zip(&self.pivot_drafts)
                    .any(|(value, draft)| draft != &value.to_string())
        })
    }
    fn selected_node(&self) -> Option<&NativeTransformNode> {
        self.selection_presentation()
            .0
            .and_then(|id| self.history.scene().node(id).ok())
    }
    /// The raw `SetPivots` the touched pivot fields ask for, or `None` when no field was touched.
    /// Only a touched pivot is sent, and an untouched component of it keeps the node's own value:
    /// re-sending a value that is not a change would still be a write when a typed text equals it.
    pub(super) fn pivot_command(&self, id: NodeId) -> Result<Option<Command>, String> {
        let node = self.history.scene().node(id).map_err(|e| e.to_string())?;
        let original = pivot_values(node);
        let mut touched = [false; 6];
        let mut values = original;
        for index in 0..6 {
            if self.pivot_intents[index] || self.pivot_drafts[index] != original[index].to_string()
            {
                touched[index] = true;
                values[index] = self.pivot_drafts[index]
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .ok_or("Enter a valid number in the edited pivot")?;
            }
        }
        let triple = |from: usize| {
            touched[from..from + 3]
                .iter()
                .any(|v| *v)
                .then(|| [values[from], values[from + 1], values[from + 2]])
        };
        let (rotate_pivot, scale_pivot) = (triple(0), triple(3));
        Ok(
            (rotate_pivot.is_some() || scale_pivot.is_some()).then_some(Command::SetPivots {
                id,
                rotate_pivot,
                scale_pivot,
                // PROVISIONAL (Q5, P-B4): an Attribute Editor field is a node attribute, so a raw write.
                preserve: false,
            }),
        )
    }
    /// The distinct nodes of the object selection for an edit on the selection. A remote caller
    /// with several objects selected must name the generation it inspected.
    fn selected_object_ids(
        &self,
        expected_generation: Option<u64>,
        provenance: ManipulationProvenance,
    ) -> Result<Vec<NodeId>, OperationError> {
        self.check_generation(expected_generation)?;
        if self.selection.is_empty() {
            return Err(OperationError::NoSelection);
        }
        if self.selection.len() > 1
            && expected_generation.is_none()
            && provenance == ManipulationProvenance::Remote
        {
            return Err(OperationError::MultipleSelection);
        }
        let mut ids = Vec::new();
        for member in self.selection.members() {
            if !ids.contains(&member.node) {
                ids.push(member.node);
            }
        }
        Ok(ids)
    }
    pub(super) fn commit_pending(&mut self) -> bool {
        if self.history.active_manipulation().is_some() {
            self.status = OperationError::BusyManipulation.to_string();
            self.error = true;
            return false;
        }
        if self
            .session_color_draft
            .as_ref()
            .is_some_and(SessionColorDraft::changed)
        {
            if self.has_channel_draft() {
                self.status = "Finish the channel edit before editing material data".into();
                self.error = true;
                return false;
            }
            return self.commit_session_color_draft();
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
        let result = || -> Result<(Option<Command>, Option<Command>), String> {
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
            let edit = self.has_transform_draft().then(|| Command::EditNode {
                id,
                name: (self.name_intent || node.name() != self.name).then(|| self.name.clone()),
                channels,
            });
            Ok((edit, self.pivot_command(id)?))
        };
        let (edit, pivots) = match result() {
            Ok(commands) => commands,
            Err(error) => {
                self.status = error;
                self.error = true;
                return false;
            }
        };
        let mut changed = false;
        let mut failure = None;
        // Two undo entries when both kinds of draft are pending: Edit, then Pivot (documented).
        if let Some(edit) = edit {
            match self.history.execute(edit) {
                Ok(outcome) => changed |= outcome != EditOutcome::Unchanged,
                Err(error) => failure = Some(error.to_string()),
            }
            if failure.is_none() && pivots.is_some() {
                // The channel draft landed; refill it without dropping the pending pivot draft.
                let (drafts, intents) = (self.pivot_drafts.clone(), self.pivot_intents);
                self.sync_fields();
                (self.pivot_drafts, self.pivot_intents) = (drafts, intents);
            }
        }
        if failure.is_none()
            && let Some(pivots) = pivots
        {
            match self.history.execute(pivots) {
                Ok(outcome) => changed |= outcome != EditOutcome::Unchanged,
                Err(error) => failure = Some(error.to_string()),
            }
        }
        if let Some(error) = failure {
            self.status = error;
            self.error = true;
            return false;
        }
        self.sync_fields();
        if changed {
            self.status = "Scene edited".into();
            self.error = false;
        }
        true
    }
}

fn plate_reply(change: azteccraft::image_plane::model::PlateChange) -> OperationReply {
    match change {
        azteccraft::image_plane::model::PlateChange::Changed => OperationReply::Changed,
        azteccraft::image_plane::model::PlateChange::Unchanged => OperationReply::Unchanged,
    }
}
