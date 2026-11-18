//! The component Move: a drag along one world direction that previews mesh points from a stored
//! baseline and commits as one `Command::MoveVertices` carrying the same centimetre delta.

use super::{
    Command, History, ManipulationBegin, ManipulationError, ManipulationId, ManipulationKind,
    ManipulationKindState, ManipulationProvenance,
};
use crate::native_scene::{
    ComponentMoveFrame, MoveSpace, NativeError, NativeScene, TransformTarget, VertexBaseline,
    VertexSetTarget, WorldAxis, WorldMoveUnsupported,
};
use crate::view_scene::ViewError;
use glam::DVec3;
use std::mem::size_of;
use uuid::Uuid;

/// Where a component Move goes: along an axis of a Move Axis Orientation, or along a free unit
/// world direction (an extrude arrow, for instance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveDirection {
    /// `space` is `World` or `Object`; the other orientations are refused at begin.
    Axis { axis: WorldAxis, space: MoveSpace },
    /// A world vector; refused when zero or not finite, normalised at begin.
    Direction(DVec3),
}

pub(super) struct ComponentMove {
    direction: MoveDirection,
    /// The unit world direction the drag distance runs along.
    world: DVec3,
    frame: ComponentMoveFrame,
    baseline: VertexBaseline,
}

impl ComponentMove {
    /// The mesh-local delta for a drag of `distance_cm` world centimetres, the one computation the
    /// preview and the commit share.
    fn delta_cm(&self, distance_cm: f64) -> Result<[f64; 3], NativeError> {
        self.frame.local_delta_cm(self.world * distance_cm)
    }

    pub(super) fn state(&self) -> ManipulationKindState {
        match self.direction {
            MoveDirection::Axis { axis, space } => ManipulationKindState::MoveComponents {
                axis: Some(axis),
                space: Some(space),
            },
            MoveDirection::Direction(_) => ManipulationKindState::MoveComponents {
                axis: None,
                space: None,
            },
        }
    }

    /// Writes the points for `distance_cm` from the baseline; whether that differs from the
    /// preview at `last_good_cm`.
    pub(super) fn preview(
        &self,
        scene: &mut NativeScene,
        distance_cm: f64,
        last_good_cm: f64,
    ) -> Result<bool, NativeError> {
        let delta = self.delta_cm(distance_cm)?;
        scene.preview_vertices(&self.baseline, delta)?;
        Ok(self.delta_cm(last_good_cm).ok() != Some(delta))
    }

    pub(super) fn restore(&self, scene: &mut NativeScene) -> Result<(), NativeError> {
        scene.restore_vertices(&self.baseline)
    }

    /// The commit for a drag of `distance_cm`, or `None` when the previewed points in `scene` are
    /// byte-identical to the baseline (a zero move, or a delta that rounds away in the stored
    /// precision), so no undo step or dirty scene results.
    pub(super) fn command(
        &self,
        scene: &NativeScene,
        distance_cm: f64,
    ) -> Result<Option<Command>, NativeError> {
        let delta_cm = self.delta_cm(distance_cm)?;
        if !scene.vertices_differ_from(&self.baseline)? {
            return Ok(None);
        }
        Ok(Some(Command::MoveVertices {
            shape: self.baseline.shape(),
            vertices: self.baseline.vertices().to_vec(),
            delta_cm,
        }))
    }
}

fn world_direction(
    scene: &NativeScene,
    target: &TransformTarget,
    direction: MoveDirection,
) -> Result<DVec3, NativeError> {
    match direction {
        MoveDirection::Axis {
            axis,
            space: MoveSpace::World,
        } => Ok(axis.direction()),
        MoveDirection::Axis {
            axis,
            space: MoveSpace::Object,
        } => {
            let axes = scene.component_move_object_axes(target)?;
            Ok(axes[axis.index()])
        }
        MoveDirection::Axis { .. } => Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::ComponentAxisOrientation,
        )),
        MoveDirection::Direction(vector) => {
            // Scale by the largest component first so a huge vector's length cannot overflow.
            let largest = vector.abs().max_element();
            if vector.is_finite() && largest > 0.0 {
                let unit = (vector / largest).normalize();
                if unit.is_finite() {
                    return Ok(unit);
                }
            }
            Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ComponentDirection,
            ))
        }
    }
}

impl History {
    /// Begins a drag of the vertices of `set` (a shape instance and its vertex indices) along
    /// `direction`; `update_move` then takes the distance in world centimetres. The manipulation's
    /// target is the owning transform instance. Refuses an identity that does not resolve, a
    /// source-bound scene and the other refusals of `NativeScene::capture_vertices`.
    pub fn begin_move_components(
        &mut self,
        set: &VertexSetTarget,
        direction: MoveDirection,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        let (_, owner_path) = set
            .path
            .split_last()
            .ok_or(NativeError::InvalidInstancePath)?;
        let target = TransformTarget {
            node: set.transform,
            path: owner_path.to_vec(),
        };
        self.begin_manipulation(&target, scene_token, provenance, byte_budget, |scene| {
            scene.resolve_vertex_set(set)?;
            let frame = scene.component_move_frame(&target)?;
            let world = world_direction(scene, &target, direction)?;
            let baseline = scene.capture_vertices(set.shape, &set.vertex_indices)?;
            // The baseline's points and indices and the vertex list a commit rebuilds.
            let member_bytes = set
                .vertex_indices
                .len()
                .checked_mul(2 * (size_of::<usize>() + size_of::<[f64; 3]>()))
                .ok_or(ViewError::ResourceLimit)?;
            Ok(ManipulationBegin {
                kind: ManipulationKind::MoveComponents(ComponentMove {
                    direction,
                    world,
                    frame,
                    baseline,
                }),
                member_count: 1,
                covered_count: 0,
                member_bytes,
            })
        })
    }
}
