use crate::{
    native_scene::{NativeError, NativeId, NativeScene, NativeTransform},
    scene::{NodeId, Primitive},
};
mod move_transaction;
use move_transaction::MoveTransaction;
pub use move_transaction::{MoveError, MoveFinish, MoveId, MoveProvenance, MoveState};

#[derive(Clone, Debug)]
pub enum Command {
    Create {
        primitive: Primitive,
        name: String,
    },
    Remove(NodeId),
    Rename {
        id: NodeId,
        name: String,
    },
    SetTransform {
        id: NodeId,
        transform: NativeTransform,
    },
    EditNode {
        id: NodeId,
        name: Option<String>,
        channels: [Option<f64>; 9],
    },
    ExtrudeFace {
        shape: NativeId,
        face_index: usize,
        distance: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Created(NodeId),
    Changed,
    Unchanged,
}

#[derive(Default)]
pub struct History {
    scene: NativeScene,
    undo: Vec<NativeScene>,
    redo: Vec<NativeScene>,
    active_move: Option<MoveTransaction>,
    next_move_id: u64,
    move_scene_token: Option<uuid::Uuid>,
}

impl History {
    pub fn new(scene: NativeScene) -> Self {
        Self {
            scene,
            undo: Vec::new(),
            redo: Vec::new(),
            active_move: None,
            next_move_id: 0,
            move_scene_token: None,
        }
    }

    pub fn scene(&self) -> &NativeScene {
        &self.scene
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn execute(&mut self, command: Command) -> Result<EditOutcome, NativeError> {
        if self.active_move.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        let mut candidate = self.scene.clone();
        let outcome = match command {
            Command::Create { primitive, name } => {
                EditOutcome::Created(candidate.create(primitive, &name)?)
            }
            Command::Remove(id) => {
                candidate.remove(id)?;
                EditOutcome::Changed
            }
            Command::Rename { id, name } => {
                candidate.rename(id, &name)?;
                EditOutcome::Changed
            }
            Command::SetTransform { id, transform } => {
                candidate.set_transform(id, transform)?;
                EditOutcome::Changed
            }
            Command::EditNode { id, name, channels } => {
                candidate.edit_node(id, name.as_deref(), channels)?;
                EditOutcome::Changed
            }
            Command::ExtrudeFace {
                shape,
                face_index,
                distance,
            } => {
                candidate.extrude_face(shape, face_index, distance)?;
                EditOutcome::Changed
            }
        };
        if self.scene == candidate {
            return Ok(EditOutcome::Unchanged);
        }
        self.undo
            .push(std::mem::replace(&mut self.scene, candidate));
        self.redo.clear();
        Ok(outcome)
    }

    pub fn undo(&mut self) -> bool {
        if self.active_move.is_some() {
            return false;
        }
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.scene.clone());
        self.scene.restore_authoring(previous);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.active_move.is_some() {
            return false;
        }
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.scene.clone());
        self.scene.restore_authoring(next);
        true
    }
}
