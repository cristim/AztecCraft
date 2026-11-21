//! Modify > Prefix Hierarchy Names and Search and Replace Names (#313). Both rename transform
//! nodes through the same `set_transform_name` as a plain rename (so derived shape names follow)
//! and keep the scene's name rules: the ASCII identifier subset and names unique across the
//! scene. Every new name is checked before any node changes, so a refusal changes nothing.
//! Maya's pages (2014 and 2016 help) do not state the separator, clash handling, case rules or
//! regular expression support; this is PROVISIONAL (#5): the prefix is prepended verbatim with no
//! separator, matching is a plain substring, and a clash is refused instead of made unique.
use super::{NativeError, NativeScene};
use crate::scene::{NodeId, SceneError};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Why a naming edit was refused (beyond `InvalidName` and `NameCollision`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamingRefusal {
    /// No node was given.
    EmptySet,
    /// The prefix is empty.
    EmptyPrefix,
    /// The text to search for is empty.
    EmptySearch,
    /// No name contains the search text, or the replacement leaves every name as it was.
    NothingToRename,
}

impl fmt::Display for NamingRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptySet => "no objects were given",
            Self::EmptyPrefix => "the prefix is empty",
            Self::EmptySearch => "the text to search for is empty",
            Self::NothingToRename => "no node name changes: nothing contains the search text",
        })
    }
}

impl From<NamingRefusal> for NativeError {
    fn from(reason: NamingRefusal) -> Self {
        Self::InvalidNameEdit(reason)
    }
}

impl NativeScene {
    /// Prepends `prefix` to the name of every node of `ids` and of every transform below them,
    /// each node once (an instanced node reached twice is renamed once; its name is shared by all
    /// its paths). All or nothing.
    pub fn prefix_hierarchy_names(
        &mut self,
        ids: &[NodeId],
        prefix: &str,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        if prefix.is_empty() {
            return Err(NamingRefusal::EmptyPrefix.into());
        }
        let renames = self
            .name_targets(ids, true)?
            .into_iter()
            .map(|at| (at, format!("{prefix}{}", self.nodes[at].name)))
            .collect();
        self.apply_names(renames)
    }

    /// Replaces every occurrence of `search` in the name of each node of `ids` (and, with
    /// `hierarchy`, of every transform below them). Nodes whose name does not contain `search`
    /// keep it. All or nothing; nothing matching is refused as `NothingToRename`.
    pub fn search_replace_names(
        &mut self,
        ids: &[NodeId],
        hierarchy: bool,
        search: &str,
        replace: &str,
        match_case: bool,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        if search.is_empty() {
            return Err(NamingRefusal::EmptySearch.into());
        }
        let renames: Vec<(usize, String)> = self
            .name_targets(ids, hierarchy)?
            .into_iter()
            .filter_map(|at| {
                let name = &self.nodes[at].name;
                let replaced = replace_in(name, search, replace, match_case)?;
                (replaced != *name).then_some((at, replaced))
            })
            .collect();
        if renames.is_empty() {
            return Err(NamingRefusal::NothingToRename.into());
        }
        self.apply_names(renames)
    }

    /// Node indices of `ids` (plus everything below when `hierarchy`), each once, parents before
    /// children. A missing id or a node the scene cannot edit refuses the whole set.
    fn name_targets(&self, ids: &[NodeId], hierarchy: bool) -> Result<Vec<usize>, NativeError> {
        if ids.is_empty() {
            return Err(NamingRefusal::EmptySet.into());
        }
        let by_transform: HashMap<_, _> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.transform_id, at))
            .collect();
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut work: Vec<usize> = Vec::new();
        for id in ids.iter().rev() {
            let at = self
                .nodes
                .iter()
                .position(|node| node.id == *id)
                .ok_or(SceneError::MissingNode(*id))?;
            work.push(at);
        }
        while let Some(at) = work.pop() {
            if !seen.insert(at) {
                continue;
            }
            self.editable_node(self.nodes[at].id)?;
            order.push(at);
            if hierarchy {
                let transform = self.nodes[at].transform_id;
                let below = self
                    .dag
                    .iter()
                    .filter(|edge| edge.parent == Some(transform))
                    .filter_map(|edge| by_transform.get(&edge.child).copied());
                work.extend(below.collect::<Vec<_>>().into_iter().rev());
            }
        }
        Ok(order)
    }

    /// Validates every new name, then renames all of them on a copy that is adopted only when
    /// the whole scene still validates.
    fn apply_names(&mut self, renames: Vec<(usize, String)>) -> Result<(), NativeError> {
        let renamed: HashSet<usize> = renames.iter().map(|(at, _)| *at).collect();
        let mut taken: HashSet<&str> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(at, _)| !renamed.contains(at))
            .map(|(_, node)| node.name.as_str())
            .chain(self.shapes.iter().map(|shape| shape.name.as_str()))
            .collect();
        for (_, name) in &renames {
            if !super::is_valid_name(name) {
                return Err(NativeError::InvalidName);
            }
            if !taken.insert(name) {
                return Err(NativeError::NameCollision);
            }
        }
        let mut candidate = self.clone();
        for (at, name) in &renames {
            candidate.set_transform_name(*at, name);
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

/// `name` with every non-overlapping occurrence of `search` (left to right) replaced, or None
/// when there is none. Names are ASCII (the identifier subset), so the case-insensitive
/// comparison lowercases ASCII only and keeps byte offsets.
fn replace_in(name: &str, search: &str, replace: &str, match_case: bool) -> Option<String> {
    if match_case {
        return name.contains(search).then(|| name.replace(search, replace));
    }
    let (haystack, needle) = (name.to_ascii_lowercase(), search.to_ascii_lowercase());
    let mut out = String::with_capacity(name.len());
    let mut last = 0;
    for (at, _) in haystack.match_indices(&needle) {
        out.push_str(&name[last..at]);
        out.push_str(replace);
        last = at + needle.len();
    }
    if last == 0 && !haystack.contains(&needle) {
        return None;
    }
    out.push_str(&name[last..]);
    Some(out)
}

#[cfg(test)]
mod tests;
