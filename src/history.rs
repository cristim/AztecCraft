use crate::{
    native_scene::{
        MaterialChoice, NativeError, NativeId, NativeScene, NativeTransform, SessionMaterialId,
        UvProjectAxis,
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
    /// Removes several nodes as one undo step; any refusal leaves the scene and history untouched.
    RemoveMany(Vec<NodeId>),
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
    ProjectUvPlanar {
        shape: NativeId,
        axis: UvProjectAxis,
    },
    DeleteUvs {
        shape: NativeId,
    },
}

impl Command {
    /// The name an Undo or Redo entry shows for this command.
    fn label(&self) -> String {
        match self {
            Self::Create { .. } | Self::CreateDefaultNamed { .. } => "Create".to_string(),
            Self::Remove(_) | Self::RemoveMany(_) => "Delete".to_string(),
            Self::Rename { .. } => "Rename".to_string(),
            // execute_labeled names it from the stored before/after values.
            Self::SetTransform { .. } => "Transform".to_string(),
            Self::EditNode { .. } => "Edit".to_string(),
            Self::SetMaterialDiffuse { .. }
            | Self::CreateSessionMaterial { .. }
            | Self::AssignSessionMaterial { .. }
            | Self::SetSessionMaterialColor { .. } => "Material".to_string(),

            Self::ExtrudeFace { .. } => "Extrude".to_string(),
            // PROVISIONAL: Maya's docs name no undo entries.
            Self::ProjectUvPlanar { .. } => "Planar Mapping".to_string(),
            Self::DeleteUvs { .. } => "Delete UVs".to_string(),
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
    /// Counts every write to `scene`, so a reader can tell that the scene changed between two
    /// reads. Replacing the whole `History` starts again at 0.
    revision: u64,
}

impl History {
    pub fn new(scene: NativeScene) -> Self {
        Self {
            scene,
            undo: VecDeque::new(),
            redo: Vec::new(),
            active_manipulation: None,
            next_manipulation_id: 0,
            revision: 0,
        }
    }

    pub fn scene(&self) -> &NativeScene {
        &self.scene
    }
    /// Changes whenever the scene is written (edit, undo, redo, manipulation preview, restore).
    /// A refused or Unchanged execute does not bump; manipulation writes always do.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// The only write access to the scene; every call counts as a change.
    fn scene_mut(&mut self) -> &mut NativeScene {
        Self::touch(&mut self.scene, &mut self.revision)
    }
    /// `scene_mut` for callers that hold a borrow of another field.
    fn touch<'a>(scene: &'a mut NativeScene, revision: &mut u64) -> &'a mut NativeScene {
        *revision = revision.wrapping_add(1);
        scene
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
            Command::RemoveMany(ids) => {
                candidate.remove_many(&ids)?;
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
            Command::ProjectUvPlanar { shape, axis } => {
                candidate.project_uv_planar(shape, axis)?;
                EditOutcome::Changed
            }
            Command::DeleteUvs { shape } => {
                candidate.delete_uvs(shape)?;
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
        let previous = std::mem::replace(self.scene_mut(), candidate);
        self.undo.push_back(Entry {
            scene: previous,
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
        self.scene_mut().restore_authoring(previous.scene);
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
        self.scene_mut().restore_authoring(next.scene);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_cube_history() -> (History, [NodeId; 3]) {
        let mut history = History::default();
        let mut ids = Vec::new();
        for name in ["a", "b", "c"] {
            match history
                .execute(Command::Create {
                    primitive: crate::scene::Primitive::Cube,
                    options: None,
                    name: name.into(),
                })
                .unwrap()
            {
                EditOutcome::Created(id) => ids.push(id),
                outcome => panic!("{outcome:?}"),
            }
        }
        (history, ids.try_into().unwrap())
    }

    #[test]
    fn the_revision_starts_at_zero_and_changes_with_every_scene_write() {
        assert_eq!(History::default().revision(), 0);
        let mut history = History::new(NativeScene::default());
        let mut last = history.revision();
        let mut changed = |history: &History| {
            let now = history.revision();
            assert_ne!(now, last);
            last = now;
        };
        let EditOutcome::Created(id) = history
            .execute(Command::Create {
                primitive: crate::scene::Primitive::Cube,
                options: None,
                name: "a".into(),
            })
            .unwrap()
        else {
            panic!("create");
        };
        changed(&history);
        history
            .execute(Command::Rename {
                id,
                name: "b".into(),
            })
            .unwrap();
        changed(&history);
        assert!(history.undo());
        changed(&history);
        assert!(history.redo());
        changed(&history);
        history.execute(Command::Remove(id)).unwrap();
        changed(&history);
    }

    #[test]
    fn an_unchanged_edit_and_a_refused_undo_leave_the_revision_alone() {
        let (mut history, ids) = three_cube_history();
        history.execute(Command::Remove(ids[2])).unwrap();
        let before = history.revision();
        let same = history.scene().node(ids[0]).unwrap().transform();
        assert_eq!(
            history
                .execute(Command::SetTransform {
                    id: ids[0],
                    transform: same
                })
                .unwrap(),
            EditOutcome::Unchanged
        );
        assert_eq!(history.revision(), before);
        assert!(history.execute(Command::Remove(ids[2])).is_err());
        assert_eq!(history.revision(), before);
        let mut empty = History::default();
        assert!(!empty.undo() && !empty.redo());
        assert_eq!(empty.revision(), 0);
    }

    #[test]
    fn remove_many_is_one_undo_step_that_redoes_and_undoes_as_a_whole() {
        let (mut history, ids) = three_cube_history();
        let before = history.scene().clone();
        history
            .execute(Command::RemoveMany(vec![ids[0], ids[2]]))
            .unwrap();
        assert_eq!(history.scene().nodes().len(), 1);
        assert_eq!(history.undo_label().as_deref(), Some("Delete"));
        assert!(history.undo());
        assert_eq!(history.scene(), &before);
        assert!(history.redo());
        assert_eq!(history.scene().nodes().len(), 1);
        assert!(history.undo() && history.undo());
        assert_eq!(history.scene().nodes().len(), 2);
    }

    #[test]
    fn remove_many_with_a_refused_member_leaves_scene_and_stacks_untouched() {
        let (scene, root, child, _) = crate::native_scene::test_fixtures::grouped_shape();
        let mut history = History::new(scene);
        let before = history.scene().clone();
        assert!(matches!(
            history.execute(Command::RemoveMany(vec![root, child])),
            Err(NativeError::UnsupportedDelete(_))
        ));
        assert_eq!(history.scene(), &before);
        assert!(!history.can_undo() && !history.can_redo());
        let (mut history, ids) = three_cube_history();
        let missing = NodeId::from_decimal("999").unwrap();
        let before = history.scene().clone();
        assert!(
            history
                .execute(Command::RemoveMany(vec![ids[0], ids[1], missing]))
                .is_err()
        );
        assert_eq!(history.scene(), &before);
        assert!(history.can_undo() && !history.can_redo());
        let mut undone = 0;
        while history.undo() {
            undone += 1;
        }
        assert_eq!(undone, 3);
    }

    #[test]
    fn remove_many_of_nothing_is_unchanged_and_adds_no_undo_entry() {
        let (mut history, _) = three_cube_history();
        assert!(matches!(
            history.execute(Command::RemoveMany(Vec::new())),
            Ok(EditOutcome::Unchanged)
        ));
        let mut undone = 0;
        while history.undo() {
            undone += 1;
        }
        assert_eq!(undone, 3);
    }

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
