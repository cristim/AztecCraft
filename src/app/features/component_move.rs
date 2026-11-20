//! The native begin of a component Move (#41): which set a drag moves and every check that must
//! pass before the transaction starts. Vertex selection only; Edge and Face keep the mode refusal.

use super::manipulation_tool::{begin_error, tool_name};
use crate::app::Editor;
use crate::editor_operations::{ActiveTool, ComponentKind, OperationError, OperationReply};
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

    /// Begins a native drag of the selected vertices along `axis` of the Move Axis Orientation.
    pub(in crate::app) fn begin_component_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        provenance: ManipulationProvenance,
    ) -> Result<OperationReply, OperationError> {
        if provenance == ManipulationProvenance::Remote {
            return Err(OperationError::InvalidInput(
                "Component Move is not available to remote callers yet (tracked in #41)".into(),
            ));
        }
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
mod tests;
