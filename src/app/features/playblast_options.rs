//! Windows > Playblast and its option box (#460). The window is presentation only: its Start
//! button dispatches the same typed `Operation::Playblast` that the MCP `playblast` tool does, and
//! while a job runs it shows the progress `inspect_scene.playblast` reports with a Cancel button
//! that dispatches `Operation::CancelPlayblast`. PROVISIONAL, with no Maya source: the 1280x720
//! default size (the editor stores no viewport size), the 1 to 24 default range when the timeline
//! has none, the empty output directory and the option box layout; Maya's Playblast Options also
//! offer movie formats, a viewer and more, which stay out.

use crate::app::features::primitive_options::describe_unavailable;
use crate::app::*;
use crate::editor_operations::{PlayblastFormat, PlayblastInspection, PlayblastRequest};

const DEFAULT_SIZE: (u32, u32) = (1280, 720);
const MOVIE_REASON: &str = "Movie formats are not built; only a PNG image sequence (#460)";

pub(in crate::app) fn field_ids() -> [egui::Id; 7] {
    [
        "playblast_option_start",
        "playblast_option_end",
        "playblast_option_width",
        "playblast_option_height",
        "playblast_option_percent",
        "playblast_option_directory",
        "playblast_option_name",
    ]
    .map(egui::Id::unique)
}

pub(in crate::app) fn window_id() -> egui::Id {
    egui::Id::unique("playblast_options_window")
}

struct Dialog {
    start: String,
    end: String,
    width: String,
    height: String,
    percent: String,
    directory: String,
    name: String,
    ornaments: bool,
    message: Option<String>,
}

#[derive(Default)]
pub(in crate::app) struct PlayblastUi {
    dialog: Option<Dialog>,
}

fn whole<T: std::str::FromStr>(label: &str, text: &str) -> Result<T, String> {
    text.trim()
        .parse()
        .map_err(|_| format!("{label} must be a whole number"))
}

/// The request the Start button dispatches; the same shape the MCP parser builds.
fn request(dialog: &Dialog) -> Result<PlayblastRequest, String> {
    let percent: u32 = whole("Percent", &dialog.percent)?;
    if !(1..=100).contains(&percent) {
        return Err("Percent must be 1 to 100".into());
    }
    let directory = std::path::PathBuf::from(dialog.directory.trim());
    if !directory.is_absolute() {
        return Err("Output directory must be an absolute path".into());
    }
    let (start, end): (i64, i64) = (
        whole("Start frame", &dialog.start)?,
        whole("End frame", &dialog.end)?,
    );
    Ok(PlayblastRequest {
        directory,
        name: dialog.name.trim().to_owned(),
        start: Some(start as f64),
        end: Some(end as f64),
        width: Some(whole("Width", &dialog.width)?),
        height: Some(whole("Height", &dialog.height)?),
        percent,
        format: PlayblastFormat::Png,
        offscreen: true,
        show_ornaments: dialog.ornaments,
    })
}

/// What the window says about the job; it reads the inspection, like MCP does.
fn progress_text(inspection: &PlayblastInspection) -> String {
    let (done, total) = (inspection.frames_done, inspection.frames_total);
    match inspection.state {
        "running" => format!("Rendering {done} of {total} frames"),
        "finished" => format!("Finished: {total} frames"),
        "cancelled" => format!("Cancelled after {done} of {total} frames"),
        "failed" => format!(
            "Failed after {done} of {total} frames: {}",
            inspection.last_error.as_deref().unwrap_or_default()
        ),
        _ => "No playblast yet".into(),
    }
}

fn name_control(response: &egui::Response, role: egui::Role, label: &str) {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(role, enabled, label));
}

impl Editor {
    pub(in crate::app) fn windows_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Windows", |ui| {
            unsupported(ui, NOT_IMPLEMENTED_YET);
            let blocked = (self.files.is_closing() || self.files.is_modal())
                .then(|| self.playblast_refusal())
                .flatten();
            let item = ui
                .add_enabled(blocked.is_none(), egui::Button::new("Playblast"))
                .on_disabled_hover_text(blocked.as_deref().unwrap_or_default());
            if let Some(reason) = &blocked {
                describe_unavailable(ui.ctx(), item.id, reason);
            }
            if item.clicked() {
                self.open_playblast_options();
                ui.close();
            }
        });
    }

    pub(in crate::app) fn open_playblast_options(&mut self) {
        if self.playblast_ui.dialog.is_none() {
            let (width, height) = DEFAULT_SIZE;
            self.playblast_ui.dialog = Some(Dialog {
                start: self.timeline.start.to_string(),
                end: self.timeline.end.to_string(),
                width: width.to_string(),
                height: height.to_string(),
                percent: "100".into(),
                directory: String::new(),
                name: "playblast".into(),
                ornaments: true,
                message: None,
            });
        }
    }

    /// Why Start is disabled: a file prompt, a close or a running job. A human draft is not a
    /// reason: the native dispatch commits it first, like the other native actions.
    pub(in crate::app) fn playblast_refusal(&self) -> Option<String> {
        if self.files.is_closing() {
            Some(crate::app::menus::unavailable_while_closing("Playblast"))
        } else if self.files.is_modal() {
            Some("Finish the open file prompt first".into())
        } else if self.playblast.run.is_some() {
            Some("A playblast is already running".into())
        } else {
            None
        }
    }

    fn start_from_dialog(&mut self, dialog: &mut Dialog) {
        dialog.message = match request(dialog) {
            Err(message) => Some(message),
            Ok(request) => self
                .native_operation_result(Operation::Playblast(request))
                .err()
                .map(|error| error.to_string()),
        };
    }

    pub(in crate::app) fn playblast_options_window(&mut self, ctx: &egui::Context) {
        let hidden = self.files.is_modal() || self.files.is_closing();
        let Some(mut dialog) = self.playblast_ui.dialog.take() else {
            return;
        };
        if hidden {
            self.playblast_ui.dialog = Some(dialog);
            return;
        }
        let refusal = self.playblast_refusal();
        let inspection = self.inspect_playblast();
        let mut open = true;
        let (mut start, mut cancel, mut close) = (false, false, false);
        egui::Window::new("Playblast Options")
            .id(window_id())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                (start, cancel, close) =
                    dialog_body(ui, &mut dialog, refusal.as_deref(), &inspection);
            });
        if start {
            self.start_from_dialog(&mut dialog);
        }
        if cancel {
            self.native_operation(Operation::CancelPlayblast);
        }
        if open && !close {
            self.playblast_ui.dialog = Some(dialog);
        }
    }
}

/// Returns (start clicked, cancel clicked, close clicked).
fn dialog_body(
    ui: &mut egui::Ui,
    dialog: &mut Dialog,
    refusal: Option<&str>,
    inspection: &PlayblastInspection,
) -> (bool, bool, bool) {
    let ids = field_ids();
    egui::Grid::new("playblast_option_fields")
        .num_columns(2)
        .show(ui, |ui| {
            let fields = [
                ("Start frame", &mut dialog.start, 0),
                ("End frame", &mut dialog.end, 1),
                ("Width", &mut dialog.width, 2),
                ("Height", &mut dialog.height, 3),
                ("Percent", &mut dialog.percent, 4),
                ("Output directory", &mut dialog.directory, 5),
                ("Name", &mut dialog.name, 6),
            ];
            for (label, text, index) in fields {
                let caption = ui.label(label);
                let width = if index == 5 { 260.0 } else { 80.0 };
                let response = ui.add(
                    egui::TextEdit::singleline(text)
                        .id(ids[index])
                        .desired_width(width),
                );
                let response = response.labelled_by(caption.id);
                name_control(&response, egui::Role::TextInput, label);
                ui.end_row();
            }
            ui.label("Format");
            ui.label("PNG image sequence");
            ui.end_row();
        });
    ui.checkbox(&mut dialog.ornaments, "Show ornaments");
    ui.add_enabled(false, egui::Button::new("Movie formats"))
        .on_disabled_hover_text(MOVIE_REASON);
    ui.separator();
    if let Some(message) = &dialog.message {
        ui.colored_label(Color32::from_rgb(255, 160, 130), message);
    }
    let text = progress_text(inspection);
    let fraction = match inspection.frames_total {
        0 => 0.0,
        total => inspection.frames_done as f32 / total as f32,
    };
    let bar = ui.add(egui::ProgressBar::new(fraction).text(text.clone()));
    let enabled = bar.enabled();
    bar.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, enabled, &text));
    ui.horizontal(|ui| {
        let start = ui
            .add_enabled(refusal.is_none(), egui::Button::new("Start"))
            .on_disabled_hover_text(refusal.unwrap_or_default());
        if let Some(reason) = refusal {
            describe_unavailable(ui.ctx(), start.id, reason);
        }
        let running = inspection.state == "running";
        let cancel = ui
            .add_enabled(running, egui::Button::new("Cancel playblast"))
            .on_disabled_hover_text("No playblast is running");
        if !running {
            describe_unavailable(ui.ctx(), cancel.id, "No playblast is running");
        }
        let close = ui.button("Close");
        (start.clicked(), cancel.clicked(), close.clicked())
    })
    .inner
}
