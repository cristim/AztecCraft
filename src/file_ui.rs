use super::Editor;
#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::DocumentOrigin;
#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::PendingInspection;
use crate::editor_operations::{
    FileActionKind, FileDecision, Operation, OperationError, OperationReply, PendingActionStatus,
};
use azteccraft::{
    document::{
        self, NativeDocument, NativeOrigin, NativeSaveError, SaveDestination, SaveOutcome,
        SaveReceipt,
    },
    history::History,
    native_scene::NativeScene,
    view_scene::ViewScene,
    viewport::Camera,
};
use eframe::egui::{self, Key};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(all(test, unix))]
mod binary_fixture {
    include!("../tests/support/maya_binary.rs");
}

#[derive(Default)]
pub(super) struct FileState {
    path: Option<PathBuf>,
    saved: NativeScene,
    origin: NativeOrigin,
    pending: Option<PendingAction>,
    close_approved: bool,
    close_sent: bool,
    uncertain: Option<UncertainSave>,
}
struct UncertainSave {
    scene_token: Uuid,
    written: NativeScene,
    receipt: SaveReceipt,
    cause: NativeSaveError,
}
struct PendingAction {
    id: Uuid,
    action: FileAction,
    source_changed: bool,
}
enum FileAction {
    New,
    Open {
        document: Box<NativeDocument>,
        path: PathBuf,
        camera: Camera,
        framed_aspect: f32,
    },
    Close,
}
impl FileAction {
    fn kind(&self) -> FileActionKind {
        match self {
            Self::New => FileActionKind::New,
            Self::Open { .. } => FileActionKind::Open,
            Self::Close => FileActionKind::Close,
        }
    }
}
impl FileState {
    pub(super) fn save_available(&self) -> bool {
        self.uncertain.is_some() || self.origin.active_save_available()
    }
    pub(super) fn save_as_available(&self) -> bool {
        self.uncertain.is_some() || self.origin.save_as_available()
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn reconciliation_required(&self) -> bool {
        self.uncertain.is_some()
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn has_file(&self) -> bool {
        self.path.is_some()
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn document_origin(&self) -> DocumentOrigin {
        match self.origin {
            NativeOrigin::Untitled => DocumentOrigin::Untitled,
            NativeOrigin::Binary(_) => DocumentOrigin::MayaBinary,
            NativeOrigin::Legacy(_) => DocumentOrigin::LegacyAztec,
        }
    }
    pub(super) fn is_closing(&self) -> bool {
        self.close_approved
    }
    pub(super) fn is_modal(&self) -> bool {
        self.pending.is_some()
    }
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn pending_inspection(&self) -> Option<PendingInspection> {
        self.pending.as_ref().map(|p| PendingInspection {
            id: p.id.to_string(),
            kind: p.action.kind(),
            source_changed: p.source_changed,
        })
    }
}
impl Editor {
    pub(super) fn document_title(&self) -> String {
        let name = self
            .files
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".into());
        format!(
            "AztecCraft | {name}{}",
            if self.is_dirty() { " *" } else { "" }
        )
    }
    pub(super) fn is_dirty(&self) -> bool {
        self.files.uncertain.is_some()
            || self.history.scene() != &self.files.saved
            || self.has_draft()
    }
    pub(super) fn save_document(
        &mut self,
        destination: SaveDestination<'_>,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        if !self.commit_pending() {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        if let Some(uncertain) = &self.files.uncertain {
            if uncertain.scene_token != self.scene_token {
                return Err(OperationError::InvalidInput(
                    "Save receipt belongs to another document".into(),
                ));
            }
            let (origin, path) = uncertain
                .receipt
                .reconcile()
                .map_err(OperationError::NativeSave)?;
            self.files.saved = uncertain.written.clone();
            self.files.origin = origin;
            self.files.path = Some(path);
            self.files.uncertain = None;
            self.status = "Save reconciled".into();
            self.error = false;
            return Ok(OperationReply::SaveReconciled {
                scene_token: self.scene_token.to_string(),
                dirty: self.is_dirty(),
                pending_action_status: self.pending_save_status(),
            });
        }
        let scene_token = self.scene_token;
        let written = self.history.scene().clone();
        match document::save_native(&self.files.origin, &written, destination, may_install)
            .map_err(OperationError::NativeSave)?
        {
            SaveOutcome::Saved { origin, path } => {
                self.files.saved = written;
                self.files.origin = origin;
                self.files.path = Some(path);
                self.status = "Scene saved".into();
                self.error = false;
                Ok(OperationReply::Saved {
                    scene_token: scene_token.to_string(),
                    dirty: self.is_dirty(),
                    pending_action_status: self.pending_save_status(),
                })
            }
            SaveOutcome::Uncertain { receipt, cause } => {
                self.files.uncertain = Some(UncertainSave {
                    scene_token,
                    written,
                    receipt,
                    cause,
                });
                self.status = format!("Save installed but requires reconciliation: {cause}");
                self.error = true;
                Ok(OperationReply::SaveUncertain {
                    scene_token: scene_token.to_string(),
                    dirty: self.is_dirty(),
                    pending_action_status: self.pending_save_status(),
                    message: cause.to_string(),
                })
            }
        }
    }
    fn pending_save_status(&self) -> PendingActionStatus {
        match &self.files.pending {
            None => PendingActionStatus::None,
            Some(pending) if pending.source_changed => PendingActionStatus::SourceChanged,
            Some(_) => PendingActionStatus::Retained,
        }
    }
    fn request_file_action(
        &mut self,
        action: FileAction,
    ) -> Result<OperationReply, OperationError> {
        if self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        if !self.commit_pending() {
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        if self.is_dirty() {
            let pending = PendingAction {
                id: Uuid::new_v4(),
                action,
                source_changed: false,
            };
            let reply = OperationReply::PendingConfirmation {
                scene_token: self.scene_token.to_string(),
                pending_action_id: pending.id.to_string(),
                kind: pending.action.kind(),
            };
            self.files.pending = Some(pending);
            Ok(reply)
        } else {
            self.apply_file_action(action)
        }
    }
    pub(super) fn request_new(&mut self) {
        self.native_operation(Operation::NewScene);
    }
    pub(super) fn new_document(&mut self) -> Result<OperationReply, OperationError> {
        self.request_file_action(FileAction::New)
    }
    pub(super) fn request_close(&mut self) {
        self.native_operation(Operation::RequestClose);
    }
    pub(super) fn close_document(&mut self) -> Result<OperationReply, OperationError> {
        self.request_file_action(FileAction::Close)
    }
    pub(super) fn open_dialog(&mut self) {
        if self.files.is_modal() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open Maya binary or legacy scene")
            .add_filter("Maya binary scene", &["mb"])
            .add_filter("Legacy AztecCraft scene", &["aztec"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }
    pub(super) fn save_as_dialog(&mut self) {
        if self.files.is_modal() || !self.files.save_as_available() {
            return;
        }
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Save Maya binary scene to a new file")
            .add_filter("Maya binary scene", &["mb"])
            .save_file()
        {
            self.native_operation(Operation::SaveSceneAs { path });
        }
    }
    fn open_path(&mut self, path: &Path) {
        self.native_operation(Operation::OpenScene { path: path.into() });
    }
    pub(super) fn open_candidate(&mut self, path: &Path) -> Result<OperationReply, OperationError> {
        if self.files.is_modal() {
            return Err(OperationError::BusyEditing);
        }
        let document =
            document::load_native(path).map_err(|e| OperationError::File(e.to_string()))?;
        let camera = camera_for_scene(&document.scene, self.aspect)?;
        self.request_file_action(FileAction::Open {
            document: Box::new(document),
            path: path.into(),
            camera,
            framed_aspect: self.aspect,
        })
    }
    fn apply_file_action(&mut self, action: FileAction) -> Result<OperationReply, OperationError> {
        match action {
            FileAction::Close => {
                self.files.close_approved = true;
                Ok(OperationReply::CloseRequested {
                    scene_token: self.scene_token.to_string(),
                })
            }
            FileAction::New => {
                self.replace_scene(
                    NativeScene::default(),
                    NativeOrigin::Untitled,
                    None,
                    Camera::default(),
                );
                Ok(OperationReply::DocumentApplied {
                    scene_token: self.scene_token.to_string(),
                })
            }
            FileAction::Open {
                document,
                path,
                camera,
                ..
            } => {
                let NativeDocument { scene, origin } = *document;
                self.replace_scene(scene, origin, Some(path), camera);
                Ok(OperationReply::DocumentApplied {
                    scene_token: self.scene_token.to_string(),
                })
            }
        }
    }
    fn replace_scene(
        &mut self,
        scene: NativeScene,
        origin: NativeOrigin,
        path: Option<PathBuf>,
        camera: Camera,
    ) {
        self.scene_token = Uuid::new_v4();
        self.files.saved = scene.clone();
        self.files.origin = origin;
        self.files.close_approved = false;
        self.files.close_sent = false;
        self.files.path = path;
        self.files.uncertain = None;
        self.history = History::new(scene);
        self.selected = None;
        self.selected_path.clear();
        self.remembered_face = None;
        self.name.clear();
        self.channels = Default::default();
        self.channel_intents = [false; 9];
        self.name_intent = false;
        self.material_shape = None;
        self.material_draft = None;
        self.camera = camera;
        self.status = "Scene ready".into();
        self.error = false;
    }
    #[cfg(test)]
    pub(super) fn resolve_file_action(
        &mut self,
        id: Uuid,
        decision: FileDecision,
    ) -> Result<OperationReply, OperationError> {
        self.resolve_file_action_guarded(id, decision, &|| true)
    }
    pub(super) fn resolve_file_action_guarded(
        &mut self,
        id: Uuid,
        decision: FileDecision,
        may_install: &dyn Fn() -> bool,
    ) -> Result<OperationReply, OperationError> {
        if self.files.pending.as_ref().is_none_or(|p| p.id != id) {
            return Err(OperationError::StalePendingAction);
        }
        let mut saved_reply = None;
        if !matches!(decision, FileDecision::Cancel)
            && self
                .files
                .pending
                .as_ref()
                .is_some_and(|p| p.source_changed)
        {
            return Err(OperationError::NativeSave(NativeSaveError::SourceChanged));
        }
        if matches!(decision, FileDecision::Save) {
            let mut reply = self.save_document(SaveDestination::Active, may_install)?;
            if matches!(reply, OperationReply::SaveUncertain { .. }) {
                return Ok(reply);
            }
            if let Some(PendingAction {
                action: FileAction::Open { document, path, .. },
                source_changed,
                ..
            }) = &mut self.files.pending
                && document::verify_native_origin(path, &document.origin).is_err()
            {
                *source_changed = true;
            }
            let pending_status = self.pending_save_status();
            match &mut reply {
                OperationReply::Saved {
                    pending_action_status,
                    ..
                }
                | OperationReply::SaveReconciled {
                    pending_action_status,
                    ..
                } => {
                    *pending_action_status = pending_status;
                }
                _ => {}
            }
            if pending_status == PendingActionStatus::SourceChanged || self.is_dirty() {
                return Ok(reply);
            }
            saved_reply = Some(reply);
        }
        if matches!(decision, FileDecision::Discard | FileDecision::Save)
            && let Some(PendingAction {
                action:
                    FileAction::Open {
                        document,
                        camera,
                        framed_aspect,
                        ..
                    },
                ..
            }) = &mut self.files.pending
            && *framed_aspect != self.aspect
        {
            let reframed = match camera_for_scene(&document.scene, self.aspect) {
                Ok(camera) => camera,
                Err(error) => return saved_reply.map_or_else(|| Err(error), Ok),
            };
            *camera = reframed;
            *framed_aspect = self.aspect;
        }
        if let Some(reply) = saved_reply
            && !may_install()
        {
            return Ok(reply);
        }
        let pending = self
            .files
            .pending
            .take()
            .ok_or(OperationError::StalePendingAction)?;
        match decision {
            FileDecision::Cancel => Ok(OperationReply::Cancelled {
                scene_token: self.scene_token.to_string(),
            }),
            FileDecision::Discard | FileDecision::Save => self.apply_file_action(pending.action),
        }
    }
    #[cfg(test)]
    fn discard_pending(&mut self) {
        if let Some(id) = self.files.pending.as_ref().map(|p| p.id) {
            self.resolve_file_action(id, FileDecision::Discard).unwrap();
        }
    }
    fn close_ready(&mut self) -> bool {
        #[cfg(feature = "mcp")]
        if let Some(bridge) = &mut self.mcp {
            match bridge.poll_close_delivery() {
                Some(crate::mcp::CloseDelivery::Waiting) => return false,
                Some(crate::mcp::CloseDelivery::Failed) => {
                    self.files.close_approved = false;
                    self.files.close_sent = false;
                    self.status = "Close response was not delivered; the scene remains open. Inspect before retrying.".into();
                    self.error = true;
                    return false;
                }
                Some(crate::mcp::CloseDelivery::Flushed) | None => {}
            }
        }
        self.files.close_approved
    }
    fn send_close_when_ready(&mut self, ctx: &egui::Context) {
        if !self.files.close_sent && self.close_ready() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            self.files.close_sent = true;
        }
    }
    pub(super) fn handle_close(&mut self, ctx: &egui::Context) {
        let ready = self.close_ready();
        if ctx.input(|i| i.viewport().close_requested()) && !ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.files.close_approved && !self.native_input_pending {
                self.request_close();
            }
        }
        self.send_close_when_ready(ctx);
    }
    pub(super) fn file_modal(&mut self, ctx: &egui::Context) {
        self.send_close_when_ready(ctx);
        let Some(id) = self.files.pending.as_ref().map(|p| p.id) else {
            return;
        };
        let mut decision = ctx
            .input(|i| i.key_pressed(Key::Escape))
            .then_some(FileDecision::Cancel);
        egui::Modal::new(egui::Id::new("unsaved_scene")).show(ctx, |ui| {
            ui.heading("Discard changes to this scene?");
            if let Some(uncertain) = &self.files.uncertain {
                ui.label(format!(
                    "A destination may already have been written: {}",
                    uncertain.cause
                ));
                ui.label("Discard abandons reconciliation and the in-memory edits.");
            }
            if self
                .files
                .pending
                .as_ref()
                .is_some_and(|pending| pending.source_changed)
            {
                ui.label("The queued Open source changed. Cancel and reopen the file.");
            }
            ui.label("Discarding changes cannot be undone.");
            ui.horizontal(|ui| {
                let source_changed = self
                    .files
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.source_changed);
                if ui
                    .add_enabled(
                        !source_changed && self.files.save_available(),
                        egui::Button::new("Save"),
                    )
                    .clicked()
                {
                    decision = Some(FileDecision::Save);
                }
                if ui
                    .add_enabled(!source_changed, egui::Button::new("Discard"))
                    .clicked()
                {
                    decision = Some(FileDecision::Discard);
                }
                if ui.button("Cancel").clicked() {
                    decision = Some(FileDecision::Cancel);
                }
            });
        });
        if let Some(decision) = decision {
            if let Err(error) = self.apply_operation(Operation::ResolveFileAction {
                pending_id: id,
                decision,
            }) {
                self.status = error.to_string();
                self.error = true;
            }
            self.send_close_when_ready(ctx);
        }
    }
}

fn camera_for_scene(scene: &NativeScene, aspect: f32) -> Result<Camera, OperationError> {
    let view = ViewScene::from_native(scene, 256 * 1024 * 1024)
        .map_err(|error| OperationError::File(error.to_string()))?;
    let mut camera = Camera::default();
    if view
        .instances()
        .iter()
        .any(|instance| !instance.world_vertices().is_empty())
    {
        camera
            .frame_view(&view, None, aspect)
            .map_err(OperationError::Camera)?;
    }
    Ok(camera)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor_operations::SelectionMode;
    use azteccraft::native_scene::FaceTarget;
    use azteccraft::scene::Primitive;

    #[cfg(unix)]
    fn relocate_completed_stage_once(
        parent: &Path,
        moved: &Path,
        expected: &[u8],
        relocated: &std::cell::Cell<bool>,
    ) -> bool {
        if !relocated.get()
            && std::fs::read_dir(parent).unwrap().any(|entry| {
                let entry = entry.unwrap();
                entry.file_name().to_string_lossy().ends_with(".craft_tmp")
                    && std::fs::read(entry.path()).ok().as_deref() == Some(expected)
            })
        {
            std::fs::rename(parent, moved).unwrap();
            relocated.set(true);
        }
        true
    }

    #[cfg(unix)]
    #[test]
    fn native_save_as_active_save_and_history_use_one_live_document() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.mb");
        let destination = directory.path().join("saved.mb");
        let original = binary_fixture::specimen();
        std::fs::write(&source, &original).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let camera = editor.camera.clone();
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let first = editor.history.scene().clone();
        assert!(matches!(
            editor
                .apply_operation(Operation::SaveSceneAs {
                    path: destination.clone()
                })
                .unwrap(),
            OperationReply::Saved {
                dirty: false,
                pending_action_status: PendingActionStatus::None,
                ..
            }
        ));
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.history.scene(), &first);
        assert_eq!(
            document::load_native(&destination)
                .unwrap()
                .scene
                .node(id)
                .unwrap()
                .transform()
                .translation,
            transform.translation
        );
        transform.translation[0] += 0.5;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let second = editor.history.scene().clone();
        assert!(matches!(
            editor.apply_operation(Operation::SaveScene).unwrap(),
            OperationReply::Saved { dirty: false, .. }
        ));
        editor.apply_operation(Operation::Undo).unwrap();
        assert_eq!(editor.history.scene(), &first);
        assert!(editor.is_dirty());
        editor.apply_operation(Operation::Redo).unwrap();
        assert_eq!(editor.history.scene(), &second);
        assert!(!editor.is_dirty());
        let bytes = std::fs::read(&destination).unwrap();
        editor.apply_operation(Operation::SaveScene).unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), bytes);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.camera, camera);
        let inspection = editor.inspect(false).unwrap();
        assert!(inspection.has_file && inspection.save_available && inspection.save_as_available);
        assert!(!inspection.save_reconciliation_required);
        assert_eq!(inspection.document_origin, DocumentOrigin::MayaBinary);
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_save_as_reconciles_original_destination_without_a_second_install() {
        use std::{cell::Cell, os::unix::fs::MetadataExt};
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("original.mb");
        let parent = directory.path().join("destination");
        let moved = directory.path().join("moved");
        std::fs::create_dir(&parent).unwrap();
        let destination = parent.join("saved.mb");
        let original = binary_fixture::specimen();
        std::fs::write(&source, &original).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let old_saved = editor.files.saved.clone();
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let first = editor.history.scene().clone();
        let expected = azteccraft::native_scene::binary::encode(&first).unwrap();
        let relocated = Cell::new(false);
        let guard = || relocate_completed_stage_once(&parent, &moved, &expected, &relocated);
        assert!(matches!(
            editor
                .apply_operation_guarded(
                    Operation::SaveSceneAs {
                        path: destination.clone()
                    },
                    &guard
                )
                .unwrap(),
            OperationReply::SaveUncertain { .. }
        ));
        assert!(relocated.get());
        assert_eq!(editor.files.path.as_ref(), Some(&source));
        assert_eq!(editor.files.saved, old_saved);
        assert!(document::verify_native_origin(&source, &editor.files.origin).is_ok());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        std::fs::rename(&moved, &parent).unwrap();
        let inode = std::fs::metadata(&destination).unwrap().ino();
        transform.translation[0] += 0.5;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let second = editor.history.scene().clone();
        let occupied = parent.join("occupied.mb");
        std::fs::write(&occupied, b"keep").unwrap();
        assert!(matches!(
            editor
                .apply_operation(Operation::SaveSceneAs {
                    path: occupied.clone()
                })
                .unwrap(),
            OperationReply::SaveReconciled { dirty: true, .. }
        ));
        assert_eq!(
            editor.files.path.as_ref(),
            Some(&destination.canonicalize().unwrap())
        );
        assert_eq!(editor.files.saved, first);
        assert_eq!(editor.history.scene(), &second);
        assert_eq!(editor.scene_token, token);
        assert!(editor.files.uncertain.is_none() && editor.is_dirty());
        assert_eq!(std::fs::read(&destination).unwrap(), expected);
        assert_eq!(std::fs::metadata(&destination).unwrap().ino(), inode);
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
        assert_eq!(std::fs::read(&source).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn dirty_reconciliation_verifies_queued_open_before_returning() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("document");
        let moved = directory.path().join("moved");
        std::fs::create_dir(&parent).unwrap();
        let source = parent.join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let first = editor.history.scene().clone();
        let expected = azteccraft::native_scene::binary::encode(&first).unwrap();
        let relocated = std::cell::Cell::new(false);
        let guard = || relocate_completed_stage_once(&parent, &moved, &expected, &relocated);
        assert!(matches!(
            editor
                .apply_operation_guarded(Operation::SaveScene, &guard)
                .unwrap(),
            OperationReply::SaveUncertain { .. }
        ));
        std::fs::rename(&moved, &parent).unwrap();
        transform.translation[0] += 0.5;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let second = editor.history.scene().clone();
        let candidate = directory.path().join("candidate.aztec");
        write_legacy_cube(&candidate, 0.0);
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor
            .apply_operation(Operation::OpenScene {
                path: candidate.clone(),
            })
            .unwrap()
        else {
            panic!("expected pending Open")
        };
        let pending = Uuid::parse_str(&pending_action_id).unwrap();
        std::fs::write(&candidate, b"changed candidate").unwrap();
        assert!(matches!(
            editor
                .apply_operation(Operation::ResolveFileAction {
                    pending_id: pending,
                    decision: FileDecision::Save
                })
                .unwrap(),
            OperationReply::SaveReconciled {
                dirty: true,
                pending_action_status: PendingActionStatus::SourceChanged,
                ..
            }
        ));
        assert_eq!(editor.files.saved, first);
        assert_eq!(editor.history.scene(), &second);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
        assert!(editor.files.pending.as_ref().unwrap().source_changed);
        assert!(matches!(
            editor.apply_operation(Operation::ResolveFileAction {
                pending_id: pending,
                decision: FileDecision::Discard
            }),
            Err(OperationError::NativeSave(NativeSaveError::SourceChanged))
        ));
        assert_eq!(editor.history.scene(), &second);
        assert_eq!(std::fs::read(&source).unwrap(), expected);
    }

    #[cfg(all(unix, feature = "mcp"))]
    #[test]
    fn envelope_guard_cancels_active_after_stage() {
        check_envelope_install_guard(false, false);
    }

    #[cfg(all(unix, feature = "mcp"))]
    #[test]
    fn envelope_guard_expires_active_after_stage() {
        check_envelope_install_guard(false, true);
    }

    #[cfg(all(unix, feature = "mcp"))]
    #[test]
    fn envelope_guard_cancels_save_as_after_stage() {
        check_envelope_install_guard(true, false);
    }

    #[cfg(all(unix, feature = "mcp"))]
    #[test]
    fn envelope_guard_expires_save_as_after_stage() {
        check_envelope_install_guard(true, true);
    }

    #[cfg(all(unix, feature = "mcp"))]
    fn check_envelope_install_guard(save_as: bool, expire: bool) {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::time::{Duration, Instant};
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("scene.mb");
        let destination = directory.path().join("saved.mb");
        let original = binary_fixture::specimen();
        std::fs::write(&source, &original).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let saved = editor.files.saved.clone();
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let before = editor.history.scene().clone();
        let expected = azteccraft::native_scene::binary::encode(&before).unwrap();
        let context = egui::Context::default();
        let (mut bridge, ingress) = crate::mcp::test_bridge(context.clone());
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancelled = cancellation.clone();
        let deadline = Instant::now() + Duration::from_secs(1);
        let observed = Arc::new(AtomicBool::new(false));
        let seen = observed.clone();
        let parent = directory.path().to_path_buf();
        let probe = move || {
            if !seen.load(Ordering::SeqCst)
                && std::fs::read_dir(&parent).unwrap().any(|entry| {
                    let entry = entry.unwrap();
                    entry.file_name().to_string_lossy().ends_with(".craft_tmp")
                        && std::fs::read(entry.path()).ok().as_deref() == Some(expected.as_slice())
                })
            {
                seen.store(true, Ordering::SeqCst);
                if expire {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                } else {
                    cancelled.cancel();
                }
            }
        };
        let operation = if save_as {
            Operation::SaveSceneAs {
                path: destination.clone(),
            }
        } else {
            Operation::SaveScene
        };
        let mut reply = ingress
            .queue_with_save_probe(
                crate::editor_operations::RemoteRequest {
                    expected_scene_token: Some(token),
                    operation,
                },
                cancellation,
                deadline,
                Some(Box::new(probe)),
            )
            .unwrap();
        bridge.dispatch(&mut editor, &context);
        assert!(observed.load(Ordering::SeqCst));
        assert!(matches!(
            reply.try_recv().unwrap(),
            Err(OperationError::NativeSave(NativeSaveError::Cancelled))
        ));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!destination.exists());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.files.saved, saved);
        assert_eq!(editor.files.path.as_ref(), Some(&source));
        assert_eq!(editor.scene_token, token);
        assert!(editor.files.uncertain.is_none() && editor.is_dirty());
        assert!(!editor.files.close_approved);
    }

    #[cfg(unix)]
    #[test]
    fn installed_save_uncertainty_reconciles_s1_without_reinstalling_or_replacing_s2() {
        use std::{cell::Cell, os::unix::fs::MetadataExt};
        for initial_delta in [0.0, 1.0] {
            let directory = tempfile::tempdir().unwrap();
            let parent = directory.path().join("document");
            let moved = directory.path().join("moved");
            std::fs::create_dir(&parent).unwrap();
            let source = parent.join("scene.mb");
            std::fs::write(&source, binary_fixture::specimen()).unwrap();
            let mut editor = Editor::default();
            editor
                .apply_operation(Operation::OpenScene {
                    path: source.clone(),
                })
                .unwrap();
            let token = editor.scene_token;
            let original_saved = editor.files.saved.clone();
            let id = editor.history.scene().nodes()[0].id();
            let mut transform = editor
                .transform_input(editor.history.scene().node(id).unwrap().transform())
                .unwrap();
            transform.translation[0] += initial_delta;
            editor
                .apply_operation(Operation::SetTransform { id, transform })
                .unwrap();
            let first = editor.history.scene().clone();
            let expected = azteccraft::native_scene::binary::encode(&first).unwrap();
            let relocated = Cell::new(false);
            let guard = || relocate_completed_stage_once(&parent, &moved, &expected, &relocated);
            let reply = editor
                .apply_operation_guarded(Operation::SaveScene, &guard)
                .unwrap();
            assert!(relocated.get());
            assert!(matches!(
                reply,
                OperationReply::SaveUncertain { dirty: true, .. }
            ));
            assert_eq!(
                editor.files.uncertain.as_ref().unwrap().cause,
                NativeSaveError::SourceChanged
            );
            assert_eq!(editor.files.saved, original_saved);
            assert_eq!(editor.history.scene(), &first);
            assert_eq!(editor.scene_token, token);
            let installed = moved.join("scene.mb");
            assert_eq!(std::fs::read(&installed).unwrap(), expected);
            let inode = std::fs::metadata(&installed).unwrap().ino();
            assert!(
                editor
                    .apply_operation(Operation::OpenScene {
                        path: source.clone()
                    })
                    .is_err()
            );
            assert!(editor.files.uncertain.is_some());
            assert_eq!(editor.history.scene(), &first);
            let OperationReply::PendingConfirmation {
                pending_action_id, ..
            } = editor.apply_operation(Operation::NewScene).unwrap()
            else {
                panic!("expected pending New");
            };
            editor
                .apply_operation(Operation::ResolveFileAction {
                    pending_id: Uuid::parse_str(&pending_action_id).unwrap(),
                    decision: FileDecision::Cancel,
                })
                .unwrap();
            assert!(editor.files.uncertain.is_some());
            assert_eq!(editor.scene_token, token);
            editor.files.uncertain.as_mut().unwrap().scene_token = Uuid::new_v4();
            assert!(matches!(
                editor.apply_operation(Operation::SaveScene),
                Err(OperationError::InvalidInput(_))
            ));
            assert!(editor.files.uncertain.is_some());
            editor.files.uncertain.as_mut().unwrap().scene_token = token;
            std::fs::rename(&moved, &parent).unwrap();
            transform.translation[0] += 0.5;
            editor
                .apply_operation(Operation::SetTransform { id, transform })
                .unwrap();
            let second = editor.history.scene().clone();
            let occupied = parent.join("occupied.mb");
            std::fs::write(&occupied, b"keep").unwrap();
            let reply = if initial_delta == 1.0 {
                let OperationReply::PendingConfirmation {
                    pending_action_id, ..
                } = editor.apply_operation(Operation::NewScene).unwrap()
                else {
                    panic!("expected pending New");
                };
                let pending = Uuid::parse_str(&pending_action_id).unwrap();
                let reply = editor
                    .apply_operation(Operation::ResolveFileAction {
                        pending_id: pending,
                        decision: FileDecision::Save,
                    })
                    .unwrap();
                assert!(matches!(
                    reply,
                    OperationReply::SaveReconciled {
                        dirty: true,
                        pending_action_status: PendingActionStatus::Retained,
                        ..
                    }
                ));
                assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
                assert_eq!(editor.scene_token, token);
                editor
                    .apply_operation(Operation::ResolveFileAction {
                        pending_id: pending,
                        decision: FileDecision::Cancel,
                    })
                    .unwrap();
                reply
            } else {
                editor
                    .apply_operation(Operation::SaveSceneAs {
                        path: occupied.clone(),
                    })
                    .unwrap()
            };
            assert!(matches!(
                reply,
                OperationReply::SaveReconciled { dirty: true, .. }
            ));
            assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
            assert_eq!(std::fs::read(&source).unwrap(), expected);
            assert_eq!(std::fs::metadata(&source).unwrap().ino(), inode);
            assert_eq!(editor.files.saved, first);
            assert_eq!(editor.history.scene(), &second);
            assert_eq!(editor.scene_token, token);
            assert!(editor.files.uncertain.is_none() && editor.is_dirty());
            assert!(matches!(
                editor.apply_operation(Operation::SaveScene).unwrap(),
                OperationReply::Saved { dirty: false, .. }
            ));
            assert_eq!(
                document::load_native(&source).unwrap().scene.nodes()[0]
                    .transform()
                    .translation,
                transform.translation
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn save_acknowledges_s1_but_blocks_the_same_file_queued_open_s0() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let first = editor.history.scene().clone();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap()
        else {
            panic!("expected queued Open");
        };
        let pending = Uuid::parse_str(&pending_action_id).unwrap();
        let reply = editor
            .apply_operation(Operation::ResolveFileAction {
                pending_id: pending,
                decision: FileDecision::Save,
            })
            .unwrap();
        assert!(matches!(
            reply,
            OperationReply::Saved {
                dirty: false,
                pending_action_status: PendingActionStatus::SourceChanged,
                ..
            }
        ));
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &first);
        assert_eq!(editor.files.saved, first);
        assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
        assert!(editor.files.pending.as_ref().unwrap().source_changed);
        assert!(matches!(
            editor.apply_operation(Operation::ResolveFileAction {
                pending_id: pending,
                decision: FileDecision::Discard
            }),
            Err(OperationError::NativeSave(NativeSaveError::SourceChanged))
        ));
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &first);
        editor
            .apply_operation(Operation::ResolveFileAction {
                pending_id: pending,
                decision: FileDecision::Cancel,
            })
            .unwrap();
        assert!(editor.files.pending.is_none());
        assert!(!editor.is_dirty());
        assert!(matches!(
            editor
                .apply_operation(Operation::OpenScene { path: source })
                .unwrap(),
            OperationReply::DocumentApplied { .. }
        ));
        assert_ne!(editor.scene_token, token);
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation,
            transform.translation
        );
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_after_install_acknowledges_save_without_consuming_pending_new() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("scene.mb");
        std::fs::write(&source, binary_fixture::specimen()).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene {
                path: source.clone(),
            })
            .unwrap();
        let token = editor.scene_token;
        let id = editor.history.scene().nodes()[0].id();
        let mut transform = editor
            .transform_input(editor.history.scene().node(id).unwrap().transform())
            .unwrap();
        transform.translation[0] += 1.0;
        editor
            .apply_operation(Operation::SetTransform { id, transform })
            .unwrap();
        let first = editor.history.scene().clone();
        let expected = azteccraft::native_scene::binary::encode(&first).unwrap();
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor.apply_operation(Operation::NewScene).unwrap()
        else {
            panic!("expected queued New");
        };
        let pending = Uuid::parse_str(&pending_action_id).unwrap();
        let guard = || std::fs::read(&source).ok().as_deref() != Some(expected.as_slice());
        let reply = editor
            .apply_operation_guarded(
                Operation::ResolveFileAction {
                    pending_id: pending,
                    decision: FileDecision::Save,
                },
                &guard,
            )
            .unwrap();
        assert!(matches!(
            reply,
            OperationReply::Saved {
                dirty: false,
                pending_action_status: PendingActionStatus::Retained,
                ..
            }
        ));
        assert_eq!(std::fs::read(&source).unwrap(), expected);
        assert_eq!(editor.files.saved, first);
        assert_eq!(editor.history.scene(), &first);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
        assert!(!editor.files.close_approved);
        editor
            .apply_operation(Operation::ResolveFileAction {
                pending_id: pending,
                decision: FileDecision::Cancel,
            })
            .unwrap();
        assert_eq!(editor.history.scene(), &first);
        assert!(!editor.is_dirty());
    }

    #[cfg(unix)]
    #[test]
    fn native_save_as_from_read_through_symlink_preserves_original_and_verifies_origin() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("original.mb");
        let link = directory.path().join("link.mb");
        let destination = directory.path().join("fresh.mb");
        let original = binary_fixture::specimen();
        std::fs::write(&source, &original).unwrap();
        std::os::unix::fs::symlink(&source, &link).unwrap();
        let mut editor = Editor::default();
        editor
            .apply_operation(Operation::OpenScene { path: link.clone() })
            .unwrap();
        let origin = editor.files.origin.clone();
        assert!(!editor.files.save_available());
        assert!(editor.files.save_as_available());
        assert!(document::verify_native_origin(&link, &origin).is_ok());
        assert!(matches!(
            editor.apply_operation(Operation::SaveScene),
            Err(OperationError::NativeSave(NativeSaveError::Unsupported))
        ));
        assert!(matches!(
            editor
                .apply_operation(Operation::SaveSceneAs {
                    path: destination.clone()
                })
                .unwrap(),
            OperationReply::Saved { dirty: false, .. }
        ));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read(&destination).unwrap(), original);
        assert!(editor.files.save_available());
        assert!(document::verify_native_origin(&destination, &editor.files.origin).is_ok());
        let mut changed = original.clone();
        changed[0] ^= 1;
        std::fs::write(&source, &changed).unwrap();
        assert_eq!(
            document::verify_native_origin(&link, &origin),
            Err(NativeSaveError::SourceChanged)
        );
        assert_eq!(std::fs::read(&source).unwrap(), changed);
    }

    #[cfg(unix)]
    #[test]
    fn uncertain_receipt_is_discarded_only_after_successful_new_or_open_replacement() {
        for open in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let parent = directory.path().join("document");
            let moved = directory.path().join("moved");
            std::fs::create_dir(&parent).unwrap();
            let source = parent.join("scene.mb");
            std::fs::write(&source, binary_fixture::specimen()).unwrap();
            let mut editor = Editor::default();
            editor
                .apply_operation(Operation::OpenScene {
                    path: source.clone(),
                })
                .unwrap();
            let token = editor.scene_token;
            let before = editor.history.scene().clone();
            let expected = azteccraft::native_scene::binary::encode(&before).unwrap();
            let relocated = std::cell::Cell::new(false);
            let guard = || relocate_completed_stage_once(&parent, &moved, &expected, &relocated);
            assert!(matches!(
                editor
                    .apply_operation_guarded(Operation::SaveScene, &guard)
                    .unwrap(),
                OperationReply::SaveUncertain { dirty: true, .. }
            ));
            assert!(relocated.get());
            assert_eq!(editor.history.scene(), &editor.files.saved);
            assert!(editor.is_dirty());
            std::fs::rename(&moved, &parent).unwrap();
            let candidate = directory.path().join("candidate.aztec");
            write_legacy_cube(&candidate, 0.0);
            let action = if open {
                Operation::OpenScene { path: candidate }
            } else {
                Operation::NewScene
            };
            let OperationReply::PendingConfirmation {
                pending_action_id, ..
            } = editor.apply_operation(action).unwrap()
            else {
                panic!("expected pending replacement");
            };
            let pending = Uuid::parse_str(&pending_action_id).unwrap();
            if open {
                editor.aspect = f32::NAN;
                assert!(
                    editor
                        .apply_operation(Operation::ResolveFileAction {
                            pending_id: pending,
                            decision: FileDecision::Discard
                        })
                        .is_err()
                );
                assert!(editor.files.uncertain.is_some());
                assert_eq!(editor.scene_token, token);
                assert_eq!(editor.history.scene(), &before);
                assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
                editor.aspect = 1.0;
            }
            assert!(matches!(
                editor
                    .apply_operation(Operation::ResolveFileAction {
                        pending_id: pending,
                        decision: FileDecision::Discard
                    })
                    .unwrap(),
                OperationReply::DocumentApplied { .. }
            ));
            assert!(editor.files.uncertain.is_none());
            assert_ne!(editor.scene_token, token);
            assert!(!editor.is_dirty());
            assert_eq!(std::fs::read(&source).unwrap(), expected);
        }
    }

    mod maya_binary {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_binary.rs"
        ));
    }

    #[test]
    fn face_lifecycle_preserves_imported_source_and_cancelled_files_but_clears_replacement_ids() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("readonly.mb");
        let bytes = maya_binary::specimen();
        std::fs::write(&path, &bytes).unwrap();
        let other = directory.path().join("other.aztec");
        write_legacy_cube(&other, 0.0);
        let bad = directory.path().join("malformed.mb");
        std::fs::write(&bad, b"malformed").unwrap();
        let mut editor = Editor::default();
        editor.open_path(&path);
        assert!(!editor.error);
        let source = editor.files.origin.source_bytes().unwrap().clone();
        let scene = editor.history.scene().clone();
        let saved = editor.files.saved.clone();
        let token = editor.scene_token;
        let camera = editor.camera.clone();
        let view = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
        let instance = &view.instances()[0];
        let target = FaceTarget {
            transform: instance.transform(),
            shape: instance.shape(),
            path: instance.path().to_vec(),
            face_index: instance.faces()[0].source_index(),
        };
        for mode in [
            SelectionMode::Face,
            SelectionMode::Object,
            SelectionMode::Face,
        ] {
            editor.native_operation(Operation::SetSelectionMode(mode));
            assert!(!editor.error);
            if mode == SelectionMode::Face {
                editor.native_operation(Operation::SelectFace {
                    target: Some(target.clone()),
                });
                assert!(!editor.error);
            }
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.files.saved, saved);
            assert!(std::sync::Arc::ptr_eq(
                editor.files.origin.source_bytes().unwrap(),
                &source
            ));
            assert_eq!(source.as_ref(), bytes);
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.camera, camera);
            assert!(!editor.is_dirty());
            assert!(!editor.history.can_undo());
            assert!(!editor.history.can_redo());
        }
        let selection = (
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
        );
        editor.open_path(&bad);
        assert!(editor.error);
        assert_eq!(
            (
                editor.selected,
                editor.selected_path.clone(),
                editor.remembered_face.clone()
            ),
            selection
        );
        let name = editor.name.clone();
        editor.name = "pending_human_name".into();
        for candidate in [None, Some(&other)] {
            let reply = match candidate {
                None => editor.new_document(),
                Some(candidate) => editor.open_candidate(candidate),
            };
            assert!(matches!(reply, Err(OperationError::InvalidInput(_))));
            assert!(!editor.files.is_modal());
            assert_eq!(editor.selection_mode, SelectionMode::Face);
            assert_eq!(
                (
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone()
                ),
                selection
            );
            assert_eq!(editor.name, "pending_human_name");
            assert_eq!(editor.history.scene(), &scene);
            assert_eq!(editor.files.saved, saved);
            assert_eq!(editor.files.path.as_ref(), Some(&path));
            assert!(std::sync::Arc::ptr_eq(
                editor.files.origin.source_bytes().unwrap(),
                &source
            ));
            assert_eq!(editor.scene_token, token);
            assert_eq!(editor.camera, camera);
            assert!(editor.is_dirty());
            assert!(!editor.history.can_undo());
            assert!(!editor.history.can_redo());
        }
        editor.name = name;
        editor.new_document().unwrap();
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.selected, None);
        assert!(editor.selected_path.is_empty());
        assert_eq!(editor.remembered_face, None);
        assert_ne!(editor.scene_token, token);
        editor.open_path(&path);
        assert!(!editor.error);
        assert_eq!(editor.remembered_face, None);
        let reopened = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
        let instance = &reopened.instances()[0];
        let target = FaceTarget {
            transform: instance.transform(),
            shape: instance.shape(),
            path: instance.path().to_vec(),
            face_index: instance.faces()[0].source_index(),
        };
        editor.native_operation(Operation::SelectFace {
            target: Some(target),
        });
        assert!(!editor.error);
        editor.open_path(&other);
        assert!(!editor.error);
        assert_eq!(editor.selection_mode, SelectionMode::Face);
        assert_eq!(editor.selected, None);
        assert!(editor.selected_path.is_empty());
        assert_eq!(editor.remembered_face, None);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
        let instance = &view.instances()[0];
        editor.native_operation(Operation::SelectFace {
            target: Some(FaceTarget {
                transform: instance.transform(),
                shape: instance.shape(),
                path: instance.path().to_vec(),
                face_index: instance.faces()[0].source_index(),
            }),
        });
        assert!(!editor.error);
        let legacy_source = editor.files.origin.source_bytes().unwrap().clone();
        let legacy_saved = editor.files.saved.clone();
        let legacy_token = editor.scene_token;
        let legacy_camera = editor.camera.clone();
        let legacy_selection = (
            editor.selected,
            editor.selected_path.clone(),
            editor.remembered_face.clone(),
        );
        editor.name = "committed_before_cancel".into();
        for candidate in [None, Some(&path)] {
            let reply = match candidate {
                None => editor.new_document(),
                Some(candidate) => editor.open_candidate(candidate),
            }
            .unwrap();
            assert!(matches!(reply, OperationReply::PendingConfirmation { .. }));
            let post_commit = editor.history.scene().clone();
            assert_eq!(
                post_commit.node(instance.transform()).unwrap().name(),
                "committed_before_cancel"
            );
            let pending = editor.files.pending.as_ref().unwrap().id;
            editor
                .resolve_file_action(pending, FileDecision::Cancel)
                .unwrap();
            assert_eq!(editor.history.scene(), &post_commit);
            assert_eq!(editor.selection_mode, SelectionMode::Face);
            assert_eq!(
                (
                    editor.selected,
                    editor.selected_path.clone(),
                    editor.remembered_face.clone()
                ),
                legacy_selection
            );
            assert_eq!(editor.files.saved, legacy_saved);
            assert_eq!(editor.files.path.as_ref(), Some(&other));
            assert!(std::sync::Arc::ptr_eq(
                editor.files.origin.source_bytes().unwrap(),
                &legacy_source
            ));
            assert_eq!(editor.scene_token, legacy_token);
            assert_eq!(editor.camera, legacy_camera);
            assert!(editor.history.can_undo());
            assert!(!editor.history.can_redo());
            assert!(editor.is_dirty());
        }
    }

    fn write_legacy_cube(path: &Path, translation: f32) -> azteccraft::scene::NodeId {
        let mut scene = azteccraft::scene::Scene::default();
        let id = scene.create(Primitive::Cube, "saved_cube").unwrap();
        scene
            .set_transform(
                id,
                azteccraft::scene::Transform {
                    translation: glam::Vec3::new(translation, 0.0, 0.0),
                    ..Default::default()
                },
            )
            .unwrap();
        document::save_new(path, &scene).unwrap();
        id
    }

    #[test]
    fn pending_open_reframes_staged_bytes_after_resize_and_failed_reframe_preserves_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("staged.aztec");
        write_legacy_cube(&path, 20.0);
        let original_bytes = std::fs::read(&path).unwrap();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.aspect = 4.0;
        assert!(matches!(
            editor.open_candidate(&path).unwrap(),
            OperationReply::PendingConfirmation { .. }
        ));
        let pending = editor.files.pending.as_ref().unwrap().id;
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let camera = editor.camera.clone();
        std::fs::write(&path, b"changed after staging").unwrap();
        editor.aspect = 0.0;
        assert!(
            editor
                .resolve_file_action(pending, FileDecision::Discard)
                .is_err()
        );
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.files.pending.as_ref().unwrap().id, pending);
        editor.aspect = 0.1;
        assert!(matches!(
            editor
                .resolve_file_action(pending, FileDecision::Discard)
                .unwrap(),
            OperationReply::DocumentApplied { .. }
        ));
        assert_eq!(
            editor.files.origin.source_bytes().unwrap().as_ref(),
            original_bytes
        );
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            20.0
        );
        let view = ViewScene::from_native(editor.history.scene(), 1024 * 1024).unwrap();
        for point in view
            .instances()
            .iter()
            .flat_map(|instance| instance.world_vertices())
        {
            let ndc = editor
                .camera
                .project(*point, editor.aspect)
                .unwrap()
                .unwrap();
            assert!(ndc.truncate().abs().max_element() < 1.0);
        }
    }

    fn ui_frame(
        editor: &mut Editor,
        ctx: &egui::Context,
        input: egui::RawInput,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(input, |ui| editor.render(ui));
        output.textures_delta.clear();
        output
    }

    fn escape_modal(editor: &mut Editor) {
        let ctx = egui::Context::default();
        ui_frame(editor, &ctx, egui::RawInput::default());
        ui_frame(
            editor,
            &ctx,
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
        );
    }

    #[test]
    fn unavailable_save_preserves_files_and_malformed_open_preserves_editor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.aztec");
        let saved_id = write_legacy_cube(&path, 2.0);
        let saved = std::fs::read(&path).unwrap();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let cube = editor.selected.unwrap();
        editor.channels[0] = "2".into();
        let unsaved = dir.path().join("unsaved.mb");
        assert!(!editor.inspect(false).unwrap().save_available);
        assert!(!unsaved.exists());
        assert!(editor.is_dirty());
        editor.channels[0] = "3".into();
        assert!(!editor.inspect(false).unwrap().save_available);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert!(editor.is_dirty());
        editor.channels[0] = "4".into();
        let before = editor.history.scene().clone();
        let draft = editor.channels.clone();
        let bad = dir.path().join("bad.aztec");
        std::fs::write(&bad, b"{").unwrap();
        editor.open_path(&bad);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.channels, draft);
        assert_eq!(editor.selected, Some(cube));
        assert!(editor.history.can_undo());
        editor.open_path(&path);
        assert!(editor.files.is_modal());
        editor.discard_pending();
        assert_eq!(
            editor
                .history
                .scene()
                .node(saved_id)
                .unwrap()
                .transform()
                .translation[0],
            2.0
        );
        assert!(!editor.history.can_undo());
        assert!(!editor.is_dirty());
    }

    #[test]
    fn native_new_open_and_new_window_invalidate_remote_scene_tokens() {
        use crate::editor_operations::{Operation, OperationError, RemoteRequest};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large-id.aztec");
        let mut editor = Editor::default();
        let first_token = editor.scene_token;
        assert_ne!(Editor::default().scene_token, first_token);
        write_legacy_cube(&path, 0.0);
        let mut data: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let large_id = 9_007_199_254_740_993_u64;
        data["scene"]["nodes"][0]["id"] = large_id.into();
        data["scene"]["next_id"] = (large_id + 1).into();
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        editor.open_path(&path);
        assert!(!editor.files.is_modal());
        assert_ne!(editor.scene_token, first_token);
        let id = editor.history.scene().nodes()[0].id();
        assert_eq!(id.to_decimal(), "9007199254740993");
        assert_eq!(
            azteccraft::scene::NodeId::from_decimal(&id.to_decimal()).unwrap(),
            id
        );
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(first_token),
                    operation: Operation::Delete(id),
                },
                &egui::Context::default()
            ),
            Err(OperationError::StaleScene)
        ));
        let opened_token = editor.scene_token;
        editor.request_new();
        assert_ne!(editor.scene_token, opened_token);
        assert!(editor.history.scene().nodes().is_empty());
        assert!(matches!(
            editor.remote_operation(
                RemoteRequest {
                    expected_scene_token: Some(opened_token),
                    operation: Operation::Undo,
                },
                &egui::Context::default()
            ),
            Err(OperationError::StaleScene)
        ));
    }

    #[test]
    fn new_and_close_cancel_preserve_scene_and_discard_is_explicit() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        editor.request_new();
        assert!(editor.files.is_modal());
        escape_modal(&mut editor);
        assert!(!editor.files.is_modal());
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_undo());
        editor.request_close();
        assert!(editor.files.is_modal());
        escape_modal(&mut editor);
        assert!(!editor.files.is_modal());
        assert!(!editor.files.close_approved);
        assert_eq!(editor.history.scene(), &before);
        editor.request_new();
        editor.discard_pending();
        assert!(editor.history.scene().nodes().is_empty());
        assert!(!editor.is_dirty());
        editor.request_close();
        assert!(editor.files.close_approved);
    }

    #[test]
    fn native_close_event_is_cancelled_until_unsaved_decision() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ui_frame(&mut editor, &ctx, input);
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );
        assert!(editor.files.is_modal());
        escape_modal(&mut editor);
        assert!(!editor.files.close_approved);
        assert_eq!(editor.history.scene().nodes().len(), 1);
    }

    #[test]
    fn invalid_draft_blocks_new_close_and_save_without_resetting_editor() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.name = "".into();
        let before = editor.history.scene().clone();
        editor.request_new();
        editor.request_close();
        assert!(!editor.files.is_modal());
        assert!(!editor.files.close_approved);
        assert!(!editor.inspect(false).unwrap().save_available);
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.name, "");
        assert!(editor.error);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        editor.sync_fields();
        editor.native_operation(Operation::Rename {
            id: editor.selected.unwrap(),
            name: "changed".into(),
        });
        assert!(editor.is_dirty());
    }
}
