use super::*;
use azteccraft::ui_icons::UiIcon;

/// One File menu entry that the Status Line also draws, so the two cannot diverge.
pub(super) struct FileAction {
    pub(super) label: &'static str,
    pub(super) icon: UiIcon,
    /// Strip hover text when there is no disabled reason.
    pub(super) tooltip: &'static str,
    pub(super) enabled: bool,
    /// Shown on hover while disabled (the read-only reason).
    pub(super) reason: Option<String>,
    pub(super) run: fn(&mut Editor),
}

/// Maya 2026 always shows these seven menus, in this order (Menus and menu sets page).
pub(super) const ALWAYS_ON_MENUS: [&str; 7] = [
    "File", "Edit", "Create", "Select", "Modify", "Display", "Windows",
];

const CONTENT_MENUS: usize = 4;

/// Maya lists these Modeling topics after the default menus; their order is PROVISIONAL.
pub(super) const MODELING_SET_MENUS: [&str; 9] = [
    "Mesh",
    "Edit Mesh",
    "Mesh Tools",
    "Mesh Display",
    "Curves",
    "Surfaces",
    "Deform",
    "UV",
    "Generate",
];

/// Help follows the Modeling set; its Maya position is UNCONFIRMED (the pages do not place it).
pub(super) const HELP_MENU: &str = "Help";

fn placeholder_menu(ui: &mut egui::Ui, label: &str) {
    ui.menu_button(label, |ui| {
        unsupported(ui, "Not implemented yet");
        if label == "Display" {
            // Maya's Display > Grid option box: size, spacing, divisions and colours (#160).
            ui.add_enabled(false, egui::Button::new("Grid Options"))
                .on_disabled_hover_text(GRID_OPTIONS_REASON);
        }
    });
}

pub(super) const GRID_OPTIONS_REASON: &str = "Not implemented: the grid uses Maya 2025.1's defaults, +/-12 units (PROVISIONAL) with lines every 5 and 5 divisions";

/// "Undo Rotate" or "Redo Scale"; the bare verb when there is no entry to name.
fn history_hover(verb: &str, label: Option<&str>) -> String {
    match label {
        Some(label) => format!("{verb} {label}"),
        None => verb.to_owned(),
    }
}

impl FileAction {
    pub(super) fn hover(&self) -> &str {
        match (self.enabled, self.reason.as_deref()) {
            (false, Some(reason)) => reason,
            _ => self.tooltip,
        }
    }
}

impl Editor {
    /// New, Open and Save in menu order: enable state, disabled reason and dispatch.
    pub(super) fn file_actions(&self) -> [FileAction; 3] {
        let read_only_reason = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string());
        [
            FileAction {
                label: "New scene",
                icon: UiIcon::FileNew,
                tooltip: "New scene",
                enabled: true,
                reason: None,
                run: Editor::request_new,
            },
            FileAction {
                label: "Open...",
                icon: UiIcon::FileOpen,
                tooltip: "Open a scene...",
                enabled: true,
                reason: None,
                run: Editor::open_dialog,
            },
            FileAction {
                label: "Save",
                icon: UiIcon::FileSave,
                tooltip: "Save the scene",
                enabled: self.files.save_available(),
                reason: read_only_reason,
                run: |editor| editor.native_operation(Operation::SaveScene),
            },
        ]
    }

    fn menu_bar(
        &mut self,
        ui: &mut egui::Ui,
        selection_intent: &mut Option<SelectionModeIntent>,
        delete_intent: &mut bool,
        read_only_reason: Option<&str>,
    ) {
        let authoring = read_only_reason.is_none();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                for action in self.file_actions() {
                    let mut response =
                        ui.add_enabled(action.enabled, egui::Button::new(action.label));
                    if let Some(reason) = action.reason.as_deref() {
                        response = response.on_disabled_hover_text(reason);
                    }
                    if response.clicked() {
                        ui.close();
                        (action.run)(self);
                    }
                }
                let mut save_as = ui.add_enabled(
                    self.files.save_as_available(),
                    egui::Button::new("Save As..."),
                );
                if let Some(reason) = read_only_reason {
                    save_as = save_as.on_disabled_hover_text(reason);
                }
                if save_as.clicked() {
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
                        egui::Button::new(format!("Undo {}   Ctrl/Cmd Z", self.history.undo_label().unwrap_or_default())),
                    )
                    .on_hover_text(history_hover("Undo", self.history.undo_label().as_deref()))
                    .clicked()
                {
                    self.history_step(false);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(format!("Redo {}   Ctrl/Cmd Shift Z", self.history.redo_label().unwrap_or_default())),
                    )
                    .on_hover_text(history_hover("Redo", self.history.redo_label().as_deref()))
                    .clicked()
                {
                    self.history_step(true);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        authoring
                            && self.single_target().is_ok()
                            && !self.selection_mode.is_component(),
                        egui::Button::new("Delete"),
                    )
                    .on_disabled_hover_text(self.delete_disabled_hover(read_only_reason, true))
                    .clicked()
                {
                    *delete_intent = true;
                    ui.close();
                }
            });
            ui.menu_button("Create", |ui| {
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon cube"))
                    .on_disabled_hover_text(read_only_reason.unwrap_or_default())
                    .clicked()
                {
                    self.create(Primitive::Cube);
                    ui.close();
                }
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon plane"))
                    .on_disabled_hover_text(read_only_reason.unwrap_or_default())
                    .clicked()
                {
                    self.create(Primitive::Plane);
                    ui.close();
                }
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon sphere"))
                    .on_disabled_hover_text(read_only_reason.unwrap_or_default())
                    .clicked()
                {
                    self.create(Primitive::Sphere);
                    ui.close();
                }
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon cylinder"))
                    .on_disabled_hover_text(read_only_reason.unwrap_or_default())
                    .clicked()
                {
                    self.create(Primitive::Cylinder);
                    ui.close();
                }
                if ui
                    .add_enabled(authoring, egui::Button::new("Polygon cone"))
                    .on_disabled_hover_text(read_only_reason.unwrap_or_default())
                    .clicked()
                {
                    self.create(Primitive::Cone);
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
                        .selectable_label(
                            self.selection_mode == SelectionMode::Vertex,
                            "Vertex   F9",
                        )
                        .clicked()
                    {
                        *selection_intent = Some(SelectionModeIntent::Vertex);
                        ui.close();
                    }
                    if ui
                        .selectable_label(self.selection_mode == SelectionMode::Edge, "Edge   F10")
                        .clicked()
                    {
                        *selection_intent = Some(SelectionModeIntent::Edge);
                        ui.close();
                    }
                    if ui
                        .selectable_label(self.selection_mode == SelectionMode::Face, "Face   F11")
                        .clicked()
                    {
                        *selection_intent = Some(SelectionModeIntent::Face);
                        ui.close();
                    }
                    unsupported(ui, "UVs");
                });
            });
            for label in &ALWAYS_ON_MENUS[CONTENT_MENUS..] {
                placeholder_menu(ui, label);
            }
            for label in MODELING_SET_MENUS {
                placeholder_menu(ui, label);
            }
            ui.menu_button(HELP_MENU, |ui| {
                ui.label("Local editor prototype");
                ui.label("Maya parity has not been verified.");
            });
        });
    }

    pub(super) fn menus(
        &mut self,
        ui: &mut egui::Ui,
        selection_intent: &mut Option<SelectionModeIntent>,
        delete_intent: &mut bool,
    ) {
        let read_only_reason = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string());
        let authoring = read_only_reason.is_none();
        // Seventeen menus overflow the 960 point minimum window at enlarged text or padding;
        // scroll instead of wrapping so the strips below keep their rows.
        egui::ScrollArea::horizontal()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                self.menu_bar(
                    ui,
                    selection_intent,
                    delete_intent,
                    read_only_reason.as_deref(),
                );
            });
        ui.separator();
        self.status_line(ui, selection_intent);
        ui.separator();
        ui.horizontal(|ui| {
            if crate::tool_icons::button(ui, "Cube", UiIcon::PolyCube, authoring)
                .on_disabled_hover_text(read_only_reason.as_deref().unwrap_or_default())
                .clicked()
            {
                self.create(Primitive::Cube);
            }
            if crate::tool_icons::button(ui, "Plane", UiIcon::PolyPlane, authoring)
                .on_disabled_hover_text(read_only_reason.as_deref().unwrap_or_default())
                .clicked()
            {
                self.create(Primitive::Plane);
            }
            ui.separator();
            if crate::tool_icons::button(ui, "Undo", UiIcon::Undo, self.history.can_undo())
                .on_hover_text(history_hover("Undo", self.history.undo_label().as_deref()))
                .clicked()
            {
                self.history_step(false);
            }
            if crate::tool_icons::button(ui, "Redo", UiIcon::Redo, self.history.can_redo())
                .on_hover_text(history_hover("Redo", self.history.redo_label().as_deref()))
                .clicked()
            {
                self.history_step(true);
            }
            if crate::tool_icons::button(
                ui,
                "Frame selected  F",
                UiIcon::FrameSelected,
                self.single_target().is_ok(),
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
}
