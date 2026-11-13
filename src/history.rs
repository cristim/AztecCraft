use crate::{
    native_scene::{
        ChannelMask, DEFAULT_FRAME, JointRequest, MaterialChoice, NativeError, NativeId,
        NativeScene, NativeTransform, ObjImportPlan, PivotReset, SessionMaterialId,
        TransformTarget, UvProjectAxis,
    },
    primitive_options::PrimitiveOptions,
    scene::{NodeId, Primitive},
    view_scene::{VIEW_BUDGET, ViewError, ViewScene},
};
mod move_transaction;
use move_transaction::MoveTransaction;
pub use move_transaction::{
    ManipulationError, ManipulationFinish, ManipulationId, ManipulationKindState,
    ManipulationProvenance, ManipulationState, is_covered, selected_transform_ids,
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
    /// Edits channels of several nodes as one undo step; any refusal leaves the scene and history
    /// untouched. Duplicate ids are refused.
    EditNodes(Vec<(NodeId, [Option<f64>; 9])>),
    /// Sets the rotate and scale pivots of one node in declared units. One undo step; any refusal
    /// leaves the scene and history untouched.
    SetPivots {
        id: NodeId,
        rotate_pivot: Option<[f64; 3]>,
        scale_pivot: Option<[f64; 3]>,
        preserve: bool,
    },
    /// Centres or zeroes the pivots of several nodes as one undo step; any refusal leaves the
    /// scene and history untouched. Duplicate ids are refused.
    ResetPivots {
        ids: Vec<NodeId>,
        mode: PivotReset,
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
    /// Moves vertices of one shape by `delta` (scene units, mesh axes) as one undo step.
    MoveVertices {
        shape: NativeId,
        vertices: Vec<usize>,
        delta: [f64; 3],
    },
    CreateJoint(JointRequest),
    /// Keys every node at `frame`, as one undo step, then evaluates at `current_frame`.
    SetKeys {
        nodes: Vec<NodeId>,
        frame: i32,
        channels: ChannelMask,
        current_frame: i32,
    },
    /// Removes keys at `frame`, or at every frame without one, then evaluates at `current_frame`.
    DeleteKeys {
        nodes: Vec<NodeId>,
        frame: Option<i32>,
        channels: ChannelMask,
        current_frame: i32,
    },
    ProjectUvPlanar {
        shape: NativeId,
        axis: UvProjectAxis,
    },
    DeleteUvs {
        shape: NativeId,
    },
    /// Adds parsed OBJ objects as one undo step. The candidate scene must also pass view
    /// admission, so an import can never make the viewport refuse the scene.
    ImportObjects(Box<ObjImportPlan>),
}

impl Command {
    /// The name an Undo or Redo entry shows for this command.
    fn label(&self) -> String {
        match self {
            // PROVISIONAL for a joint: Maya's docs name no undo entries.
            Self::Create { .. } | Self::CreateDefaultNamed { .. } | Self::CreateJoint(_) => {
                "Create".to_string()
            }
            Self::Remove(_) | Self::RemoveMany(_) => "Delete".to_string(),
            Self::Rename { .. } => "Rename".to_string(),
            // execute_labeled names it from the stored before/after values.
            Self::SetTransform { .. } => "Transform".to_string(),
            Self::EditNode { .. } | Self::EditNodes(_) => "Edit".to_string(),
            // PROVISIONAL (P-B6): Maya's docs name no undo entries.
            Self::SetPivots { .. } | Self::ResetPivots { .. } => "Pivot".to_string(),
            Self::SetMaterialDiffuse { .. }
            | Self::CreateSessionMaterial { .. }
            | Self::AssignSessionMaterial { .. }
            | Self::SetSessionMaterialColor { .. } => "Material".to_string(),

            Self::ExtrudeFace { .. } => "Extrude".to_string(),
            Self::MoveVertices { .. } => "Move".to_string(),
            // PROVISIONAL: Maya's docs name no undo entries.
            Self::SetKeys { .. } => "Set Key".to_string(),
            Self::DeleteKeys { .. } => "Delete Keys".to_string(),
            Self::ProjectUvPlanar { .. } => "Planar Mapping".to_string(),
            Self::DeleteUvs { .. } => "Delete UVs".to_string(),
            // PROVISIONAL: Maya's docs name no undo entries.
            Self::ImportObjects(_) => "Import".to_string(),
        }
    }
}

/// Refusal texts for an import candidate the viewport cannot display. View admission knows no
/// OBJ line number. Only the n-gon case carries the convexity and triangulate hint.
const UNDISPLAYABLE_POLYGON: &str = "the viewport cannot display a face: faces with 5 or more corners must be convex and without holes; triangulate the model and export again";
const UNDISPLAYABLE_GEOMETRY: &str =
    "the viewport cannot display the imported geometry because it is invalid";
const UNDISPLAYABLE_TRANSFORM: &str =
    "the viewport cannot display a node transform or the scene units";
const UNDISPLAYABLE_WORLD: &str =
    "the viewport cannot represent the imported coordinates in world space";
const UNDISPLAYABLE_VIEW: &str = "the viewport view could not be built for the imported scene";

/// Maps a failed view admission of an import candidate: budget and allocation failures are
/// resource limits, every shape failure is `ImportUndisplayable` with its own accurate text.
fn view_refusal(error: ViewError) -> NativeError {
    match error {
        ViewError::Native(error) => error,
        ViewError::ResourceLimit => NativeError::ImportResourceLimit,
        ViewError::UnsupportedPolygon => NativeError::ImportUndisplayable(UNDISPLAYABLE_POLYGON),
        ViewError::InvalidGeometry => NativeError::ImportUndisplayable(UNDISPLAYABLE_GEOMETRY),
        ViewError::UnsupportedTransform => {
            NativeError::ImportUndisplayable(UNDISPLAYABLE_TRANSFORM)
        }
        ViewError::NonFiniteWorld => NativeError::ImportUndisplayable(UNDISPLAYABLE_WORLD),
        ViewError::Camera(_) => NativeError::ImportUndisplayable(UNDISPLAYABLE_VIEW),
    }
}

/// Maps a failed view admission of moved vertices: the scene must stay displayable.
fn moved_view_refusal(error: ViewError) -> NativeError {
    match error {
        ViewError::Native(error) => error,
        ViewError::ResourceLimit => NativeError::ResourceLimit,
        _ => NativeError::UnsupportedView("moved vertices leave the displayable range"),
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
    /// The frame `scene`'s keyed channels were last evaluated at.
    pose_frame: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Created(NodeId),
    /// The roots an import added, in file order.
    Imported(Vec<NodeId>),
    /// A joint with its full path, proved valid by the core before the edit committed.
    CreatedJoint(TransformTarget),
    SessionMaterial(SessionMaterialId),
    Changed,
    Unchanged,
}

/// Most undo steps kept; matches Maya's documented default Finite queue size
/// (Preferences > Undo > Queue size = 50). Older steps are dropped silently.
pub const UNDO_LIMIT: usize = 50;
/// Largest Finite queue size accepted. Every entry holds a whole-scene snapshot, so the size is
/// bounded (PROVISIONAL: the docs name no maximum); Infinite is the documented unbounded choice.
pub const UNDO_LIMIT_MAX: usize = 1000;

/// Preferences > Undo > Queue: Finite with a size, or Infinite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoQueue {
    Finite(usize),
    Infinite,
}

impl UndoQueue {
    /// A Finite queue of `size` steps, 1 to [`UNDO_LIMIT_MAX`].
    pub fn finite(size: usize) -> Result<Self, NativeError> {
        if (1..=UNDO_LIMIT_MAX).contains(&size) {
            Ok(Self::Finite(size))
        } else {
            Err(NativeError::UndoState("the queue size must be 1 to 1000"))
        }
    }
}

impl Default for UndoQueue {
    fn default() -> Self {
        Self::Finite(UNDO_LIMIT)
    }
}

/// An open `undoInfo -openChunk`: the scene before its first change, if it has changed yet.
struct OpenChunk {
    name: String,
    base: Option<(NativeScene, i32)>,
}

pub struct History {
    scene: NativeScene,
    undo: VecDeque<Entry>,
    redo: Vec<Entry>,
    active_manipulation: Option<MoveTransaction>,
    next_manipulation_id: u64,
    /// Counts every write to `scene`, so a reader can tell that the scene changed between two
    /// reads. Replacing the whole `History` starts again at 0.
    revision: u64,
    /// The frame the scene's keyed channels were last evaluated at, so Undo and Redo can tell a
    /// restored snapshot that is stale for the current time from one that already is current.
    /// `apply_evaluated`, a key command (at its `current_frame`), Undo and Redo write it.
    pose_frame: i32,
    queue: UndoQueue,
    undo_enabled: bool,
    chunk: Option<OpenChunk>,
}

impl Default for History {
    fn default() -> Self {
        Self::new(NativeScene::default())
    }
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
            pose_frame: DEFAULT_FRAME,
            queue: UndoQueue::default(),
            undo_enabled: true,
            chunk: None,
        }
    }

    pub fn undo_queue(&self) -> UndoQueue {
        self.queue
    }
    pub fn undo_enabled(&self) -> bool {
        self.undo_enabled
    }
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }
    /// The name of the open undo chunk, if any.
    pub fn open_chunk_name(&self) -> Option<&str> {
        self.chunk.as_ref().map(|chunk| chunk.name.as_str())
    }

    /// Sets the queue. A smaller Finite size drops the oldest entries at once (PROVISIONAL: the
    /// docs state no trim rule). Refused while a chunk or manipulation is open.
    pub fn set_undo_queue(&mut self, queue: UndoQueue) -> Result<(), NativeError> {
        self.refuse_while_open()?;
        if let UndoQueue::Finite(size) = queue {
            UndoQueue::finite(size)?;
        }
        self.queue = queue;
        self.trim();
        Ok(())
    }

    /// `undoInfo -state` (`flush`) or `-stateWithoutFlush`. While off, edits apply and record
    /// nothing, and any edit flushes both stacks (a snapshot from before an unrecorded edit would
    /// erase it on Undo; stricter than Maya, which warns of unstable undos instead).
    pub fn set_undo_enabled(&mut self, enabled: bool, flush: bool) -> Result<(), NativeError> {
        self.refuse_while_open()?;
        self.undo_enabled = enabled;
        if flush {
            self.undo.clear();
            self.redo.clear();
        }
        Ok(())
    }

    /// `undoInfo -openChunk -chunkName`: until `close_chunk`, edits join one undo step. The empty
    /// name becomes "Chunk" (PROVISIONAL). Chunks do not nest (the docs are silent).
    pub fn open_chunk(&mut self, name: &str) -> Result<(), NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        if self.chunk.is_some() {
            return Err(NativeError::UndoState("an undo chunk is already open"));
        }
        let name = if name.trim().is_empty() {
            "Chunk"
        } else {
            name
        };
        self.chunk = Some(OpenChunk {
            name: name.to_string(),
            base: None,
        });
        Ok(())
    }

    /// `undoInfo -closeChunk`: records the chunk as one entry, or none if the scene is unchanged.
    pub fn close_chunk(&mut self) -> Result<EditOutcome, NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        let chunk = self
            .chunk
            .take()
            .ok_or(NativeError::UndoState("no undo chunk is open"))?;
        let Some((scene, pose_frame)) = chunk.base.filter(|(base, _)| *base != self.scene) else {
            return Ok(EditOutcome::Unchanged);
        };
        self.record(Entry {
            scene,
            label: chunk.name,
            pose_frame,
        });
        Ok(EditOutcome::Changed)
    }

    /// Undo and Redo first close an open chunk, so a client that never closes one cannot leave
    /// the native Undo blocked; the chunk's step is then the one Undo reverts.
    fn close_chunk_for_step(&mut self) {
        if self.active_manipulation.is_none() && self.chunk.is_some() {
            let _ = self.close_chunk();
        }
    }

    /// Carries the Undo preferences (queue and on/off) across a replaced History; they are
    /// global settings in Maya, not part of a scene.
    pub fn inherit_undo_settings(&mut self, other: &History) {
        self.queue = other.queue;
        self.undo_enabled = other.undo_enabled;
    }

    fn refuse_while_open(&self) -> Result<(), NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        if self.chunk.is_some() {
            return Err(NativeError::UndoState("close the open undo chunk first"));
        }
        Ok(())
    }

    fn trim(&mut self) {
        if let UndoQueue::Finite(size) = self.queue {
            while self.undo.len() > size {
                self.undo.pop_front();
            }
        }
    }

    fn record(&mut self, entry: Entry) {
        self.undo.push_back(entry);
        self.trim();
        self.redo.clear();
    }

    /// The frame the scene's keyed channels were last evaluated at.
    pub fn pose_frame(&self) -> i32 {
        self.pose_frame
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
    /// The step Undo would revert once it has closed an open chunk: the chunk when it changed the
    /// scene, otherwise the entry below it.
    pub fn undo_label_after_chunk(&self) -> Option<String> {
        match self.chunk_with_net_change() {
            Some(chunk) => Some(chunk.name.clone()),
            None => self.undo_label(),
        }
    }
    /// The step Redo would reapply once it has closed an open chunk (closing one with a net
    /// change clears Redo).
    pub fn redo_label_after_chunk(&self) -> Option<String> {
        if self.chunk_with_net_change().is_some() {
            None
        } else {
            self.redo_label()
        }
    }
    fn chunk_with_net_change(&self) -> Option<&OpenChunk> {
        self.chunk.as_ref().filter(|chunk| {
            chunk
                .base
                .as_ref()
                .is_some_and(|(base, _)| *base != self.scene)
        })
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
        let evaluated_at = match &command {
            Command::SetKeys { current_frame, .. } | Command::DeleteKeys { current_frame, .. } => {
                Some(*current_frame)
            }
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
            Command::EditNodes(edits) => {
                candidate.edit_nodes(&edits)?;
                EditOutcome::Changed
            }
            Command::SetPivots {
                id,
                rotate_pivot,
                scale_pivot,
                preserve,
            } => {
                candidate.set_pivots(id, rotate_pivot, scale_pivot, preserve)?;
                EditOutcome::Changed
            }
            Command::ResetPivots { ids, mode } => {
                candidate.reset_pivots(&ids, mode)?;
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
            Command::MoveVertices {
                shape,
                vertices,
                delta,
            } => {
                candidate.move_vertices(shape, &vertices, delta)?;
                ViewScene::from_native(&candidate, VIEW_BUDGET).map_err(moved_view_refusal)?;
                EditOutcome::Changed
            }
            Command::CreateJoint(request) => {
                EditOutcome::CreatedJoint(candidate.create_joint(&request)?)
            }
            Command::SetKeys {
                nodes,
                frame,
                channels,
                current_frame,
            } => {
                candidate.set_keys(&nodes, frame, channels, current_frame)?;
                EditOutcome::Changed
            }
            Command::DeleteKeys {
                nodes,
                frame,
                channels,
                current_frame,
            } => {
                candidate.delete_keys(&nodes, frame, channels, current_frame)?;
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
            Command::ImportObjects(plan) => {
                let ids = candidate.import_polygon_objects(&plan)?;
                ViewScene::from_native(&candidate, VIEW_BUDGET).map_err(view_refusal)?;
                EditOutcome::Imported(ids)
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
        let previous = std::mem::replace(self.scene_mut(), candidate);
        let pose_frame = self.pose_frame;
        if !self.undo_enabled {
            self.undo.clear();
            self.redo.clear();
        } else if let Some(chunk) = self.chunk.as_mut() {
            chunk.base.get_or_insert((previous, pose_frame));
        } else {
            self.record(Entry {
                scene: previous,
                label,
                pose_frame,
            });
        }
        if let Some(frame) = evaluated_at {
            self.pose_frame = frame;
        }
        Ok(outcome)
    }

    /// Writes every keyed channel's value at `frame` through the revision path. It takes no undo
    /// snapshot and leaves both stacks alone, because a time change is not an undo step. All
    /// writes are computed before any lands; the revision moves only on a real change.
    pub fn apply_evaluated(&mut self, frame: i32) -> Result<bool, NativeError> {
        if self.active_manipulation.is_some() {
            return Err(NativeError::BusyManipulation);
        }
        let writes = self.scene.evaluated_writes(frame)?;
        self.pose_frame = frame;
        if writes.is_empty() {
            return Ok(false);
        }
        self.scene_mut().apply_evaluated(writes);
        Ok(true)
    }

    pub fn undo(&mut self) -> bool {
        self.close_chunk_for_step();
        if self.active_manipulation.is_some() || !self.undo_enabled {
            return false;
        }
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(Entry {
            scene: self.scene.clone(),
            label: previous.label,
            pose_frame: self.pose_frame,
        });
        self.pose_frame = previous.pose_frame;
        self.scene_mut().restore_authoring(previous.scene);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.close_chunk_for_step();
        if self.active_manipulation.is_some() || !self.undo_enabled {
            return false;
        }
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push_back(Entry {
            scene: self.scene.clone(),
            label: next.label,
            pose_frame: self.pose_frame,
        });
        self.trim();
        self.pose_frame = next.pose_frame;
        self.scene_mut().restore_authoring(next.scene);
        true
    }
}

#[cfg(test)]
mod pivot_tests;
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
        let (mut scene, _, child, _) = crate::native_scene::test_fixtures::grouped_shape();
        let other = scene
            .create(crate::scene::Primitive::Cube, "other")
            .unwrap();
        crate::native_scene::test_fixtures::restrict_transform(&mut scene, other);
        let mut history = History::new(scene);
        let before = history.scene().clone();
        assert!(matches!(
            history.execute(Command::RemoveMany(vec![child, other])),
            Err(NativeError::DeleteMemberRefused { reason, .. })
                if *reason == NativeError::UnsupportedEdit
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

    fn x_edit(id: NodeId, value: f64) -> (NodeId, [Option<f64>; 9]) {
        let mut channels = [None; 9];
        channels[0] = Some(value);
        (id, channels)
    }

    #[test]
    fn edit_nodes_is_one_undo_step() {
        let (mut history, ids) = three_cube_history();
        let before = history.scene().clone();
        let undo_depth = history.undo.len();
        history
            .execute(Command::EditNodes(vec![
                x_edit(ids[0], 1.0),
                x_edit(ids[2], 3.0),
            ]))
            .unwrap();
        assert_eq!(history.undo.len(), undo_depth + 1);
        assert_eq!(history.undo_label().as_deref(), Some("Edit"));
        let after = history.scene().clone();
        assert_eq!(after.node(ids[0]).unwrap().transform().translation[0], 1.0);
        assert_eq!(after.node(ids[1]).unwrap().transform().translation[0], 0.0);
        assert_eq!(after.node(ids[2]).unwrap().transform().translation[0], 3.0);
        assert!(history.undo());
        assert_eq!(history.scene(), &before);
        assert!(history.redo());
        assert_eq!(history.scene(), &after);
        assert!(history.undo());
        assert_eq!(history.scene(), &before);
    }

    #[test]
    fn edit_nodes_failure_leaves_scene_and_history_untouched() {
        let (mut history, ids) = three_cube_history();
        let before = history.scene().clone();
        let revision = history.revision();
        let depth = history.undo.len();
        let missing = NodeId::from_decimal("999").unwrap();
        assert!(
            history
                .execute(Command::EditNodes(vec![
                    x_edit(ids[0], 1.0),
                    x_edit(missing, 2.0)
                ]))
                .is_err()
        );
        assert_eq!(history.scene(), &before);
        assert_eq!(history.revision(), revision);
        assert_eq!((history.undo.len(), history.redo.len()), (depth, 0));
    }

    #[test]
    fn edit_nodes_rejects_duplicate_ids() {
        let (mut history, ids) = three_cube_history();
        let before = history.scene().clone();
        assert!(
            history
                .execute(Command::EditNodes(vec![
                    x_edit(ids[0], 1.0),
                    x_edit(ids[0], 2.0)
                ]))
                .is_err()
        );
        assert_eq!(history.scene(), &before);
    }

    #[test]
    fn edit_nodes_is_refused_while_a_manipulation_is_active() {
        let mut history = History::new(crate::native_scene::test_fixtures::cube().0);
        let target = history.scene().transform_paths().unwrap().remove(0);
        history
            .begin_move(
                &target,
                crate::native_scene::WorldAxis::X,
                uuid::Uuid::new_v4(),
                ManipulationProvenance::Native,
                1 << 28,
            )
            .unwrap();
        assert_eq!(
            history.execute(Command::EditNodes(vec![x_edit(target.node, 1.0)])),
            Err(NativeError::BusyManipulation)
        );
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

    fn keyed_history() -> (History, NodeId) {
        let (mut history, ids) = three_cube_history();
        let mut channels = [None; 9];
        channels[0] = Some(1.0);
        history
            .execute(Command::EditNode {
                id: ids[0],
                name: None,
                channels,
            })
            .unwrap();
        history.execute(key(ids[0], 1, 1)).unwrap();
        channels[0] = Some(5.0);
        history
            .execute(Command::EditNode {
                id: ids[0],
                name: None,
                channels,
            })
            .unwrap();
        history.execute(key(ids[0], 9, 9)).unwrap();
        (history, ids[0])
    }

    fn key(id: NodeId, frame: i32, current_frame: i32) -> Command {
        Command::SetKeys {
            nodes: vec![id],
            frame,
            channels: crate::native_scene::ALL_CHANNELS,
            current_frame,
        }
    }

    fn tx(history: &History, id: NodeId) -> f64 {
        history.scene().node(id).unwrap().transform().translation[0]
    }

    #[test]
    fn apply_evaluated_keeps_both_stacks_and_moves_the_revision_only_on_a_change() {
        let (mut history, id) = keyed_history();
        assert!(history.undo());
        let (undo, redo) = (history.undo.len(), history.redo.len());
        let before = history.revision();
        assert!(history.apply_evaluated(1).unwrap());
        assert_eq!(tx(&history, id), 1.0);
        assert_ne!(history.revision(), before);
        assert_eq!((history.undo.len(), history.redo.len()), (undo, redo));
        let before = history.revision();
        assert!(!history.apply_evaluated(1).unwrap());
        assert_eq!(history.revision(), before);
    }

    #[test]
    fn a_key_command_records_the_frame_it_evaluated_at() {
        let (mut history, id) = keyed_history();
        history.apply_evaluated(1).unwrap();
        assert_eq!(history.pose_frame(), 1);
        history.execute(key(id, 5, 4)).unwrap();
        assert_eq!(history.pose_frame(), 4);
        // Undo restores the pose frame the snapshot was taken at.
        assert!(history.undo());
        assert_eq!(history.pose_frame(), 1);
        history
            .execute(Command::DeleteKeys {
                nodes: vec![id],
                frame: Some(9),
                channels: crate::native_scene::ALL_CHANNELS,
                current_frame: 7,
            })
            .unwrap();
        assert_eq!(history.pose_frame(), 7);
    }

    #[test]
    fn apply_evaluated_is_refused_during_a_manipulation() {
        let (mut history, id) = keyed_history();
        let node = history.scene().node(id).unwrap();
        let path = vec![node.transform_id()];
        let before = history.revision();
        let moved = history.scene().node(id).unwrap().transform();
        history
            .begin_move(
                &crate::native_scene::TransformTarget { node: id, path },
                crate::native_scene::WorldAxis::X,
                uuid::Uuid::new_v4(),
                ManipulationProvenance::Native,
                1024 * 1024,
            )
            .unwrap();
        assert_eq!(
            history.apply_evaluated(1),
            Err(NativeError::BusyManipulation)
        );
        assert_eq!(history.scene().node(id).unwrap().transform(), moved);
        assert_eq!(history.revision(), before);
    }

    #[test]
    fn a_key_is_one_undo_step_named_set_key_and_restores_the_keys() {
        let (mut history, id) = keyed_history();
        assert_eq!(history.undo_label().as_deref(), Some("Set Key"));
        assert_eq!(history.scene().session_key_count(), 18);
        assert!(history.undo());
        assert_eq!(history.scene().session_key_count(), 9);
        assert!(history.redo());
        assert_eq!(history.scene().session_key_count(), 18);
        assert_eq!(
            history.execute(key(id, 9, 9)).unwrap(),
            EditOutcome::Unchanged
        );
        history
            .execute(Command::DeleteKeys {
                nodes: vec![id],
                frame: None,
                channels: crate::native_scene::ALL_CHANNELS,
                current_frame: 9,
            })
            .unwrap();
        assert_eq!(history.undo_label().as_deref(), Some("Delete Keys"));
        assert_eq!(history.scene().session_key_count(), 0);
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

#[cfg(test)]
mod import_tests;
