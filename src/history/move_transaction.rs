use super::{Command, EditOutcome, History};
use crate::native_scene::{
    ChannelBaseline, ChannelEdits, MoveBeginError, MoveSpace, NativeError, NativeId, NativeScene,
    RotateAxis, RotateFrame, RotateSpace, ScaleFrame, ScaleHandle, ScaleSpace, ScaleUnsupported,
    TransformTarget, WorldAxis, WorldMoveFrame, WorldMoveUnsupported,
};
use crate::scene::{NodeId, SceneError};
use crate::view_scene::{ViewError, ViewScene};
use std::{
    collections::{HashMap, HashSet},
    fmt,
    mem::size_of,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManipulationId(u64);

impl ManipulationId {
    pub fn to_decimal(self) -> String {
        self.0.to_string()
    }

    pub fn from_decimal(value: &str) -> Result<Self, std::num::ParseIntError> {
        value.parse().map(Self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationProvenance {
    Native,
    Remote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationFinish {
    Commit,
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManipulationError {
    Native(NativeError),
    /// One selected object refused a multi-object Move; the scene is unchanged.
    Member {
        name: String,
        source: Box<NativeError>,
    },
    View(ViewError),
    Busy,
    StaleScene,
    StaleManipulation,
    WrongOwner,
    IdsExhausted,
}

impl From<NativeError> for ManipulationError {
    fn from(error: NativeError) -> Self {
        Self::Native(error)
    }
}

impl From<ViewError> for ManipulationError {
    fn from(error: ViewError) -> Self {
        Self::View(error)
    }
}

impl fmt::Display for ManipulationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Native(error) => error.fmt(f),
            Self::Member { name, source } => write!(f, "{name}: {source}"),
            Self::View(error) => error.fmt(f),
            Self::Busy => f.write_str("finish or cancel the active manipulation"),
            Self::StaleScene => f.write_str("manipulation scene token is stale"),
            Self::StaleManipulation => f.write_str("manipulation identifier is absent or stale"),
            Self::WrongOwner => f.write_str("manipulation belongs to another input source"),
            Self::IdsExhausted => f.write_str("manipulation identifiers are exhausted"),
        }
    }
}

impl std::error::Error for ManipulationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulationKindState {
    Move {
        axis: WorldAxis,
    },
    Rotate {
        axis: RotateAxis,
        space: RotateSpace,
    },
    Scale {
        handle: ScaleHandle,
        space: ScaleSpace,
    },
}

/// Read-only view of the active manipulation; `parameter` is in the kind's internal unit
/// (centimeters for Move, degrees for Rotate, a unitless factor for Scale).
pub struct ManipulationState<'a> {
    pub id: ManipulationId,
    pub target: &'a TransformTarget,
    pub provenance: ManipulationProvenance,
    pub parameter: f64,
    pub kind: ManipulationKindState,
    /// The three channels this manipulation edits, as stored at begin.
    pub baseline_channels: [f64; 3],
    /// The same three channels now (the live preview); `None` if the node cannot be read.
    pub current_channels: Option<[f64; 3]>,
    member_count: usize,
    covered_count: usize,
}

impl ManipulationState<'_> {
    /// Selected objects the Move acts on, covered members included (1 for Rotate and Scale).
    pub fn member_count(&self) -> usize {
        self.member_count
    }

    /// Members that follow a selected ancestor instead of getting their own translation.
    pub fn covered_count(&self) -> usize {
        self.covered_count
    }

    pub fn move_axis(&self) -> Option<WorldAxis> {
        match self.kind {
            ManipulationKindState::Move { axis } => Some(axis),
            ManipulationKindState::Rotate { .. } | ManipulationKindState::Scale { .. } => None,
        }
    }

    pub fn move_distance_cm(&self) -> Option<f64> {
        self.move_axis().map(|_| self.parameter)
    }
}

/// The kind of manipulation and its kind-specific state; the guards in `History` are shared.
enum ManipulationKind {
    /// Every uncovered member's frame and baseline, the lead last unless the lead is covered,
    /// in which case `covered_lead` holds the lead's own baseline for inspection.
    Move {
        axis: WorldAxis,
        space: MoveSpace,
        movers: Vec<(WorldMoveFrame, ChannelBaseline)>,
        covered_lead: Option<ChannelBaseline>,
    },
    Rotate {
        axis: RotateAxis,
        frame: RotateFrame,
        baseline: ChannelBaseline,
    },
    /// Like `Move`: every uncovered member's frame and baseline, the lead last unless covered.
    Scale {
        handle: ScaleHandle,
        space: ScaleSpace,
        movers: Vec<(ScaleFrame, ChannelBaseline)>,
        covered_lead: Option<ChannelBaseline>,
    },
}

impl ManipulationKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Move { .. } => "Move",
            Self::Rotate { .. } => "Rotate",
            Self::Scale { .. } => "Scale",
        }
    }

    fn initial_parameter(&self) -> f64 {
        match self {
            Self::Move { .. } | Self::Rotate { .. } => 0.0,
            Self::Scale { .. } => 1.0,
        }
    }

    fn state(&self) -> ManipulationKindState {
        match self {
            Self::Move { axis, .. } => ManipulationKindState::Move { axis: *axis },
            Self::Rotate { axis, frame, .. } => ManipulationKindState::Rotate {
                axis: *axis,
                space: frame.space(),
            },
            Self::Scale { handle, space, .. } => ManipulationKindState::Scale {
                handle: *handle,
                space: *space,
            },
        }
    }

    /// The baseline the inspection reports: the lead's own channels.
    fn baseline(&self) -> Option<&ChannelBaseline> {
        match self {
            Self::Move {
                movers,
                covered_lead,
                ..
            } => covered_lead
                .as_ref()
                .or_else(|| movers.last().map(|mover| &mover.1)),
            Self::Scale {
                movers,
                covered_lead,
                ..
            } => covered_lead
                .as_ref()
                .or_else(|| movers.last().map(|mover| &mover.1)),
            Self::Rotate { baseline, .. } => Some(baseline),
        }
    }

    /// Writes the preview for `parameter`; returns each written node's previous baseline and
    /// whether anything changed. For a Move every value is computed before the first write.
    fn preview(
        &self,
        scene: &mut NativeScene,
        parameter: f64,
        declared: Option<f64>,
    ) -> Result<(Vec<ChannelBaseline>, bool), NativeError> {
        let values = match self {
            Self::Move {
                axis,
                space,
                movers,
                ..
            } => {
                let values = movers
                    .iter()
                    .map(|(frame, _)| frame.translation_in(*space, *axis, parameter))
                    .collect::<Result<Vec<_>, _>>()?;
                return scene.preview_channels_many(
                    movers.iter().map(|(_, baseline)| baseline).zip(values),
                );
            }
            Self::Rotate {
                axis,
                frame,
                baseline,
            } => (
                baseline,
                frame.rotation_declared(*axis, parameter, declared)?,
            ),
            Self::Scale { handle, movers, .. } => {
                let values = movers
                    .iter()
                    .map(|(frame, _)| frame.scale(*handle, parameter))
                    .collect::<Result<Vec<_>, _>>()?;
                return scene.preview_channels_many(
                    movers.iter().map(|(_, baseline)| baseline).zip(values),
                );
            }
        };
        let (previous, changed) = scene.preview_channels(values.0, values.1)?;
        Ok((vec![previous], changed))
    }

    fn restore(&self, scene: &mut NativeScene) -> Result<(), NativeError> {
        match self {
            Self::Move { movers, .. } => {
                scene.restore_channels_many(movers.iter().map(|(_, baseline)| baseline))
            }
            Self::Scale { movers, .. } => {
                scene.restore_channels_many(movers.iter().map(|(_, baseline)| baseline))
            }
            Self::Rotate { baseline, .. } => scene.restore_channels(baseline),
        }
    }

    /// The channel edits a commit applies: one `(node, channels)` per node that changed.
    fn changes(&self, scene: &NativeScene) -> Result<ChannelEdits, NativeError> {
        match self {
            Self::Move { movers, .. } => {
                scene.channel_changes_many(movers.iter().map(|(_, baseline)| baseline))
            }
            Self::Scale { movers, .. } => {
                scene.channel_changes_many(movers.iter().map(|(_, baseline)| baseline))
            }
            Self::Rotate { baseline, .. } => {
                let channels = scene.channel_changes(baseline)?;
                Ok(if channels.iter().all(Option::is_none) {
                    Vec::new()
                } else {
                    vec![(baseline.node(), channels)]
                })
            }
        }
    }
}

/// Retained bytes charged per mover against the view budget, beside its path.
const MOVER_BYTES: usize = size_of::<ChannelBaseline>() + size_of::<WorldMoveFrame>();
const SCALE_MOVER_BYTES: usize = size_of::<ChannelBaseline>() + size_of::<ScaleFrame>();

/// What a begin builds: the kind, how many objects it acts on and the extra bytes to charge.
struct ManipulationBegin {
    kind: ManipulationKind,
    member_count: usize,
    covered_count: usize,
    member_bytes: usize,
}

impl ManipulationBegin {
    fn single(kind: ManipulationKind) -> Self {
        Self {
            kind,
            member_count: 1,
            covered_count: 0,
            member_bytes: 0,
        }
    }

    fn new(
        axis: WorldAxis,
        space: MoveSpace,
        movers: Vec<(WorldMoveFrame, ChannelBaseline)>,
        covered_lead: Option<ChannelBaseline>,
        member_count: usize,
        covered_count: usize,
        member_bytes: usize,
    ) -> Self {
        Self {
            kind: ManipulationKind::Move {
                axis,
                space,
                movers,
                covered_lead,
            },
            member_count,
            covered_count,
            member_bytes,
        }
    }
}

fn member_error(scene: &NativeScene, node: NodeId, source: NativeError) -> ManipulationError {
    let name = scene
        .node(node)
        .map_or_else(|_| String::from("?"), |node| node.name().to_string());
    ManipulationError::Member {
        name,
        source: Box::new(source),
    }
}

pub(super) struct MoveTransaction {
    id: ManipulationId,
    scene_token: Uuid,
    target: TransformTarget,
    provenance: ManipulationProvenance,
    kind: ManipulationKind,
    view_budget: usize,
    parameter: f64,
    member_count: usize,
    covered_count: usize,
}

impl MoveTransaction {
    fn matches(
        &self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
    ) -> Result<(), ManipulationError> {
        if scene_token != self.scene_token {
            return Err(ManipulationError::StaleScene);
        }
        if id != self.id {
            return Err(ManipulationError::StaleManipulation);
        }
        if provenance != self.provenance {
            return Err(ManipulationError::WrongOwner);
        }
        Ok(())
    }
}

/// The transform ids of the selected targets: a target is covered when one of these is among its
/// ancestors (P20, by node identity, not path prefix).
pub fn selected_transform_ids(targets: &[TransformTarget]) -> HashSet<NativeId> {
    targets
        .iter()
        .filter_map(|target| target.path.last().copied())
        .collect()
}

/// Whether `target` follows a selected ancestor instead of getting its own translation. The one
/// predicate begin and the handle drawing share.
pub fn is_covered(selected: &HashSet<NativeId>, target: &TransformTarget) -> bool {
    let ancestors = &target.path[..target.path.len().saturating_sub(1)];
    ancestors.iter().any(|id| selected.contains(id))
}

/// An uncovered mover whose node is also reachable through another path under a selected node
/// would be moved a second time there: its translation is shared by all its instances. Refuse the
/// whole Move. A covered member is exempt, since it gets no translation of its own.
fn refuse_alternate_path_ancestors(
    scene: &NativeScene,
    movers: &[&TransformTarget],
    selected: &HashSet<NativeId>,
    refusal: &NativeError,
) -> Result<(), ManipulationError> {
    let uncovered: HashMap<NodeId, &TransformTarget> =
        movers.iter().map(|target| (target.node, *target)).collect();
    for path in scene.transform_paths()? {
        let Some(target) = uncovered.get(&path.node) else {
            continue;
        };
        let ancestors = &path.path[..path.path.len().saturating_sub(1)];
        if path.path != target.path && ancestors.iter().any(|id| selected.contains(id)) {
            return Err(member_error(scene, target.node, refusal.clone()));
        }
    }
    Ok(())
}

/// The members of a multi-object Move or Scale that get their own edit.
struct PlannedMembers<'a> {
    /// Uncovered members in selection order: the ones that are edited.
    movers: Vec<&'a TransformTarget>,
    /// Bytes charged against the view budget for every member, covered ones included.
    member_bytes: usize,
}

/// Resolves, dedupes and classifies `targets` (the last is the lead), or gives the first refusal.
/// `shared` is the refusal for one node selected through two instances and `alternate` the one for
/// an uncovered member reachable under a selected ancestor through another instance.
fn plan_members<'a>(
    scene: &NativeScene,
    targets: &'a [TransformTarget],
    mover_bytes: usize,
    shared: NativeError,
    alternate: NativeError,
) -> Result<PlannedMembers<'a>, ManipulationError> {
    scene
        .resolve_move_targets(targets)
        .map_err(|error| match error {
            MoveBeginError::Scene(error) => ManipulationError::Native(error),
            MoveBeginError::Member { index, source } => {
                member_error(scene, targets[index].node, source)
            }
        })?;
    let mut seen: HashMap<NodeId, &TransformTarget> = HashMap::new();
    for target in targets {
        if let Some(first) = seen.insert(target.node, target) {
            // The same object twice is a malformed request; the same object through
            // two instances shares its channels and cannot be edited as two members.
            let refusal = if first.path == target.path {
                NativeError::from(SceneError::InvalidIds)
            } else {
                shared
            };
            return Err(member_error(scene, target.node, refusal));
        }
    }
    let selected = selected_transform_ids(targets);
    let movers: Vec<&TransformTarget> = targets
        .iter()
        .filter(|target| !is_covered(&selected, target))
        .collect();
    debug_assert!(
        !movers.is_empty(),
        "a selection always has an uncovered mover"
    );
    refuse_alternate_path_ancestors(scene, &movers, &selected, &alternate)?;
    let mut member_bytes = 0usize;
    for target in targets {
        member_bytes = target
            .path
            .len()
            .checked_mul(size_of::<NativeId>())
            .and_then(|bytes| bytes.checked_add(mover_bytes))
            .and_then(|bytes| member_bytes.checked_add(bytes))
            .ok_or(ViewError::ResourceLimit)?;
    }
    Ok(PlannedMembers {
        movers,
        member_bytes,
    })
}

/// Frames, baselines and counts for a Move of `targets` (the last is `lead`), or the first refusal.
fn plan_move(
    scene: &NativeScene,
    targets: &[TransformTarget],
    space: MoveSpace,
    axis: Option<WorldAxis>,
    lead: &TransformTarget,
) -> Result<ManipulationBegin, ManipulationError> {
    if let ([target], Some(axis)) = (targets, axis) {
        let (frame, baseline) = scene.begin_move_translation_in(target, space, axis)?;
        return Ok(ManipulationBegin::new(
            axis,
            space,
            vec![(frame, baseline)],
            None,
            1,
            0,
            MOVER_BYTES,
        ));
    }
    let PlannedMembers {
        movers,
        member_bytes,
    } = plan_members(
        scene,
        targets,
        MOVER_BYTES,
        NativeError::UnsupportedMove(WorldMoveUnsupported::SharedInstances),
        NativeError::UnsupportedMove(WorldMoveUnsupported::AlternatePathAncestor),
    )?;
    let frames = scene
        .world_move_frames_in(&movers, space, axis)
        .map_err(|error| match error {
            MoveBeginError::Scene(error) => ManipulationError::Native(error),
            MoveBeginError::Member { index, source } => {
                member_error(scene, movers[index].node, source)
            }
        })?;
    let covered_lead = if movers.last().is_some_and(|last| last.node == lead.node) {
        None
    } else {
        Some(scene.translation_baseline(lead.node)?)
    };
    Ok(ManipulationBegin::new(
        axis.unwrap_or(WorldAxis::X),
        space,
        frames,
        covered_lead,
        targets.len(),
        targets.len() - movers.len(),
        member_bytes,
    ))
}

/// Frames, baselines and counts for a Scale of `targets` (the last is `lead`), or the first
/// refusal. `handle` is `None` when only the frames are wanted (the availability check).
fn plan_scale(
    scene: &NativeScene,
    targets: &[TransformTarget],
    space: ScaleSpace,
    handle: Option<ScaleHandle>,
    lead: &TransformTarget,
) -> Result<ManipulationBegin, ManipulationError> {
    if let ([target], Some(handle)) = (targets, handle) {
        let (frame, baseline) = scene.begin_scale_frame(target, handle, space)?;
        return Ok(ManipulationBegin::single(ManipulationKind::Scale {
            handle,
            space,
            movers: vec![(frame, baseline)],
            covered_lead: None,
        }));
    }
    let PlannedMembers {
        movers,
        member_bytes,
    } = plan_members(
        scene,
        targets,
        SCALE_MOVER_BYTES,
        NativeError::UnsupportedScale(ScaleUnsupported::SharedInstances),
        NativeError::UnsupportedScale(ScaleUnsupported::AlternatePathAncestor),
    )?;
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(movers.len())
        .map_err(|_| NativeError::ResourceLimit)?;
    for target in &movers {
        let member = scene
            .scale_frame_of(target, space)
            .and_then(|(frame, baseline)| {
                if let Some(handle) = handle {
                    frame.require_leaves(handle)?;
                }
                Ok((frame, baseline))
            });
        frames.push(member.map_err(|error| member_error(scene, target.node, error))?);
    }
    let covered_lead = if movers.last().is_some_and(|last| last.node == lead.node) {
        None
    } else {
        Some(scene.scale_baseline(lead.node)?)
    };
    Ok(ManipulationBegin {
        kind: ManipulationKind::Scale {
            handle: handle.unwrap_or(ScaleHandle::Uniform),
            space,
            movers: frames,
            covered_lead,
        },
        member_count: targets.len(),
        covered_count: targets.len() - movers.len(),
        member_bytes,
    })
}

impl History {
    pub fn active_manipulation(&self) -> Option<ManipulationState<'_>> {
        let active = self.active_manipulation.as_ref()?;
        let baseline = active.kind.baseline()?;
        Some(ManipulationState {
            id: active.id,
            target: &active.target,
            provenance: active.provenance,
            parameter: active.parameter,
            kind: active.kind.state(),
            baseline_channels: baseline.values(),
            current_channels: self.scene.node(baseline.node()).ok().map(|node| {
                let first = baseline.first_channel();
                let values = node.transform().values();
                [values[first], values[first + 1], values[first + 2]]
            }),
            member_count: active.member_count,
            covered_count: active.covered_count,
        })
    }

    pub fn begin_move(
        &mut self,
        target: &TransformTarget,
        axis: WorldAxis,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_move_many(
            std::slice::from_ref(target),
            axis,
            scene_token,
            provenance,
            byte_budget,
        )
    }

    /// Begins one Move of every target; the last target is the lead. A target that sits under
    /// another selected target (by node identity, not path prefix) is covered: it follows its
    /// ancestor and gets no translation of its own (PROVISIONAL, P20). One node selected through
    /// two instances is refused whole (P21), as is any member that cannot be moved.
    pub fn begin_move_many(
        &mut self,
        targets: &[TransformTarget],
        axis: WorldAxis,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_move_many_in(
            targets,
            MoveSpace::World,
            axis,
            scene_token,
            provenance,
            byte_budget,
        )
    }

    /// `begin_move_many` along the axes of `space`: each member moves along its own.
    pub fn begin_move_many_in(
        &mut self,
        targets: &[TransformTarget],
        space: MoveSpace,
        axis: WorldAxis,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        let lead = targets
            .last()
            .ok_or(ManipulationError::Native(NativeError::InvalidIdentity))?;
        self.begin_manipulation(lead, scene_token, provenance, byte_budget, |scene| {
            plan_move(scene, targets, space, Some(axis), lead)
        })
    }

    /// For each of X, Y and Z, the refusal a Move of `targets` along it would give, or `None`; the
    /// plan is built once for all three axes and nothing is started or charged. `Err` is a refusal
    /// that applies to every axis.
    pub fn check_move_axes(
        &self,
        targets: &[TransformTarget],
        space: MoveSpace,
    ) -> Result<[Option<ManipulationError>; 3], ManipulationError> {
        let lead = targets
            .last()
            .ok_or(ManipulationError::Native(NativeError::InvalidIdentity))?;
        let plan = plan_move(&self.scene, targets, space, None, lead)?;
        let ManipulationKind::Move { movers, .. } = &plan.kind else {
            return Ok([None, None, None]);
        };
        Ok([WorldAxis::X, WorldAxis::Y, WorldAxis::Z].map(|axis| {
            movers.iter().find_map(|(frame, baseline)| {
                frame
                    .require_leaves_in(space, axis)
                    .err()
                    .map(|error| member_error(&self.scene, baseline.node(), error))
            })
        }))
    }

    /// Begins a rotation in object or world space; `update_move` then takes the angle in degrees.
    pub fn begin_rotate(
        &mut self,
        target: &TransformTarget,
        axis: RotateAxis,
        space: RotateSpace,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_manipulation(target, scene_token, provenance, byte_budget, |scene| {
            let (frame, baseline) = scene.begin_rotation(target, axis, space)?;
            Ok(ManipulationBegin::single(ManipulationKind::Rotate {
                axis,
                frame,
                baseline,
            }))
        })
    }

    /// Begins a scale in object or world space; `update_move` then takes the unitless factor.
    pub fn begin_scale(
        &mut self,
        target: &TransformTarget,
        handle: ScaleHandle,
        space: ScaleSpace,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        self.begin_scale_many(
            std::slice::from_ref(target),
            handle,
            space,
            scene_token,
            provenance,
            byte_budget,
        )
    }

    /// Begins one Scale of every target about each object's own pivot; the last target is the
    /// lead. Covered members and instance refusals follow the Move rules (P20, P21).
    pub fn begin_scale_many(
        &mut self,
        targets: &[TransformTarget],
        handle: ScaleHandle,
        space: ScaleSpace,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
    ) -> Result<ManipulationId, ManipulationError> {
        let lead = targets
            .last()
            .ok_or(ManipulationError::Native(NativeError::InvalidIdentity))?;
        self.begin_manipulation(lead, scene_token, provenance, byte_budget, |scene| {
            plan_scale(scene, targets, space, Some(handle), lead)
        })
    }

    /// For each of X, Y, Z and the uniform handle, the refusal a Scale of `targets` would give, or
    /// `None`; the frames are built once and nothing is started. `Err` applies to every handle.
    pub fn check_scale_handles(
        &self,
        targets: &[TransformTarget],
        space: ScaleSpace,
    ) -> Result<[Option<ManipulationError>; 4], ManipulationError> {
        let lead = targets
            .last()
            .ok_or(ManipulationError::Native(NativeError::InvalidIdentity))?;
        let plan = plan_scale(&self.scene, targets, space, None, lead)?;
        let ManipulationKind::Scale { movers, .. } = &plan.kind else {
            return Ok([None, None, None, None]);
        };
        Ok([
            ScaleHandle::X,
            ScaleHandle::Y,
            ScaleHandle::Z,
            ScaleHandle::Uniform,
        ]
        .map(|handle| {
            movers.iter().find_map(|(frame, baseline)| {
                frame
                    .require_leaves(handle)
                    .err()
                    .map(|error| member_error(&self.scene, baseline.node(), error))
            })
        }))
    }

    fn begin_manipulation(
        &mut self,
        target: &TransformTarget,
        scene_token: Uuid,
        provenance: ManipulationProvenance,
        byte_budget: usize,
        make_kind: impl FnOnce(&NativeScene) -> Result<ManipulationBegin, ManipulationError>,
    ) -> Result<ManipulationId, ManipulationError> {
        if self.active_manipulation.is_some() {
            return Err(ManipulationError::Busy);
        }
        let next_id = self
            .next_manipulation_id
            .checked_add(1)
            .ok_or(ManipulationError::IdsExhausted)?;
        let ManipulationBegin {
            kind,
            member_count,
            covered_count,
            member_bytes,
        } = make_kind(&self.scene)?;
        let mut path = Vec::new();
        path.try_reserve_exact(target.path.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        path.extend_from_slice(&target.path);
        let metadata = path
            .capacity()
            .checked_mul(size_of::<NativeId>())
            .and_then(|bytes| bytes.checked_add(size_of::<MoveTransaction>()))
            .and_then(|bytes| bytes.checked_add(member_bytes))
            .ok_or(ViewError::ResourceLimit)?;
        let view_budget = byte_budget
            .checked_sub(metadata)
            .ok_or(ViewError::ResourceLimit)?;
        ViewScene::from_native(&self.scene, view_budget)?;
        self.active_manipulation = Some(MoveTransaction {
            id: ManipulationId(next_id),
            scene_token,
            target: TransformTarget {
                node: target.node,
                path,
            },
            provenance,
            parameter: kind.initial_parameter(),
            kind,
            view_budget,
            member_count,
            covered_count,
        });
        self.next_manipulation_id = next_id;
        Ok(ManipulationId(next_id))
    }

    pub fn update_move(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        distance_cm: f64,
    ) -> Result<EditOutcome, ManipulationError> {
        self.update_active(scene_token, id, provenance, distance_cm, None)
    }

    /// Like `update_move`, for a Rotate whose angle was requested in the scene's angular unit.
    /// `degrees` drives the preview parameter; `declared` is the exact requested value that a
    /// single-channel result is stored from (see `RotateFrame::rotation`).
    pub fn update_move_declared(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        degrees: f64,
        declared: Option<f64>,
    ) -> Result<EditOutcome, ManipulationError> {
        self.update_active(scene_token, id, provenance, degrees, declared)
    }

    fn update_active(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        distance_cm: f64,
        declared: Option<f64>,
    ) -> Result<EditOutcome, ManipulationError> {
        let active = self
            .active_manipulation
            .as_mut()
            .ok_or(ManipulationError::StaleManipulation)?;
        active.matches(scene_token, id, provenance)?;
        let (previous, changed) = active.kind.preview(
            Self::touch(&mut self.scene, &mut self.revision),
            distance_cm,
            declared,
        )?;
        if let Err(error) = ViewScene::from_native(&self.scene, active.view_budget) {
            Self::touch(&mut self.scene, &mut self.revision)
                .restore_channels_many(previous.iter())?;
            return Err(error.into());
        }
        active.parameter = distance_cm;
        Ok(if changed {
            EditOutcome::Changed
        } else {
            EditOutcome::Unchanged
        })
    }

    pub fn finish_move(
        &mut self,
        scene_token: Uuid,
        id: ManipulationId,
        provenance: ManipulationProvenance,
        finish: ManipulationFinish,
    ) -> Result<EditOutcome, ManipulationError> {
        self.active_manipulation
            .as_ref()
            .ok_or(ManipulationError::StaleManipulation)?
            .matches(scene_token, id, provenance)?;
        // Clear before any fallible scene work so a failure cannot leave an unfinishable move.
        let active = self
            .active_manipulation
            .take()
            .ok_or(ManipulationError::StaleManipulation)?;
        let mut changes = active.kind.changes(&self.scene)?;
        active.kind.restore(self.scene_mut())?;
        if finish == ManipulationFinish::Cancel || changes.is_empty() {
            return Ok(if changes.is_empty() {
                EditOutcome::Unchanged
            } else {
                EditOutcome::Changed
            });
        }
        let command = if changes.len() == 1 {
            let (id, channels) = changes.remove(0);
            Command::EditNode {
                id,
                name: None,
                channels,
            }
        } else {
            Command::EditNodes(changes)
        };
        self.execute_labeled(command, active.kind.label().to_string())
            .map_err(Into::into)
    }

    pub fn cancel_active_move(&mut self) -> Result<EditOutcome, ManipulationError> {
        // Clear before any fallible scene work so a failure cannot leave an unfinishable move.
        let Some(active) = self.active_manipulation.take() else {
            return Ok(EditOutcome::Unchanged);
        };
        let changes = active.kind.changes(&self.scene)?;
        active.kind.restore(self.scene_mut())?;
        Ok(if changes.is_empty() {
            EditOutcome::Unchanged
        } else {
            EditOutcome::Changed
        })
    }
}

#[cfg(test)]
mod multi_tests;
#[cfg(test)]
mod tests;
