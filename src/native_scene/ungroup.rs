//! Ungroup (#42): Maya's `ungroup` command. The children of each selected group move to the
//! group's parent (or the world) and the emptied group is deleted. `-absolute` (the default)
//! keeps every child's world transform by rewriting its channels, `-relative` keeps the local
//! values. Maya renames a child whose name clashes; this refuses with `NameCollision`.

use super::{GroupUnder, NativeError, NativeId, NativeScene, ReparentKeep, ReparentRefusal, Units};
use crate::scene::{NodeId, SceneError};
use std::collections::HashSet;

/// The Ungroup options this editor offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UngroupOptions {
    pub under: GroupUnder,
    pub keep: ReparentKeep,
}

impl NativeScene {
    /// Ungroups the top-most of `ids` (a listed group below another listed group is carried by
    /// its ancestor, as in Hierarchy selection mode) and returns the children that moved, in
    /// order. A target must be a plain group: a transform without shapes; a joint is refused as `Joint`.
    /// A refusal leaves the scene unchanged: the work runs on a copy that replaces the scene
    /// only once it validates.
    pub fn ungroup(
        &mut self,
        ids: &[NodeId],
        options: UngroupOptions,
    ) -> Result<Vec<NodeId>, NativeError> {
        let mut candidate = self.clone();
        let moved = candidate.ungroup_in_place(ids, options)?;
        candidate.validate()?;
        candidate.transform_paths()?;
        *self = candidate;
        Ok(moved)
    }

    fn ungroup_in_place(
        &mut self,
        ids: &[NodeId],
        options: UngroupOptions,
    ) -> Result<Vec<NodeId>, NativeError> {
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
        self.refuse_joints(ids)?;
        let groups = self.top_most_with_parents(ids)?;
        let mut moved = Vec::new();
        let mut gone = Vec::with_capacity(groups.len());
        for (index, parents) in groups {
            let group = &self.nodes[index];
            let group_transform = group.transform_id;
            let children = self.plain_group_children(group_transform)?;
            let parent = match options.under {
                GroupUnder::Parent => parents.last().copied(),
                GroupUnder::World => None,
            };
            for child in children {
                let child_index = self
                    .nodes
                    .iter()
                    .position(|node| node.transform_id == child)
                    .ok_or(NativeError::InvalidIdentity)?;
                self.move_node(child_index, parent, options.keep)?;
                moved.push(self.nodes[child_index].id);
            }
            gone.push(self.nodes[index].id);
        }
        self.remove_many(&gone)?;
        Ok(moved)
    }

    /// The transform children of `group`, or `NotAGroup` when it holds a shape or is a joint,
    /// and `Instanced` when a child has another parent too.
    fn plain_group_children(&self, group: NativeId) -> Result<Vec<NativeId>, NativeError> {
        let mut children = Vec::new();
        for edge in self.dag.iter().filter(|edge| edge.parent == Some(group)) {
            if !self
                .nodes
                .iter()
                .any(|node| node.transform_id == edge.child)
            {
                return Err(ReparentRefusal::NotAGroup.into());
            }
            if self.dag.iter().filter(|e| e.child == edge.child).count() != 1 {
                return Err(ReparentRefusal::Instanced.into());
            }
            children.push(edge.child);
        }
        Ok(children)
    }
}

#[cfg(test)]
mod tests;
