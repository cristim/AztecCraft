//! Removing a node from a scene opened from Maya Binary (PROVISIONAL, issue #272).
//!
//! The scene drops the nodes at once; the source bytes keep their records until Save prunes them.

use super::{
    DeleteUnsupported, NativeError, NativeId, NativeScene,
    binary::{self, RemovalRefusal},
};
use crate::scene::NodeId;
use std::sync::Arc;

/// Old id to new id once the removed bindings are gone from a reopened file: the survivors keep
/// their order and are numbered again from one, in both id spaces.
pub(super) struct IdRemap {
    native: Vec<u64>,
    nodes: Vec<u64>,
}

impl IdRemap {
    /// `removed` are the dropped bindings; `original` is the scene decoded before any removal.
    pub(super) fn new(removed: &[NativeId], original: &NativeScene) -> Self {
        let mut nodes: Vec<u64> = original
            .nodes
            .iter()
            .filter(|node| removed.contains(&node.transform_id))
            .map(|node| node.id.value())
            .collect();
        nodes.sort_unstable();
        Self {
            native: removed.iter().map(|id| id.0).collect(),
            nodes,
        }
    }
    fn native(&self, id: NativeId) -> NativeId {
        NativeId(id.0 - self.native.partition_point(|gone| *gone < id.0) as u64)
    }
    fn node(&self, id: NodeId) -> NodeId {
        NodeId::new(id.value() - self.nodes.partition_point(|gone| *gone < id.value()) as u64)
    }
}

impl NativeScene {
    /// Whether `gone` (a transform and its shapes) can leave the opened file, together with the
    /// nodes already removed. Refusals are typed and leave the scene untouched.
    pub(super) fn check_source_removal(&self, gone: &[NativeId]) -> Result<(), NativeError> {
        let source = self.source.as_ref().ok_or(NativeError::UnsupportedEdit)?;
        let refuse = |reason: RemovalRefusal| {
            NativeError::UnsupportedDelete(DeleteUnsupported::Source(reason))
        };
        let pending = self
            .nodes
            .iter()
            .filter(|node| !gone.contains(&node.transform_id))
            .flat_map(|node| {
                node.channels
                    .iter()
                    .chain(&node.rotate_pivot)
                    .chain(&node.scale_pivot)
            })
            .any(|scalar| scalar.replaced && scalar.origin.is_none());
        if pending {
            return Err(refuse(RemovalRefusal::PendingInsertion));
        }
        let all: Vec<NativeId> = self.removed.iter().chain(gone).copied().collect();
        binary::check_removal(&source.bytes, &all).map_err(refuse)?;
        if let Some(appearance) = &source.appearance {
            binary::check_materials(appearance, &all).map_err(refuse)?;
        }
        Ok(())
    }

    /// Names of the removed transforms and shapes, as the file spells them.
    pub(super) fn removed_names(&self, removed: &[NativeId]) -> Vec<String> {
        let nodes = self
            .nodes
            .iter()
            .filter(|n| removed.contains(&n.transform_id));
        let shapes = self.shapes.iter().filter(|s| removed.contains(&s.id));
        nodes
            .map(|n| n.name.clone())
            .chain(shapes.map(|s| s.name.clone()))
            .collect()
    }

    /// This scene as a reopen of the pruned file would number it: removed entries are gone and the
    /// survivors are renumbered. Source offsets and leaves are not translated.
    pub(super) fn renumbered(&self, remap: &IdRemap) -> NativeScene {
        let mut scene = self.clone();
        for node in &mut scene.nodes {
            node.id = remap.node(node.id);
            node.transform_id = remap.native(node.transform_id);
        }
        for shape in &mut scene.shapes {
            shape.id = remap.native(shape.id);
        }
        for edge in &mut scene.dag {
            edge.child = remap.native(edge.child);
            edge.parent = edge.parent.map(|parent| remap.native(parent));
        }
        if let Some(appearance) = scene.source.as_mut().and_then(|s| s.appearance.as_mut()) {
            let appearance = Arc::make_mut(appearance);
            appearance
                .shapes
                .retain(|entry| self.removed.binary_search(&entry.shape).is_err());
            for entry in &mut appearance.shapes {
                entry.shape = remap.native(entry.shape);
                if let Ok(binding) = &mut entry.binding {
                    binding.instance_parent = remap.native(binding.instance_parent);
                }
            }
            for binding in &mut appearance.diffuse {
                for shape in &mut binding.shapes {
                    *shape = remap.native(*shape);
                }
            }
        }
        scene.removed.clear();
        scene
    }

    /// Drops the transform and shapes `gone`. In a sourced scene their ids are recorded so the
    /// retained appearance, leaves and later Save can skip them.
    pub(super) fn apply_removal(&mut self, gone: &[NativeId]) {
        self.nodes.retain(|node| !gone.contains(&node.transform_id));
        self.shapes.retain(|shape| !gone.contains(&shape.id));
        self.dag.retain(|edge| {
            !gone.contains(&edge.child) && edge.parent.is_none_or(|p| !gone.contains(&p))
        });
        self.session_materials.prune(&self.shapes);
        self.session_joints.prune(&gone.iter().copied().collect());
        if let Some(source) = &mut self.source {
            source.has_removals = true;
            if let Some(appearance) = &mut source.appearance {
                let appearance = Arc::make_mut(appearance);
                for binding in &mut appearance.diffuse {
                    binding.shapes.retain(|shape| !gone.contains(shape));
                }
            }
            self.removed.extend_from_slice(gone);
            self.removed.sort_unstable();
            self.removed.dedup();
        }
    }
}
