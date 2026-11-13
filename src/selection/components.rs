use crate::native_scene::NativeId;
use crate::scene::NodeId;
use std::collections::HashSet;

/// The shape instance a component set belongs to. `path` ends with `shape`, as in the
/// face, vertex and edge targets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentOwner {
    pub transform: NodeId,
    pub shape: NativeId,
    pub path: Vec<NativeId>,
}

/// Ordered set of the face, vertex or edge indices of one shape instance; the last member is the
/// lead. The owner is stored once. Membership is a hash lookup, so every batch mutator costs
/// O(members + request) however many components a marquee hits. The set holds no cap of its own:
/// the owner mesh bounds the indices, which the caller checks first.
///
/// Mutators that name an owner other than the current one drop the old members first (Add and
/// Toggle) or change nothing (Remove), so a set never mixes owners. Equality compares the owner
/// and the ordered members.
#[derive(Clone, Debug, Default)]
pub struct ComponentSet {
    owner: Option<ComponentOwner>,
    indices: Vec<usize>,
    members: HashSet<usize>,
}

impl PartialEq for ComponentSet {
    fn eq(&self, other: &Self) -> bool {
        self.owner == other.owner && self.indices == other.indices
    }
}

impl Eq for ComponentSet {}

impl ComponentSet {
    pub fn owner(&self) -> Option<&ComponentOwner> {
        self.owner.as_ref()
    }

    /// Members in selection order; the last is the lead.
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    pub fn lead(&self) -> Option<usize> {
        self.indices.last().copied()
    }

    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn contains(&self, index: usize) -> bool {
        self.members.contains(&index)
    }

    /// Whether `owner` is the set's owner (an empty set has none).
    pub fn is_owned_by(&self, owner: &ComponentOwner) -> bool {
        self.owner.as_ref() == Some(owner)
    }

    pub fn clear(&mut self) {
        self.owner = None;
        self.indices.clear();
        self.members.clear();
    }

    /// Replaces the members with `indices` in order, the last being the lead; a repeated index
    /// keeps its first position. An empty request empties the set. Returns whether anything changed.
    pub fn replace_with(
        &mut self,
        owner: &ComponentOwner,
        indices: impl IntoIterator<Item = usize>,
    ) -> bool {
        let mut members = HashSet::new();
        let unique: Vec<usize> = indices
            .into_iter()
            .filter(|index| members.insert(*index))
            .collect();
        let owner = (!unique.is_empty()).then(|| owner.clone());
        if self.owner == owner && self.indices == unique {
            return false;
        }
        self.owner = owner;
        self.indices = unique;
        self.members = members;
        true
    }

    /// Appends the indices that are not members, in order and each once; members keep their
    /// place. Returns whether anything changed.
    pub fn extend_absent(
        &mut self,
        owner: &ComponentOwner,
        indices: impl IntoIterator<Item = usize>,
    ) -> bool {
        let rebased = self.rebase(owner);
        let before = self.indices.len();
        for index in indices {
            if self.members.insert(index) {
                self.indices.push(index);
            }
        }
        self.settle(rebased, before)
    }

    /// Removes the indices that are members; an owner other than the set's changes nothing.
    /// Removing the lead promotes the previous member. Returns whether anything changed.
    pub fn remove_all(
        &mut self,
        owner: &ComponentOwner,
        indices: impl IntoIterator<Item = usize>,
    ) -> bool {
        if !self.is_owned_by(owner) {
            return false;
        }
        let before = self.indices.len();
        for index in indices {
            self.members.remove(&index);
        }
        self.indices.retain(|index| self.members.contains(index));
        self.settle(false, before)
    }

    /// Removes the indices that are members and appends the others in order, each request index
    /// once; the last appended is the lead. Returns whether anything changed.
    pub fn toggle_all(
        &mut self,
        owner: &ComponentOwner,
        indices: impl IntoIterator<Item = usize>,
    ) -> bool {
        let rebased = self.rebase(owner);
        let mut seen = HashSet::new();
        let mut doomed = HashSet::new();
        let mut added = Vec::new();
        for index in indices {
            if !seen.insert(index) {
                continue;
            }
            if self.members.contains(&index) {
                doomed.insert(index);
            } else {
                added.push(index);
            }
        }
        if doomed.is_empty() && added.is_empty() {
            return self.settle(rebased, self.indices.len());
        }
        self.indices.retain(|index| !doomed.contains(index));
        for index in &doomed {
            self.members.remove(index);
        }
        self.members.extend(added.iter().copied());
        self.indices.extend(added);
        self.settle(true, 0)
    }

    /// Keeps the members `keep` accepts, in order. Returns whether any was dropped.
    pub fn retain(&mut self, mut keep: impl FnMut(usize) -> bool) -> bool {
        let before = self.indices.len();
        self.indices.retain(|index| keep(*index));
        if self.indices.len() == before {
            return false;
        }
        self.members = self.indices.iter().copied().collect();
        self.settle(true, before)
    }

    /// Drops the members when `owner` is not the owner; returns whether it did.
    fn rebase(&mut self, owner: &ComponentOwner) -> bool {
        if self.is_owned_by(owner) {
            return false;
        }
        let had_members = !self.indices.is_empty();
        self.clear();
        self.owner = Some(owner.clone());
        had_members
    }

    /// Normalises the owner after a mutation and reports whether the members changed.
    fn settle(&mut self, forced: bool, len_before: usize) -> bool {
        if self.indices.is_empty() {
            self.owner = None;
        }
        forced || self.indices.len() != len_before
    }
}

#[cfg(test)]
mod tests;
