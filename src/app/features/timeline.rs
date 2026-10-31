//! Current time, playback range and playback: session state of the editor, reset on New and Open
//! and never dirtying by itself. The keys themselves live in `NativeScene`. Playback runs as a tick
//! in `render`, not as an `Operation`, so it cannot fill the undo queue.

use crate::app::*;
use azteccraft::native_scene::{ALL_CHANNELS, ChannelMask, NativeError, whole_frame};

/// PROVISIONAL: the film time unit is 24 fps, and real-time playback is an assumed default.
pub(in crate::app) const FRAMES_PER_SECOND: f64 = 24.0;
/// The Maya 2027 New Scene Options default animation start and end. The docs state no playback
/// range default, so this range starts equal to them (PROVISIONAL).
const DEFAULT_START: i32 = 1;
const DEFAULT_END: i32 = 48;

pub(in crate::app) struct Timeline {
    pub(in crate::app) current: i32,
    pub(in crate::app) start: i32,
    pub(in crate::app) end: i32,
    pub(in crate::app) playing: bool,
    /// The wall-clock second and frame playback counts from; the first tick sets it.
    anchor: Option<(f64, i32)>,
    time_field: FrameField,
    start_field: FrameField,
    end_field: FrameField,
}

/// A frame text field. It shows the live value until it takes focus, then holds what is typed
/// until focus leaves. It is not a draft: only leaving it acts, and only on changed text.
#[derive(Default)]
struct FrameField {
    text: String,
    editing: bool,
}

impl FrameField {
    /// The typed text, once, on the frame focus leaves. Esc abandons it.
    fn show(&mut self, ui: &mut egui::Ui, id: egui::Id, shown: i32, label: &str) -> Option<String> {
        if !self.editing {
            self.text = shown.to_string();
        }
        ui.label(label);
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.text)
                .id(id)
                .desired_width(52.0),
        );
        if response.has_focus() {
            self.editing = true;
            return None;
        }
        let left = std::mem::take(&mut self.editing);
        let abandoned = ui.input(|input| input.key_pressed(Key::Escape));
        (left && !abandoned && self.text.trim() != shown.to_string())
            .then(|| self.text.trim().to_owned())
    }
}

/// A native timeline request. Hotkeys and controls share `run_timeline_action`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum TimelineAction {
    SetKey,
    DeleteKey,
    TogglePlay,
    GoTo(i32),
    StepFrame(i32),
    NextKey,
    PreviousKey,
}

fn time_field_id() -> egui::Id {
    egui::Id::new("timeline_current_time")
}

fn start_field_id() -> egui::Id {
    egui::Id::new("timeline_playback_start")
}

fn end_field_id() -> egui::Id {
    egui::Id::new("timeline_playback_end")
}

fn time_slider_id() -> egui::Id {
    egui::Id::new("time_slider")
}

pub(in crate::app) fn field_ids() -> [egui::Id; 3] {
    [time_field_id(), start_field_id(), end_field_id()]
}

/// The Maya 2027 Animation hotkeys page: S sets a key, Alt+V plays or stops, Alt+, and Alt+. step a
/// frame, comma and period step a key. Modifiers are per event, as in `history_chord`; a held step
/// repeats (PROVISIONAL, the docs do not say) and S does not. Option on macOS relies on egui-winit's
/// physical-key fallback, PROVISIONAL.
pub(in crate::app) fn timeline_chord(events: &[egui::Event]) -> Option<TimelineAction> {
    events.iter().find_map(|event| match event {
        egui::Event::Key {
            key,
            pressed: true,
            repeat,
            modifiers,
            ..
        } => {
            let alt = modifiers.alt && !modifiers.ctrl && !modifiers.shift && !modifiers.command;
            match key {
                Key::S if modifiers.is_none() && !repeat => Some(TimelineAction::SetKey),
                Key::V if alt && !repeat => Some(TimelineAction::TogglePlay),
                Key::Comma if alt => Some(TimelineAction::StepFrame(-1)),
                Key::Period if alt => Some(TimelineAction::StepFrame(1)),
                Key::Comma if modifiers.is_none() => Some(TimelineAction::PreviousKey),
                Key::Period if modifiers.is_none() => Some(TimelineAction::NextKey),
                _ => None,
            }
        }
        _ => None,
    })
}

/// Marks near the right edge put their label left of the tick, so it is not clipped. The margin
/// holds the longest frame number at the ruler's font size.
fn ruler_label_anchor(x: f32, right: f32) -> (f32, egui::Align2) {
    const LABEL_MARGIN: f32 = 80.0;
    if right - x < LABEL_MARGIN {
        (x - 2.0, egui::Align2::RIGHT_TOP)
    } else {
        (x + 2.0, egui::Align2::LEFT_TOP)
    }
}

/// Distance between ruler marks, in frames: the first 1, 2 or 5 times a power of ten that leaves
/// room for a label.
fn ruler_step(span: f32, width: f32) -> i32 {
    const MIN_GAP: f32 = 40.0;
    let mut decade = 1;
    while decade < 10_000_000 {
        for step in [decade, decade * 2, decade * 5] {
            if width / span * step as f32 >= MIN_GAP {
                return step;
            }
        }
        decade *= 10;
    }
    decade
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            current: DEFAULT_START,
            start: DEFAULT_START,
            end: DEFAULT_END,
            playing: false,
            anchor: None,
            time_field: FrameField::default(),
            start_field: FrameField::default(),
            end_field: FrameField::default(),
        }
    }
}

impl Timeline {
    /// The frame `now` seconds into playback, looping continuously over the range (PROVISIONAL:
    /// the docs state no default loop mode). `None` when no frame can be derived from `now`.
    fn frame_at(&self, now: f64, anchor: (f64, i32)) -> Option<i32> {
        let elapsed = now - anchor.0;
        if !elapsed.is_finite() || elapsed < 0.0 {
            return None;
        }
        let length = f64::from(self.end - self.start) + 1.0;
        let played = (elapsed * FRAMES_PER_SECOND).floor() + f64::from(anchor.1 - self.start);
        Some(self.start + played.rem_euclid(length) as i32)
    }
}

/// Whether an operation stops playback before it runs. Reads, selection, the camera and view
/// settings leave the scene alone and keep playing; a new operation stops it by default.
pub(in crate::app) fn stops_playback(operation: &Operation) -> bool {
    match operation {
        Operation::Play
        | Operation::Stop
        | Operation::SetActiveTool(_)
        | Operation::SetToolSpace { .. }
        | Operation::SetToolHandle(_)
        | Operation::SetManipulatorSize(_)
        | Operation::Select { .. }
        | Operation::ModifySelection { .. }
        | Operation::SelectByRule(_)
        | Operation::SetSelectionMode(_)
        | Operation::DeselectAll
        | Operation::SelectFace { .. }
        | Operation::SelectVertex { .. }
        | Operation::SelectEdge { .. }
        | Operation::FrameSelected { .. }
        | Operation::FrameAll
        | Operation::SetGridVisible { .. }
        | Operation::SetShadingMode(_)
        | Operation::SetSidebarPanelVisible { .. }
        | Operation::SelectAttributeEditorTab { .. }
        | Operation::Orbit { .. }
        | Operation::Pan { .. }
        | Operation::Dolly { .. }
        | Operation::SetCameraPose { .. } => false,
        #[cfg(any(test, feature = "mcp"))]
        Operation::Inspect { .. } | Operation::InspectKeys { .. } => false,
        _ => true,
    }
}

impl Editor {
    /// Writes `frame`'s pose, then moves the time and refreshes the fields. Callers check for a
    /// draft first: a stale field would read as a draft, and a draft would be lost to the refresh.
    fn write_time(&mut self, frame: i32) -> Result<(), OperationError> {
        let wrote = self
            .history
            .apply_evaluated(frame)
            .map_err(OperationError::from_history)?;
        self.timeline.current = frame;
        if wrote {
            self.sync_fields();
        }
        Ok(())
    }

    pub(in crate::app) fn set_current_time(
        &mut self,
        time: f64,
    ) -> Result<OperationReply, OperationError> {
        self.history
            .scene()
            .check_time_unit()
            .map_err(OperationError::Scene)?;
        let frame = whole_frame(time).map_err(OperationError::Scene)?;
        if self.has_draft() {
            return Err(OperationError::BusyEditing);
        }
        if frame == self.timeline.current {
            return Ok(OperationReply::Unchanged);
        }
        self.write_time(frame)?;
        Ok(OperationReply::Changed)
    }

    /// The range as whole frames. It runs before a draft commit, so a refusal changes nothing.
    pub(in crate::app) fn check_playback_range(
        &self,
        start: f64,
        end: f64,
    ) -> Result<(i32, i32), OperationError> {
        self.history
            .scene()
            .check_time_unit()
            .map_err(OperationError::Scene)?;
        let start = whole_frame(start).map_err(OperationError::Scene)?;
        let end = whole_frame(end).map_err(OperationError::Scene)?;
        if start > end {
            return Err(OperationError::Scene(NativeError::InvalidPlaybackRange));
        }
        Ok((start, end))
    }

    pub(in crate::app) fn set_playback_range(
        &mut self,
        start: f64,
        end: f64,
    ) -> Result<OperationReply, OperationError> {
        let (start, end) = self.check_playback_range(start, end)?;
        let changed = (start, end) != (self.timeline.start, self.timeline.end);
        self.timeline.start = start;
        self.timeline.end = end;
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }

    /// Starts playback from the current time, or from the range start when that is outside the
    /// range (PROVISIONAL). A draft refuses it: playback never commits one.
    pub(in crate::app) fn play(&mut self) -> Result<OperationReply, OperationError> {
        self.history
            .scene()
            .check_time_unit()
            .map_err(OperationError::Scene)?;
        if self.has_draft() {
            return Err(OperationError::BusyEditing);
        }
        if self.timeline.playing {
            return Ok(OperationReply::Unchanged);
        }
        if !(self.timeline.start..=self.timeline.end).contains(&self.timeline.current) {
            self.write_time(self.timeline.start)?;
        }
        self.timeline.playing = true;
        self.timeline.anchor = None;
        Ok(OperationReply::Changed)
    }

    pub(in crate::app) fn stop(&mut self) -> OperationReply {
        let was_playing = self.timeline.playing;
        self.stop_playback();
        if was_playing {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        }
    }

    pub(in crate::app) fn stop_playback(&mut self) {
        self.timeline.playing = false;
        self.timeline.anchor = None;
    }

    /// Why playback cannot go on: each of these owns the fields or the pose a tick would rewrite.
    fn playback_blocker(&self, text_focused: bool) -> Option<&'static str> {
        if self.files.is_closing() {
            Some("the window is closing")
        } else if self.files.is_modal() {
            Some("a file action needs an answer")
        } else if self.history.active_manipulation().is_some() || self.native_move.is_some() {
            Some("a manipulation is active")
        } else if text_focused {
            Some("a text field is focused")
        } else if self.has_draft() {
            Some("a field holds a draft")
        } else {
            None
        }
    }

    /// One playback step at `now` seconds. A frame is skipped rather than slowing playback down,
    /// so several may pass between two ticks, and a blocker stops playback before any write.
    pub(in crate::app) fn tick_playback(&mut self, now: f64, text_focused: bool) {
        if !self.timeline.playing {
            return;
        }
        if let Some(reason) = self.playback_blocker(text_focused) {
            self.stop_playback();
            self.status = format!("Playback stopped: {reason}");
            self.error = false;
            return;
        }
        let anchor = self.timeline.anchor.unwrap_or((now, self.timeline.current));
        let Some(frame) = self.timeline.frame_at(now, anchor) else {
            self.timeline.anchor = Some((now, self.timeline.current));
            return;
        };
        self.timeline.anchor = Some(anchor);
        if frame == self.timeline.current {
            return;
        }
        if let Err(error) = self.write_time(frame) {
            self.stop_playback();
            self.set_error_status(&error);
            self.error = true;
        }
    }

    /// Esc stops playback before any of its other meanings.
    pub(in crate::app) fn escape_stops_playback(&mut self, ctx: &egui::Context) -> bool {
        let stops = self.timeline.playing && ctx.input(|input| input.key_pressed(Key::Escape));
        if stops {
            self.stop_playback();
            self.status = "Playback stopped".into();
            self.error = false;
        }
        stops
    }

    /// The nodes the Time Slider shows keys for: the selected objects.
    fn selected_key_frames(&self) -> std::collections::BTreeSet<i32> {
        let nodes: Vec<NodeId> = self.selection.members().iter().map(|t| t.node).collect();
        self.history.scene().key_frames(&nodes)
    }

    /// The nearest selected key after or before the current time. It does not wrap (PROVISIONAL).
    fn adjacent_key(&self, next: bool) -> Option<i32> {
        let frames = self.selected_key_frames();
        let current = self.timeline.current;
        if next {
            frames.range(current.saturating_add(1)..).next().copied()
        } else {
            frames.range(..current).next_back().copied()
        }
    }

    /// The one native entry for timeline requests. Every step goes through the shared pipeline.
    pub(in crate::app) fn run_timeline_action(&mut self, action: TimelineAction) {
        let current = self.timeline.current;
        let operation = match action {
            TimelineAction::SetKey => Operation::SetKeyframe {
                nodes: None,
                time: None,
                channels: ALL_CHANNELS,
            },
            TimelineAction::DeleteKey => Operation::DeleteKeys {
                nodes: None,
                time: Some(f64::from(current)),
                channels: ALL_CHANNELS,
            },
            TimelineAction::TogglePlay if self.timeline.playing => Operation::Stop,
            TimelineAction::TogglePlay => Operation::Play,
            TimelineAction::GoTo(frame) => Operation::SetCurrentTime {
                time: f64::from(frame),
            },
            TimelineAction::StepFrame(delta) => Operation::SetCurrentTime {
                time: f64::from(current) + f64::from(delta),
            },
            TimelineAction::NextKey | TimelineAction::PreviousKey => {
                let next = action == TimelineAction::NextKey;
                let Some(frame) = self.adjacent_key(next) else {
                    self.status = format!(
                        "No {} key on the selection",
                        if next { "next" } else { "previous" }
                    );
                    self.error = false;
                    return;
                };
                Operation::SetCurrentTime {
                    time: f64::from(frame),
                }
            }
        };
        self.native_operation(operation);
    }

    fn time_slider(&mut self, ui: &mut egui::Ui) -> Option<TimelineAction> {
        const INSET: f32 = 10.0;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::hover());
        let sense = if ui.is_enabled() {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::hover()
        };
        let response = ui.interact(rect, time_slider_id(), sense);
        let (start, end) = (self.timeline.start, self.timeline.end);
        let span = (end - start).max(1) as f32;
        let width = (rect.width() - 2.0 * INSET).max(1.0);
        let x_of = |frame: i32| rect.left() + INSET + (frame - start) as f32 / span * width;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, Color32::from_gray(38));
        let step = ruler_step(span, width);
        let mut mark = start.div_euclid(step) * step;
        while mark <= end {
            if mark >= start {
                let x = x_of(mark);
                painter.vline(
                    x,
                    rect.top()..=rect.top() + 7.0,
                    egui::Stroke::new(1.0, Color32::GRAY),
                );
                let (label_x, align) = ruler_label_anchor(x, rect.right());
                painter.text(
                    egui::pos2(label_x, rect.top() + 7.0),
                    align,
                    mark,
                    egui::FontId::proportional(11.0),
                    Color32::LIGHT_GRAY,
                );
            }
            mark += step;
        }
        for frame in self.selected_key_frames().range(start..=end) {
            painter.vline(
                x_of(*frame),
                rect.y_range(),
                egui::Stroke::new(2.0, Color32::from_rgb(214, 74, 64)),
            );
        }
        if (start..=end).contains(&self.timeline.current) {
            let x = x_of(self.timeline.current);
            painter.rect_filled(
                egui::Rect::from_x_y_ranges(x - 2.0..=x + 2.0, rect.y_range()),
                0.0,
                Color32::from_rgba_unmultiplied(130, 190, 240, 170),
            );
        }
        // A click or drag scrubs. Each frame the pointer crosses is one time change.
        if !(response.is_pointer_button_down_on() || response.clicked()) {
            return None;
        }
        let pointer = response.interact_pointer_pos()?;
        let along = ((pointer.x - rect.left() - INSET) / width).clamp(0.0, 1.0);
        let frame = start + (along * span).round() as i32;
        let frame = frame.clamp(start, end);
        (frame != self.timeline.current).then_some(TimelineAction::GoTo(frame))
    }

    /// The bottom row: Time Slider, playback range and current time fields, transport and keys.
    /// Button order follows the Maya 2027 Playback Controls page.
    pub(in crate::app) fn timeline_controls(&mut self, ui: &mut egui::Ui) {
        let live = !self.files.is_modal() && !self.files.is_closing();
        let mut action = None;
        let mut typed_time = None;
        let mut typed_range = None;
        ui.add_enabled_ui(live, |ui| {
            action = self.time_slider(ui);
            ui.horizontal_wrapped(|ui| {
                let (start, end, current) = (
                    self.timeline.start,
                    self.timeline.end,
                    self.timeline.current,
                );
                let typed_start =
                    self.timeline
                        .start_field
                        .show(ui, start_field_id(), start, "Start");
                let typed_end = self.timeline.end_field.show(ui, end_field_id(), end, "End");
                if typed_start.is_some() || typed_end.is_some() {
                    typed_range = Some((
                        typed_start.unwrap_or_else(|| start.to_string()),
                        typed_end.unwrap_or_else(|| end.to_string()),
                    ));
                }
                ui.separator();
                for (label, hover, request) in [
                    ("|<", "Go To Start", TimelineAction::GoTo(start)),
                    ("<k", "Step Back Key (,)", TimelineAction::PreviousKey),
                    (
                        "<",
                        "Step Back Frame (Alt+,)",
                        TimelineAction::StepFrame(-1),
                    ),
                ] {
                    if ui.button(label).on_hover_text(hover).clicked() {
                        action = Some(request);
                    }
                }
                ui.add_enabled(false, egui::Button::new("<|"))
                    .on_disabled_hover_text("Play Backwards: not implemented yet");
                let (label, hover) = if self.timeline.playing {
                    ("Stop", "Stop (Esc or Alt+V)")
                } else {
                    ("Play", "Play Forwards (Alt+V)")
                };
                if ui.button(label).on_hover_text(hover).clicked() {
                    action = Some(TimelineAction::TogglePlay);
                }
                for (label, hover, request) in [
                    (
                        ">",
                        "Step Forward Frame (Alt+.)",
                        TimelineAction::StepFrame(1),
                    ),
                    ("k>", "Step Forward Key (.)", TimelineAction::NextKey),
                    (">|", "Go To End", TimelineAction::GoTo(end)),
                ] {
                    if ui.button(label).on_hover_text(hover).clicked() {
                        action = Some(request);
                    }
                }
                ui.separator();
                typed_time = self
                    .timeline
                    .time_field
                    .show(ui, time_field_id(), current, "Time");
                ui.separator();
                if ui
                    .button("Set Key (S)")
                    .on_hover_text("Key the selected objects' transform channels")
                    .clicked()
                {
                    action = Some(TimelineAction::SetKey);
                }
                if ui
                    .button("Delete Key")
                    .on_hover_text("Delete the selected objects' keys at the current time")
                    .clicked()
                {
                    action = Some(TimelineAction::DeleteKey);
                }
                ui.weak("Keys are session-only and block Save");
            });
        });
        if let Some(text) = typed_time {
            match text.parse::<f64>() {
                Ok(frame) if frame.fract() == 0.0 && frame.abs() <= 2_147_483_647.0 => {
                    action = Some(TimelineAction::GoTo(frame as i32));
                }
                _ => self.refuse_frame_text(),
            }
        }
        if let Some((start, end)) = typed_range {
            match (start.parse::<f64>(), end.parse::<f64>()) {
                (Ok(start), Ok(end)) => {
                    self.native_operation(Operation::SetPlaybackRange { start, end });
                }
                _ => self.refuse_frame_text(),
            }
        }
        if let Some(action) = action {
            self.run_timeline_action(action);
        }
    }

    fn refuse_frame_text(&mut self) {
        self.status = "Enter a whole frame".into();
        self.error = true;
    }

    /// Refuses a key request before a draft commit. Omitted nodes mean the object selection, which
    /// a component mode does not offer.
    pub(in crate::app) fn check_key_request(
        &self,
        nodes: Option<&[NodeId]>,
        time: Option<f64>,
        channels: ChannelMask,
    ) -> Result<(), OperationError> {
        self.history
            .scene()
            .check_time_unit()
            .map_err(OperationError::Scene)?;
        if let Some(time) = time {
            whole_frame(time).map_err(OperationError::Scene)?;
        }
        if !channels.contains(&true) {
            return Err(OperationError::InvalidInput(
                "Name at least one channel".into(),
            ));
        }
        match nodes {
            Some([]) => Err(OperationError::InvalidInput(
                "Name at least one object".into(),
            )),
            Some(_) => Ok(()),
            None if self.selection_mode.is_component() => Err(OperationError::InvalidInput(
                "Keys need Object or Hierarchy selection mode".into(),
            )),
            None if self.selection.is_empty() => Err(OperationError::InvalidInput(
                "Select an object first".into(),
            )),
            None => Ok(()),
        }
    }

    fn key_nodes(&self, nodes: Option<Vec<NodeId>>) -> Vec<NodeId> {
        nodes.unwrap_or_else(|| self.selection.members().iter().map(|t| t.node).collect())
    }

    pub(in crate::app) fn set_key_command(
        &self,
        nodes: Option<Vec<NodeId>>,
        time: Option<f64>,
        channels: ChannelMask,
    ) -> Result<Command, OperationError> {
        let current_frame = self.timeline.current;
        Ok(Command::SetKeys {
            nodes: self.key_nodes(nodes),
            frame: time
                .map(whole_frame)
                .transpose()
                .map_err(OperationError::Scene)?
                .unwrap_or(current_frame),
            channels,
            current_frame,
        })
    }

    pub(in crate::app) fn delete_keys_command(
        &self,
        nodes: Option<Vec<NodeId>>,
        time: Option<f64>,
        channels: ChannelMask,
    ) -> Result<Command, OperationError> {
        Ok(Command::DeleteKeys {
            nodes: self.key_nodes(nodes),
            frame: time
                .map(whole_frame)
                .transpose()
                .map_err(OperationError::Scene)?,
            channels,
            current_frame: self.timeline.current,
        })
    }

    /// One page of keys. A read passes no token guard, so the cursor binds the document and the
    /// revision; a playback tick that moves a keyed channel makes it stale.
    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn inspect_keys(
        &self,
        node: Option<NodeId>,
        cursor: Option<&crate::editor_operations::PageCursor>,
    ) -> Result<crate::editor_operations::KeysInspection, OperationError> {
        use crate::editor_operations::{
            KeyInspection, KeysInspection, MAX_KEYS_PER_PAGE, PageCursor,
        };
        use azteccraft::native_scene::KEY_CHANNELS;
        let scene = self.history.scene();
        if let Some(node) = node {
            scene.node(node).map_err(OperationError::Scene)?;
        }
        let keys_total = scene.key_rows(node).count();
        let key_offset = match cursor {
            None => 0,
            Some(cursor) => {
                if cursor.scene_token != self.scene_token
                    || cursor.revision != self.history.revision()
                {
                    return Err(OperationError::StaleScene);
                }
                if cursor.offset > keys_total {
                    return Err(OperationError::InvalidInput(
                        "cursor offset is past the last key".into(),
                    ));
                }
                cursor.offset
            }
        };
        let keys: Vec<KeyInspection> = scene
            .key_rows(node)
            .skip(key_offset)
            .take(MAX_KEYS_PER_PAGE)
            .map(|row| KeyInspection {
                node_id: row.node.to_decimal(),
                channel: KEY_CHANNELS[row.channel],
                frame: row.frame,
                value: row.value,
            })
            .collect();
        let end = key_offset + keys.len();
        Ok(KeysInspection {
            scene_token: self.scene_token.to_string(),
            revision: self.history.revision(),
            unavailable_reason: scene.has_source().then_some(
                "Animation curves of a sourced scene are not exposed or evaluated, which does not mean there are none",
            ),
            keys_total,
            key_offset,
            keys,
            next_cursor: (end < keys_total).then(|| {
                PageCursor {
                    scene_token: self.scene_token,
                    revision: self.history.revision(),
                    offset: end,
                }
                .to_string()
            }),
        })
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn inspect_timeline(&self) -> crate::editor_operations::TimelineInspection {
        crate::editor_operations::TimelineInspection {
            current_time: self.timeline.current,
            playback_start: self.timeline.start,
            playback_end: self.timeline.end,
            playing: self.timeline.playing,
            frames_per_second: FRAMES_PER_SECOND,
            loop_mode: "continuous",
            key_count: self.history.scene().session_key_count(),
        }
    }
}

#[cfg(test)]
mod tests;
