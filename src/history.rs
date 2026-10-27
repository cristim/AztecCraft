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
    /// Create with Maya's default names, such as `pCube1` with shape `pCubeShape1`.
    CreateDefaultNamed {
        primitive: Primitive,
        options: Option<PrimitiveOptions>,
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
    fn label(&self) -> String {
        match self {
            Self::Create { .. } | Self::CreateDefaultNamed { .. } => "Create".to_string(),
            Self::Remove(_) => "Delete".to_string(),
            Self::Rename { .. } => "Rename".to_string(),
            // execute_labeled names it from the stored before/after values.
            Self::SetTransform { .. } => "Transform".to_string(),
            Self::EditNode { .. } => "Edit".to_string(),
            Self::SetMaterialDiffuse { .. }
            | Self::CreateSessionMaterial { .. }
            | Self::AssignSessionMaterial { .. }
            | Self::SetSessionMaterialColor { .. } => "Material".to_string(),

            Self::ExtrudeFace { .. } => "Extrude".to_string(),
        }
    }
}

/// Names a transform edit by the one value group it changed. Groups compare
/// bitwise, like `NativeTransform` equality. Several changed groups, or none
/// (a scene-only difference), read "Transform". PROVISIONAL: Maya's docs name
/// no undo entries.
fn transform_label(before: &NativeTransform, after: &NativeTransform) -> &'static str {
    let differs = |a: [f64; 3], b: [f64; 3]| a.map(f64::to_bits) != b.map(f64::to_bits);
    let groups = [
        (differs(before.translation, after.translation), "Move"),
        (differs(before.rotation, after.rotation), "Rotate"),
        (differs(before.scale, after.scale), "Scale"),
    ];
    let mut changed = groups.iter().filter(|(changed, _)| *changed);
    match (changed.next(), changed.next()) {
        (Some((_, name)), None) => name,
        _ => "Transform",
    }
}

/// A saved scene and the name of the edit that separates it from its neighbour.
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    scene: NativeScene,
    label: String,
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
    pub fn undo_label(&self) -> Option<String> {
        self.undo.back().map(|entry| entry.label.clone())
    }
    /// The name of the edit Redo would reapply.
    pub fn redo_label(&self) -> Option<String> {
        self.redo.last().map(|entry| entry.label.clone())
    }

    pub fn execute(&mut self, command: Command) -> Result<EditOutcome, NativeError> {
        let label = command.label();
        self.execute_labeled(command, label)
    }

    fn execute_labeled(
        &mut self,
        command: Command,
        label: String,
    ) -> Result<EditOutcome, NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        let mut candidate = self.scene.clone();
        let transform_target = match &command {
            Command::SetTransform { id, .. } => Some(*id),
            _ => None,
        };
        let outcome = match command {
            Command::Create {
                primitive,
                options,
                name,
            } => EditOutcome::Created(match options {
                Some(options) => candidate.create_with_options(primitive, &options, &name)?,
                None => candidate.create(primitive, &name)?,
            }),
            Command::CreateDefaultNamed { primitive, options } => {
                let options = options.unwrap_or_else(|| PrimitiveOptions::default_for(primitive));
                EditOutcome::Created(candidate.create_default_named(primitive, &options)?)
            }
            Command::Remove(id) => {
                candidate.remove(id)?;
                EditOutcome::Changed
            }
            Command::Rename { id, name } => {
                candidate.rename(id, &name)?;
                EditOutcome::Changed
            }
            Command::SetTransform { id, transform, .. } => {
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
        let label = match transform_target {
            Some(id) => match (self.scene.node(id), candidate.node(id)) {
                (Ok(before), Ok(after)) => {
                    transform_label(&before.transform(), &after.transform()).to_string()
                }
                _ => label,
            },
            None => label,
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_transforms_read_transform() {
        let same = NativeTransform::default();
        assert_eq!(transform_label(&same, &same), "Transform");
    }

    #[test]
    fn one_changed_group_names_it_and_several_read_transform() {
        let base = NativeTransform::default();
        let moved = NativeTransform {
            translation: [1.0, 0.0, 0.0],
            ..base
        };
        let rotated = NativeTransform {
            rotation: [0.0, 5.0, 0.0],
            ..base
        };
        let scaled = NativeTransform {
            scale: [2.0, 1.0, 1.0],
            ..base
        };
        assert_eq!(transform_label(&base, &moved), "Move");
        assert_eq!(transform_label(&base, &rotated), "Rotate");
        assert_eq!(transform_label(&base, &scaled), "Scale");
        assert_eq!(transform_label(&moved, &rotated), "Transform");
        assert_eq!(
            transform_label(
                &base,
                &NativeTransform {
                    scale: [2.0; 3],
                    ..moved
                }
            ),
            "Transform"
        );
    }

    #[test]
    fn a_labelled_non_transform_command_keeps_its_label() {
        let mut history = History::default();
        let id = match history
            .execute(Command::Create {
                primitive: crate::scene::Primitive::Cube,
                options: None,
                name: "cube".into(),
            })
            .unwrap()
        {
            EditOutcome::Created(id) => id,
            other => panic!("unexpected outcome: {other:?}"),
        };
        let mut channels = [None; 9];
        channels[0] = Some(1.0);
        channels[4] = Some(30.0);
        history
            .execute_labeled(
                Command::EditNode {
                    id,
                    name: None,
                    channels,
                },
                "Rotate".to_string(),
            )
            .unwrap();
        assert_eq!(history.undo_label().as_deref(), Some("Rotate"));
    }
}
