//! The native begin of a component Move (#41): which set a drag moves and every check that must
//! pass before the transaction starts. Vertex selection only; Edge and Face keep the mode refusal.

use super::manipulation_tool::{begin_error, tool_name};
use crate::app::Editor;
use crate::editor_operations::{
    ActiveTool, ComponentKind, OperationError, OperationReply, SelectionMode,
};
use azteccraft::{
    history::{ManipulationProvenance, MoveDirection},
    manipulator_handles::HandleFrame,
    native_scene::{
        MAX_MOVED_VERTICES, ManipulatorTool, MoveSpace, TransformTarget, VertexSetTarget, WorldAxis,
    },
};
use glam::DVec3;

impl Editor {
    /// The vertex set a component Move of `target` drags: the Vertex set, owned by that instance.
    fn component_move_set(
        &self,
        target: &TransformTarget,
    ) -> Result<VertexSetTarget, OperationError> {
        let set = self.component_set(ComponentKind::Vertex);
        let owner = set.owner().ok_or_else(|| {
            OperationError::InvalidInput("Move needs at least one selected vertex".into())
        })?;
        let on_target = owner.transform == target.node
            && owner
                .path
                .split_last()
                .is_some_and(|(_, parent)| parent == target.path);
        if !on_target {
            return Err(OperationError::InvalidInput(
                "The selected vertices belong to another object".into(),
            ));
        }
        let count = set.indices().len();
        if count > MAX_MOVED_VERTICES {
            return Err(OperationError::InvalidInput(format!(
                "Move is limited to {MAX_MOVED_VERTICES} vertices; {count} are selected"
            )));
        }
        Ok(VertexSetTarget {
            transform: owner.transform,
            shape: owner.shape,
            path: owner.path.clone(),
            vertex_indices: set.indices().to_vec(),
        })
    }

    /// Every refusal a component Move of `target` meets before it starts, without changing
    /// anything: the one place begin, the handles and inspection decide availability. Returns the
    /// vertex set and the Move Axis Orientation (World or Object).
    pub(in crate::app) fn component_move_checks(
        &self,
        target: &TransformTarget,
    ) -> Result<(VertexSetTarget, MoveSpace), OperationError> {
        self.check_single_instance_target("Move", target)?;
        let set = self.component_move_set(target)?;
        let scene = self.history.scene();
        if let Some(refusal) = scene.vertex_move_refusal(set.shape, &set.vertex_indices) {
            return Err(OperationError::Scene(refusal));
        }
        let space = self.manipulation.move_space();
        if !matches!(space, MoveSpace::World | MoveSpace::Object) {
            return Err(OperationError::InvalidInput(
                "Component Move supports the World and Object axis orientations only".into(),
            ));
        }
        let frame = scene
            .manipulator_frame(target)
            .map_err(OperationError::Scene)?;
        // PROVISIONAL: a mirrored or skewed frame refuses as for Rotate and Scale; Maya's Move
        // pages do not document this case.
        let axes = match space {
            MoveSpace::Object => frame.object_axes,
            _ => [DVec3::X, DVec3::Y, DVec3::Z],
        };
        HandleFrame::new(frame.handle_origin(ManipulatorTool::Move), axes).map_err(|error| {
            OperationError::InvalidInput(error.describe_for(tool_name(ActiveTool::Move)))
        })?;
        if self.snap_to_grid {
            return Err(OperationError::InvalidInput(
                "Snap to grids is not available for component Move yet".into(),
            ));
        }
        Ok((set, space))
    }

    /// Where the Move arrows sit (the world bounding-box centre of the moved vertices, PROVISIONAL:
    /// Maya says only "centre") and their unit world directions, for a component Move of `target`.
    pub(in crate::app) fn component_move_handles(
        &self,
        target: &TransformTarget,
    ) -> Result<(DVec3, [DVec3; 3]), OperationError> {
        let (set, space) = self.component_move_checks(target)?;
        let scene = self.history.scene();
        let frame = scene
            .component_move_frame(target)
            .map_err(OperationError::Scene)?;
        let mesh = scene
            .shape(set.shape)
            .map_err(OperationError::Scene)?
            .mesh();
        let points = set
            .vertex_indices
            .iter()
            .map(|index| mesh.effective_point_cm(*index))
            .collect::<Result<Vec<_>, _>>()
            .map_err(OperationError::Scene)?;
        let origin = frame
            .bbox_center_world_cm(&points)
            .map_err(OperationError::Scene)?;
        let directions = match space {
            MoveSpace::Object => scene
                .component_move_object_axes(target)
                .map_err(OperationError::Scene)?,
            _ => [DVec3::X, DVec3::Y, DVec3::Z],
        };
        Ok((origin, directions))
    }

    /// The reason the Move tool is unavailable in the current selection mode, if it is: Vertex
    /// selection moves its vertices, so only the other component modes refuse outright.
    pub(in crate::app) fn move_mode_refusal(&self) -> Option<String> {
        if self.selection_mode == SelectionMode::Vertex {
            return None;
        }
        super::manipulation_tool::mode_refusal("Move", self.selection_mode)
    }

    /// Begins a native drag of the selected vertices along `axis` of the Move Axis Orientation.
    pub(in crate::app) fn begin_component_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        provenance: ManipulationProvenance,
        expected_generation: Option<u64>,
    ) -> Result<OperationReply, OperationError> {
        // A remote caller moves the set it inspected: the generation (checked by the caller of
        // this method, so a stale one is `StaleSelection`) is mandatory, as for several objects.
        if provenance == ManipulationProvenance::Remote && expected_generation.is_none() {
            return Err(OperationError::InvalidInput(
                "A remote component Move needs expected_selection_generation from inspect_scene"
                    .into(),
            ));
        }
        let (set, space) = self.component_move_checks(target)?;
        let id = self
            .history
            .begin_move_components(
                &set,
                MoveDirection::Axis { axis, space },
                self.scene_token,
                provenance,
                256 * 1024 * 1024,
            )
            .map_err(begin_error)?;
        self.manipulation.refusal = None;
        self.manipulation.snap = None;
        self.sync_fields();
        Ok(OperationReply::ManipulationStarted {
            manipulation_id: id.to_decimal(),
        })
    }
}

#[cfg(test)]
mod remote_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod ui_tests;
