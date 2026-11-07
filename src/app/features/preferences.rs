//! Application preferences as editor operations: the compatibility version and Reset of a
//! blocked file. Both are session settings like `SetGridVisible`: they never touch the scene,
//! the undo stack or the scene token.

use crate::app::*;
use azteccraft::compatibility::MayaVersion;
use azteccraft::native_scene::{AngularUnit, LinearUnit, Units};
use azteccraft::preferences::{Preferences, ResetOutcome, Store};

const CATEGORY_LIST_HEIGHT: f32 = 320.0;
const SETTINGS_CATEGORY: &str = "Settings";
const CATEGORY_REASON: &str = "Not implemented yet; only Settings is available";
const FILE_PROMPT_REASON: &str = "a file prompt is waiting for an answer";

/// Maya 2027's Preferences categories, in the order of its Preferences window page.
pub(in crate::app) const PREFERENCE_CATEGORIES: [&str; 39] = [
    "Animation",
    "Applications",
    "Assets",
    "Blue Pencil",
    "Cached Playback",
    "Cameras",
    "Color Management",
    "Display",
    "DX11 Shader",
    "Dynamics",
    "Files/Projects",
    "File References",
    "Font",
    "GLSL Shader",
    "GPU Cache",
    "Help",
    "Hypershade",
    "Interface",
    "Kinematics",
    "Manipulators",
    "Modeling",
    "Modules",
    "Node Editor",
    "NURBS",
    "Polygons",
    "Rendering",
    "Save Actions",
    "Search",
    "Security",
    "Selection",
    "Settings",
    "Snapping",
    "Sound",
    "Subdivs",
    "Time Slider",
    "UI Elements",
    "ViewCube",
    "Undo",
    "XGen",
];

/// The modeless Preferences window's UI state. `pending` is an unsaved picker value; it is not a
/// scene draft, so it never counts toward `has_draft()` or `is_dirty()`.
#[derive(Default)]
pub(in crate::app) struct PreferencesWindow {
    pub(in crate::app) open: bool,
    #[cfg_attr(not(any(test, feature = "mcp")), allow(dead_code))]
    pub(in crate::app) pending: Option<MayaVersion>,
    pub(in crate::app) error: Option<String>,
}

#[cfg(test)]
thread_local! {
    pub(in crate::app) static PLATFORM_PATH_RESOLUTIONS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

impl Editor {
    pub(in crate::app) fn open_preferences(&mut self) {
        self.preferences_window.open = true;
    }

    /// Drops the unsaved value and any save error; no operation runs. An approved app close ends
    /// here, and so do Cancel, the title-bar X and a successful Save.
    pub(in crate::app) fn discard_preferences_draft(&mut self) {
        self.preferences_window.pending = None;
        self.preferences_window.error = None;
    }

    fn close_preferences(&mut self) {
        self.discard_preferences_draft();
        self.preferences_window.open = false;
    }

    /// Moves a blocked file aside. A failure is shown in the window; nothing is changed then.
    fn reset_preferences_file(&mut self) {
        match self.native_operation_result(Operation::ResetPreferences) {
            Ok(_) => {
                self.preferences_window.error = None;
                self.status = "Preferences file reset".to_owned();
                self.error = false;
            }
            Err(_) => self.preferences_window.error = Some(self.status.clone()),
        }
    }

    /// Persists the pending value and closes. A failed save keeps the value and shows the error.
    fn save_preferences(&mut self) {
        let Some(version) = self.preferences_window.pending.take() else {
            self.close_preferences();
            return;
        };
        if self
            .native_operation_result(Operation::SetCompatibilityVersion { version })
            .is_err()
        {
            self.preferences_window.pending = Some(version);
            self.preferences_window.error = Some(self.status.clone());
        } else {
            self.status = match &self.preferences_store {
                Store::Writable { .. } => "Preferences saved".to_owned(),
                Store::Blocked { error, .. } => {
                    format!("Applied for this session; not saved ({error})")
                }
                Store::SessionOnly => {
                    "Applied for this session; not saved (no config directory)".to_owned()
                }
            };
            self.error = false;
            self.close_preferences();
        }
    }

    /// Help Line suffix while the session follows an older Maya than the documentation pin.
    pub(in crate::app) fn compatibility_help(&self, ui: &mut egui::Ui) {
        let version = self.preferences.maya_compatibility_version;
        if version != MayaVersion::PINNED {
            ui.weak(format!("Maya compatibility: {}", version.year()));
        }
    }

    pub(in crate::app) fn show_preferences(&mut self, ctx: &egui::Context) {
        if !self.preferences_window.open {
            return;
        }
        let save_blocked = if self.files.is_closing() {
            Some(crate::app::menus::CLOSING_REASON)
        } else if self.files.is_modal() {
            Some(FILE_PROMPT_REASON)
        } else {
            None
        };
        let mut open = true;
        let (mut save, mut cancel) = (false, false);
        let error = self.preferences_window.error.clone();
        let units = self.history.scene().units().clone();
        let current = self.preferences.maya_compatibility_version;
        let persistence = match &self.preferences_store {
            Store::Writable { .. } => "Saved".to_owned(),
            Store::Blocked { error, .. } => format!("Not saved: {error}"),
            Store::SessionOnly => "Not saved: no config directory".to_owned(),
        };
        let blocked = matches!(self.preferences_store, Store::Blocked { .. });
        let pending = self.preferences_window.pending.is_some();
        let mut reset = false;
        let mut shown = self.preferences_window.pending.unwrap_or(current);
        egui::Window::new("Preferences")
            .id(egui::Id::unique("preferences_window"))
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("Edit", |ui| {
                        for label in ["Revert to Saved", "Restore Default Settings"] {
                            ui.add_enabled(false, egui::Button::new(label))
                                .on_disabled_hover_text(NOT_IMPLEMENTED_YET);
                        }
                    });
                });
                ui.separator();
                ui.horizontal_top(|ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("preference_categories")
                        .max_height(CATEGORY_LIST_HEIGHT)
                        .min_scrolled_height(CATEGORY_LIST_HEIGHT)
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                for name in PREFERENCE_CATEGORIES {
                                    if name == SETTINGS_CATEGORY {
                                        let _ = ui.selectable_label(true, name);
                                    } else {
                                        ui.add_enabled(false, egui::Button::new(name))
                                            .on_disabled_hover_text(CATEGORY_REASON);
                                    }
                                }
                            });
                        });
                    ui.vertical(|ui| {
                        ui.heading(SETTINGS_CATEGORY);
                        working_units(ui, &units);
                        ui.separator();
                        compatibility_picker(ui, &mut shown);
                        ui.label(&persistence);
                        if blocked {
                            let mut button =
                                ui.add_enabled(!pending, egui::Button::new(RESET_LABEL));
                            button = if pending {
                                button.on_disabled_hover_text(RESET_PENDING_REASON)
                            } else {
                                button.on_hover_text(RESET_HOVER)
                            };
                            reset = button.clicked();
                            if pending {
                                ui.weak(RESET_PENDING_REASON);
                            }
                        }
                        if let Some(error) = &error {
                            ui.colored_label(ui.visuals().error_fg_color, error);
                        }
                    });
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let mut button =
                        ui.add_enabled(save_blocked.is_none(), egui::Button::new("Save"));
                    if let Some(reason) = save_blocked {
                        button = button.on_disabled_hover_text(reason);
                    }
                    save = button.clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        self.preferences_window.pending = (shown != current).then_some(shown);
        if reset {
            self.reset_preferences_file();
        } else if save {
            self.save_preferences();
        } else if cancel || !open {
            self.close_preferences();
        }
    }

    /// Loads the per-user preferences file. Only the app entry point calls this; constructors
    /// stay session-only so tests never read a developer's real settings.
    pub fn load_preferences(&mut self) {
        #[cfg(test)]
        PLATFORM_PATH_RESOLUTIONS.with(|count| count.set(count.get() + 1));
        let Ok(path) = azteccraft::preferences::platform_path(
            &|name| std::env::var_os(name),
            azteccraft::preferences::Os::current(),
        ) else {
            return;
        };
        let loaded = azteccraft::preferences::load(&path);
        self.preferences = loaded.preferences;
        self.preferences_store = loaded.store;
    }

    /// A change is persisted first when the store is writable; a failed write leaves value and
    /// file as they were. A blocked or session-only store applies the value to this session only.
    pub(in crate::app) fn set_compatibility_version(
        &mut self,
        version: MayaVersion,
    ) -> Result<OperationReply, OperationError> {
        if self.preferences.maya_compatibility_version == version {
            return Ok(OperationReply::Unchanged);
        }
        let next = Preferences {
            maya_compatibility_version: version,
        };
        if matches!(self.preferences_store, Store::Writable { .. }) {
            self.preferences_store
                .save(&next)
                .map_err(OperationError::Preferences)?;
        }
        self.preferences = next;
        Ok(OperationReply::Changed)
    }

    pub(in crate::app) fn reset_preferences(&mut self) -> Result<OperationReply, OperationError> {
        match self
            .preferences_store
            .reset()
            .map_err(OperationError::Preferences)?
        {
            ResetOutcome::Unchanged => Ok(OperationReply::Unchanged),
            ResetOutcome::MovedAside { .. } | ResetOutcome::NothingToMove => {
                self.preferences = Preferences::default();
                Ok(OperationReply::Changed)
            }
        }
    }
}

/// Working Units belong to the scene; changing them would convert scene data, which is not
/// supported yet, so the three combos only show what the scene declares.
pub(in crate::app) const UNITS_REASON: &str = "Working units belong to the scene; changing them would convert scene data, which is not supported yet";

fn linear_token(unit: &LinearUnit) -> &str {
    match unit {
        LinearUnit::Centimeter => "centimeter",
        LinearUnit::Millimeter => "millimeter",
        LinearUnit::Meter => "meter",
        LinearUnit::Inch => "inch",
        LinearUnit::Foot => "foot",
        LinearUnit::Unsupported(token) => token,
    }
}

fn angular_token(unit: &AngularUnit) -> &str {
    match unit {
        AngularUnit::Degree => "degree",
        AngularUnit::Radian => "radian",
        AngularUnit::Unsupported(token) => token,
    }
}

const COMPATIBILITY_HOVER: &str = "AztecCraft extension, not a Maya setting. Documentation for 2015 and 2021 is partly unavailable, and so far only Frame Selection (F, Ctrl+F) follows this version.";

const RESET_LABEL: &str = "Reset preferences file";
const RESET_HOVER: &str =
    "Moves the unreadable file aside as a numbered backup and starts from defaults";
const RESET_PENDING_REASON: &str = "Save or cancel the picked version first";

fn version_label(version: MayaVersion) -> String {
    if version == MayaVersion::PINNED {
        format!("{} (documentation pin 2027.2)", version.year())
    } else {
        version.year().to_string()
    }
}

fn compatibility_picker(ui: &mut egui::Ui, shown: &mut MayaVersion) {
    ui.strong("AztecCraft extension: Maya compatibility");
    egui::ComboBox::from_label("Maya compatibility")
        .selected_text(version_label(*shown))
        .show_ui(ui, |ui| {
            for year in MayaVersion::OLDEST.year()..=MayaVersion::PINNED.year() {
                let version =
                    MayaVersion::try_from(u64::from(year)).expect("year within the range");
                ui.selectable_value(shown, version, version_label(version));
            }
        })
        .response
        .on_hover_text(COMPATIBILITY_HOVER);
}

pub(in crate::app) fn working_units(ui: &mut egui::Ui, units: &Units) {
    ui.strong("Working Units");
    for (label, token) in [
        ("Linear", linear_token(&units.linear)),
        ("Angular", angular_token(&units.angular)),
        ("Time", units.time.as_str()),
    ] {
        ui.add_enabled_ui(false, |ui| {
            egui::ComboBox::from_label(label)
                .selected_text(token)
                .show_ui(ui, |_| {})
                .response
                .on_disabled_hover_text(UNITS_REASON);
        });
    }
    ui.weak(UNITS_REASON);
}

#[cfg(test)]
mod tests;
