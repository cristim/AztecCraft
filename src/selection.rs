use crate::native_scene::{MAX_TRANSFORM_PATHS, TransformTarget};
use std::fmt;

/// The selection already holds `MAX_TRANSFORM_PATHS` objects, so another one is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionFull;

impl fmt::Display for SelectionFull {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A selection holds at most {MAX_TRANSFORM_PATHS} objects")
    }
}

impl std::error::Error for SelectionFull {}

/// Ordered set of selected transform instances; the last member is the lead.
///
/// Identity is (node, path), so one node reached through two instance paths is two members.
/// The type does not look targets up in a scene; the caller resolves them first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectSelection {
    members: Vec<TransformTarget>,
}

impl ObjectSelection {
    pub fn members(&self) -> &[TransformTarget] {
        &self.members
    }

    pub fn lead(&self) -> Option<&TransformTarget> {
        self.members.last()
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn contains(&self, target: &TransformTarget) -> bool {
        self.members.contains(target)
    }

    pub fn clear(&mut self) {
        self.members.clear();
    }

    /// Makes `target` the only member.
    pub fn replace(&mut self, target: TransformTarget) {
        self.members.clear();
        self.members.push(target);
    }

    /// Replaces the members with `members` in order, the last being the lead. A repeated target
    /// keeps its first position. Over the cap the selection is unchanged.
    pub fn replace_with(&mut self, members: Vec<TransformTarget>) -> Result<(), SelectionFull> {
        let mut seen = std::collections::HashSet::new();
        let mut unique = Vec::with_capacity(members.len().min(MAX_TRANSFORM_PATHS));
        for member in members {
            if seen.insert(member.clone()) {
                if unique.len() >= MAX_TRANSFORM_PATHS {
                    return Err(SelectionFull);
                }
                unique.push(member);
            }
        }
        self.members = unique;
        Ok(())
    }

    /// Appends `target` as the lead; an existing member moves to the end and becomes the lead.
    /// Over the cap the selection is unchanged.
    pub fn add(&mut self, target: TransformTarget) -> Result<(), SelectionFull> {
        match self.members.iter().position(|member| *member == target) {
            Some(index) => {
                self.members.remove(index);
            }
            None if self.members.len() >= MAX_TRANSFORM_PATHS => return Err(SelectionFull),
            None => {}
        }
        self.members.push(target);
        Ok(())
    }

    /// Removes a member or appends an absent one as the lead. Returns whether `target` is a member
    /// afterwards. Removing the lead promotes the previous member.
    pub fn toggle(&mut self, target: TransformTarget) -> Result<bool, SelectionFull> {
        if self.remove(&target) {
            return Ok(false);
        }
        self.add(target)?;
        Ok(true)
    }

    /// Removes a member and returns whether it was present. Removing the lead promotes the
    /// previous member.
    pub fn remove(&mut self, target: &TransformTarget) -> bool {
        match self.members.iter().position(|member| member == target) {
            Some(index) => {
                self.members.remove(index);
                true
            }
            None => false,
        }
    }

    /// Keeps the members `keep` accepts, in their order.
    pub fn retain(&mut self, keep: impl FnMut(&TransformTarget) -> bool) {
        self.members.retain(keep);
    }
}

#[cfg(test)]
mod tests;
