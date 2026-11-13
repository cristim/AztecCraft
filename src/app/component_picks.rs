//! The remembered face, vertex and edge selections: three independent ordered sets, one per
//! component type, so switching mode keeps each (Maya's Preserve Component Selections). Each
//! set belongs to one shape instance; its lead (last member) is the pick the single-pick
//! readers see. Reached only through `Editor` accessors.

use super::Editor;
use crate::editor_operations::ComponentKind;
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
    fn apply(self, set: &mut ComponentSet) {
        match self {
            Self::Keep => {}
            Self::Clear => set.clear(),
            Self::Below(count) => {
                set.retain(|index| index < count);
            }
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

    /// Makes `target` the only selected face, or empties the set.
    pub(super) fn set_remembered_face(&mut self, target: Option<FaceTarget>) -> bool {
        replace_one(
            &mut self.picks.faces,
            target.map(|t| (t.transform, t.shape, t.path, t.face_index)),
        )
    }

    pub(super) fn set_remembered_vertex(&mut self, target: Option<VertexTarget>) -> bool {
        replace_one(
            &mut self.picks.vertices,
            target.map(|t| (t.transform, t.shape, t.path, t.vertex_index)),
        )
    }

    pub(super) fn set_remembered_edge(&mut self, target: Option<EdgeTarget>) -> bool {
        replace_one(
            &mut self.picks.edges,
            target.map(|t| (t.transform, t.shape, t.path, t.edge_index)),
        )
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(super) fn component_set(&self, kind: ComponentKind) -> &ComponentSet {
        match kind {
            ComponentKind::Face => &self.picks.faces,
            ComponentKind::Vertex => &self.picks.vertices,
            ComponentKind::Edge => &self.picks.edges,
        }
    }

    pub(super) fn component_set_mut(&mut self, kind: ComponentKind) -> &mut ComponentSet {
        match kind {
            ComponentKind::Face => &mut self.picks.faces,
            ComponentKind::Vertex => &mut self.picks.vertices,
            ComponentKind::Edge => &mut self.picks.edges,
        }
    }

    pub(super) fn clear_remembered_components(&mut self) {
        self.picks = ComponentPicks::default();
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
        prunes.faces.apply(&mut self.picks.faces);
        prunes.vertices.apply(&mut self.picks.vertices);
        prunes.edges.apply(&mut self.picks.edges);
    }
}
