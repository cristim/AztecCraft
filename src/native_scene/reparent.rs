//! Parent and unparent of authored transforms (#42): the typed core of Maya's `parent` command
//! with `-absolute` (keep the world transform, the documented default), `-relative` (keep the
//! local values) and `-world`. Instancing (`-add`) and joints are not covered. Maya renames the
//! object when its name clashes under the new parent; this refuses with `NameCollision`.

use super::{DagEdge, NativeError, NativeId, NativeScene, TransformExtras, TransformTarget, Units};
use crate::scene::NodeId;
use glam::{DMat3, DMat4, DVec3};
use std::fmt;

/// What a reparent keeps: the world transform (Maya `-absolute`) or the local channels
/// (Maya `-relative`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReparentKeep {
    World,
    Local,
}

/// Why a reparent was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReparentRefusal {
    /// The new parent is the node itself or one of its descendants.
    Cycle,
    /// The node or one of its ancestors is instanced, so it has no single parent or world.
    Instanced,
    /// Keeping the world transform needs a node with default pivots and extras and no keys.
    WorldNeedsPlainTransform,
    /// The local transform that keeps the world one is not a translate, XYZ rotate and scale.
    WorldNotRepresentable,
}

impl fmt::Display for ReparentRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cycle => "an object cannot be parented under itself or one of its descendants",
            Self::Instanced => "instanced objects cannot be reparented",
            Self::WorldNeedsPlainTransform => {
                "keeping the world transform needs default pivots and transform extras and no keys; use keep local"
            }
            Self::WorldNotRepresentable => {
                "the world transform cannot be kept as translate, rotate and scale under that parent (shear, negative or zero scale); use keep local"
            }
        })
    }
}

/// Relative tolerance for accepting a decomposed transform as the matrix it came from.
const RECOMPOSE_TOLERANCE: f64 = 1e-9;
/// Smallest scale or determinant treated as invertible.
const MIN_SCALE: f64 = 1e-12;

fn rotation_zyx(rotation: [f64; 3]) -> DMat4 {
    DMat4::from_rotation_z(rotation[2])
        * DMat4::from_rotation_y(rotation[1])
        * DMat4::from_rotation_x(rotation[0])
}

/// `matrix` as `[translation, XYZ rotation in radians, scale]`, or `None` when it is not
/// exactly `T * Rz * Ry * Rx * S` (shear, negative or zero scale, non-finite).
fn decompose(matrix: DMat4) -> Option<[f64; 9]> {
    let translation = matrix.w_axis.truncate();
    let columns = [
        matrix.x_axis.truncate(),
        matrix.y_axis.truncate(),
        matrix.z_axis.truncate(),
    ];
    let scale = columns.map(DVec3::length);
    if !matrix.is_finite() || scale.iter().any(|s| !s.is_finite() || *s <= MIN_SCALE) {
        return None;
    }
    let rotation = DMat3::from_cols(
        columns[0] / scale[0],
        columns[1] / scale[1],
        columns[2] / scale[2],
    );
    // Rz(c) Ry(b) Rx(a): element (2,0) is -sin b.
    let sin_b = -rotation.x_axis.z;
    let b = sin_b.clamp(-1.0, 1.0).asin();
    let (a, c) = if sin_b.abs() < 1.0 - 1e-12 {
        (
            rotation.y_axis.z.atan2(rotation.z_axis.z),
            rotation.x_axis.y.atan2(rotation.x_axis.x),
        )
    } else {
        (f64::atan2(-rotation.z_axis.y, rotation.y_axis.y), 0.0)
    };
    let rebuilt = DMat4::from_translation(translation)
        * rotation_zyx([a, b, c])
        * DMat4::from_scale(DVec3::from_array(scale));
    // Translation is copied exactly, so only the linear part is compared, against its own size.
    let largest = scale.iter().fold(f64::MIN_POSITIVE, |m, v| m.max(*v));
    let rebuilt_linear = [rebuilt.x_axis, rebuilt.y_axis, rebuilt.z_axis];
    let close = [matrix.x_axis, matrix.y_axis, matrix.z_axis]
        .iter()
        .zip(rebuilt_linear)
        .all(|(x, y)| {
            (x.truncate() - y.truncate()).abs().max_element() <= RECOMPOSE_TOLERANCE * largest
        });
    close.then(|| {
        [
            translation.x,
            translation.y,
            translation.z,
            a.to_degrees(),
            b.to_degrees(),
            c.to_degrees(),
            scale[0],
            scale[1],
            scale[2],
        ]
    })
}

impl NativeScene {
    /// Moves `id` under `new_parent` (`None` is the world) as one edit. Maya's `parent`: with
    /// `ReparentKeep::World` the local channels are rewritten so the world transform is
    /// unchanged; with `Local` they are kept and the object moves in world space. The node
    /// becomes the new parent's last child. Same parent is a no-op. Sourced scenes (a read-only
    /// ASCII file, or an opened binary) are refused. A refusal leaves the scene unchanged.
    pub fn reparent(
        &mut self,
        id: NodeId,
        new_parent: Option<NodeId>,
        keep: ReparentKeep,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        if self.units != Units::default() {
            return Err(NativeError::UnsupportedEdit);
        }
        let index = self.editable_node(id)?;
        let node_id = self.nodes[index].transform_id;
        let parent_id = match new_parent {
            Some(parent) => {
                let at = self.editable_node(parent)?;
                Some(self.nodes[at].transform_id)
            }
            None => None,
        };
        let mut incoming = self
            .dag
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.child == node_id);
        let (edge_at, edge) = incoming.next().ok_or(NativeError::InvalidIdentity)?;
        if incoming.next().is_some() {
            return Err(ReparentRefusal::Instanced.into());
        }
        if edge.parent == parent_id {
            return Ok(());
        }
        if let Some(parent) = parent_id
            && (parent == node_id || self.is_below(node_id, parent)?)
        {
            return Err(ReparentRefusal::Cycle.into());
        }
        // Maya renames the parented object on a clash; this editor refuses, as rename_node does.
        let name = self.nodes[index].name.as_str();
        let clash = self
            .dag
            .iter()
            .filter(|e| e.parent == parent_id && e.child != node_id)
            .any(|e| {
                self.nodes
                    .iter()
                    .find(|n| n.transform_id == e.child)
                    .map(|n| n.name.as_str())
                    .or_else(|| {
                        self.shapes
                            .iter()
                            .find(|s| s.id == e.child)
                            .map(|s| s.name.as_str())
                    })
                    == Some(name)
            });
        if clash {
            return Err(NativeError::NameCollision);
        }
        let channels = match keep {
            ReparentKeep::Local => None,
            ReparentKeep::World => self.world_keeping_channels(index, parent_id)?,
        };
        let old_channels = self.nodes[index].channels.clone();
        let old_dag = self.dag.clone();
        if let Some(values) = channels {
            self.nodes[index].channels = self.replacement_channels(index, values.map(Some))?;
        }
        self.dag.remove(edge_at);
        self.dag.push(DagEdge {
            parent: parent_id,
            child: node_id,
        });
        if let Err(error) = self
            .validate()
            .and_then(|()| self.transform_paths().map(drop))
        {
            self.nodes[index].channels = old_channels;
            self.dag = old_dag;
            return Err(error);
        }
        Ok(())
    }

    /// Whether `descendant` lies strictly below `ancestor`.
    fn is_below(&self, ancestor: NativeId, descendant: NativeId) -> Result<bool, NativeError> {
        let index = self.dag_index()?;
        let mut seen = std::collections::HashSet::new();
        let mut work = vec![ancestor];
        while let Some(current) = work.pop() {
            for &child in index.children(Some(current)) {
                if child == descendant {
                    return Ok(true);
                }
                if seen.insert(child) {
                    work.push(child);
                }
            }
        }
        Ok(false)
    }

    /// World matrix of the transform at the end of `path`.
    fn path_world(&self, path: &[NativeId]) -> Result<DMat4, NativeError> {
        let mut world = DMat4::IDENTITY;
        for id in path {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == *id)
                .ok_or(NativeError::InvalidInstancePath)?;
            world = self
                .internal_transform(node.id)?
                .compose_world(world)
                .ok_or(NativeError::NonFinite)?;
        }
        Ok(world)
    }

    /// The one instance of `node` in the scene.
    fn sole_target(&self, node: NodeId) -> Result<TransformTarget, NativeError> {
        let mut targets = self
            .transform_paths()?
            .into_iter()
            .filter(|target| target.node == node);
        match (targets.next(), targets.next()) {
            (Some(target), None) => Ok(target),
            _ => Err(ReparentRefusal::Instanced.into()),
        }
    }

    /// The local channels (translation, XYZ rotation in degrees, scale) that keep node `index`'s
    /// world transform once it sits under `new_parent`; `None` when the world frames agree and
    /// the channels stay as they are.
    fn world_keeping_channels(
        &self,
        index: usize,
        new_parent: Option<NativeId>,
    ) -> Result<Option<[f64; 9]>, NativeError> {
        let node = &self.nodes[index];
        let plain = node
            .rotate_pivot
            .iter()
            .chain(&node.scale_pivot)
            .all(|v| v.value.to_bits() == 0)
            && node.extras == TransformExtras::default()
            && self.key_rows(Some(node.id)).next().is_none();
        if !plain {
            return Err(ReparentRefusal::WorldNeedsPlainTransform.into());
        }
        let target = self.sole_target(node.id)?;
        let old_world = self.path_world(&target.path[..target.path.len() - 1])?;
        let new_world = match new_parent {
            None => DMat4::IDENTITY,
            Some(parent) => {
                let owner = self
                    .nodes
                    .iter()
                    .find(|node| node.transform_id == parent)
                    .ok_or(NativeError::InvalidIdentity)?;
                self.path_world(&self.sole_target(owner.id)?.path)?
            }
        };
        let linear = |m: &DMat4| [m.x_axis, m.y_axis, m.z_axis].map(|c| c.truncate().to_array());
        if old_world == new_world {
            return Ok(None);
        }
        let determinant = new_world.determinant();
        if !determinant.is_finite() || determinant.abs() <= MIN_SCALE {
            return Err(ReparentRefusal::WorldNotRepresentable.into());
        }
        let local = self
            .internal_transform(node.id)?
            .matrix()
            .ok_or(NativeError::NonFinite)?;
        let wanted = new_world.inverse() * old_world * local;
        if linear(&old_world) == linear(&new_world) {
            // The parents differ by a translation only: rotation and scale stay bit for bit.
            let mut values = node.transform().values();
            if !wanted.w_axis.is_finite() {
                return Err(NativeError::NonFinite);
            }
            values[..3].copy_from_slice(&wanted.w_axis.truncate().to_array());
            return Ok(Some(values));
        }
        decompose(wanted)
            .map(Some)
            .ok_or_else(|| ReparentRefusal::WorldNotRepresentable.into())
    }
}

impl From<ReparentRefusal> for NativeError {
    fn from(reason: ReparentRefusal) -> Self {
        Self::InvalidReparent(reason)
    }
}

#[cfg(test)]
mod tests;
