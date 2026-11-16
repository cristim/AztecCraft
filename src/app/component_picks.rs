//! The remembered face, vertex and edge selections: three independent ordered sets, one per
//! component type, so switching mode keeps each (Maya's Preserve Component Selections). Each
//! set belongs to one shape instance; its lead (last member) is the pick the single-pick
//! readers see. Reached only through `Editor` accessors.

use super::Editor;
use crate::editor_operations::{ComponentKind, SelectionMode};
use azteccraft::native_scene::{ComponentCounts, EdgeTarget, FaceTarget, NativeId, VertexTarget};
use azteccraft::scene::NodeId;
use azteccraft::selection::{ComponentOwner, ComponentSet};

#[derive(Default)]
pub(super) struct ComponentPicks {
    faces: ComponentSet,
    vertices: ComponentSet,
    edges: ComponentSet,
}

/// What `sync_fields` does to one set: keep it, drop it (its owner is not the selected instance
/// or no longer resolves) or drop the members at or past the owner's component count.
#[derive(Clone, Copy)]
pub(super) enum Prune {
    Keep,
    Clear,
    Below(usize),
}

#[derive(Clone, Copy)]
pub(super) struct ComponentPrunes {
    faces: Prune,
    vertices: Prune,
    edges: Prune,
}

impl Prune {
    /// Whether the set changed.
    fn apply(self, set: &mut ComponentSet) -> bool {
        match self {
            Self::Keep => false,
            Self::Clear => {
                let changed = !set.is_empty();
                set.clear();
                changed
            }
            Self::Below(count) => set.retain(|index| index < count),
        }
    }
}

/// The set's lead as `(owner, index)`.
fn lead(set: &ComponentSet) -> Option<(&ComponentOwner, usize)> {
    Some((set.owner()?, set.lead()?))
}

/// Makes `index` of the shape instance the set's only member, or empties the set. Returns
/// whether the set changed (a lead alone is not enough: the set may hold more members).
fn replace_one(
    set: &mut ComponentSet,
    target: Option<(NodeId, NativeId, Vec<NativeId>, usize)>,
) -> bool {
    match target {
        Some((transform, shape, path, index)) => set.replace_with(
            &ComponentOwner {
                transform,
                shape,
                path,
            },
            [index],
        ),
        None => {
            let changed = !set.is_empty();
            set.clear();
            changed
        }
    }
}

impl Editor {
    pub(super) fn remembered_face(&self) -> Option<FaceTarget> {
        lead(&self.picks.faces).map(|(owner, face_index)| FaceTarget {
            transform: owner.transform,
            shape: owner.shape,
            path: owner.path.clone(),
            face_index,
        })
    }

    pub(super) fn remembered_vertex(&self) -> Option<VertexTarget> {
        lead(&self.picks.vertices).map(|(owner, vertex_index)| VertexTarget {
            transform: owner.transform,
            shape: owner.shape,
            path: owner.path.clone(),
            vertex_index,
        })
    }

    pub(super) fn remembered_edge(&self) -> Option<EdgeTarget> {
        lead(&self.picks.edges).map(|(owner, edge_index)| EdgeTarget {
            transform: owner.transform,
            shape: owner.shape,
            path: owner.path.clone(),
            edge_index,
        })
    }

    /// The one staleness token for the selection a remote caller inspected: the object
    /// selection's generation plus a count of every component-set and selection-mode change.
    /// Both parts only grow, so the sum never repeats a value and cannot collide across modes
    /// or return to an old value (a wrap takes 2^64 changes). It guards the selection, not the
    /// geometry: an undo or redo that moves points does not change it.
    pub(super) fn selection_generation(&self) -> u64 {
        self.selection.generation().wrapping_add(self.pick_epoch)
    }

    fn bump_pick_epoch(&mut self, changed: bool) -> bool {
        if changed {
            self.pick_epoch = self.pick_epoch.wrapping_add(1);
        }
        changed
    }

    /// Sets the selection mode; whether it changed.
    pub(super) fn set_selection_mode_field(&mut self, mode: SelectionMode) -> bool {
        let changed = self.selection_mode != mode;
        self.selection_mode = mode;
        self.bump_pick_epoch(changed)
    }

    /// Makes `target` the only selected face, or empties the set.
    pub(super) fn set_remembered_face(&mut self, target: Option<FaceTarget>) -> bool {
        let changed = replace_one(
            &mut self.picks.faces,
            target.map(|t| (t.transform, t.shape, t.path, t.face_index)),
        );
        self.bump_pick_epoch(changed)
    }

    pub(super) fn set_remembered_vertex(&mut self, target: Option<VertexTarget>) -> bool {
        let changed = replace_one(
            &mut self.picks.vertices,
            target.map(|t| (t.transform, t.shape, t.path, t.vertex_index)),
        );
        self.bump_pick_epoch(changed)
    }

    pub(super) fn set_remembered_edge(&mut self, target: Option<EdgeTarget>) -> bool {
        let changed = replace_one(
            &mut self.picks.edges,
            target.map(|t| (t.transform, t.shape, t.path, t.edge_index)),
        );
        self.bump_pick_epoch(changed)
    }

    pub(super) fn component_set(&self, kind: ComponentKind) -> &ComponentSet {
        match kind {
            ComponentKind::Face => &self.picks.faces,
            ComponentKind::Vertex => &self.picks.vertices,
            ComponentKind::Edge => &self.picks.edges,
        }
    }

    /// Edits one set through `change`, which reports whether it changed the set.
    pub(super) fn modify_component_set(
        &mut self,
        kind: ComponentKind,
        change: impl FnOnce(&mut ComponentSet) -> bool,
    ) -> bool {
        let set = match kind {
            ComponentKind::Face => &mut self.picks.faces,
            ComponentKind::Vertex => &mut self.picks.vertices,
            ComponentKind::Edge => &mut self.picks.edges,
        };
        let changed = change(set);
        self.bump_pick_epoch(changed)
    }

    pub(super) fn clear_remembered_components(&mut self) {
        let held = !(self.picks.faces.is_empty()
            && self.picks.vertices.is_empty()
            && self.picks.edges.is_empty());
        self.picks = ComponentPicks::default();
        self.bump_pick_epoch(held);
    }

    /// Decides, with one owner resolution per set, which members survive the current scene and
    /// selection: a set stays only while its owner is the selected instance.
    pub(super) fn component_prunes(&self) -> ComponentPrunes {
        let (selected, selected_path) = self.selection_presentation();
        let scene = self.history.scene();
        let decide = |set: &ComponentSet, count: fn(&ComponentCounts) -> usize| {
            let Some(owner) = set.owner() else {
                return Prune::Keep;
            };
            let belongs = Some(owner.transform) == selected
                && owner
                    .path
                    .split_last()
                    .is_some_and(|(_, parent)| parent == selected_path);
            if !belongs {
                return Prune::Clear;
            }
            match scene.component_counts(owner.transform, owner.shape, &owner.path) {
                Ok(counts) => Prune::Below(count(&counts)),
                Err(_) => Prune::Clear,
            }
        };
        ComponentPrunes {
            faces: decide(&self.picks.faces, |c| c.faces),
            vertices: decide(&self.picks.vertices, |c| c.vertices),
            edges: decide(&self.picks.edges, |c| c.edges),
        }
    }

    pub(super) fn apply_component_prunes(&mut self, prunes: ComponentPrunes) {
        let changed = [
            prunes.faces.apply(&mut self.picks.faces),
            prunes.vertices.apply(&mut self.picks.vertices),
            prunes.edges.apply(&mut self.picks.edges),
        ];
        self.bump_pick_epoch(changed.contains(&true));
    }
}
