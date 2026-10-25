use super::manipulation_tool::{
    begin_error, is_manipulation_tool, kind_name, mode_refusal, tool_name,
};
use crate::app::Editor;
use crate::editor_operations::{
    ActiveTool, ManipulationValue, MoveDistance, Operation, OperationError, OperationReply,
};
use azteccraft::{
    history::{
        EditOutcome, ManipulationError, ManipulationFinish, ManipulationId, ManipulationProvenance,
    },
    manipulator_handles::{RotateDrag, ScaleDrag},
    move_handles::{AxisDrag, HandleError, ViewportCapture, WorldHandles},
    native_scene::{TransformTarget, WorldAxis},
};
use glam::Vec2;
use uuid::Uuid;

/// The pointer drag behind a native manipulation; each kind turns a pointer position into the
/// operation that previews it.
pub(in crate::app) enum NativeDrag {
    Move(AxisDrag),
    Rotate(RotateDrag),
    Scale(ScaleDrag),
}

impl NativeDrag {
    fn tool(&self) -> ActiveTool {
        match self {
            Self::Move(_) => ActiveTool::Move,
            Self::Rotate(_) => ActiveTool::Rotate,
            Self::Scale(_) => ActiveTool::Scale,
        }
    }

    fn same_capture(&self, capture: &ViewportCapture) -> bool {
        match self {
            Self::Move(drag) => drag.same_capture(capture),
            Self::Rotate(drag) => drag.same_capture(capture),
            Self::Scale(drag) => drag.same_capture(capture),
        }
    }

    fn update(
        &mut self,
        id: ManipulationId,
        capture: &ViewportCapture,
        pointer: Vec2,
    ) -> Result<Operation, HandleError> {
        Ok(match self {
            Self::Move(drag) => Operation::UpdateMove {
                id,
                distance: MoveDistance::Centimeters(drag.distance_cm(capture, pointer)?),
            },
            Self::Rotate(drag) => Operation::UpdateManipulation {
                id,
                value: ManipulationValue::Degrees(drag.angle_degrees(capture, pointer)?),
            },
            Self::Scale(drag) => Operation::UpdateManipulation {
                id,
                value: ManipulationValue::Factor(drag.factor(capture, pointer)?),
            },
        })
    }
}

pub(in crate::app) struct NativeMove {
    id: ManipulationId,
    scene_token: Uuid,
    drag: NativeDrag,
}

// Both fields are read only by the cfg-gated inspect_move below.
#[cfg_attr(not(any(test, feature = "mcp")), allow(dead_code))]
pub(in crate::app) struct MoveViewportFeedback {
    capture: ViewportCapture,
    pointer: Option<Vec2>,
}

fn axis_name(axis: WorldAxis) -> &'static str {
    match axis {
        WorldAxis::X => "x",
        WorldAxis::Y => "y",
        WorldAxis::Z => "z",
    }
}

fn edit_reply(outcome: EditOutcome) -> OperationReply {
    match outcome {
        EditOutcome::Changed | EditOutcome::Created(_) | EditOutcome::SessionMaterial(_) => {
            OperationReply::Changed
        }
        EditOutcome::Unchanged => OperationReply::Unchanged,
    }
}

impl Editor {
    pub(in crate::app) fn selected_move_target(&self) -> Result<TransformTarget, OperationError> {
        let (node, path) = self.single_target()?;
        Ok(TransformTarget {
            node,
            path: path.to_vec(),
        })
    }

    // Returns whether a Move was actually cancelled. Only then is the current pointer sequence
    // consumed; an idle cancel must not swallow the next frame's input.
    pub(in crate::app) fn cancel_move(&mut self) -> Result<bool, OperationError> {
        let cancelled = self.history.active_manipulation().is_some() || self.native_move.is_some();
        let result = if self.history.active_manipulation().is_some() {
            self.history
                .cancel_active_move()
                .map(|_| ())
                .map_err(OperationError::Move)
        } else {
            Ok(())
        };
        self.native_move = None;
        self.manipulation.refusal = None;
        self.manipulation.declared = None;
        if cancelled {
            self.move_consumed_frame = true;
            self.move_suppress_primary = true;
        }
        if cancelled {
            // Also after a failed cancel, so the Channel Box shows the scene's actual values.
            self.sync_fields();
        }
        result?;
        Ok(cancelled)
    }

    pub(in crate::app) fn consume_pointer_sequence(&mut self) {
        self.move_consumed_frame = true;
        self.move_suppress_primary = true;
    }

    pub(in crate::app) fn guard_move_dispatch(
        &mut self,
        operation: &Operation,
        provenance: ManipulationProvenance,
    ) -> Result<(), OperationError> {
        if provenance == ManipulationProvenance::Native
            && matches!(
                operation,
                Operation::SetActiveTool(_)
                    | Operation::Select { .. }
                    | Operation::SetSelectionMode(_)
                    | Operation::SelectFace { .. }
                    | Operation::SelectVertex { .. }
                    | Operation::SelectEdge { .. }
                    | Operation::NewScene
                    | Operation::OpenScene { .. }
                    | Operation::RequestClose
            )
        {
            let tool_change =
                matches!(operation, Operation::SetActiveTool(tool) if *tool != self.active_tool);
            let cancelled = self.cancel_move()?;
            if tool_change && !cancelled {
                self.consume_pointer_sequence();
            }
            return Ok(());
        }
        if self.history.active_manipulation().is_none() {
            return Ok(());
        }
        if matches!(
            operation,
            Operation::UpdateMove { .. }
                | Operation::UpdateManipulation { .. }
                | Operation::FinishMove { .. }
        ) {
            return Ok(());
        }
        Err(OperationError::BusyManipulation)
    }

    pub(in crate::app) fn set_active_tool(
        &mut self,
        tool: ActiveTool,
    ) -> Result<OperationReply, OperationError> {
        let changed = self.active_tool != tool;
        self.active_tool = tool;
        self.clear_manipulation_hover();
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }

    pub(in crate::app) fn begin_editor_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        self.check_manipulation_target("Move", target)?;
        let id = self
            .history
            .begin_move(
                target,
                axis,
                self.scene_token,
                provenance,
                256 * 1024 * 1024,
            )
            .map_err(begin_error)?;
        self.manipulation.refusal = None;
        self.sync_fields();
        Ok(OperationReply::ManipulationStarted {
            manipulation_id: id.to_decimal(),
        })
    }

    // Without the Declared arm the distance match is a single-arm destructure.
    #[cfg_attr(
        not(any(test, feature = "mcp")),
        allow(clippy::infallible_destructuring_match)
    )]
    pub(in crate::app) fn update_editor_move(
        &mut self,
        id: ManipulationId,
        distance: MoveDistance,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        if let Some(active) = self.history.active_manipulation()
            && active.id == id
            && active.move_axis().is_none()
        {
            return Err(OperationError::KindMismatch {
                parameter: "distance",
                active: kind_name(active.kind),
            });
        }
        let factor = self
            .history
            .scene()
            .units()
            .linear_factor()
            .map_err(OperationError::Scene)?;
        let declared = match distance {
            MoveDistance::Centimeters(_) => None,
            #[cfg(any(test, feature = "mcp"))]
            MoveDistance::Declared(value) => Some(value),
        };
        let distance_cm = match distance {
            MoveDistance::Centimeters(value) => value,
            #[cfg(any(test, feature = "mcp"))]
            MoveDistance::Declared(value) => {
                if !value.is_finite() {
                    return Err(OperationError::InvalidInput(
                        "Move distance must be finite".into(),
                    ));
                }
                value * factor
            }
        };
        if !distance_cm.is_finite() || !(distance_cm / factor).is_finite() {
            return Err(OperationError::InvalidInput(
                "Move distance cannot be represented in the declared linear unit".into(),
            ));
        }
        let updated = self
            .history
            .update_move(self.scene_token, id, provenance, distance_cm);
        let outcome = self.record_update(updated)?;
        self.manipulation.declared = declared.map(|value| (id, value));
        self.sync_fields();
        Ok(edit_reply(outcome))
    }

    pub(in crate::app) fn finish_editor_move(
        &mut self,
        id: ManipulationId,
        finish: ManipulationFinish,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        let outcome = self
            .history
            .finish_move(self.scene_token, id, provenance, finish)
            .map_err(OperationError::Move)?;
        self.native_move = None;
        self.manipulation.refusal = None;
        self.manipulation.declared = None;
        self.sync_fields();
        Ok(edit_reply(outcome))
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn inspect_manipulation(
        &self,
    ) -> Result<crate::editor_operations::ManipulationInspection, OperationError> {
        use crate::app::inspect::{angular_unit_name, linear_unit_name};
        use crate::editor_operations::{
            ActiveManipulationInspection, KindInspection, ManipulationInspection,
            ManipulationUnits, MoveAxisInspection, MoveViewportInspection,
        };
        use azteccraft::{
            history::ManipulationKindState,
            native_scene::{RotateAxis, RotateSpace, ScaleHandle, ScaleSpace},
        };
        let units = self.history.scene().units();
        // Each kind names itself in the selection-mode refusal.
        let target_for = |tool: &str| {
            if let Some(refusal) = mode_refusal(tool, self.selection_mode) {
                Err(OperationError::InvalidInput(refusal))
            } else {
                self.selected_move_target()
            }
        };
        let target = target_for("Move");
        let rotate_target = target_for("Rotate");
        let scale_target = target_for("Scale");
        let frame = match &target {
            Err(error) => Err(error.to_string()),
            Ok(target) => self
                .history
                .scene()
                .world_move_frame(target)
                .map_err(|error| error.to_string()),
        };
        // A kind is available when any of its handles could begin; otherwise the reasons are joined.
        let availability = |kind, refusals: Result<Vec<(&str, Option<String>)>, String>| {
            let (available, unavailable_reason) = match refusals {
                Err(reason) => (false, Some(reason)),
                Ok(refusals) => {
                    let blocked: Vec<String> = refusals
                        .iter()
                        .filter_map(|(name, refusal)| {
                            refusal.as_ref().map(|reason| format!("{name}: {reason}"))
                        })
                        .collect();
                    (
                        blocked.len() < refusals.len(),
                        (!blocked.is_empty()).then(|| blocked.join("; ")),
                    )
                }
            };
            KindInspection {
                kind,
                available,
                unavailable_reason,
            }
        };
        let move_refusals = match &frame {
            Err(error) => Err(error.clone()),
            Ok(frame) => Ok([WorldAxis::X, WorldAxis::Y, WorldAxis::Z]
                .into_iter()
                .map(|axis| {
                    (
                        match axis {
                            WorldAxis::X => "X axis",
                            WorldAxis::Y => "Y axis",
                            WorldAxis::Z => "Z axis",
                        },
                        frame
                            .require_leaves(axis)
                            .err()
                            .map(|error| error.to_string()),
                    )
                })
                .collect()),
        };
        let rotate_space = self.manipulation.space(ActiveTool::Rotate).rotate();
        let rotate_refusals = match &rotate_target {
            Err(error) => Err(error.to_string()),
            Ok(target) => Ok([
                (RotateAxis::X, "X axis"),
                (RotateAxis::Y, "Y axis"),
                (RotateAxis::Z, "Z axis"),
            ]
            .into_iter()
            .map(|(axis, name)| {
                (
                    name,
                    self.history
                        .scene()
                        .rotation_refusal(target, axis, rotate_space)
                        .map(|error| error.to_string()),
                )
            })
            .collect()),
        };
        let scale_space = self.manipulation.space(ActiveTool::Scale).scale();
        let scale_refusals = match &scale_target {
            Err(error) => Err(error.to_string()),
            Ok(target) => Ok([
                (ScaleHandle::X, "X handle"),
                (ScaleHandle::Y, "Y handle"),
                (ScaleHandle::Z, "Z handle"),
                (ScaleHandle::Uniform, "uniform handle"),
            ]
            .into_iter()
            .map(|(handle, name)| {
                (
                    name,
                    self.history
                        .scene()
                        .scale_refusal(target, handle, scale_space)
                        .map(|error| error.to_string()),
                )
            })
            .collect()),
        };
        let kinds = vec![
            availability("move", move_refusals),
            availability("rotate", rotate_refusals),
            availability("scale", scale_refusals),
        ];
        let active = self
            .history
            .active_manipulation()
            .map(|active| {
                let (kind, space, axis, handle, parameter, value, unit) = match active.kind {
                    ManipulationKindState::Move { axis } => {
                        let value = match self.manipulation.declared {
                            Some((id, declared)) if id == active.id => declared,
                            _ => {
                                active.parameter
                                    / units.linear_factor().map_err(OperationError::Scene)?
                            }
                        };
                        (
                            "move",
                            "world",
                            Some(axis_name(axis)),
                            None,
                            "distance",
                            value,
                            linear_unit_name(&units.linear).to_owned(),
                        )
                    }
                    ManipulationKindState::Rotate { axis, space } => {
                        let value = match self.manipulation.declared {
                            Some((id, declared)) if id == active.id => declared,
                            _ => {
                                active.parameter
                                    / units
                                        .angular_degrees_factor()
                                        .map_err(OperationError::Scene)?
                            }
                        };
                        (
                            "rotate",
                            match space {
                                RotateSpace::Object => "object",
                                RotateSpace::World => "world",
                            },
                            Some(match axis {
                                RotateAxis::X => "x",
                                RotateAxis::Y => "y",
                                RotateAxis::Z => "z",
                            }),
                            None,
                            "angle",
                            value,
                            angular_unit_name(&units.angular).to_owned(),
                        )
                    }
                    ManipulationKindState::Scale { handle, space } => (
                        "scale",
                        match space {
                            ScaleSpace::Object => "object",
                            ScaleSpace::World => "world",
                        },
                        None,
                        Some(match handle {
                            ScaleHandle::X => "x",
                            ScaleHandle::Y => "y",
                            ScaleHandle::Z => "z",
                            ScaleHandle::Uniform => "uniform",
                        }),
                        "factor",
                        active.parameter,
                        "unitless".to_owned(),
                    ),
                };
                if !value.is_finite() {
                    return Err(OperationError::InvalidInput(
                        "Manipulation inspection overflow".into(),
                    ));
                }
                Ok(ActiveManipulationInspection {
                    manipulation_id: active.id.to_decimal(),
                    kind,
                    node_id: active.target.node.to_decimal(),
                    instance_path: active
                        .target
                        .path
                        .iter()
                        .map(|id| id.to_decimal())
                        .collect(),
                    space,
                    axis,
                    handle,
                    provenance: match active.provenance {
                        ManipulationProvenance::Native => "native",
                        ManipulationProvenance::Remote => "remote",
                    },
                    parameter,
                    value,
                    unit,
                    channels: match active.kind {
                        ManipulationKindState::Move { .. } => "translate",
                        ManipulationKindState::Rotate { .. } => "rotate",
                        ManipulationKindState::Scale { .. } => "scale",
                    },
                    baseline_channels: active.baseline_channels,
                    current_channels: active.current_channels.ok_or_else(|| {
                        OperationError::InvalidInput("Manipulated node is unreadable".into())
                    })?,
                })
            })
            .transpose()?;
        let viewport = self
            .move_viewport
            .as_ref()
            .filter(|feedback| {
                feedback.capture.same_camera(&self.camera)
                    && !self
                        .renderer
                        .as_ref()
                        .is_some_and(|renderer| renderer.terminal_error().is_some())
            })
            .and_then(|feedback| frame.as_ref().ok().map(|frame| (feedback, frame)))
            .map(|(feedback, frame)| {
                let geometry = WorldHandles::new(&feedback.capture, frame.origin_cm());
                let pointer = feedback
                    .pointer
                    .and_then(|pointer| feedback.capture.pointer_pixel(pointer).ok());
                let pointer_hit = geometry
                    .as_ref()
                    .ok()
                    .and_then(|handles| pointer.and_then(|pixel| handles.hit_test(pixel)))
                    .map(axis_name);
                let axes = match &geometry {
                    Ok(handles) => handles
                        .axes()
                        .iter()
                        .map(|handle| MoveAxisInspection {
                            axis: axis_name(handle.axis),
                            visible: handle.arrow().is_some(),
                            unavailable_reason: handle
                                .unavailable_reason()
                                .map(|error| error.to_string()),
                        })
                        .collect(),
                    Err(error) => [WorldAxis::X, WorldAxis::Y, WorldAxis::Z]
                        .into_iter()
                        .map(|axis| MoveAxisInspection {
                            axis: axis_name(axis),
                            visible: false,
                            unavailable_reason: Some(error.to_string()),
                        })
                        .collect(),
                };
                MoveViewportInspection {
                    provenance: "last_native_viewport_capture",
                    axes,
                    pointer_hit,
                    pointer_available: pointer.is_some(),
                }
            });
        Ok(ManipulationInspection {
            active_tool: self.active_tool,
            rotate_space: self.manipulation.space(ActiveTool::Rotate),
            scale_space: self.manipulation.space(ActiveTool::Scale),
            units: ManipulationUnits {
                distance: linear_unit_name(&units.linear).to_owned(),
                angle: angular_unit_name(&units.angular).to_owned(),
                factor: "unitless",
            },
            kinds,
            active,
            refusal: self.manipulation.refusal.clone(),
            viewport,
        })
    }
}

impl Editor {
    pub(in crate::app) fn missing_move_viewport(&mut self) -> Result<(), OperationError> {
        self.move_viewport = None;
        if self.native_move.is_some() {
            let name = self.active_tool_name();
            self.cancel_move()?;
            self.status = format!("{name} cancelled: viewport capture is unavailable");
            self.error = true;
        }
        Ok(())
    }

    pub(in crate::app) fn active_tool_name(&self) -> &'static str {
        self.native_move
            .as_ref()
            .map_or("Move", |native| tool_name(native.drag.tool()))
    }

    // Records the native drag of the manipulation just begun, and commits it at once when the
    // press and release arrived in one frame.
    pub(in crate::app) fn start_native_drag(
        &mut self,
        drag: NativeDrag,
        released: bool,
    ) -> Result<(), OperationError> {
        let id = self
            .history
            .active_manipulation()
            .ok_or(OperationError::Move(ManipulationError::StaleManipulation))?
            .id;
        self.native_move = Some(NativeMove {
            id,
            scene_token: self.scene_token,
            drag,
        });
        if released {
            self.apply_operation(Operation::FinishMove {
                id,
                finish: ManipulationFinish::Commit,
            })?;
        }
        Ok(())
    }

    fn preview_status(&self, tool: ActiveTool) -> String {
        let parameter = self
            .history
            .active_manipulation()
            .map_or(0.0, |active| active.parameter);
        match tool {
            ActiveTool::Rotate => format!("Rotate preview: {parameter:.2} deg"),
            ActiveTool::Scale => format!("Scale preview: factor {parameter:.3}"),
            ActiveTool::Move | ActiveTool::Select => "World-axis Move preview".into(),
        }
    }

    // A refused final sample leaves the previous preview intact, so release commits that preview.
    // A refusal by the scene mid-drag (for example a zero or negative Scale factor) is shown and
    // the drag goes on from the last valid preview; it is never clamped or skipped silently.
    fn finish_native_sample(
        &mut self,
        id: ManipulationId,
        tool: ActiveTool,
        released: bool,
        updated: Result<OperationReply, OperationError>,
    ) -> Result<(), OperationError> {
        let name = tool_name(tool);
        if !released {
            match updated {
                Ok(_) => {
                    self.status = self.preview_status(tool);
                    self.error = false;
                }
                Err(OperationError::Move(ManipulationError::Native(error))) => {
                    self.status = format!("{error}; showing the last valid preview");
                    self.error = true;
                }
                Err(error) => return Err(error),
            }
            return Ok(());
        }
        self.apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        })?;
        match updated {
            Ok(_) => {
                self.status = format!("{name} committed");
                self.error = false;
            }
            Err(error) => {
                self.status = format!("{error}; last valid preview committed");
                self.error = true;
            }
        }
        Ok(())
    }

    pub(in crate::app) fn handle_move_input(
        &mut self,
        input: &crate::viewport_ui::ViewportInput,
        frame_token: Uuid,
    ) -> Result<bool, OperationError> {
        let pointer = input.move_pointer();
        let capture = match input.move_capture(&self.camera) {
            Ok(capture) => capture,
            Err(error) => {
                self.missing_move_viewport()?;
                return Err(OperationError::InvalidInput(error.to_string()));
            }
        };
        self.move_viewport = Some(MoveViewportFeedback {
            capture: capture.clone(),
            pointer: pointer.position.filter(|point| point.is_finite()),
        });
        if self.move_suppress_primary || self.move_consumed_frame {
            if !pointer.down {
                self.move_suppress_primary = false;
            }
            return Ok(true);
        }
        if let Some(native) = &self.native_move {
            let matching = native.scene_token == self.scene_token
                && frame_token == self.scene_token
                && self.history.active_manipulation().is_some_and(|active| {
                    active.id == native.id && active.provenance == ManipulationProvenance::Native
                });
            if !matching {
                self.native_move = None;
                self.move_consumed_frame = true;
                self.move_suppress_primary = pointer.down;
                return Err(OperationError::Move(
                    azteccraft::history::ManipulationError::StaleManipulation,
                ));
            }
            let name = tool_name(native.drag.tool());
            if !native.drag.same_capture(&capture)
                || pointer.modified
                || !pointer.focused
                || pointer.gone
                || (!pointer.down && !pointer.released)
                || self.files.is_modal()
                || eframe::egui::Popup::is_any_open(input.painter().ctx())
                || self.files.is_closing()
                || self
                    .renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.terminal_error().is_some())
            {
                self.cancel_move()?;
                self.status = format!("{name} cancelled: camera, viewport or native input changed");
                self.error = false;
                return Ok(true);
            }
            let Some(position) = pointer.position.filter(|point| point.is_finite()) else {
                self.cancel_move()?;
                self.status = format!("{name} cancelled: pointer position is unavailable");
                self.error = true;
                return Ok(true);
            };
            let id = native.id;
            let tool = native.drag.tool();
            let Some(native) = self.native_move.as_mut() else {
                return Ok(true);
            };
            let sample = native.drag.update(id, &capture, position);
            match sample {
                Ok(operation) => {
                    let updated = self.apply_operation(operation);
                    self.finish_native_sample(id, tool, pointer.released, updated)?;
                }
                Err(
                    error @ (HandleError::ParallelAxis
                    | HandleError::BehindRay
                    | HandleError::InvalidSolve
                    | HandleError::Degenerate),
                ) => {
                    if pointer.released {
                        self.apply_operation(Operation::FinishMove {
                            id,
                            finish: ManipulationFinish::Commit,
                        })?;
                        self.status = format!(
                            "{}; last valid position committed",
                            error.describe_for(name)
                        );
                    } else {
                        self.status = error.describe_for(name);
                    }
                    self.error = true;
                }
                Err(error) => {
                    self.cancel_move()?;
                    self.status = error.describe_for(name);
                    self.error = true;
                }
            }
            return Ok(true);
        }
        if self.history.active_manipulation().is_some() {
            if pointer.pressed {
                self.cancel_move()?;
                self.status = "Move cancelled by native viewport input".into();
                self.error = false;
            }
            return Ok(true);
        }
        let hovering = !self.files.is_modal()
            && !self.files.is_closing()
            && frame_token == self.scene_token
            && pointer.focused
            && !pointer.gone
            && !eframe::egui::Popup::is_any_open(input.painter().ctx())
            && !self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.terminal_error().is_some());
        self.update_manipulation_hover(
            &capture,
            pointer
                .position
                .filter(|point| point.is_finite() && hovering),
        );
        if let Some(refusal) = mode_refusal(tool_name(self.active_tool), self.selection_mode) {
            if is_manipulation_tool(self.active_tool) && pointer.pressed {
                self.status = refusal;
                self.error = false;
            }
            return Ok(false);
        }
        if !is_manipulation_tool(self.active_tool)
            || !pointer.pressed
            || pointer.modified
            || !pointer.focused
            || self.files.is_modal()
            || eframe::egui::Popup::is_any_open(input.painter().ctx())
            || self.files.is_closing()
            || frame_token != self.scene_token
            || self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.terminal_error().is_some())
        {
            return Ok(false);
        }
        if !self.commit_pending() {
            self.move_consumed_frame = true;
            self.move_suppress_primary = true;
            return Err(OperationError::InvalidInput(self.status.clone()));
        }
        let Some(position) = pointer.position.filter(|point| point.is_finite()) else {
            return Ok(false);
        };
        if self.active_tool != ActiveTool::Move {
            return self.begin_native_manipulation(capture, position, pointer.released);
        }
        let Ok(target) = self.selected_move_target() else {
            return Ok(false);
        };
        let Ok(frame) = self.history.scene().world_move_frame(&target) else {
            return Ok(false);
        };
        let Ok(handles) = WorldHandles::new(&capture, frame.origin_cm()) else {
            return Ok(false);
        };
        let pixel = capture
            .pointer_pixel(position)
            .map_err(|error| OperationError::InvalidInput(error.to_string()))?;
        let Some(axis) = handles.hit_test(pixel) else {
            return Ok(false);
        };
        let drag = AxisDrag::begin(capture, frame.origin_cm(), axis, position)
            .map_err(|error| OperationError::InvalidInput(error.to_string()))?;
        self.apply_operation(Operation::BeginMove { target, axis })?;
        self.start_native_drag(NativeDrag::Move(drag), pointer.released)?;
        self.status = format!("World {} Move", axis_name(axis).to_uppercase());
        self.error = false;
        Ok(true)
    }

    pub(in crate::app) fn paint_move(&self, input: &crate::viewport_ui::ViewportInput) {
        if self.active_tool != ActiveTool::Move
            || self
                .renderer
                .as_ref()
                .is_some_and(|renderer| renderer.terminal_error().is_some())
        {
            return;
        }
        let geometry = (|| {
            if let Some(refusal) = mode_refusal("Move", self.selection_mode) {
                return Err(OperationError::InvalidInput(refusal));
            }
            let target = self.selected_move_target()?;
            let frame = self
                .history
                .scene()
                .world_move_frame(&target)
                .map_err(OperationError::Scene)?;
            let mut refusals: Vec<_> = [WorldAxis::X, WorldAxis::Y, WorldAxis::Z]
                .into_iter()
                .filter_map(|axis| frame.require_leaves(axis).err())
                .collect();
            if refusals.len() == 3 {
                return Err(OperationError::Scene(refusals.swap_remove(0)));
            }
            let capture = input
                .move_capture(&self.camera)
                .map_err(|error| OperationError::InvalidInput(error.to_string()))?;
            let handles = WorldHandles::new(&capture, frame.origin_cm())
                .map_err(|error| OperationError::InvalidInput(error.to_string()))?;
            Ok::<_, OperationError>((capture, handles))
        })();
        let painter = input.painter().with_clip_rect(input.rect());
        let scale = input.pixels_per_point();
        let (capture, handles) = match geometry {
            Ok(geometry) => geometry,
            Err(error) => {
                painter.text(
                    input.rect().left_top() + eframe::egui::vec2(12.0, 12.0),
                    eframe::egui::Align2::LEFT_TOP,
                    format!("Move unavailable: {error}"),
                    eframe::egui::FontId::proportional(12.0),
                    eframe::egui::Color32::LIGHT_GRAY,
                );
                return;
            }
        };
        let active = self
            .history
            .active_manipulation()
            .and_then(|active| active.move_axis());
        let hovered = if active.is_none() {
            input
                .move_pointer()
                .position
                .and_then(|pointer| capture.pointer_pixel(pointer).ok())
                .and_then(|pixel| handles.hit_test(pixel))
        } else {
            None
        };
        for handle in handles.axes() {
            let Some(arrow) = handle.arrow() else {
                continue;
            };
            let color = if active == Some(handle.axis) {
                eframe::egui::Color32::from_rgb(255, 225, 80)
            } else if hovered == Some(handle.axis) {
                eframe::egui::Color32::WHITE
            } else {
                match handle.axis {
                    WorldAxis::X => eframe::egui::Color32::from_rgb(235, 75, 75),
                    WorldAxis::Y => eframe::egui::Color32::from_rgb(90, 220, 105),
                    WorldAxis::Z => eframe::egui::Color32::from_rgb(85, 150, 245),
                }
            };
            let point = |pixel: glam::DVec2| {
                input.rect().min
                    + eframe::egui::vec2(pixel.x as f32 / scale, pixel.y as f32 / scale)
            };
            for segment in arrow.segments() {
                painter.line_segment(
                    [point(segment.start), point(segment.end)],
                    eframe::egui::Stroke::new(2.0 / scale, color),
                );
            }
            painter.text(
                point(arrow.label()),
                eframe::egui::Align2::CENTER_CENTER,
                axis_name(handle.axis).to_uppercase(),
                eframe::egui::FontId::proportional(11.0 / scale),
                color,
            );
        }
    }
}

#[cfg(test)]
#[path = "move_tool/tests.rs"]
pub(in crate::app) mod tests;
