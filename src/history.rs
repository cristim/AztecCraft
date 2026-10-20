use crate::{
    native_scene::{
        MaterialChoice, NativeError, NativeId, NativeScene, NativeTransform, SessionMaterialId,
    },
    primitive_options::PrimitiveOptions,
    scene::{NodeId, Primitive},
};
mod move_transaction;
use move_transaction::MoveTransaction;
pub use move_transaction::{
    ManipulationError, ManipulationFinish, ManipulationId, ManipulationKindState,
    ManipulationProvenance, ManipulationState,
};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub enum Command {
    Create {
        primitive: Primitive,
        options: Option<PrimitiveOptions>,
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
    SetMaterialDiffuse {
        shape: NativeId,
        value: f64,
    },
    CreateSessionMaterial {
        color: [f64; 3],
    },
    AssignSessionMaterial {
        shape: NativeId,
        material: MaterialChoice,
    },
    SetSessionMaterialColor {
        material: SessionMaterialId,
        color: [f64; 3],
    },
    ExtrudeFace {
        shape: NativeId,
        face_index: usize,
        distance: f64,
    },
}

impl Command {
    /// The name an Undo or Redo entry shows for this command.
    fn label(&self) -> &'static str {
        match self {
            Self::Create { .. } => "Create",
            Self::Remove(_) => "Delete",
            Self::Rename { .. } => "Rename",
            Self::SetTransform { .. } => "Transform",
            Self::EditNode { .. } => "Edit",
            Self::SetMaterialDiffuse { .. }
            | Self::CreateSessionMaterial { .. }
            | Self::AssignSessionMaterial { .. }
            | Self::SetSessionMaterialColor { .. } => "Material",
            Self::ExtrudeFace { .. } => "Extrude",
        }
    }
}

/// A saved scene and the name of the edit that separates it from its neighbour.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    scene: NativeScene,
    label: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Created(NodeId),
    SessionMaterial(SessionMaterialId),
    Changed,
    Unchanged,
}

/// Most undo steps kept; matches Maya's documented default Finite queue size
/// (Preferences > Undo > Queue size = 50). Older steps are dropped silently.
pub const UNDO_LIMIT: usize = 50;

#[derive(Default)]
pub struct History {
    scene: NativeScene,
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    active_manipulation: Option<MoveTransaction>,
    next_manipulation_id: u64,
}

impl History {
    pub fn new(scene: NativeScene) -> Self {
        Self {
            scene,
            undo: VecDeque::new(),
            redo: Vec::new(),
            active_manipulation: None,
            next_manipulation_id: 0,
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
    /// The name of the edit Undo would revert.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.back().map(|entry| entry.label)
    }
    /// The name of the edit Redo would reapply.
    pub fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|entry| entry.label)
    }

    pub fn execute(&mut self, command: Command) -> Result<EditOutcome, NativeError> {
        let label = command.label();
        self.execute_labeled(command, label)
    }

    fn execute_labeled(
        &mut self,
        command: Command,
        label: &'static str,
    ) -> Result<EditOutcome, NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        let mut candidate = self.scene.clone();
        let outcome = match command {
            Command::Create {
                primitive,
                options,
                name,
            } => EditOutcome::Created(match options {
                Some(options) => candidate.create_with_options(primitive, &options, &name)?,
                None => candidate.create(primitive, &name)?,
            }),
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
            Command::SetMaterialDiffuse { shape, value } => {
                candidate.set_material_diffuse(shape, value)?;
                EditOutcome::Changed
            }
            Command::CreateSessionMaterial { color } => {
                EditOutcome::SessionMaterial(candidate.create_session_material(color)?)
            }
            Command::AssignSessionMaterial { shape, material } => {
                EditOutcome::SessionMaterial(candidate.assign_session_material(shape, material)?)
            }
            Command::SetSessionMaterialColor { material, color } => {
                candidate.set_session_material_color(material, color)?;
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
        if self.undo.len() == UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.undo.push_back(Entry {
            scene: std::mem::replace(&mut self.scene, candidate),
            label,
        });
        self.redo.clear();
        Ok(outcome)
    }

    pub fn undo(&mut self) -> bool {
        if self.active_manipulation.is_some() {
            return false;
        }
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(Entry {
            scene: self.scene.clone(),
            label: previous.label,
        });
        self.scene.restore_authoring(previous.scene);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.active_manipulation.is_some() {
            return false;
        }
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push_back(Entry {
            scene: self.scene.clone(),
            label: next.label,
        });
        self.scene.restore_authoring(next.scene);
        true
    }
}
