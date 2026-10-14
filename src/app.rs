#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{CameraInspection, NodeInspection, RemoteRequest, SceneInspection};
use crate::editor_operations::{Operation, OperationError, OperationReply};
use azteccraft::{
    history::{Command, EditOutcome, History},
    scene::{NodeId, Primitive, Transform},
    viewport::Camera,
};
use eframe::egui::{self, Color32, Key, RichText};
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
    camera: Camera,
    name: String,
    channels: [String; 9],
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
            camera: Camera::default(),
            name: String::new(),
            channels: Default::default(),
            native_input_pending: false,
            status: "Ready | Create a cube or plane to start".into(),
            error: false,
            aspect: 1.0,
            files: files::FileState::default(),
        }
    }
}

impl Editor {
    #[cfg(feature = "mcp")]
    pub(crate) fn start_mcp(&mut self, context: &egui::Context) -> std::io::Result<()> {
        self.mcp = Some(crate::mcp::UiBridge::start(context.clone())?);
        Ok(())
    }
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
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
        Self::default()
    }

    fn sync_fields(&mut self) {
        let Some(node) = self
            .selected
            .and_then(|id| self.history.scene().node(id).ok())
        else {
            self.selected = None;
            return;
        };
        self.name = node.name().into();
        let t = node.transform();
        let values = [
            t.translation.x,
            t.translation.y,
            t.translation.z,
            t.rotation_degrees.x,
            t.rotation_degrees.y,
            t.rotation_degrees.z,
            t.scale.x,
            t.scale.y,
            t.scale.z,
        ];
        self.channels = values.map(|value| value.to_string());
    }

    fn select(&mut self, selected: Option<NodeId>) {
        self.native_operation(Operation::Select(selected));
    }

    fn execute(&mut self, command: Command) {
        let operation = match command {
            Command::Create { primitive, name } => Operation::Create {
                primitive,
                name: Some(name),
            },
            Command::Remove(id) => Operation::Delete(id),
            Command::Rename { id, name } => Operation::Rename { id, name },
            Command::SetTransform { id, transform } => Operation::SetTransform { id, transform },
            Command::ExtrudeFace {
                id,
                face_index,
                distance,
            } => Operation::ExtrudeFace {
                id,
                face_index,
                distance,
            },
        };
        self.native_operation(operation);
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
            Operation::Select(_) | Operation::FrameSelected => None,
            #[cfg(any(test, feature = "mcp"))]
            Operation::Orbit { .. } | Operation::Pan { .. } | Operation::Dolly { .. } => None,
            Operation::Undo => Some("Undo"),
            Operation::Redo => Some("Redo"),
            _ => Some("Scene edited"),
        };
        match self.apply_operation(operation) {
            Ok(reply) => {
                if let Some(status) = status {
                    self.status = match reply {
                        OperationReply::Unchanged => "No change",
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
                inspection: self.inspect(focused)?,
            });
        }
        if request.expected_scene_token != Some(self.scene_token) {
            return Err(OperationError::StaleScene);
        }
        if self.files.is_modal() || focused || self.has_draft() || self.native_input_pending {
            return Err(OperationError::BusyEditing);
        }
        self.apply_operation(request.operation)
    }

    #[cfg(any(test, feature = "mcp"))]
    fn inspect(&self, text_focused: bool) -> Result<SceneInspection, OperationError> {
        crate::editor_operations::check_inspection_size(self.history.scene())?;
        Ok(SceneInspection {
            scene_token: self.scene_token.to_string(),
            nodes: self
                .history
                .scene()
                .nodes()
                .iter()
                .map(|node| NodeInspection {
                    id: node.id().to_decimal(),
                    name: node.name().into(),
                    transform: node.transform(),
                    mesh: node.mesh().clone(),
                })
                .collect(),
            selected_node: self.selected.map(NodeId::to_decimal),
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

    fn apply_operation(&mut self, operation: Operation) -> Result<OperationReply, OperationError> {
        if let Operation::Select(Some(id)) = &operation {
            self.history
                .scene()
                .node(*id)
                .map_err(OperationError::Scene)?;
        }
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
                        .any(|n| n.name() == format!("{base}{suffix}"))
                    {
                        suffix += 1;
                    }
                    format!("{base}{suffix}")
                });
                Command::Create { primitive, name }
            }
            Operation::Rename { id, name } => Command::Rename { id, name },
            Operation::SetTransform { id, transform } => Command::SetTransform { id, transform },
            Operation::ExtrudeFace {
                id,
                face_index,
                distance,
            } => Command::ExtrudeFace {
                id,
                face_index,
                distance,
            },
            Operation::Delete(id) => Command::Remove(id),
            Operation::Select(selected) => {
                let changed = self.selected != selected;
                self.selected = selected;
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
                return Ok(if changed {
                    OperationReply::Changed
                } else {
                    OperationReply::Unchanged
                });
            }
            #[cfg(any(test, feature = "mcp"))]
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
            Operation::FrameSelected => {
                let id = self.selected.ok_or(OperationError::NoSelection)?;
                let node = self
                    .history
                    .scene()
                    .node(id)
                    .map_err(OperationError::Scene)?;
                let before = self.camera.clone();
                self.camera
                    .frame(node, self.aspect)
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
                    inspection: self.inspect(false)?,
                });
            }
        };
        let outcome = self
            .history
            .execute(command)
            .map_err(OperationError::Scene)?;
        if let EditOutcome::Created(id) = outcome {
            self.selected = Some(id);
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
                let t = node.transform();
                let values = [
                    t.translation.x,
                    t.translation.y,
                    t.translation.z,
                    t.rotation_degrees.x,
                    t.rotation_degrees.y,
                    t.rotation_degrees.z,
                    t.scale.x,
                    t.scale.y,
                    t.scale.z,
                ];
                node.name() != self.name
                    || values
                        .iter()
                        .zip(&self.channels)
                        .any(|(value, draft)| draft.parse::<f32>().ok() != Some(*value))
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
            self.execute(Command::Remove(id));
        }
        if frame {
            self.frame_selected();
        }
    }

    fn commit_pending(&mut self) -> bool {
        let Some(id) = self.selected else {
            return true;
        };
        let result = || -> Result<(Transform, bool, bool), String> {
            let node = self.history.scene().node(id).map_err(|e| e.to_string())?;
            let v: Vec<f32> = self
                .channels
                .iter()
                .map(|v| v.parse::<f32>())
                .collect::<Result<_, _>>()
                .map_err(|_| "Enter a valid number in every transform channel".to_string())?;
            let transform = Transform {
                translation: Vec3::new(v[0], v[1], v[2]),
                rotation_degrees: Vec3::new(v[3], v[4], v[5]),
                scale: Vec3::new(v[6], v[7], v[8]),
            };
            let rename = node.name() != self.name;
            let changed = node.transform() != transform;
            let mut candidate = self.history.scene().clone();
            candidate
                .rename(id, &self.name)
                .map_err(|e| e.to_string())?;
            candidate
                .set_transform(id, transform)
                .map_err(|e| e.to_string())?;
            Ok((transform, rename, changed))
        };
        let outcome = result().and_then(|(transform, rename, changed)| {
            if rename {
                self.history
                    .execute(Command::Rename {
                        id,
                        name: self.name.clone(),
                    })
                    .map_err(|e| e.to_string())?;
            }
            if changed {
                self.history
                    .execute(Command::SetTransform { id, transform })
                    .map_err(|e| e.to_string())?;
            }
            Ok(rename || changed)
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
                if ui.button("Save As...").clicked() {
                    ui.close();
                    self.save_dialog();
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
                    .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                    .clicked()
                {
                    if let Some(id) = self.selected {
                        self.execute(Command::Remove(id));
                    }
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
            unsupported(ui, "Extrude");
            unsupported(ui, "Bevel");
            unsupported(ui, "Materials");
        });
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Outliner").strong());
        ui.separator();
        ui.weak("Scene root");
        let mut choice = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for node in self.history.scene().nodes() {
                if ui
                    .selectable_label(
                        self.selected == Some(node.id()),
                        format!("◇  {}", node.name()),
                    )
                    .clicked()
                {
                    choice = Some(node.id());
                }
            }
        });
        if let Some(id) = choice {
            self.select(Some(id));
        }
        if self.history.scene().nodes().is_empty() {
            ui.weak("No objects. Create a primitive above.");
        }
    }

    fn channels(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Channel Box").strong());
        ui.separator();
        let Some(id) = self.selected else {
            ui.weak("Select an object to edit its channels.");
            return;
        };
        ui.label("Object name");
        let response = ui.add(egui::TextEdit::singleline(&mut self.name).id(name_edit_id()));
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
                    apply |= response.lost_focus();
                    ui.end_row();
                }
            });
        ui.weak("Rotation in degrees | XYZ");
        if apply {
            self.commit_pending();
        }
        ui.separator();
        if ui.button("Delete selected").clicked() {
            self.execute(Command::Remove(id));
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
            .show(ui, |ui| self.channels(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("persp");
                ui.separator();
                ui.weak("Alt + left: orbit | Middle: pan | Right / wheel: dolly");
            });
            match crate::viewport_ui::show(
                ui,
                &mut self.camera,
                self.history.scene(),
                self.selected,
            ) {
                Ok(output) => {
                    self.aspect = output.aspect;
                    if let Some(selection) = output.selection {
                        self.select(selection);
                    }
                }
                Err(error) => {
                    self.status = error.to_string();
                    self.error = true;
                }
            }
        });
        self.file_modal(ui.ctx());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Title(self.document_title()));
        self.native_input_pending = false;
        #[cfg(feature = "mcp")]
        self.process_mcp(ui.ctx());
    }

    #[cfg(feature = "mcp")]
    fn process_mcp(&mut self, ctx: &egui::Context) {
        if let Some(mut bridge) = self.mcp.take() {
            if !self.native_input_pending && bridge.take_close_request() {
                self.request_close();
                ctx.request_repaint();
            }
            if !self.files.close_approved() {
                bridge.dispatch(self, ctx);
                self.mcp = Some(bridge);
            }
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
    fn remote_operations_share_native_scene_selection_history_and_camera() {
        let mut editor = Editor::default();
        assert_eq!(crate::editor_operations::TOOL_NAMES.len(), 12);
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
        let transform = Transform {
            translation: Vec3::new(4.0, 2.0, 1.0),
            ..Default::default()
        };
        assert!(matches!(
            remote(&mut editor, Operation::SetTransform { id, transform }).unwrap(),
            OperationReply::Changed
        ));
        assert!(matches!(
            remote(&mut editor, Operation::SetTransform { id, transform }).unwrap(),
            OperationReply::Unchanged
        ));
        remote(&mut editor, Operation::FrameSelected).unwrap();
        assert!(editor.camera.target().distance(transform.translation) < 0.01);
        remote(&mut editor, Operation::Select(None)).unwrap();
        assert_eq!(editor.selected, None);
        remote(&mut editor, Operation::Select(Some(id))).unwrap();
        assert_eq!(editor.name, "edited");
        editor.history_step(false);
        assert_eq!(
            editor.history.scene().node(id).unwrap().transform(),
            Transform::default()
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
                .translation
                .x,
            6.0
        );
        let OperationReply::Scene { inspection } = remote(&mut editor, Operation::Inspect).unwrap()
        else {
            panic!("expected inspection")
        };
        assert_eq!(inspection.nodes.len(), 2);
        assert_eq!(inspection.nodes[0].id, node_id);
        assert_eq!(inspection.nodes[0].transform.translation.x, 6.0);
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
            remote(&mut editor, Operation::Select(Some(missing))),
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
        let transform = Transform {
            scale: Vec3::splat(f32::INFINITY),
            ..Default::default()
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
        editor.channels[0] = "2".into();
        editor.execute(Command::ExtrudeFace {
            id,
            face_index: 0,
            distance: 0.0,
        });
        assert!(!editor.error);
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .mesh()
                .vertices()
                .len(),
            12
        );
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .transform()
                .translation
                .x,
            2.0
        );
        editor.history_step(false);
        assert_eq!(
            editor
                .history
                .scene()
                .node(id)
                .unwrap()
                .mesh()
                .vertices()
                .len(),
            8
        );
        let before = editor.history.scene().clone();
        editor.channels[0] = "nan".into();
        editor.execute(Command::ExtrudeFace {
            id,
            face_index: 0,
            distance: 1.0,
        });
        assert!(editor.error);
        assert_eq!(editor.channels[0], "nan");
        assert_eq!(editor.history.scene(), &before);
        assert!(editor.history.can_redo());
        editor.sync_fields();
        editor.execute(Command::ExtrudeFace {
            id,
            face_index: 99,
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
                .node(id)
                .unwrap()
                .mesh()
                .vertices()
                .len(),
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
                .translation
                .x,
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
                .translation
                .x,
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
                .translation
                .x,
            4.0
        );
        assert_eq!(
            editor.history.scene().node(next).unwrap().transform(),
            Transform::default()
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
                .translation
                .y,
            5.0
        );
        assert_eq!(
            editor.history.scene().node(next).unwrap().transform(),
            Transform::default()
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
