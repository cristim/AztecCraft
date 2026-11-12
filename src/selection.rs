use crate::native_scene::{MAX_TRANSFORM_PATHS, TransformTarget};
use std::collections::HashSet;
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
///
/// `generation` changes (wrapping) whenever a mutator changes the members or their order, so a
/// caller can tell that the selection it inspected is no longer the current one. Equality compares
/// the members only.
#[derive(Clone, Debug, Default)]
pub struct ObjectSelection {
    members: Vec<TransformTarget>,
    generation: u64,
}

impl PartialEq for ObjectSelection {
    fn eq(&self, other: &Self) -> bool {
        self.members == other.members
    }
}

impl Eq for ObjectSelection {}

impl ObjectSelection {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn bump(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

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
        if !self.members.is_empty() {
            self.members.clear();
            self.bump();
        }
    }

    /// Makes `target` the only member.
    pub fn replace(&mut self, target: TransformTarget) {
        if self.members.as_slice() != std::slice::from_ref(&target) {
            self.members.clear();
            self.members.push(target);
            self.bump();
        }
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
        if self.members != unique {
            self.members = unique;
            self.bump();
        }
        Ok(())
    }

    /// Appends `target` as the lead; an existing member moves to the end and becomes the lead.
    /// Over the cap the selection is unchanged.
    pub fn add(&mut self, target: TransformTarget) -> Result<(), SelectionFull> {
        match self.members.iter().position(|member| *member == target) {
            Some(index) if index + 1 == self.members.len() => return Ok(()),
            Some(index) => {
                self.members.remove(index);
            }
            None if self.members.len() >= MAX_TRANSFORM_PATHS => return Err(SelectionFull),
            None => {}
        }
        self.members.push(target);
        self.bump();
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
                self.bump();
                true
            }
            None => false,
        }
    }

    /// Appends the targets that are not members, in order and each once; the last appended is the
    /// lead. Members keep their place. Over the cap the selection is unchanged.
    pub fn extend_absent(
        &mut self,
        targets: impl IntoIterator<Item = TransformTarget>,
    ) -> Result<(), SelectionFull> {
        let mut seen: HashSet<TransformTarget> = self.members.iter().cloned().collect();
        let absent: Vec<TransformTarget> = targets
            .into_iter()
            .filter(|target| seen.insert(target.clone()))
            .collect();
        if absent.is_empty() {
            return Ok(());
        }
        if self.members.len() + absent.len() > MAX_TRANSFORM_PATHS {
            return Err(SelectionFull);
        }
        self.members.extend(absent);
        self.bump();
        Ok(())
    }

    /// Removes the targets that are members and returns whether any was. Removing the lead
    /// promotes the previous member.
    pub fn remove_all(&mut self, targets: &[TransformTarget]) -> bool {
        let doomed: HashSet<&TransformTarget> = targets.iter().collect();
        let before = self.members.len();
        self.members.retain(|member| !doomed.contains(member));
        let changed = self.members.len() != before;
        if changed {
            self.bump();
        }
        changed
    }

    /// Removes the targets that are members and appends the others in order, each once; the last
    /// appended is the lead, else a removed lead promotes the previous member. Over the cap, counted
    /// after the removals, the selection is unchanged.
    pub fn toggle_all(&mut self, targets: &[TransformTarget]) -> Result<(), SelectionFull> {
        let members: HashSet<&TransformTarget> = self.members.iter().collect();
        let mut seen = HashSet::new();
        let (present, absent): (Vec<_>, Vec<_>) = targets
            .iter()
            .filter(|target| seen.insert(*target))
            .partition(|target| members.contains(target));
        if present.is_empty() && absent.is_empty() {
            return Ok(());
        }
        if self.members.len() - present.len() + absent.len() > MAX_TRANSFORM_PATHS {
            return Err(SelectionFull);
        }
        let doomed: HashSet<&TransformTarget> = present.into_iter().collect();
        self.members.retain(|member| !doomed.contains(member));
        self.members.extend(absent.into_iter().cloned());
        self.bump();
        Ok(())
    }

    /// Keeps the members `keep` accepts, in their order.
    pub fn retain(&mut self, keep: impl FnMut(&TransformTarget) -> bool) {
        let before = self.members.len();
        self.members.retain(keep);
        if self.members.len() != before {
            self.bump();
        }
    }
}

#[cfg(test)]
mod tests;
