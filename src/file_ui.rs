use super::Editor;
#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{DocumentOrigin, PendingInspection};
use crate::editor_operations::{
    FileActionKind, FileDecision, Operation, OperationError, OperationReply,
};
use azteccraft::{
    document::{self, NativeDocument, NativeOrigin},
    history::History,
    native_scene::NativeScene,
    view_scene::ViewScene,
    viewport::Camera,
};
use eframe::egui::{self, Key};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct FileState {
    path: Option<PathBuf>,
    saved: NativeScene,
    origin: NativeOrigin,
    pending: Option<PendingAction>,
    close_approved: bool,
    close_sent: bool,
}
struct PendingAction {
    id: Uuid,
    action: FileAction,
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
    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn document_origin(&self) -> DocumentOrigin {
        match self.origin {
            NativeOrigin::Untitled => DocumentOrigin::Untitled,
            NativeOrigin::Binary(_) => DocumentOrigin::MayaBinary,
            NativeOrigin::Ascii(_) => DocumentOrigin::MayaAscii,
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
        self.history.scene() != &self.files.saved || self.has_draft()
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
            .set_title("Open Maya or legacy scene")
            .add_filter("Maya static cubic ASCII scene", &["ma"])
            .add_filter("Maya binary scene", &["mb"])
            .add_filter("Legacy AztecCraft scene", &["aztec"])
            .pick_file()
        {
            self.open_path(&path);
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
        self.history = History::new(scene);
        self.selected = None;
        self.selected_path.clear();
        self.name.clear();
        self.channels = Default::default();
        self.channel_intents = [false; 9];
        self.name_intent = false;
        self.camera = camera;
        self.status = "Scene ready".into();
        self.error = false;
    }
    pub(super) fn resolve_file_action(
        &mut self,
        id: Uuid,
        decision: FileDecision,
    ) -> Result<OperationReply, OperationError> {
        if self.files.pending.as_ref().is_none_or(|p| p.id != id) {
            return Err(OperationError::StalePendingAction);
        }
        if matches!(decision, FileDecision::Discard)
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
            let reframed = camera_for_scene(&document.scene, self.aspect)?;
            *camera = reframed;
            *framed_aspect = self.aspect;
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
            FileDecision::Discard => self.apply_file_action(pending.action),
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
            ui.label("Native scene writing is not available yet.");
            ui.label("Discarding changes cannot be undone.");
            ui.horizontal(|ui| {
                ui.add_enabled(false, egui::Button::new("Save As..."))
                    .on_hover_text("Native Maya writing is not implemented");
                if ui.button("Discard").clicked() {
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
        || !view.curve_instances().is_empty()
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
    use azteccraft::scene::Primitive;

    mod ascii_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_ascii.rs"
        ));
    }

    #[test]
    fn ascii_failed_native_open_preserves_draft_history_selection_and_camera() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bad.ma");
        let source = String::from_utf8(ascii_fixture::specimen())
            .unwrap()
            .replace("12 0 0", "100000000 0 0");
        std::fs::write(&path, &source).unwrap();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let id = editor.selected.unwrap();
        editor
            .apply_operation(Operation::Rename {
                id,
                name: "undone".into(),
            })
            .unwrap();
        editor.apply_operation(Operation::Undo).unwrap();
        editor.name = "human draft".into();
        let before = editor.history.scene().clone();
        let camera = editor.camera.clone();
        let selected = (editor.selected, editor.selected_path.clone());
        let fields = (editor.name.clone(), editor.channels.clone());
        let token = editor.scene_token;
        assert!(editor.open_candidate(&path).is_err());
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.camera, camera);
        assert_eq!((editor.selected, editor.selected_path.clone()), selected);
        assert_eq!((editor.name, editor.channels), fields);
        assert_eq!(editor.scene_token, token);
        assert!(editor.history.can_undo() && editor.history.can_redo());
        assert!(!editor.files.is_modal());
        assert_eq!(std::fs::read(path).unwrap(), source.as_bytes());
    }

    #[test]
    fn ascii_pending_open_keeps_staged_source_and_reframes_after_resize() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("curve.ma");
        let source = ascii_fixture::specimen();
        std::fs::write(&path, &source).unwrap();
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let before = editor.history.scene().clone();
        let token = editor.scene_token;
        let OperationReply::PendingConfirmation {
            pending_action_id, ..
        } = editor.open_candidate(&path).unwrap()
        else {
            panic!("expected pending Open")
        };
        assert_eq!(editor.history.scene(), &before);
        assert_eq!(editor.scene_token, token);
        std::fs::write(&path, b"changed after staging").unwrap();
        editor.aspect = 0.1;
        editor
            .resolve_file_action(
                Uuid::parse_str(&pending_action_id).unwrap(),
                FileDecision::Discard,
            )
            .unwrap();
        assert_ne!(editor.scene_token, token);
        assert_eq!(editor.files.origin.source_bytes().unwrap().as_ref(), source);
        assert_eq!(editor.files.document_origin(), DocumentOrigin::MayaAscii);
        assert!(!editor.is_dirty() && !editor.history.can_undo());
        assert!(editor.selected.is_none() && editor.selected_path.is_empty());
        let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
        for point in view.curve_instances()[0].segments().iter().flatten() {
            let ndc = editor
                .camera
                .project(*point, editor.aspect)
                .unwrap()
                .unwrap();
            assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
        }
        assert_eq!(std::fs::read(path).unwrap(), b"changed after staging");
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
