use super::*;
use azteccraft::ui_icons::UiIcon;

/// Select > All by Type needs a vocabulary of object types, which does not exist yet (#325).
pub(super) const ALL_BY_TYPE_REASON: &str =
    "Not implemented: selecting by object type needs an object type list, tracked in #325";
/// Select > All, Hierarchy and Inverse change the object selection only; components cannot be
/// multi-selected yet (#30).
pub(super) const SELECT_RULE_COMPONENT_REASON: &str =
    "Selection changes need Object or Hierarchy selection mode";

/// One File menu entry that the Status Line also draws, so the two cannot diverge.
pub(super) struct FileAction {
    pub(super) label: &'static str,
    /// Hotkey text shown after the label in the menu row, only for keys that are bound.
    pub(super) hint: Option<&'static str>,
    /// The key and whether Shift is held, with Command (Ctrl off macOS); `shortcuts()` dispatches
    /// the row this names, so the key, the hint and the click share one `enabled` and `run`.
    pub(super) chord: (Key, bool),
    /// False for entries the Status Line does not draw (Save Scene As has no button).
    pub(super) strip: bool,
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

/// Maya's Edit > Delete by Type and Delete All by Type rows; the type list is not implemented.
pub(super) const DELETE_BY_TYPE_REASON: &str =
    "Deleting by node type is not supported yet; select the object and use Delete";

pub(super) const POLYGON_PRIMITIVES_MENU: &str = "Polygon Primitives";

pub(super) const INTERACTIVE_CREATION_REASON: &str =
    "Interactive creation is not implemented; primitives are created at the origin";
pub(super) const EXIT_ON_COMPLETION_REASON: &str =
    "Only applies to interactive creation, which is not implemented";

/// Maya 2027's Create > Polygon Primitives list (docs GUID-45D2EAD4-5BCF-42DA-A1AB-EC6EE09FE705);
/// the order within the submenu is PROVISIONAL. `None` marks an entry not built yet.
pub(super) const POLYGON_PRIMITIVES: [(&str, Option<Primitive>); 15] = [
    ("Sphere", Some(Primitive::Sphere)),
    ("Cube", Some(Primitive::Cube)),
    ("Cylinder", Some(Primitive::Cylinder)),
    ("Cone", Some(Primitive::Cone)),
    ("Torus", None),
    ("Plane", Some(Primitive::Plane)),
    ("Disc", None),
    ("Platonic Solid", None),
    ("Pyramid", None),
    ("Prism", None),
    ("Pipe", None),
    ("Helix", None),
    ("Gear", None),
    ("Soccer Ball", None),
    ("Super Shapes", None),
];

fn unbuilt_primitive_reason(label: &str) -> String {
    format!(
        "{label} is not implemented yet; the supported primitives are Sphere, Cube, Cylinder, Cone and Plane"
    )
}

fn placeholder_menu(ui: &mut egui::Ui, label: &str) {
    ui.menu_button(label, |ui| {
        unsupported(ui, NOT_IMPLEMENTED_YET);
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

pub(super) const NEW_LABEL: &str = "New Scene";
pub(super) const OPEN_LABEL: &str = "Open Scene";
pub(super) const SAVE_LABEL: &str = "Save Scene";
pub(super) const SAVE_AS_LABEL: &str = "Save Scene As";
/// Why every File action is unavailable once the window is closing.
pub(super) const CLOSING_REASON: &str = "the window is closing";

/// The disabled hover for a File action while closing.
pub(super) fn unavailable_while_closing(label: &str) -> String {
    format!("{label}: unavailable while {CLOSING_REASON}")
}

const NEW_HINT: &str = "Ctrl/Cmd N";
const OPEN_HINT: &str = "Ctrl/Cmd O";
const SAVE_HINT: &str = "Ctrl/Cmd S";
const SAVE_AS_HINT: &str = "Ctrl/Cmd Shift S";

/// The first File chord pressed this frame: N, O or S with Command, and whether Shift is held.
/// Key repeat is ignored so a held key cannot start a second New or dialog.
pub(super) fn file_chord(events: &[egui::Event]) -> Option<(Key, bool)> {
    events.iter().find_map(|event| match event {
        egui::Event::Key {
            key: key @ (Key::N | Key::O | Key::S),
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } if modifiers.command && !modifiers.alt => Some((*key, modifiers.shift)),
        _ => None,
    })
}

/// The table row a chord belongs to.
pub(super) fn chord_entry(actions: &[FileAction], chord: (Key, bool)) -> Option<usize> {
    actions.iter().position(|action| action.chord == chord)
}

impl FileAction {
    /// The menu row text: the label, then the hotkey after three spaces like Undo and Redo.
    pub(super) fn menu_text(&self) -> String {
        match self.hint {
            Some(hint) => format!("{}   {hint}", self.label),
            None => self.label.to_owned(),
        }
    }

    pub(super) fn hover(&self) -> &str {
        match (self.enabled, self.reason.as_deref()) {
            (false, Some(reason)) => reason,
            _ => self.tooltip,
        }
    }
}

impl Editor {
    /// New, Open, Save and Save As in menu order: label, hint, enable state, disabled reason and
    /// dispatch. The File menu and the Status Line both read this table.
    pub(super) fn file_actions(&self) -> [FileAction; 4] {
        let read_only_reason = self
            .history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string());
        let closing = self.files.is_closing();
        // Closing is the stronger state, so it names the reason ahead of a read-only scene.
        let reason_for = |label: &str| {
            closing
                .then(|| unavailable_while_closing(label))
                .or_else(|| read_only_reason.clone())
        };
        let closing_only = |label: &str| closing.then(|| unavailable_while_closing(label));
        [
            FileAction {
                label: NEW_LABEL,
                hint: Some(NEW_HINT),
                chord: (Key::N, false),
                strip: true,
                icon: UiIcon::FileNew,
                tooltip: NEW_LABEL,
                enabled: !closing,
                reason: closing_only(NEW_LABEL),
                run: Editor::request_new,
            },
            FileAction {
                label: OPEN_LABEL,
                hint: Some(OPEN_HINT),
                chord: (Key::O, false),
                strip: true,
                icon: UiIcon::FileOpen,
                tooltip: OPEN_LABEL,
                enabled: !closing,
                reason: closing_only(OPEN_LABEL),
                run: Editor::open_dialog,
            },
            FileAction {
                label: SAVE_LABEL,
                hint: Some(SAVE_HINT),
                chord: (Key::S, false),
                strip: true,
                icon: UiIcon::FileSave,
                tooltip: SAVE_LABEL,
                enabled: !closing && self.files.save_available(),
                reason: reason_for(SAVE_LABEL),
                run: |editor| editor.native_operation(Operation::SaveScene),
            },
            FileAction {
                label: SAVE_AS_LABEL,
                hint: Some(SAVE_AS_HINT),
                chord: (Key::S, true),
                strip: false,
                icon: UiIcon::FileSave,
                tooltip: SAVE_AS_LABEL,
                enabled: !closing && self.files.save_as_available(),
                reason: reason_for(SAVE_AS_LABEL),
                run: Editor::save_as_dialog,
            },
        ]
    }

    /// One Create item with its option box, which opens that primitive's options window.
    fn create_row(
        &mut self,
        ui: &mut egui::Ui,
        primitive: Primitive,
        label: &str,
        read_only_reason: Option<&str>,
    ) {
        let authoring = read_only_reason.is_none();
        ui.horizontal(|ui| {
            let mut item = ui.add_enabled(authoring, egui::Button::new(label));
            if let Some(reason) = read_only_reason {
                item = item.on_disabled_hover_text(reason);
            }
            if item.clicked() {
                self.create(primitive);
                ui.close();
            }
            let option_box = features::primitive_options::option_box(
                ui,
                &format!("{label} option box"),
                authoring,
                read_only_reason,
            );
            if option_box.clicked() {
                // The menu closes on this click by itself (tested).
                self.open_primitive_options(primitive);
            }
        });
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
                if let Some(reason) = read_only_reason {
                    // The opened scene cannot be saved; say so above the actions (#189).
                    ui.weak(format!("Read-only scene: {reason}"));
                    ui.separator();
                }
                for action in self.file_actions() {
                    let mut response =
                        ui.add_enabled(action.enabled, egui::Button::new(action.menu_text()));
                    if let Some(reason) = action.reason.as_deref() {
                        response = response.on_disabled_hover_text(reason);
                    }
                    if response.clicked() {
                        ui.close();
                        (action.run)(self);
                    }
                }
                unsupported(ui, "Export...");
                ui.separator();
                let closing = self.files.is_closing();
                let mut close = ui.add_enabled(!closing, egui::Button::new("Close"));
                if closing {
                    close = close.on_disabled_hover_text(unavailable_while_closing("Close"));
                }
                if close.clicked() {
                    ui.close();
                    self.request_close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(match self.history.undo_label() {
                            Some(label) => format!("Undo {}   Ctrl/Cmd Z", label),
                            None => "Undo   Ctrl/Cmd Z".to_string(),
                        }),
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
                        egui::Button::new(match self.history.redo_label() {
                            Some(label) => format!("Redo {}   Ctrl/Cmd Y", label),
                            None => "Redo   Ctrl/Cmd Y".to_string(),
                        }),
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
                            && !self.selection.is_empty()
                            && !self.selection_mode.is_component(),
                        egui::Button::new("Delete"),
                    )
                    .on_disabled_hover_text(self.delete_disabled_hover(read_only_reason, true))
                    .clicked()
                {
                    *delete_intent = true;
                    ui.close();
                }
                for label in ["Delete by Type", "Delete All by Type"] {
                    ui.add_enabled(false, egui::Button::new(label))
                        .on_disabled_hover_text(DELETE_BY_TYPE_REASON);
                }
            });
            ui.menu_button("Create", |ui| {
                ui.menu_button(POLYGON_PRIMITIVES_MENU, |ui| {
                    // Toggles per Maya-Modeling GUID-6D21314A-54AD-41D4-AFC0-AAED13CD50A6; their
                    // position and the unchecked Exit on Completion default are PROVISIONAL.
                    for (label, reason) in [
                        ("Interactive Creation", INTERACTIVE_CREATION_REASON),
                        ("Exit on Completion", EXIT_ON_COMPLETION_REASON),
                    ] {
                        ui.add_enabled(false, egui::Checkbox::new(&mut false, label))
                            .on_disabled_hover_text(reason);
                    }
                    ui.separator();
                    for (label, primitive) in POLYGON_PRIMITIVES {
                        match primitive {
                            Some(primitive) => {
                                self.create_row(ui, primitive, label, read_only_reason);
                            }
                            None => {
                                ui.add_enabled(false, egui::Button::new(label))
                                    .on_disabled_hover_text(unbuilt_primitive_reason(label));
                            }
                        }
                    }
                });
            });
            ui.menu_button("Select", |ui| {
                // Maya 2025.1 order (buildSelectMenu.mel) and hotkeys (hotkeySetup.mel), confirmed by the
                // 2027 Select menu page. All by Type is a submenu in Maya; here a plain disabled row.
                let rule_rows_enabled = !self.selection_mode.is_component();
                let rule_row = |ui: &mut egui::Ui, label: &str| {
                    let clicked = ui
                        .add_enabled(rule_rows_enabled, egui::Button::new(label))
                        .on_disabled_hover_text(SELECT_RULE_COMPONENT_REASON)
                        .clicked();
                    if clicked {
                        ui.close();
                    }
                    clicked
                };
                if rule_row(ui, "All   Ctrl/Cmd Shift A") {
                    self.native_select_by_rule(SelectRule::All);
                }
                ui.add_enabled(false, egui::Button::new("All by Type"))
                    .on_disabled_hover_text(ALL_BY_TYPE_REASON);
                if ui.button("Deselect All   Alt D").clicked() {
                    self.deselect_all();
                    ui.close();
                }
                if rule_row(ui, "Hierarchy") {
                    self.native_select_by_rule(SelectRule::Hierarchy);
                }
                if rule_row(ui, "Inverse   Ctrl/Cmd Shift I") {
                    self.native_select_by_rule(SelectRule::Inverse);
                }
                ui.separator();
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
            let frame_button = crate::tool_icons::button(
                ui,
                "Frame selected  F",
                UiIcon::FrameSelected,
                !self.selection.is_empty(),
            );
            if frame_button.clicked() {
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
