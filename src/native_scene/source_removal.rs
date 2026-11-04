//! Removing a node from a scene opened from Maya Binary (PROVISIONAL, issue #272).
//!
//! The scene drops the nodes at once; the source bytes keep their records until Save prunes them.

use super::{
    DeleteUnsupported, NativeError, NativeId, NativeScene,
    binary::{self, RemovalRefusal},
};
use std::sync::Arc;

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

    /// Drops the transform and shapes `gone`. In a sourced scene their ids are recorded so the
    /// retained appearance, leaves and later Save can skip them.
    pub(super) fn apply_removal(&mut self, gone: &[NativeId]) {
        self.nodes.retain(|node| !gone.contains(&node.transform_id));
        self.shapes.retain(|shape| !gone.contains(&shape.id));
        self.dag.retain(|edge| {
            !gone.contains(&edge.child) && edge.parent.is_none_or(|p| !gone.contains(&p))
        });
        self.session_materials.prune(&self.shapes);
        if let Some(source) = &mut self.source {
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
