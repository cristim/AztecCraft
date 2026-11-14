//! Group (#42): Maya's `group` command. The selected top-most objects become the last children
//! of a new empty transform `group<N>`. Without `-world` the group goes under the lowest common
//! ancestor of the objects' parents (the world for objects in different hierarchies, the
//! object's own parent for a single object), and `-absolute` (the default) keeps every world
//! transform by rewriting the members' channels. Instancing is not covered.

use super::{
    DagEdge, GeometryViewFacts, NativeError, NativeId, NativeScene, NativeTransform,
    NativeTransformNode, PivotReset, ReparentKeep, ReparentRefusal, Scalar, TransformExtras, Units,
    Uuid,
};
use crate::scene::{NodeId, SceneError};
use std::collections::HashSet;

/// Where the new group goes: under the lowest common parent of the objects (Maya's default) or
/// under the world (`-world`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupUnder {
    Parent,
    World,
}

/// Where the group's pivot goes: the centre of its members' bounding box (Center) or the group's
/// own origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupPivot {
    Center,
    Origin,
}

/// The Group options this editor offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupOptions {
    pub under: GroupUnder,
    pub pivot: GroupPivot,
    /// Keep every member's world transform (`-absolute`, the default) or its local channels
    /// (`-relative`, so the members shift when the new group's frame differs).
    pub preserve_position: bool,
}

impl NativeScene {
    /// Groups the top-most of `ids` (an object whose ancestor is also listed is carried along,
    /// not listed twice) under a new group and returns the group's id. The members become the
    /// group's last children in list order. A refusal leaves the scene unchanged: the work runs
    /// on a copy that replaces the scene only once it validates.
    pub fn group(&mut self, ids: &[NodeId], options: GroupOptions) -> Result<NodeId, NativeError> {
        let mut candidate = self.clone();
        let group = candidate.group_in_place(ids, options)?;
        candidate.validate()?;
        candidate.transform_paths()?;
        *self = candidate;
        Ok(group)
    }

    fn group_in_place(
        &mut self,
        ids: &[NodeId],
        options: GroupOptions,
    ) -> Result<NodeId, NativeError> {
        self.structural_edit()?;
        if self.units != Units::default() {
            return Err(NativeError::UnsupportedEdit);
        }
        if ids.is_empty() {
            return Err(ReparentRefusal::EmptySet.into());
        }
        let mut seen = HashSet::new();
        if !ids.iter().all(|id| seen.insert(*id)) {
            return Err(SceneError::InvalidIds.into());
        }
        let members = self.top_most_with_parents(ids)?;
        let common = common_prefix(members.iter().map(|(_, parents)| parents.as_slice()));
        let parent = match options.under {
            GroupUnder::Parent => common.last().copied(),
            GroupUnder::World => None,
        };
        if let Some(parent) = parent {
            let owner = self
                .nodes
                .iter()
                .find(|node| node.transform_id == parent)
                .ok_or(NativeError::InvalidIdentity)?
                .id;
            self.editable_node(owner)?;
        }
        let (group, group_transform) = self.push_group(parent)?;
        let keep = if options.preserve_position {
            ReparentKeep::World
        } else {
            ReparentKeep::Local
        };
        for (index, _) in members {
            self.move_node(index, Some(group_transform), keep)?;
        }
        if options.pivot == GroupPivot::Center {
            self.reset_pivots(&[group], PivotReset::Center)?;
        }
        Ok(group)
    }

    /// The node index and ancestor path (root first, the node itself excluded) of each of `ids`
    /// that has no ancestor in `ids`, in list order. Refuses an instanced or restricted object.
    fn top_most_with_parents(
        &self,
        ids: &[NodeId],
    ) -> Result<Vec<(usize, Vec<NativeId>)>, NativeError> {
        let targets = self.transform_paths()?;
        let mut found = Vec::with_capacity(ids.len());
        for id in ids {
            let index = self.editable_node(*id)?;
            let mut paths = targets.iter().filter(|target| target.node == *id);
            let path = match (paths.next(), paths.next()) {
                (Some(target), None) => target.path[..target.path.len() - 1].to_vec(),
                _ => return Err(ReparentRefusal::Instanced.into()),
            };
            found.push((index, path));
        }
        let listed: HashSet<NativeId> = found
            .iter()
            .map(|(index, _)| self.nodes[*index].transform_id)
            .collect();
        found.retain(|(_, parents)| !parents.iter().any(|ancestor| listed.contains(ancestor)));
        Ok(found)
    }

    /// Appends the empty transform `group<N>` as the last child of `parent`.
    fn push_group(&mut self, parent: Option<NativeId>) -> Result<(NodeId, NativeId), NativeError> {
        let name = self.free_name("group")?;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SceneError::IdsExhausted)?;
        let next_native_id = self
            .next_native_id
            .checked_add(1)
            .ok_or(SceneError::IdsExhausted)?;
        let id = NodeId::new(self.next_id);
        let transform_id = NativeId(self.next_native_id);
        self.nodes.push(NativeTransformNode {
            id,
            transform_id,
            transform_uuid: Some(Uuid::new_v4()),
            name,
            channels: NativeTransform::default().values().map(Scalar::new),
            rotate_pivot: [0.0; 3].map(Scalar::new),
            scale_pivot: [0.0; 3].map(Scalar::new),
            restrictions: Vec::new(),
            unsupported_transform: Vec::new(),
            extras: TransformExtras::default(),
            view: GeometryViewFacts::authored(),
        });
        self.dag.push(DagEdge {
            parent,
            child: transform_id,
        });
        self.next_id = next_id;
        self.next_native_id = next_native_id;
        Ok((id, transform_id))
    }
}

/// The longest common leading run of `paths`; empty for no paths.
fn common_prefix<'a>(mut paths: impl Iterator<Item = &'a [NativeId]>) -> &'a [NativeId] {
    let Some(first) = paths.next() else {
        return &[];
    };
    paths.fold(first, |common, path| {
        let shared = common.iter().zip(path).take_while(|(a, b)| a == b).count();
        &common[..shared]
    })
}

#[cfg(test)]
mod tests;
