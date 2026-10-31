//! Session-only joints: a bare transform in the DAG plus one row in this table.
//!
//! Translate, rotate and scale stay in the node's channels and the hierarchy stays in the DAG,
//! so the table holds only what a plain transform has no field for. It is not saved, and it
//! never coexists with a retained source: writers refuse a scene holding joints instead of
//! writing them as plain transforms. Behaviour follows the Maya 2027 `joint` command and node
//! pages; the items marked PROVISIONAL are this project's reading, recorded in the issue (#363).

use super::{
    DagEdge, GeometryViewFacts, NativeError, NativeId, NativeScene, NativeTransform,
    NativeTransformNode, Scalar, TransformExtras, TransformField, TransformTarget, Uuid, authored,
};
use crate::scene::{NodeId, SceneError};
use glam::DVec3;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

/// The node page's `radius` default; the command page lists none (PROVISIONAL at creation).
pub const DEFAULT_JOINT_RADIUS: f64 = 1.0;
/// A safety bound, not a Maya limit (PROVISIONAL, #5): the primitive size limit, which keeps a
/// joint's circles and framing inside what the renderer and camera accept.
pub const MAX_JOINT_RADIUS_CM: f64 = crate::primitive_options::MAX_SIZE_CM;

#[derive(Clone, Copy, Debug)]
pub struct Joint {
    radius: f64,
}

impl Joint {
    pub fn radius(self) -> f64 {
        self.radius
    }
}

impl PartialEq for Joint {
    fn eq(&self, other: &Self) -> bool {
        self.radius.to_bits() == other.radius.to_bits()
    }
}

/// What `NativeScene::create_joint` builds.
#[derive(Clone, Debug, PartialEq)]
pub struct JointRequest {
    /// The joint above the new one; `None` starts a new hierarchy.
    pub parent: Option<NodeId>,
    /// World position, or the offset from the parent when `relative` is set.
    pub position: [f64; 3],
    /// Ignored without a parent, whose space is the world.
    pub relative: bool,
    pub radius: f64,
    /// `None` takes the lowest free `joint<N>` (casing PROVISIONAL: the page lists "Joint#").
    pub name: Option<String>,
}

/// Why a joint cannot be created as asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointUnsupported {
    ParentNotJoint,
    SingularParent,
    Radius,
}

impl fmt::Display for JointUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ParentNotJoint => {
                "the parent must be a joint; parenting a joint under other objects is not supported yet"
            }
            Self::SingularParent => {
                "the parent has zero or unusable scale, so a world position cannot be solved; use a relative position"
            }
            Self::Radius => {
                "the joint radius must be greater than 0 and at most 1000000"
            }
        })
    }
}

fn valid_radius(radius: f64) -> bool {
    radius > 0.0 && radius <= MAX_JOINT_RADIUS_CM
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SessionJoints {
    joints: BTreeMap<NativeId, Joint>,
}

impl SessionJoints {
    pub(super) fn is_empty(&self) -> bool {
        self.joints.is_empty()
    }

    pub(super) fn contains(&self, transform: NativeId) -> bool {
        self.joints.contains_key(&transform)
    }

    pub(super) fn get(&self, transform: NativeId) -> Option<Joint> {
        self.joints.get(&transform).copied()
    }

    pub(super) fn prune(&mut self, removed: &HashSet<NativeId>) {
        self.joints.retain(|id, _| !removed.contains(id));
    }

    /// One pass over the DAG, so a long chain stays linear in its edges.
    pub(super) fn validate(
        &self,
        nodes: &[NativeTransformNode],
        dag: &[DagEdge],
        sourced: bool,
    ) -> Result<(), NativeError> {
        let invalid = || Err(NativeError::InvalidSessionJoints);
        if self.joints.is_empty() {
            return Ok(());
        }
        if sourced {
            return invalid();
        }
        for joint in self.joints.values() {
            if !valid_radius(joint.radius) {
                return invalid();
            }
        }
        // A joint needs the plain matrix path: the extras path has no inverse parent scale.
        let mut known = 0;
        for node in nodes {
            if self.contains(node.transform_id) {
                known += 1;
                if !node.extras.is_default() {
                    return invalid();
                }
            }
        }
        if known != self.joints.len() {
            return invalid();
        }
        let mut incoming: BTreeMap<NativeId, usize> = BTreeMap::new();
        for edge in dag {
            let child = self.contains(edge.child);
            if child {
                *incoming.entry(edge.child).or_default() += 1;
            }
            if let Some(parent) = edge.parent
                && self.contains(parent) != child
            {
                return invalid();
            }
        }
        if incoming.len() != self.joints.len() || incoming.values().any(|count| *count != 1) {
            return invalid();
        }
        Ok(())
    }
}

impl NativeScene {
    /// The joint data of this transform, `None` for any other node.
    pub fn joint(&self, transform: NativeId) -> Option<Joint> {
        self.session_joints.get(transform)
    }

    pub fn has_joints(&self) -> bool {
        !self.session_joints.is_empty()
    }

    /// The inverse of the parent joint's scale, for a joint under a joint: the node page's IS,
    /// on by default (segment scale compensate). It reads the parent's own channel, so there is
    /// no recursion. A component without a finite inverse uses 1 (PROVISIONAL), which keeps a
    /// zero or tiny parent scale from making the view non-finite.
    pub(super) fn joint_inverse_parent_scale(
        &self,
        node: &NativeTransformNode,
    ) -> Result<Option<[f64; 3]>, NativeError> {
        if !self.session_joints.contains(node.transform_id) {
            return Ok(None);
        }
        let parent = self
            .dag
            .iter()
            .find(|edge| edge.child == node.transform_id)
            .and_then(DagEdge::parent);
        let Some(parent) = parent else {
            return Ok(None);
        };
        let parent = self
            .nodes
            .iter()
            .find(|candidate| candidate.transform_id == parent)
            .ok_or(NativeError::InvalidInstancePath)?;
        let mut inverse = [1.0; 3];
        for (component, scalar) in parent.field(TransformField::Scale).iter().enumerate() {
            let value = 1.0
                / scalar.internal(
                    self.source.as_ref(),
                    &self.units,
                    parent.transform_id,
                    TransformField::Scale,
                    component,
                )?;
            if value.is_finite() {
                inverse[component] = value;
            }
        }
        Ok(Some(inverse))
    }

    /// Adds a joint and returns its full path, proved valid before the scene is left changed.
    /// Rotation stays 0 under a rotated parent, so the joint inherits its orientation
    /// (PROVISIONAL: this follows the `joint` command, not the Joint Tool's chain orientation).
    pub fn create_joint(&mut self, request: &JointRequest) -> Result<TransformTarget, NativeError> {
        self.structural_edit()?;
        if request.position.iter().any(|value| !value.is_finite()) || !request.radius.is_finite() {
            return Err(NativeError::NonFinite);
        }
        if !valid_radius(request.radius) {
            return Err(NativeError::UnsupportedJoint(JointUnsupported::Radius));
        }
        let name = match &request.name {
            Some(name) => {
                self.validate_new_name(None, name)?;
                if authored::names(self).any(|(_, taken)| taken == name) {
                    return Err(NativeError::NameCollision);
                }
                name.clone()
            }
            None => self.free_joint_name()?,
        };
        let (parent, translation) = match request.parent {
            Some(parent) => self.joint_placement(parent, request)?,
            None => (None, request.position),
        };
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SceneError::IdsExhausted)?;
        let next_native_id = self
            .next_native_id
            .checked_add(1)
            .ok_or(SceneError::IdsExhausted)?;
        let id = NodeId::new(self.next_id);
        let transform_id = NativeId(self.next_native_id);
        self.nodes.push(NativeTransformNode {
            id,
            transform_id,
            transform_uuid: Some(Uuid::new_v4()),
            name,
            channels: NativeTransform {
                translation,
                ..NativeTransform::default()
            }
            .values()
            .map(Scalar::new),
            rotate_pivot: [0.0; 3].map(Scalar::new),
            scale_pivot: [0.0; 3].map(Scalar::new),
            restrictions: Vec::new(),
            unsupported_transform: Vec::new(),
            extras: TransformExtras::default(),
            view: GeometryViewFacts::authored(),
        });
        self.dag.push(DagEdge {
            parent,
            child: transform_id,
        });
        self.session_joints.joints.insert(
            transform_id,
            Joint {
                radius: request.radius,
            },
        );
        // A candidate past the depth or path limits would otherwise break every later read.
        let target = self.transform_paths().and_then(|paths| {
            paths
                .into_iter()
                .find(|target| target.node == id)
                .ok_or(NativeError::InvalidInstancePath)
        });
        match target {
            Ok(target) => {
                self.next_id = next_id;
                self.next_native_id = next_native_id;
                Ok(target)
            }
            Err(error) => {
                self.nodes.pop();
                self.dag.pop();
                self.session_joints.joints.remove(&transform_id);
                Err(error)
            }
        }
    }

    /// The lowest `joint<N>` no transform or shape holds, as `create_default_named` does.
    fn free_joint_name(&self) -> Result<String, NativeError> {
        let count = authored::names(self).count();
        (1..=count + 1)
            .map(|suffix| format!("joint{suffix}"))
            .find(|name| !authored::names(self).any(|(_, taken)| taken == name))
            .ok_or(NativeError::ResourceLimit)
    }

    /// The parent edge and the local translation for a child of `parent`. T sits above IS in the
    /// joint matrix, so a world position is solved through the parent's plain world matrix.
    fn joint_placement(
        &self,
        parent: NodeId,
        request: &JointRequest,
    ) -> Result<(Option<NativeId>, [f64; 3]), NativeError> {
        let node = self.node(parent)?;
        if !self.session_joints.contains(node.transform_id) {
            return Err(NativeError::UnsupportedJoint(
                JointUnsupported::ParentNotJoint,
            ));
        }
        let edge = Some(node.transform_id);
        if request.relative {
            return Ok((edge, request.position));
        }
        let singular = NativeError::UnsupportedJoint(JointUnsupported::SingularParent);
        let target = self
            .transform_paths()?
            .into_iter()
            .find(|target| target.node == parent)
            .ok_or(NativeError::InvalidInstancePath)?;
        let world = self
            .ancestor_world(&target)?
            .zip(self.internal_transform(parent)?.matrix())
            .map(|(ancestors, local)| ancestors * local)
            .ok_or(singular.clone())?;
        let determinant = world.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return Err(singular);
        }
        let local = world
            .inverse()
            .transform_point3(DVec3::from_array(request.position));
        if !local.is_finite() {
            return Err(singular);
        }
        Ok((edge, local.to_array()))
    }

    /// Removes a joint with every joint below it (PROVISIONAL: the 2027 `delete` page is silent)
    /// and returns the removed nodes. The closure is computed once.
    pub(super) fn remove_joint_subtree(
        &mut self,
        root: NativeId,
    ) -> Result<Vec<NodeId>, NativeError> {
        self.check_effects(root)?;
        if self.source.is_some() {
            return Err(NativeError::UnsupportedEdit);
        }
        let mut closure = HashSet::new();
        closure
            .try_reserve(self.session_joints.joints.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        closure.insert(root);
        let mut queue = vec![root];
        while let Some(current) = queue.pop() {
            for edge in self.dag.iter().filter(|edge| edge.parent == Some(current)) {
                if closure.insert(edge.child) {
                    queue.push(edge.child);
                }
            }
        }
        let removed = self
            .nodes
            .iter()
            .filter(|node| closure.contains(&node.transform_id))
            .map(|node| node.id)
            .collect();
        self.nodes
            .retain(|node| !closure.contains(&node.transform_id));
        self.dag.retain(|edge| !closure.contains(&edge.child));
        self.session_joints.prune(&closure);
        Ok(removed)
    }
}

#[cfg(test)]
mod tests;
