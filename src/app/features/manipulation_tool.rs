//! Rotate and Scale: Tool Box state, Object/World selector, numeric entry, handle hover, pick and
//! paint. The drag samples, guards and history entry are shared with Move (`move_tool.rs`).

use crate::app::{Editor, NOT_IMPLEMENTED_YET};
use crate::editor_operations::{
    ActiveTool, ManipulationSpace, ManipulationValue, Operation, OperationError, OperationReply,
    SelectionMode, ToolHandle,
};
use azteccraft::{
    history::{
        EditOutcome, ManipulationError, ManipulationFinish, ManipulationId, ManipulationKindState,
        ManipulationProvenance,
    },
    manipulator_handles::{
        DisabledKind, HandleFrame, Pick, RotateDrag, RotateHandles, ScaleDrag, ScaleHandles,
        paint::{paint_rotate, paint_scale},
    },
    move_handles::{HandleError, ManipulatorSize, ViewportCapture},
    native_scene::{RotateAxis, ScaleHandle, TransformTarget},
};
use eframe::egui;
use glam::{DVec3, Vec2};

/// The Rotate or Scale handle under a pixel, as the active tool sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum ToolPick {
    Rotate(Pick<RotateAxis>),
    Scale(Pick<ScaleHandle>),
}

const VIEW_BUDGET: usize = 256 * 1024 * 1024;

/// Rotate and Scale tool settings. Maya keeps the axis orientation per tool and defaults it to
/// Object for Rotate and World for Scale (2026 tool settings pages).
pub(in crate::app) struct ManipulationOptions {
    rotate_space: ManipulationSpace,
    scale_space: ManipulationSpace,
    rotate_axis: RotateAxis,
    scale_handle: ScaleHandle,
    // Global and session-only: it never enters the scene, the history or the dirty state.
    manipulator_size: ManipulatorSize,
    numeric: String,
    // The status text last written for the handle under the pointer, and whether it is a refusal.
    hover: Option<(String, bool)>,
    // Set when the hover memory was reset (drag start, tool change): the next idle frame records
    // the handle under the pointer without writing it, so a "committed" message is not replaced.
    quiet: bool,
    // Why the scene refused the last update of the active manipulation, for the inspection.
    #[cfg_attr(not(any(test, feature = "mcp")), allow(dead_code))]
    pub(in crate::app) refusal: Option<String>,
    // The last remote value as given, in scene units: inspection echoes it bit-exactly instead of
    // converting the internal parameter back, which can differ in the last bit.
    pub(in crate::app) declared: Option<(ManipulationId, f64)>,
}

impl Default for ManipulationOptions {
    fn default() -> Self {
        Self {
            rotate_space: ManipulationSpace::Object,
            scale_space: ManipulationSpace::World,
            rotate_axis: RotateAxis::X,
            scale_handle: ScaleHandle::Uniform,
            manipulator_size: ManipulatorSize::default(),
            numeric: String::new(),
            hover: None,
            quiet: false,
            refusal: None,
            declared: None,
        }
    }
}

impl ManipulationOptions {
    pub(in crate::app) fn space(&self, tool: ActiveTool) -> ManipulationSpace {
        match tool {
            ActiveTool::Scale => self.scale_space,
            _ => self.rotate_space,
        }
    }

    pub(in crate::app) fn rotate_axis(&self) -> RotateAxis {
        self.rotate_axis
    }

    pub(in crate::app) fn scale_handle(&self) -> ScaleHandle {
        self.scale_handle
    }

    pub(in crate::app) fn manipulator_size(&self) -> ManipulatorSize {
        self.manipulator_size
    }
}

impl Editor {
    pub(in crate::app) fn set_tool_space(
        &mut self,
        tool: ActiveTool,
        space: ManipulationSpace,
    ) -> Result<OperationReply, OperationError> {
        let slot = match tool {
            ActiveTool::Rotate => &mut self.manipulation.rotate_space,
            ActiveTool::Scale => &mut self.manipulation.scale_space,
            ActiveTool::Select | ActiveTool::Move => {
                return Err(OperationError::InvalidInput(format!(
                    "{} has no Axis Orientation; use rotate or scale",
                    tool_name(tool)
                )));
            }
        };
        let changed = *slot != space;
        *slot = space;
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }
}

impl Editor {
    pub(in crate::app) fn set_tool_handle(&mut self, handle: ToolHandle) -> OperationReply {
        let changed = match handle {
            ToolHandle::RotateAxis(axis) => {
                std::mem::replace(&mut self.manipulation.rotate_axis, axis) != axis
            }
            ToolHandle::ScaleHandle(handle) => {
                std::mem::replace(&mut self.manipulation.scale_handle, handle) != handle
            }
        };
        if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        }
    }
}

impl Editor {
    pub(in crate::app) fn set_manipulator_size(&mut self, size: ManipulatorSize) -> OperationReply {
        if std::mem::replace(&mut self.manipulation.manipulator_size, size) == size {
            OperationReply::Unchanged
        } else {
            OperationReply::Changed
        }
    }
}

pub(in crate::app) fn kind_name(kind: ManipulationKindState) -> &'static str {
    match kind {
        ManipulationKindState::Move { .. } => "move",
        ManipulationKindState::Rotate { .. } => "rotate",
        ManipulationKindState::Scale { .. } => "scale",
    }
}

pub(in crate::app) fn manipulation_value_id() -> egui::Id {
    egui::Id::new("manipulation_value")
}

pub(in crate::app) fn tool_name(tool: ActiveTool) -> &'static str {
    match tool {
        ActiveTool::Select => "Select",
        ActiveTool::Move => "Move",
        ActiveTool::Rotate => "Rotate",
        ActiveTool::Scale => "Scale",
    }
}

pub(in crate::app) fn is_manipulation_tool(tool: ActiveTool) -> bool {
    matches!(
        tool,
        ActiveTool::Move | ActiveTool::Rotate | ActiveTool::Scale
    )
}

fn space_name(space: ManipulationSpace) -> &'static str {
    match space {
        ManipulationSpace::Object => "Object",
        ManipulationSpace::World => "World",
    }
}

fn rotate_axis_name(axis: RotateAxis) -> &'static str {
    match axis {
        RotateAxis::X => "X",
        RotateAxis::Y => "Y",
        RotateAxis::Z => "Z",
    }
}

fn scale_handle_name(handle: ScaleHandle) -> &'static str {
    match handle {
        ScaleHandle::X => "X",
        ScaleHandle::Y => "Y",
        ScaleHandle::Z => "Z",
        ScaleHandle::Uniform => "Uniform",
    }
}

// Maya's other modes for these tools. Each is drawn disabled in the selector.
const UNSUPPORTED_SPACES: [&str; 5] = ["Gimbal", "Parent", "Normal", "Custom", "Component"];
/// Maya 2026 Move Tool Axis Orientation entries in documented order; only World is implemented.
const MOVE_ORIENTATIONS: [&str; 8] = [
    "Object",
    "World",
    "Component",
    "Parent",
    "Normal",
    "Along Rotation Axis",
    "Along Live Object Axis",
    "Custom",
];

pub(in crate::app) fn mode_refusal(tool: &str, mode: SelectionMode) -> Option<String> {
    let name = match mode {
        SelectionMode::Face => "Face",
        SelectionMode::Vertex => "Vertex",
        SelectionMode::Edge => "Edge",
        SelectionMode::Object | SelectionMode::Hierarchy => return None,
    };
    Some(format!(
        "{tool} is unavailable in {name} selection; switch to Object selection"
    ))
}

pub(in crate::app) fn begin_error(error: ManipulationError) -> OperationError {
    match error {
        ManipulationError::Native(
            error @ azteccraft::native_scene::NativeError::ReadOnlyFormat,
        ) => OperationError::Scene(error),
        error => OperationError::Move(error),
    }
}

fn handle_error(tool: ActiveTool, error: HandleError) -> OperationError {
    OperationError::InvalidInput(error.describe_for(tool_name(tool)))
}

impl Editor {
    pub(in crate::app) fn check_manipulation_target(
        &self,
        tool: &str,
        target: &TransformTarget,
    ) -> Result<(), OperationError> {
        if let Some(refusal) = mode_refusal(tool, self.selection_mode) {
            return Err(OperationError::InvalidInput(refusal));
        }
        let (selected, path) = self.selection_presentation();
        if selected != Some(target.node) || path != target.path {
            return Err(OperationError::InvalidInput(format!(
                "{tool} target must be the exact selected transform instance"
            )));
        }
        Ok(())
    }

    pub(in crate::app) fn begin_editor_rotate(
        &mut self,
        target: &TransformTarget,
        axis: RotateAxis,
        space: ManipulationSpace,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        self.check_manipulation_target("Rotate", target)?;
        let id = self
            .history
            .begin_rotate(
                target,
                axis,
                space.rotate(),
                self.scene_token,
                provenance,
                VIEW_BUDGET,
            )
            .map_err(begin_error)?;
        self.manipulation.refusal = None;
        self.sync_fields();
        Ok(OperationReply::ManipulationStarted {
            manipulation_id: id.to_decimal(),
        })
    }

    pub(in crate::app) fn begin_editor_scale(
        &mut self,
        target: &TransformTarget,
        handle: ScaleHandle,
        space: ManipulationSpace,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        self.check_manipulation_target("Scale", target)?;
        let id = self
            .history
            .begin_scale(
                target,
                handle,
                space.scale(),
                self.scene_token,
                provenance,
                VIEW_BUDGET,
            )
            .map_err(begin_error)?;
        self.manipulation.refusal = None;
        self.sync_fields();
        Ok(OperationReply::ManipulationStarted {
            manipulation_id: id.to_decimal(),
        })
    }

    pub(in crate::app) fn update_editor_manipulation(
        &mut self,
        id: ManipulationId,
        value: ManipulationValue,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        let (number, parameter) = match value {
            ManipulationValue::Degrees(value) => (value, "angle"),
            ManipulationValue::Angle(value) => (value, "angle"),
            ManipulationValue::Factor(value) => (value, "factor"),
        };
        if let Some(active) = self.history.active_manipulation()
            && active.id == id
            && !matches!(
                (parameter, active.kind),
                ("angle", ManipulationKindState::Rotate { .. })
                    | ("factor", ManipulationKindState::Scale { .. })
            )
        {
            return Err(OperationError::KindMismatch {
                parameter,
                active: kind_name(active.kind),
            });
        }
        if !number.is_finite() {
            return Err(OperationError::InvalidInput(format!(
                "{} {parameter} must be finite",
                if parameter == "angle" {
                    "Rotate"
                } else {
                    "Scale"
                }
            )));
        }
        // A declared angle is in the scene's angular unit; Move's linear conversion never applies.
        let number = if let ManipulationValue::Angle(angle) = value {
            let degrees = angle
                * self
                    .history
                    .scene()
                    .units()
                    .angular_degrees_factor()
                    .map_err(OperationError::Scene)?;
            if !degrees.is_finite() {
                return Err(OperationError::InvalidInput(
                    "Rotate angle cannot be represented in degrees".into(),
                ));
            }
            degrees
        } else {
            number
        };
        let declared = match value {
            ManipulationValue::Angle(angle) => Some(angle),
            _ => None,
        };
        let updated =
            self.history
                .update_move_declared(self.scene_token, id, provenance, number, declared);
        let outcome = self.record_update(updated)?;
        self.manipulation.declared = declared.map(|angle| (id, angle));
        self.sync_fields();
        Ok(match outcome {
            EditOutcome::Changed | EditOutcome::Created(_) | EditOutcome::SessionMaterial(_) => {
                OperationReply::Changed
            }
            EditOutcome::Unchanged => OperationReply::Unchanged,
        })
    }

    // Remembers a refusal by the scene, for the inspection, and clears it on the next success.
    pub(in crate::app) fn record_update(
        &mut self,
        result: Result<EditOutcome, ManipulationError>,
    ) -> Result<EditOutcome, OperationError> {
        match result {
            Ok(outcome) => {
                self.manipulation.refusal = None;
                Ok(outcome)
            }
            Err(error) => {
                if matches!(
                    error,
                    ManipulationError::Native(_) | ManipulationError::View(_)
                ) {
                    self.manipulation.refusal = Some(error.to_string());
                }
                Err(OperationError::Move(error))
            }
        }
    }

    pub(in crate::app) fn tool_help(&self) -> String {
        let space = space_name(self.manipulation.space(self.active_tool));
        match self.active_tool {
            ActiveTool::Rotate => format!(
                "Rotate tool ({space}): drag a ring to turn about that axis | Esc cancels | \
                 view ring and free rotate are unsupported"
            ),
            ActiveTool::Scale => format!(
                "Scale tool ({space}): drag an axis box or the center box | Esc cancels | \
                 plane scale is unsupported"
            ),
            ActiveTool::Move => "Move tool: drag an axis arrow".into(),
            ActiveTool::Select => "Select tool".into(),
        }
    }

    // Handle frame of the selected transform in the chosen space.
    pub(in crate::app) fn manipulation_frame(
        &self,
        tool: ActiveTool,
    ) -> Result<(TransformTarget, HandleFrame), OperationError> {
        let name = tool_name(tool);
        if let Some(refusal) = mode_refusal(name, self.selection_mode) {
            return Err(OperationError::InvalidInput(refusal));
        }
        let target = self.selected_move_target()?;
        let frame = self
            .history
            .scene()
            .manipulator_frame(&target)
            .map_err(OperationError::Scene)?;
        let axes = match self.manipulation.space(tool) {
            ManipulationSpace::Object => frame.object_axes,
            ManipulationSpace::World => [DVec3::X, DVec3::Y, DVec3::Z],
        };
        let origin = if tool == ActiveTool::Scale {
            frame.scale_origin_cm
        } else {
            frame.origin_cm
        };
        let handle = HandleFrame::new(origin, axes).map_err(|e| handle_error(tool, e))?;
        Ok((target, handle))
    }

    // A refused hover shows the core's own reason before any press.
    fn hover_text(
        &self,
        capture: &ViewportCapture,
        pointer: Option<Vec2>,
    ) -> Option<(String, bool)> {
        let tool = self.active_tool;
        let pixel = capture.pointer_pixel(pointer?).ok()?;
        if mode_refusal("", self.selection_mode).is_some() {
            return None;
        }
        let (target, frame) = match self.manipulation_frame(tool) {
            Ok(found) => found,
            Err(OperationError::NoSelection) => return None,
            // A frame the handles refuse (for example a mirrored one) is named for as long as the
            // pointer is over the viewport.
            Err(error) => return Some((format!("{} unavailable: {error}", tool_name(tool)), true)),
        };
        let space = self.manipulation.space(tool);
        let scene = self.history.scene();
        Some(match self.pick_handle(tool, capture, &frame, pixel)? {
            ToolPick::Rotate(Pick::Handle(axis)) => {
                let name = rotate_axis_name(axis);
                match scene.rotation_refusal(&target, axis, space.rotate()) {
                    Some(refusal) => (
                        format!("{} {name} Rotate: {refusal}", space_name(space)),
                        true,
                    ),
                    None => (
                        format!("{} {name} Rotate: drag to turn", space_name(space)),
                        false,
                    ),
                }
            }
            ToolPick::Scale(Pick::Handle(handle)) => {
                let name = scale_handle_name(handle);
                match scene.scale_refusal(&target, handle, space.scale()) {
                    Some(refusal) => (
                        format!("{} {name} Scale: {refusal}", space_name(space)),
                        true,
                    ),
                    None => (
                        format!("{} {name} Scale: drag to scale", space_name(space)),
                        false,
                    ),
                }
            }
            ToolPick::Rotate(Pick::Disabled(kind)) | ToolPick::Scale(Pick::Disabled(kind)) => {
                (kind.to_string(), true)
            }
        })
    }

    /// The one handle pick for hover, press and inspection; paint calls `handles.pick` on the
    /// handles it already builds. None for Select, Move, or a frame the handles refuse.
    pub(in crate::app) fn pick_handle(
        &self,
        tool: ActiveTool,
        capture: &ViewportCapture,
        frame: &HandleFrame,
        pixel: glam::DVec2,
    ) -> Option<ToolPick> {
        match tool {
            ActiveTool::Rotate => RotateHandles::new(capture, frame)
                .ok()?
                .pick(pixel)
                .map(ToolPick::Rotate),
            ActiveTool::Scale => ScaleHandles::new(capture, frame)
                .ok()?
                .pick(pixel)
                .map(ToolPick::Scale),
            ActiveTool::Select | ActiveTool::Move => None,
        }
    }

    pub(in crate::app) fn update_manipulation_hover(
        &mut self,
        capture: &ViewportCapture,
        pointer: Option<Vec2>,
    ) {
        let hover = if matches!(self.active_tool, ActiveTool::Rotate | ActiveTool::Scale)
            && self.history.active_manipulation().is_none()
        {
            self.hover_text(capture, pointer)
        } else {
            None
        };
        if std::mem::take(&mut self.manipulation.quiet) {
            self.manipulation.hover = hover;
            return;
        }
        if hover == self.manipulation.hover {
            return;
        }
        let previous = std::mem::replace(&mut self.manipulation.hover, hover.clone());
        match hover {
            Some((text, refused)) => {
                self.status = text;
                self.error = refused;
            }
            None => {
                if previous.is_some_and(|(text, _)| text == self.status) {
                    self.status = self.tool_help();
                    self.error = false;
                }
            }
        }
    }

    pub(in crate::app) fn clear_manipulation_hover(&mut self) {
        self.manipulation.hover = None;
        self.manipulation.quiet = true;
    }

    // Starts a Rotate ring or Scale handle drag at the pressed pointer. Returns whether the press
    // was consumed: a refused or unsupported handle consumes it so the click cannot select.
    pub(in crate::app) fn begin_native_manipulation(
        &mut self,
        capture: ViewportCapture,
        position: Vec2,
        released: bool,
    ) -> Result<bool, OperationError> {
        let tool = self.active_tool;
        let Ok((target, frame)) = self.manipulation_frame(tool) else {
            return Ok(false);
        };
        let pixel = capture
            .pointer_pixel(position)
            .map_err(|error| handle_error(tool, error))?;
        let space = self.manipulation.space(tool);
        let Some(picked) = self.pick_handle(tool, &capture, &frame, pixel) else {
            return Ok(false);
        };
        let (drag, operation, label) = match picked {
            ToolPick::Rotate(Pick::Disabled(kind)) | ToolPick::Scale(Pick::Disabled(kind)) => {
                return self.refuse_disabled(kind);
            }
            ToolPick::Rotate(Pick::Handle(axis)) => {
                let drag = match RotateDrag::begin(capture, &frame, axis, position) {
                    Ok(drag) => drag,
                    Err(error) => return self.refuse_drag(tool, error),
                };
                (
                    crate::app::features::move_tool::NativeDrag::Rotate(drag),
                    Operation::BeginRotate {
                        target,
                        axis,
                        space,
                    },
                    format!("{} {} Rotate", space_name(space), rotate_axis_name(axis)),
                )
            }
            ToolPick::Scale(Pick::Handle(handle)) => {
                let drag = match ScaleDrag::begin(capture, &frame, handle, position) {
                    Ok(drag) => drag,
                    Err(error) => return self.refuse_drag(tool, error),
                };
                (
                    crate::app::features::move_tool::NativeDrag::Scale(drag),
                    Operation::BeginScale {
                        target,
                        handle,
                        space,
                    },
                    match handle {
                        ScaleHandle::Uniform => "Uniform Scale".to_string(),
                        _ => format!("{} {} Scale", space_name(space), scale_handle_name(handle)),
                    },
                )
            }
        };
        self.clear_manipulation_hover();
        if let Err(error) = self.apply_operation(operation) {
            self.consume_pointer_sequence();
            return Err(error);
        }
        self.start_native_drag(drag, released)?;
        self.status = label;
        self.error = false;
        Ok(true)
    }

    // A handle that was hit but cannot start a drag (edge-on ring, grab too close to the pivot)
    // is refused in the status and consumes the press.
    fn refuse_drag(
        &mut self,
        tool: ActiveTool,
        error: HandleError,
    ) -> Result<bool, OperationError> {
        self.consume_pointer_sequence();
        self.status = error.describe_for(tool_name(tool));
        self.error = true;
        Ok(true)
    }

    fn refuse_disabled(&mut self, kind: DisabledKind) -> Result<bool, OperationError> {
        self.consume_pointer_sequence();
        self.status = kind.to_string();
        self.error = true;
        Ok(true)
    }

    pub(in crate::app) fn paint_manipulation(&self, input: &crate::viewport_ui::ViewportInput) {
        let tool = self.active_tool;
        if !matches!(tool, ActiveTool::Rotate | ActiveTool::Scale)
            || self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.terminal_error().is_some())
        {
            return;
        }
        let painter = input.painter().with_clip_rect(input.rect());
        let drawn = (|| {
            let (_, frame) = self.manipulation_frame(tool)?;
            let capture = input
                .move_capture(&self.camera, self.manipulation.manipulator_size())
                .map_err(|error| handle_error(tool, error))?;
            let pointer = input
                .move_pointer()
                .position
                .and_then(|pointer| capture.pointer_pixel(pointer).ok());
            let active = self.history.active_manipulation().map(|active| active.kind);
            match tool {
                ActiveTool::Rotate => {
                    let handles = RotateHandles::new(&capture, &frame)
                        .map_err(|error| handle_error(tool, error))?;
                    let active = match active {
                        Some(ManipulationKindState::Rotate { axis, .. }) => Some(axis),
                        _ => None,
                    };
                    let hover = pointer
                        .filter(|_| active.is_none())
                        .and_then(|pixel| handles.pick(pixel))
                        .and_then(Pick::handle);
                    paint_rotate(
                        &painter,
                        input.rect().min,
                        input.pixels_per_point(),
                        &handles,
                        hover,
                        active,
                    );
                }
                _ => {
                    let handles = ScaleHandles::new(&capture, &frame)
                        .map_err(|error| handle_error(tool, error))?;
                    let active = match active {
                        Some(ManipulationKindState::Scale { handle, .. }) => Some(handle),
                        _ => None,
                    };
                    let hover = pointer
                        .filter(|_| active.is_none())
                        .and_then(|pixel| handles.pick(pixel))
                        .and_then(Pick::handle);
                    paint_scale(
                        &painter,
                        input.rect().min,
                        input.pixels_per_point(),
                        &handles,
                        hover,
                        active,
                    );
                }
            }
            Ok::<_, OperationError>(())
        })();
        if let Err(error) = drawn {
            painter.text(
                input.rect().left_top() + egui::vec2(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                format!("{} unavailable: {error}", tool_name(tool)),
                egui::FontId::proportional(12.0),
                egui::Color32::LIGHT_GRAY,
            );
        }
    }

    // The Object/World selector, axis choice and numeric field of the active tool.
    pub(in crate::app) fn manipulation_options(&mut self, ui: &mut egui::Ui) {
        let tool = self.active_tool;
        let enabled = !self.files.is_modal()
            && !self.files.is_closing()
            && self.history.active_manipulation().is_none();
        if tool == ActiveTool::Move {
            // Move drags along World axes only (Maya's default); the other orientations are shown
            // so their absence is visible.
            ui.horizontal(|ui| {
                ui.label("Move tool");
                ui.add_enabled_ui(enabled, |ui| {
                    egui::ComboBox::from_id_salt("move_axis_orientation")
                        .selected_text("World")
                        .show_ui(ui, |ui| {
                            for name in MOVE_ORIENTATIONS {
                                if name == "World" {
                                    let _ = ui.selectable_label(true, name);
                                } else {
                                    ui.add_enabled(false, egui::Button::selectable(false, name))
                                        .on_disabled_hover_text(
                                            "unsupported: Move uses World axes only",
                                        );
                                }
                            }
                        });
                });
            });
            return;
        }
        if !matches!(tool, ActiveTool::Rotate | ActiveTool::Scale) {
            return;
        }
        let mut apply = false;
        let mut chosen = None;
        let mut chosen_handle = None;
        ui.horizontal(|ui| {
            ui.label(format!("{} tool", tool_name(tool)));
            ui.add_enabled_ui(enabled, |ui| {
                egui::ComboBox::from_id_salt("manipulation_space")
                    .selected_text(space_name(self.manipulation.space(tool)))
                    .show_ui(ui, |ui| {
                        for space in [ManipulationSpace::Object, ManipulationSpace::World] {
                            let current = self.manipulation.space(tool);
                            if ui
                                .selectable_label(current == space, space_name(space))
                                .clicked()
                            {
                                chosen = Some(space);
                            }
                        }
                        for name in UNSUPPORTED_SPACES {
                            ui.add_enabled(false, egui::Button::selectable(false, name))
                                .on_disabled_hover_text(NOT_IMPLEMENTED_YET);
                        }
                    });
                if tool == ActiveTool::Rotate {
                    egui::ComboBox::from_id_salt("manipulation_axis")
                        .selected_text(rotate_axis_name(self.manipulation.rotate_axis()))
                        .show_ui(ui, |ui| {
                            for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
                                if ui
                                    .selectable_label(
                                        self.manipulation.rotate_axis() == axis,
                                        rotate_axis_name(axis),
                                    )
                                    .clicked()
                                {
                                    chosen_handle = Some(ToolHandle::RotateAxis(axis));
                                }
                            }
                        });
                } else {
                    egui::ComboBox::from_id_salt("manipulation_axis")
                        .selected_text(scale_handle_name(self.manipulation.scale_handle()))
                        .show_ui(ui, |ui| {
                            for handle in [
                                ScaleHandle::X,
                                ScaleHandle::Y,
                                ScaleHandle::Z,
                                ScaleHandle::Uniform,
                            ] {
                                if ui
                                    .selectable_label(
                                        self.manipulation.scale_handle() == handle,
                                        scale_handle_name(handle),
                                    )
                                    .clicked()
                                {
                                    chosen_handle = Some(ToolHandle::ScaleHandle(handle));
                                }
                            }
                        });
                }
                let hint = if tool == ActiveTool::Rotate {
                    crate::app::inspect::angular_unit_name(&self.history.scene().units().angular)
                        .to_owned()
                } else {
                    "factor".to_owned()
                };
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.manipulation.numeric)
                        .id(manipulation_value_id())
                        .hint_text(hint.as_str())
                        .desired_width(80.0),
                );
                let entered =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                apply = ui.button("Apply").clicked() || entered;
            });
        });
        if let Some(space) = chosen {
            self.native_operation(Operation::SetToolSpace { tool, space });
        }
        if let Some(handle) = chosen_handle {
            self.native_operation(Operation::SetToolHandle(handle));
        }
        if apply {
            self.apply_numeric_manipulation();
        }
    }

    fn apply_numeric_manipulation(&mut self) {
        match self.run_numeric_manipulation() {
            Ok(outcome) => {
                self.status = match outcome {
                    EditOutcome::Unchanged => "No change".into(),
                    _ => format!("{} applied", tool_name(self.active_tool)),
                };
                self.error = false;
                self.manipulation.numeric.clear();
            }
            Err(error) => {
                self.set_error_status(&error);
                self.error = true;
            }
        }
    }

    // One begin, update and finish through the shared operations: a single history entry.
    fn run_numeric_manipulation(&mut self) -> Result<EditOutcome, OperationError> {
        let tool = self.active_tool;
        let number: f64 = self
            .manipulation
            .numeric
            .trim()
            .parse()
            .map_err(|_| OperationError::InvalidInput("Enter a number".into()))?;
        let target = self.selected_move_target()?;
        let space = self.manipulation.space(tool);
        let (begin, value) = match tool {
            ActiveTool::Rotate => (
                Operation::BeginRotate {
                    target,
                    axis: self.manipulation.rotate_axis(),
                    space,
                },
                ManipulationValue::Angle(number),
            ),
            ActiveTool::Scale => (
                Operation::BeginScale {
                    target,
                    handle: self.manipulation.scale_handle(),
                    space,
                },
                ManipulationValue::Factor(number),
            ),
            ActiveTool::Select | ActiveTool::Move => {
                return Err(OperationError::InvalidInput(
                    "Numeric entry needs the Rotate or Scale tool".into(),
                ));
            }
        };
        self.apply_operation(begin)?;
        let id = self
            .history
            .active_manipulation()
            .ok_or(OperationError::Move(ManipulationError::StaleManipulation))?
            .id;
        let updated = self.apply_operation(Operation::UpdateManipulation { id, value });
        let finish = if updated.is_ok() {
            ManipulationFinish::Commit
        } else {
            ManipulationFinish::Cancel
        };
        let finished = self.apply_operation(Operation::FinishMove { id, finish });
        updated?;
        match finished? {
            OperationReply::Unchanged => Ok(EditOutcome::Unchanged),
            _ => Ok(EditOutcome::Changed),
        }
    }
}

#[cfg(test)]
mod tests;
