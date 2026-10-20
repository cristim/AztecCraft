use super::*;
use azteccraft::ui_icons::UiIcon;

impl Editor {
    /// egui reads a TextEdit's events before its own press focuses it, dropping same-frame Text.
    /// Focusing the field egui already hit-tested for this press makes that Text land.
    fn focus_pressed_text_field(&mut self, ctx: &egui::Context) {
        if self.history.active_manipulation().is_some()
            || !ctx.input(|input| input.pointer.primary_pressed())
        {
            return;
        }
        let focused = ctx.memory(|memory| memory.focused());
        // read_response on the focused field would surrender its focus on a click elsewhere.
        let Some(target) = text_field_ids()
            .filter(|id| Some(*id) != focused)
            .find(|id| {
                ctx.read_response(*id).is_some_and(|response| {
                    response.is_pointer_button_down_on() || response.clicked()
                })
            })
        else {
            return;
        };
        // Commit the previous field's draft first so the typed text lands on committed values.
        let leaving_draft_field = text_edit_focused(ctx)
            && focused != Some(features::manipulation_tool::manipulation_value_id());
        if leaving_draft_field {
            // A blur commit would otherwise also commit the target's just-typed text.
            self.blur_commit_skip_frames = 2;
            if self.has_draft() {
                self.commit_pending();
            }
        }
        ctx.memory_mut(|memory| memory.request_focus(target));
    }

    pub(super) fn render(&mut self, ui: &mut egui::Ui) {
        let frame_token = self.scene_token;
        self.sidebar_panel_request = None;
        self.attribute_editor_tab_request = None;
        let frame_admitted = !self.files.is_modal() && !self.files.is_closing();
        let keyboard_admitted = frame_admitted && !egui::Popup::is_any_open(ui.ctx());
        self.blur_commit_skip_frames = self.blur_commit_skip_frames.saturating_sub(1);
        if keyboard_admitted {
            self.focus_pressed_text_field(ui.ctx());
        }
        let (face, vertex, edge, toggle, delete) = ui.input(|input| {
            let pressed = |key| {
                input.events.iter().any(|event| {
                    matches!(event,
                egui::Event::Key { key: event_key, pressed: true, modifiers, .. }
                if *event_key == key && modifiers.is_none())
                })
            };
            (
                pressed(Key::F11),
                pressed(Key::F9),
                pressed(Key::F10),
                pressed(Key::F8),
                pressed(Key::Delete),
            )
        });
        let mut selection_intent = if !keyboard_admitted {
            None
        } else if face {
            Some(SelectionModeIntent::Face)
        } else if vertex {
            Some(SelectionModeIntent::Vertex)
        } else if edge {
            Some(SelectionModeIntent::Edge)
        } else if toggle {
            Some(SelectionModeIntent::Toggle)
        } else {
            None
        };
        let mut delete_intent = keyboard_admitted && delete;
        self.move_consumed_frame = false;
        self.handle_close(ui.ctx());
        // Same gate as shortcuts(): Escape that closes a menu must not also cancel an edit. The
        // popup is still open here; egui closes it later in this pass, when the menus are shown.
        if !self.files.is_modal()
            && !egui::Popup::is_any_open(ui.ctx())
            && !self.escape_closes_primitive_options(ui.ctx())
        {
            self.cancel_on_escape(ui.ctx());
        }
        let tool_intent = self.shortcuts(ui.ctx());
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
                let live = !self.files.is_closing() && !self.files.is_modal();
                for (label, icon, tool, hover) in [
                    (
                        "Select",
                        UiIcon::Select,
                        Some(ActiveTool::Select),
                        "Select tool (Q)",
                    ),
                    ("Lasso", UiIcon::Lasso, None, "Lasso: not implemented yet"),
                    (
                        "Paint Selection",
                        UiIcon::PaintSelect,
                        None,
                        "Paint Selection: not implemented yet",
                    ),
                    (
                        "Move",
                        UiIcon::Move,
                        Some(ActiveTool::Move),
                        "Move tool (W)",
                    ),
                    (
                        "Rotate",
                        UiIcon::Rotate,
                        Some(ActiveTool::Rotate),
                        "Rotate tool (E)",
                    ),
                    (
                        "Scale",
                        UiIcon::Scale,
                        Some(ActiveTool::Scale),
                        "Scale tool (R)",
                    ),
                    (
                        "Last Used",
                        UiIcon::LastUsed,
                        None,
                        "Last Used: not implemented yet; dynamic last-used behaviour unsupported",
                    ),
                ] {
                    let response = crate::tool_icons::icon_button(
                        ui,
                        label,
                        icon,
                        tool.is_some() && live,
                        tool.is_some_and(|tool| self.active_tool == tool),
                        hover,
                    );
                    if let Some(tool) = tool
                        && response.clicked()
                    {
                        self.native_operation(Operation::SetActiveTool(tool));
                    }
                }
            });
        egui::Panel::left("outliner")
            .default_size(200.0)
            .size_range(150.0..=350.0)
            .show(ui, |ui| self.outliner(ui));
        // Exactly one sidebar panel is drawn: the dock holds a single active panel.
        let sidebar_enabled = !self.files.is_closing()
            && !self.files.is_modal()
            && self.history.active_manipulation().is_none();
        match self.sidebar_dock {
            Some(SidebarPanel::ChannelBox) => {
                egui::Panel::right("channels")
                    .default_size(255.0)
                    .size_range(230.0..=400.0)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(sidebar_enabled, |ui| {
                            self.channels(ui, &mut delete_intent)
                        });
                    });
            }
            Some(SidebarPanel::AttributeEditor) => {
                egui::Panel::right("attribute_editor")
                    .default_size(255.0)
                    .size_range(230.0..=400.0)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(sidebar_enabled, |ui| self.attribute_editor(ui));
                    });
            }
            None => {}
        }
        egui::CentralPanel::default().show(ui, |ui| {
            self.viewport(
                ui,
                frame_token,
                selection_intent,
                tool_intent,
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
    pub(super) fn viewport(
        &mut self,
        ui: &mut egui::Ui,
        frame_token: Uuid,
        selection_intent: Option<SelectionModeIntent>,
        tool_intent: Option<ActiveTool>,
        delete_intent: bool,
        frame_admitted: bool,
    ) {
        let mut grid_requested = false;
        let mut shading_requested = None;
        let mut frame_all_requested = false;
        ui.horizontal(|ui| {
            ui.label("persp");
            ui.label(match self.selection_mode {
                SelectionMode::Object => "Object selection",
                SelectionMode::Face => "Face selection | visible centers only",
                SelectionMode::Hierarchy => "Hierarchy selection | Root",
                SelectionMode::Vertex => "Vertex selection",
                SelectionMode::Edge => "Edge selection",
            });
            ui.separator();
            grid_requested = ui
                .add_enabled(
                    !self.files.is_modal() && !self.files.is_closing(),
                    egui::Button::new("Grid").selected(self.grid_visible),
                )
                .clicked();
            ui.separator();
            let shading_enabled = !self.files.is_modal() && !self.files.is_closing();
            ui.add_enabled_ui(shading_enabled, |ui| {
                // Frame All hotkey A is from the Maya 2026 docs; the panel View > Frame All
                // menu path is from the Maya 2014 page (PROVISIONAL for 2026).
                ui.menu_button("View", |ui| {
                    if ui.button("Frame All (A)").clicked() {
                        frame_all_requested = true;
                        ui.close();
                    }
                });
                ui.menu_button("Shading", |ui| {
                    for (mode, label) in [
                        (ShadingMode::Wireframe, "Wireframe (4)"),
                        (ShadingMode::SmoothShadeAll, "Smooth Shade All (5)"),
                    ] {
                        if ui.selectable_label(self.shading == mode, label).clicked() {
                            shading_requested = Some(mode);
                            ui.close();
                        }
                    }
                    ui.separator();
                    for label in ["Use default material", "Wireframe on Shaded"] {
                        ui.add_enabled(false, egui::Button::new(label))
                            .on_disabled_hover_text(
                                "Not implemented: edges are always drawn and materials do not affect display",
                            );
                    }
                    // Labels and order follow the Maya 2026 Panel menu: Shading page (GUID-C6188583).
                    ui.add_enabled(false, egui::Button::new("Hardware Texturing (6)"))
                        .on_disabled_hover_text(super::shortcuts::unsupported_shading_reason(
                            super::shortcuts::SHADED_TEXTURED_NAME,
                        ));
                });
            });
            ui.separator();
            ui.weak("Alt + left: orbit | Alt + middle: pan | Alt + right drag or wheel: dolly");
        });
        self.manipulation_options(ui);
        let captured = crate::viewport_ui::capture(ui, &self.camera);
        if !matches!(&captured, Ok(Some(_)))
            && let Err(error) = self.missing_move_viewport()
        {
            self.set_error_status(&error);
            self.error = true;
        }
        if let Ok(Some(input)) = &captured {
            self.aspect = input.aspect();
        }
        let admitted = frame_admitted && !self.files.is_modal() && !self.files.is_closing();
        self.file_modal(ui.ctx());
        self.primitive_options_window(ui.ctx());
        let sidebar_request = self.sidebar_panel_request.take();
        let tab_request = self.attribute_editor_tab_request.take();
        if grid_requested && self.scene_token == frame_token {
            self.native_operation(Operation::SetGridVisible {
                visible: !self.grid_visible,
            });
        }
        if frame_all_requested && self.scene_token == frame_token {
            self.frame_all();
        }
        if let Some(mode) = shading_requested
            && self.scene_token == frame_token
        {
            self.native_operation(Operation::SetShadingMode(mode));
        }
        if let Some((panel, visible)) = sidebar_request
            && self.scene_token == frame_token
        {
            self.native_operation(Operation::SetSidebarPanelVisible { panel, visible });
        }
        if let Some(tab) = tab_request
            && self.scene_token == frame_token
        {
            self.native_operation(Operation::SelectAttributeEditorTab { tab });
        }
        if let Some(tool) = tool_intent
            && frame_token == self.scene_token
            && !self.move_consumed_frame
            && !self.files.is_modal()
            && !self.files.is_closing()
            && !ui.ctx().egui_wants_keyboard_input()
            && !egui::Popup::is_any_open(ui.ctx())
        {
            self.native_operation(Operation::SetActiveTool(tool));
        }
        let result = (|| -> Result<(), OperationError> {
            let Some(mut input) = captured.map_err(|error| {
                self.viewport_error = Some(crate::viewport_renderer::ViewportError::CameraInput);
                OperationError::Camera(error)
            })?
            else {
                return Ok(());
            };
            let consumed = self.handle_move_input(&input, frame_token)?;
            let navigation = input.take_navigation();
            let current_document = self.scene_token == frame_token;
            let can_select = admitted
                && current_document
                && !self.files.is_modal()
                && !self.files.is_closing()
                && !text_edit_focused(ui.ctx())
                && !egui::Popup::is_any_open(ui.ctx());
            let clickable = input.click_ndc().filter(|_| can_select && !consumed);
            let selection_intent = selection_intent.filter(|_| can_select);
            let delete_intent = delete_intent && can_select;
            let selection_refusal = input.unsupported_selection().filter(|_| can_select);
            let navigation = if current_document && !consumed {
                navigation
            } else {
                Vec::new()
            };
            let name = self.active_tool_name();
            let native_manipulation = self.native_move.is_some()
                || self
                    .history
                    .active_manipulation()
                    .is_some_and(|active| active.provenance == ManipulationProvenance::Native);
            if selection_intent.is_some() && native_manipulation && self.cancel_move()? {
                self.status = format!("{name} cancelled: selection mode changed");
                self.error = false;
            }
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
                    SelectionModeIntent::Vertex => SelectionMode::Vertex,
                    SelectionModeIntent::Edge => SelectionMode::Edge,
                    SelectionModeIntent::Object => SelectionMode::Object,
                    SelectionModeIntent::Hierarchy => SelectionMode::Hierarchy,
                    SelectionModeIntent::Component if self.selection_mode.is_component() => {
                        self.selection_mode
                    }
                    SelectionModeIntent::Component => SelectionMode::Face,
                    SelectionModeIntent::Toggle => match self.selection_mode {
                        SelectionMode::Face | SelectionMode::Vertex | SelectionMode::Edge => {
                            SelectionMode::Object
                        }
                        SelectionMode::Object | SelectionMode::Hierarchy => SelectionMode::Face,
                    },
                };
                self.apply_operation(Operation::SetSelectionMode(mode))?;
            }
            if delete_intent && let Some((id, _)) = self.single_target().ok() {
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
                    OperationError::View(e.to_string())
                })?;
            // Picks dispatch through `apply_operation`, not `native_operation`.
            if !self.files.is_modal()
                && let Some(ndc) = clickable
            {
                match self.selection_mode {
                    SelectionMode::Object | SelectionMode::Hierarchy => {
                        let hit = self
                            .camera
                            .pick_view(&view, ndc, self.aspect)
                            .map_err(|e| OperationError::View(e.to_string()))?;
                        let (node, instance_path) = if let Some(hit) = hit {
                            let mut prefix = hit.path;
                            prefix.pop();
                            if self.selection_mode == SelectionMode::Hierarchy {
                                prefix.truncate(1);
                                let root = self
                                    .history
                                    .scene()
                                    .root_transform(&prefix)
                                    .map_err(OperationError::Scene)?;
                                (Some(root), Some(prefix))
                            } else {
                                (Some(hit.transform), Some(prefix))
                            }
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
                            .pick_face_center(&self.camera, &view, self.single_target().ok())
                            .map_err(|e| OperationError::View(e.to_string()))?;
                        self.apply_operation(Operation::SelectFace { target })?;
                    }
                    SelectionMode::Vertex => {
                        let target = input
                            .pick_vertex(&self.camera, &view, self.single_target().ok())
                            .map_err(|e| OperationError::View(e.to_string()))?;
                        self.apply_operation(Operation::SelectVertex { target })?;
                    }
                    SelectionMode::Edge => {
                        let target = input
                            .pick_edge(&self.camera, &view, self.single_target().ok())
                            .map_err(|e| OperationError::View(e.to_string()))?;
                        self.apply_operation(Operation::SelectEdge { target })?;
                    }
                }
            }
            let painted = input.paint(
                self.renderer.as_ref(),
                &self.camera,
                &view,
                match self.selection_mode {
                    SelectionMode::Object | SelectionMode::Hierarchy => {
                        let (selected, path) = self.selection_presentation();
                        crate::viewport_renderer::SelectionFeedback::Object(selected.map(|_| path))
                    }
                    SelectionMode::Vertex => crate::viewport_renderer::SelectionFeedback::Vertex {
                        active: self.single_target().ok(),
                        selected: self.remembered_vertex.as_ref(),
                    },
                    SelectionMode::Edge => crate::viewport_renderer::SelectionFeedback::Edge {
                        active: self.single_target().ok(),
                        selected: self.remembered_edge.as_ref(),
                    },
                    SelectionMode::Face => crate::viewport_renderer::SelectionFeedback::Face {
                        active: self.single_target().ok(),
                        selected: self.remembered_face.as_ref(),
                    },
                },
                self.grid_visible,
                self.shading,
            );
            self.viewport_error = painted.as_ref().err().map(|error| error.category());
            let degraded = matches!(painted, Ok(true));
            self.face_centres_hidden = degraded && self.selection_mode == SelectionMode::Face;
            self.vertex_markers_hidden = degraded && self.selection_mode == SelectionMode::Vertex;
            self.paint_move(&input);
            self.paint_manipulation(&input);
            match painted {
                Ok(_) | Err(crate::viewport_renderer::RenderError::Uninitialized) => Ok(()),
                Err(error) => Err(OperationError::View(error.to_string())),
            }
        })();
        if let Err(error) = result {
            if matches!(error, OperationError::Camera(_)) {
                self.viewport_error = Some(crate::viewport_renderer::ViewportError::CameraInput);
            }
            self.set_error_status(&error);
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
}
