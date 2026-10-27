pub(crate) mod admission;
mod appearance;
pub mod ascii;
mod authored;
pub mod binary;
mod curve;
mod manipulator_frame;
mod mesh;
mod move_translation;
mod rotate_frame;
mod scale_frame;
mod session_material;
mod source;
mod transform_extras;
pub use appearance::{
    AppearanceIssue, AppearanceIssues, AppearanceState, AppearanceSummary, MaterialInspection,
    MaterialShader,
};
pub use curve::{CurveForm, NativeCurveShape};
pub use manipulator_frame::ManipulatorFrame;
pub use mesh::{DirectedEdge, NativeEdge, NativeFace, NativeMesh, Points};
pub(crate) use move_translation::ChannelBaseline;
pub use move_translation::{ChannelLeaf, WorldAxis, WorldMoveFrame, WorldMoveUnsupported};
pub use rotate_frame::{RotateAxis, RotateFrame, RotateSpace, RotateUnsupported};
pub use scale_frame::{ScaleFrame, ScaleHandle, ScaleSpace, ScaleUnsupported};
use session_material::SessionMaterials;
pub use session_material::{
    Lambert, MAX_SESSION_MATERIALS, MaterialChoice, SessionMaterialId, ShadingGroup,
};
use source::{ScalarOrigin, SourceFormat, SourceRetention, TransformField};
pub use transform_extras::{InternalExtras, TransformAttributes, TransformExtras};

use crate::primitive_options::PrimitiveOptions;
use crate::scene::{NodeId, Primitive, Scene, SceneError, Transform};
use glam::{DMat3, DMat4, DVec3, Vec3};
use std::fmt;
#[cfg(test)]
use std::ops::Range;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NativeId(u64);

impl NativeId {
    pub fn to_decimal(self) -> String {
        self.0.to_string()
    }
    pub fn from_decimal(value: &str) -> Result<Self, std::num::ParseIntError> {
        value.parse::<u64>().map(Self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinearUnit {
    Centimeter,
    Millimeter,
    Meter,
    Inch,
    Foot,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AngularUnit {
    Degree,
    Radian,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Units {
    pub linear: LinearUnit,
    pub angular: AngularUnit,
    pub time: String,
}

impl Default for Units {
    fn default() -> Self {
        Self {
            linear: LinearUnit::Centimeter,
            angular: AngularUnit::Degree,
            time: "film".into(),
        }
    }
}

impl Units {
    pub fn linear_factor(&self) -> Result<f64, NativeError> {
        Ok(match self.linear {
            LinearUnit::Centimeter => 1.0,
            LinearUnit::Millimeter => 0.1,
            LinearUnit::Meter => 100.0,
            LinearUnit::Inch => 2.54,
            LinearUnit::Foot => 30.48,
            LinearUnit::Unsupported(_) => return Err(NativeError::UnsupportedLinearUnit),
        })
    }
    /// Degrees in one declared angular unit.
    pub fn angular_degrees_factor(&self) -> Result<f64, NativeError> {
        match self.angular {
            AngularUnit::Degree => Ok(1.0),
            AngularUnit::Radian => Ok(180.0 / std::f64::consts::PI),
            AngularUnit::Unsupported(_) => Err(NativeError::UnsupportedView("angular unit")),
        }
    }
    pub(crate) fn factors(&self) -> Result<(f64, f64), NativeError> {
        let linear = self
            .linear_factor()
            .map_err(|_| NativeError::UnsupportedView("linear unit"))?;
        Ok((linear, self.angular_degrees_factor()?))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NativeTransform {
    pub translation: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}

impl Default for NativeTransform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl PartialEq for NativeTransform {
    fn eq(&self, other: &Self) -> bool {
        self.values().map(f64::to_bits) == other.values().map(f64::to_bits)
    }
}

impl NativeTransform {
    pub fn values(self) -> [f64; 9] {
        let [x, y, z] = self.translation;
        let [rx, ry, rz] = self.rotation;
        let [sx, sy, sz] = self.scale;
        [x, y, z, rx, ry, rz, sx, sy, sz]
    }
    fn from_values(v: [f64; 9]) -> Self {
        Self {
            translation: [v[0], v[1], v[2]],
            rotation: [v[3], v[4], v[5]],
            scale: [v[6], v[7], v[8]],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Channel {
    TranslateX,
    TranslateY,
    TranslateZ,
    RotateX,
    RotateY,
    RotateZ,
    ScaleX,
    ScaleY,
    ScaleZ,
}

#[derive(Clone)]
struct Scalar {
    value: f64,
    origin: Option<ScalarOrigin>,
    replaced: bool,
}

impl PartialEq for Scalar {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
            && self.origin == other.origin
            && self.replaced == other.replaced
    }
}

impl Scalar {
    fn new(value: f64) -> Self {
        Self {
            value,
            origin: None,
            replaced: false,
        }
    }
    fn replacement(
        &self,
        value: f64,
        source: Option<&SourceRetention>,
        units: &Units,
        owner: NativeId,
        field: TransformField,
        component: usize,
    ) -> Result<Self, NativeError> {
        if matches!(self.origin, Some(ScalarOrigin::Binary { .. })) {
            if value.to_bits() == self.value.to_bits() {
                return Ok(self.clone());
            }
            let source = source.ok_or(NativeError::InvalidRetention)?;
            let original = source.original(
                self.origin.as_ref().ok_or(NativeError::InvalidRetention)?,
                owner,
                field,
                component,
            )?;
            let word = field.storage().internal(value, units)?;
            if word.to_bits()
                == self
                    .internal(Some(source), units, owner, field, component)?
                    .to_bits()
            {
                return Ok(self.clone());
            }
            return Ok(Self {
                value: if word.to_bits() == original.to_bits() {
                    field.storage().declared(original, units)?
                } else {
                    value
                },
                origin: self.origin.clone(),
                replaced: word.to_bits() != original.to_bits(),
            });
        }
        if self.origin.is_none()
            && source.is_some_and(|source| source.format == SourceFormat::Binary)
            && value.to_bits() != self.value.to_bits()
            && !source.is_some_and(|source| source.can_insert(owner, field))
        {
            return Err(NativeError::UnsupportedEdit);
        }
        let mut next = self.clone();
        if self.value.to_bits() != value.to_bits() || self.origin.is_some() {
            next.value = value;
            next.replaced = true;
        }
        Ok(next)
    }
    fn internal(
        &self,
        source: Option<&SourceRetention>,
        units: &Units,
        owner: NativeId,
        field: TransformField,
        component: usize,
    ) -> Result<f64, NativeError> {
        if let Some(origin @ ScalarOrigin::Binary { .. }) = &self.origin {
            let source = source.ok_or(NativeError::InvalidRetention)?;
            let original = source.original(origin, owner, field, component)?;
            if !self.replaced {
                return Ok(original);
            }
        }
        field.storage().internal(self.value, units)
    }
    fn validate_origin(
        &self,
        source: Option<&SourceRetention>,
        units: &Units,
        owner: NativeId,
        field: TransformField,
        component: usize,
    ) -> Result<(), NativeError> {
        match (&self.origin, source) {
            (None, Some(source))
                if source.format == SourceFormat::Binary
                    && self.replaced
                    && !source.can_insert(owner, field) =>
            {
                Err(NativeError::InvalidRetention)
            }
            (None, _) => Ok(()),
            #[cfg(test)]
            (Some(ScalarOrigin::Ascii { literal }), Some(source))
                if source.format == SourceFormat::Ascii
                    && literal.start < literal.end
                    && source.bytes.get(literal.clone()).is_some() =>
            {
                Ok(())
            }
            (Some(origin @ ScalarOrigin::Binary { .. }), Some(source)) => {
                let original = source.original(origin, owner, field, component)?;
                let consistent = if self.replaced {
                    field.storage().internal(self.value, units)?.to_bits() != original.to_bits()
                } else {
                    self.value.to_bits() == field.storage().declared(original, units)?.to_bits()
                };
                if consistent {
                    Ok(())
                } else {
                    Err(NativeError::InvalidRetention)
                }
            }
            _ => Err(NativeError::InvalidRetention),
        }
    }
}

impl fmt::Debug for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scalar")
            .field("origin", &self.origin)
            .field("replaced", &self.replaced)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct StaticGeometry;

#[derive(Clone, Debug, PartialEq)]
struct GeometryViewFacts {
    geometry: Option<StaticGeometry>,
    local_visibility: Option<bool>,
}

impl GeometryViewFacts {
    fn authored() -> Self {
        Self {
            geometry: Some(StaticGeometry),
            local_visibility: Some(true),
        }
    }
    fn visible(&self) -> Result<bool, NativeError> {
        self.geometry
            .as_ref()
            .ok_or(NativeError::UnsupportedView("unproved geometry evaluation"))?;
        self.local_visibility
            .ok_or(NativeError::UnsupportedView("unproved local visibility"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GeometryViewAdmission {
    visible: bool,
    appearance_incomplete: bool,
}

impl GeometryViewAdmission {
    pub fn visible(&self) -> bool {
        self.visible
    }
    pub fn appearance_incomplete(&self) -> bool {
        self.appearance_incomplete
    }
}

#[derive(Clone, PartialEq)]
pub struct NativeTransformNode {
    id: NodeId,
    transform_id: NativeId,
    transform_uuid: Option<Uuid>,
    name: String,
    channels: [Scalar; 9],
    rotate_pivot: [Scalar; 3],
    scale_pivot: [Scalar; 3],
    restrictions: Vec<String>,
    unsupported_transform: Vec<String>,
    extras: TransformExtras,
    view: GeometryViewFacts,
}

impl NativeTransformNode {
    fn field(&self, field: TransformField) -> &[Scalar] {
        match field {
            TransformField::Translation => &self.channels[..3],
            TransformField::Rotation => &self.channels[3..6],
            TransformField::Scale => &self.channels[6..],
            TransformField::RotatePivot => &self.rotate_pivot,
            TransformField::ScalePivot => &self.scale_pivot,
        }
    }
    pub fn id(&self) -> NodeId {
        self.id
    }
    pub fn transform_id(&self) -> NativeId {
        self.transform_id
    }
    pub fn transform_uuid(&self) -> Option<Uuid> {
        self.transform_uuid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn rotate_pivot(&self) -> [f64; 3] {
        self.rotate_pivot.each_ref().map(|v| v.value)
    }
    pub fn scale_pivot(&self) -> [f64; 3] {
        self.scale_pivot.each_ref().map(|v| v.value)
    }
    pub fn unsupported_transform_effects(&self) -> &[String] {
        &self.unsupported_transform
    }
    pub fn extras(&self) -> &TransformExtras {
        &self.extras
    }
    pub fn edit_restrictions(&self) -> &[String] {
        &self.restrictions
    }
    pub fn transform(&self) -> NativeTransform {
        NativeTransform::from_values(self.channels.each_ref().map(|v| v.value))
    }
    /// This node's own `.v`; `None` when the source did not prove it. Not the effective visibility.
    pub fn local_visibility(&self) -> Option<bool> {
        self.view.local_visibility
    }
}

impl fmt::Debug for NativeTransformNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTransformNode")
            .field("id", &self.id)
            .field("native_id", &self.transform_id)
            .finish()
    }
}

#[derive(Clone, PartialEq)]
pub struct NativeMeshShape {
    id: NativeId,
    uuid: Option<Uuid>,
    name: String,
    mesh: NativeMesh,
    restrictions: Vec<String>,
    view: GeometryViewFacts,
}

impl fmt::Debug for NativeMeshShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeMeshShape")
            .field("id", &self.id)
            .field("edge_count", &self.mesh.edges().len())
            .field("face_count", &self.mesh.faces().len())
            .finish()
    }
}

impl NativeMeshShape {
    /// This shape's own `.v`; `None` when the source did not prove it.
    pub fn local_visibility(&self) -> Option<bool> {
        self.view.local_visibility
    }
    pub fn id(&self) -> NativeId {
        self.id
    }
    pub fn uuid(&self) -> Option<Uuid> {
        self.uuid
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mesh(&self) -> &NativeMesh {
        &self.mesh
    }
    pub fn edit_restrictions(&self) -> &[String] {
        &self.restrictions
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DagEdge {
    parent: Option<NativeId>,
    child: NativeId,
}

impl DagEdge {
    pub fn parent(&self) -> Option<NativeId> {
        self.parent
    }
    pub fn child(&self) -> NativeId {
        self.child
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceTarget {
    pub transform: NodeId,
    pub shape: NativeId,
    pub path: Vec<NativeId>,
    pub face_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VertexTarget {
    pub transform: NodeId,
    pub shape: NativeId,
    pub path: Vec<NativeId>,
    pub vertex_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeTarget {
    pub transform: NodeId,
    pub shape: NativeId,
    pub path: Vec<NativeId>,
    pub edge_index: usize,
}

pub const MAX_TRANSFORM_PATHS: usize = 4096;
pub const MAX_TRANSFORM_DEPTH: usize = 256;
pub const MAX_PATH_IDENTITIES: usize = 65536;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TransformTarget {
    pub node: NodeId,
    pub path: Vec<NativeId>,
}

#[derive(Clone)]
pub struct NativeScene {
    nodes: Vec<NativeTransformNode>,
    shapes: Vec<NativeMeshShape>,
    curves: Vec<NativeCurveShape>,
    dag: Vec<DagEdge>,
    units: Units,
    source: Option<SourceRetention>,
    materials: Vec<appearance::MaterialDiffuse>,
    session_materials: SessionMaterials,
    next_id: u64,
    next_native_id: u64,
}

impl fmt::Debug for NativeScene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeScene")
            .field("transform_count", &self.nodes.len())
            .field("shape_count", &self.shapes.len())
            .field("curve_count", &self.curves.len())
            .field("dag_edge_count", &self.dag.len())
            .field("material_count", &self.materials.len())
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InternalTransform {
    pub translation_cm: [f64; 3],
    pub rotation_radians: [f64; 3],
    pub scale: [f64; 3],
    pub rotate_pivot_cm: [f64; 3],
    pub scale_pivot_cm: [f64; 3],
    /// `None` when every extra is default; the matrix then takes the unchanged path.
    pub extras: Option<InternalExtras>,
}

impl InternalTransform {
    /// T + RT in the parent's space: the Move and Rotate pivot, since both refuse a nonzero
    /// rotate pivot (RP) and `T + RT + RP` is then the same point.
    pub(crate) fn origin_cm(self) -> DVec3 {
        let translate = DVec3::from_array(self.translation_cm);
        match self.extras {
            Some(extras) => translate + DVec3::from_array(extras.rotate_pivot_translate_cm),
            None => translate,
        }
    }

    /// R(ro) * RA as a linear map: the node's own axes before shear and scale.
    /// `None` for a rotate order validation would have rejected.
    pub(crate) fn rotation_frame(self) -> Option<DMat3> {
        let Some(extras) = self.extras else {
            return Some(rotate_frame::rotation_matrix(self.rotation_radians));
        };
        let order = transform_extras::rotation_matrix(extras.rotate_order, self.rotation_radians)?;
        Some(DMat3::from_mat4(order * extras.rotate_axis_matrix()))
    }

    pub(crate) fn matrix(self) -> Option<DMat4> {
        if let Some(extras) = self.extras {
            let matrix = extras.matrix(
                DVec3::from_array(self.translation_cm),
                self.rotation_radians,
                DVec3::from_array(self.scale),
                DVec3::from_array(self.rotate_pivot_cm),
                DVec3::from_array(self.scale_pivot_cm),
            )?;
            return matrix.is_finite().then_some(matrix);
        }
        let translate = DVec3::from_array(self.translation_cm);
        let rp = DVec3::from_array(self.rotate_pivot_cm);
        let sp = DVec3::from_array(self.scale_pivot_cm);
        let rotation = DVec3::from_array(self.rotation_radians);
        let matrix = DMat4::from_translation(translate)
            * DMat4::from_translation(rp)
            * DMat4::from_rotation_z(rotation.z)
            * DMat4::from_rotation_y(rotation.y)
            * DMat4::from_rotation_x(rotation.x)
            * DMat4::from_translation(-rp)
            * DMat4::from_translation(sp)
            * DMat4::from_scale(DVec3::from_array(self.scale))
            * DMat4::from_translation(-sp);
        matrix.is_finite().then_some(matrix)
    }
}

impl PartialEq for NativeScene {
    fn eq(&self, other: &Self) -> bool {
        self.nodes == other.nodes
            && self.shapes == other.shapes
            && self.curves == other.curves
            && self.dag == other.dag
            && self.units == other.units
            && self.source == other.source
            && self.materials == other.materials
            && self.session_materials == other.session_materials
    }
}

impl Default for NativeScene {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            shapes: Vec::new(),
            curves: Vec::new(),
            dag: Vec::new(),
            units: Units::default(),
            source: None,
            materials: Vec::new(),
            session_materials: SessionMaterials::default(),
            next_id: 1,
            next_native_id: 1,
        }
    }
}

impl NativeScene {
    #[cfg(any(unix, windows))]
    pub(crate) fn has_binary_source(&self, bytes: &std::sync::Arc<[u8]>) -> bool {
        self.source.as_ref().is_some_and(|source| {
            source.format == SourceFormat::Binary && std::sync::Arc::ptr_eq(&source.bytes, bytes)
        })
    }
    fn from_binary_candidate(candidate: Self) -> Result<Self, NativeError> {
        if candidate
            .source
            .as_ref()
            .is_none_or(|s| s.format != SourceFormat::Binary)
            || candidate.nodes.iter().any(|n| n.transform_uuid.is_some())
            || candidate.shapes.iter().any(|s| s.uuid.is_some())
            || !candidate.curves.is_empty()
        {
            return Err(NativeError::InvalidRetention);
        }
        Self::from_source_candidate(candidate)
    }
    fn from_source_candidate(mut candidate: Self) -> Result<Self, NativeError> {
        candidate.next_id = candidate
            .nodes
            .iter()
            .map(|n| n.id.value())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(NativeError::InvalidIdentity)?;
        candidate.next_native_id = candidate
            .nodes
            .iter()
            .map(|n| n.transform_id.0)
            .chain(candidate.shapes.iter().map(|s| s.id.0))
            .chain(candidate.curves.iter().map(|s| s.id.0))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(NativeError::InvalidIdentity)?;
        candidate.validate()?;
        Ok(candidate)
    }
    pub fn from_legacy(scene: Scene) -> Result<Self, NativeError> {
        scene.validate()?;
        let mut native = Self {
            next_id: scene.next_id(),
            ..Self::default()
        };
        for node in scene.nodes() {
            let t = node.transform();
            let transform = NativeTransform {
                translation: t.translation.as_dvec3().to_array(),
                rotation: t.rotation_degrees.as_dvec3().to_array(),
                scale: t.scale.as_dvec3().to_array(),
            };
            native.append(
                node.id(),
                node.name(),
                format!("{}Shape", node.name()),
                NativeMesh::from_legacy(node.mesh()),
                transform,
            )?;
        }
        native.validate()?;
        Ok(native)
    }

    pub fn nodes(&self) -> &[NativeTransformNode] {
        &self.nodes
    }
    pub fn units(&self) -> &Units {
        &self.units
    }
    pub fn appearance_incomplete(&self) -> bool {
        self.source.is_some()
    }
    pub fn node(&self, id: NodeId) -> Result<&NativeTransformNode, NativeError> {
        self.nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or(NativeError::Legacy(SceneError::MissingNode(id)))
    }
    fn transform_prefix(&self, path: &[NativeId]) -> Result<(), NativeError> {
        if path.is_empty()
            || !self
                .dag
                .iter()
                .any(|edge| edge.parent.is_none() && edge.child == path[0])
            || path.iter().enumerate().any(|(index, id)| {
                path[..index].contains(id)
                    || !self.nodes.iter().any(|node| node.transform_id == *id)
            })
            || path.windows(2).any(|pair| {
                !self
                    .dag
                    .iter()
                    .any(|edge| edge.parent == Some(pair[0]) && edge.child == pair[1])
            })
        {
            return Err(NativeError::InvalidInstancePath);
        }
        Ok(())
    }
    pub fn resolve_transform_path(
        &self,
        id: NodeId,
        path: &[NativeId],
    ) -> Result<&NativeTransformNode, NativeError> {
        self.transform_prefix(path)?;
        let node = self.node(id)?;
        if path.last() != Some(&node.transform_id) {
            return Err(NativeError::InvalidInstancePath);
        }
        Ok(node)
    }
    /// The root transform of the hierarchy containing `path`, for Hierarchy-mode picking.
    pub fn root_transform(&self, path: &[NativeId]) -> Result<NodeId, NativeError> {
        if path.len() > MAX_TRANSFORM_DEPTH {
            return Err(NativeError::ResourceLimit);
        }
        self.transform_prefix(path)?;
        self.nodes
            .iter()
            .find(|node| node.transform_id == path[0])
            .map(|node| node.id)
            .ok_or(NativeError::InvalidInstancePath)
    }
    pub fn transform_paths(&self) -> Result<Vec<TransformTarget>, NativeError> {
        let mut paths = Vec::<TransformTarget>::new();
        for root in self.dag.iter().filter(|edge| edge.parent.is_none()) {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == root.child)
                .ok_or(NativeError::InvalidInstancePath)?;
            if paths.len() == MAX_TRANSFORM_PATHS {
                return Err(NativeError::ResourceLimit);
            }
            paths
                .try_reserve(1)
                .map_err(|_| NativeError::ResourceLimit)?;
            let mut path = Vec::new();
            path.try_reserve_exact(1)
                .map_err(|_| NativeError::ResourceLimit)?;
            path.push(root.child);
            paths.push(TransformTarget {
                node: node.id,
                path,
            });
        }
        let mut identities = paths.len();
        let mut cursor = 0;
        while cursor < paths.len() {
            let parent = *paths[cursor]
                .path
                .last()
                .ok_or(NativeError::InvalidInstancePath)?;
            for edge in self.dag.iter().filter(|edge| edge.parent == Some(parent)) {
                let Some(node) = self
                    .nodes
                    .iter()
                    .find(|node| node.transform_id == edge.child)
                else {
                    continue;
                };
                let depth = paths[cursor]
                    .path
                    .len()
                    .checked_add(1)
                    .ok_or(NativeError::ResourceLimit)?;
                identities = identities
                    .checked_add(depth)
                    .ok_or(NativeError::ResourceLimit)?;
                if depth > MAX_TRANSFORM_DEPTH
                    || paths.len() == MAX_TRANSFORM_PATHS
                    || identities > MAX_PATH_IDENTITIES
                {
                    return Err(NativeError::ResourceLimit);
                }
                let mut path = Vec::new();
                path.try_reserve_exact(depth)
                    .map_err(|_| NativeError::ResourceLimit)?;
                path.extend_from_slice(&paths[cursor].path);
                path.push(edge.child);
                paths
                    .try_reserve(1)
                    .map_err(|_| NativeError::ResourceLimit)?;
                paths.push(TransformTarget {
                    node: node.id,
                    path,
                });
            }
            cursor += 1;
        }
        paths.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        Ok(paths)
    }
    /// Every transform instance strictly below one of `roots`, in `transform_paths` order. A root
    /// is not its own descendant; an instance below two roots appears once.
    pub fn descendant_targets(
        &self,
        roots: &[TransformTarget],
    ) -> Result<Vec<TransformTarget>, NativeError> {
        let root_paths: std::collections::HashSet<&[NativeId]> =
            roots.iter().map(|root| root.path.as_slice()).collect();
        Ok(self
            .transform_paths()?
            .into_iter()
            .filter(|target| {
                (1..target.path.len()).any(|len| root_paths.contains(&target.path[..len]))
            })
            .collect())
    }
    pub fn internal_transform(&self, id: NodeId) -> Result<InternalTransform, NativeError> {
        let node = self.node(id)?;
        let vector = |field| {
            let mut values = [0.0; 3];
            for (component, scalar) in node.field(field).iter().enumerate() {
                values[component] = scalar.internal(
                    self.source.as_ref(),
                    &self.units,
                    node.transform_id,
                    field,
                    component,
                )?;
            }
            Ok::<_, NativeError>(values)
        };
        Ok(InternalTransform {
            translation_cm: vector(TransformField::Translation)?,
            rotation_radians: vector(TransformField::Rotation)?,
            scale: vector(TransformField::Scale)?,
            rotate_pivot_cm: vector(TransformField::RotatePivot)?,
            scale_pivot_cm: vector(TransformField::ScalePivot)?,
            extras: node.extras.internal(&self.units)?,
        })
    }
    pub fn shapes(&self) -> &[NativeMeshShape] {
        &self.shapes
    }
    pub fn curves(&self) -> &[NativeCurveShape] {
        &self.curves
    }
    pub fn shape(&self, id: NativeId) -> Result<&NativeMeshShape, NativeError> {
        self.shapes
            .iter()
            .find(|shape| shape.id == id)
            .ok_or(NativeError::MissingShape(id))
    }
    pub fn dag_edges(&self) -> &[DagEdge] {
        &self.dag
    }
    fn resolve_shape_instance(
        &self,
        transform: NodeId,
        shape_id: NativeId,
        path: &[NativeId],
    ) -> Result<&NativeMeshShape, NativeError> {
        let node = self.node(transform)?;
        self.shape(shape_id)?;
        let shape = self.instance_shape(path)?;
        if shape.id != shape_id || path[path.len() - 2] != node.transform_id {
            return Err(NativeError::InvalidInstancePath);
        }
        Ok(shape)
    }
    pub fn resolve_face_target(
        &self,
        target: &FaceTarget,
    ) -> Result<&NativeMeshShape, NativeError> {
        let shape = self.resolve_shape_instance(target.transform, target.shape, &target.path)?;
        if target.face_index >= shape.mesh.faces().len() {
            return Err(SceneError::MissingFace(target.face_index).into());
        }
        Ok(shape)
    }
    pub fn resolve_vertex_target(
        &self,
        target: &VertexTarget,
    ) -> Result<&NativeMeshShape, NativeError> {
        let shape = self.resolve_shape_instance(target.transform, target.shape, &target.path)?;
        if target.vertex_index >= shape.mesh.point_count() {
            return Err(SceneError::MissingVertex(target.vertex_index).into());
        }
        Ok(shape)
    }
    pub fn resolve_edge_target(
        &self,
        target: &EdgeTarget,
    ) -> Result<&NativeMeshShape, NativeError> {
        let shape = self.resolve_shape_instance(target.transform, target.shape, &target.path)?;
        if target.edge_index >= shape.mesh.edges().len() {
            return Err(SceneError::MissingEdge(target.edge_index).into());
        }
        Ok(shape)
    }
    fn instance_shape(&self, path: &[NativeId]) -> Result<&NativeMeshShape, NativeError> {
        let id = self.instance_geometry_id(path)?;
        self.shapes
            .iter()
            .find(|shape| shape.id == id)
            .ok_or(NativeError::InvalidInstancePath)
    }
    fn instance_geometry_id(&self, path: &[NativeId]) -> Result<NativeId, NativeError> {
        // Construction and mutation validate the opaque scene; per-instance queries must stay bounded.
        if path.len() < 2 {
            return Err(NativeError::InvalidInstancePath);
        }
        self.transform_prefix(&path[..path.len() - 1])?;
        if path[..path.len() - 1].contains(&path[path.len() - 1])
            || !self.dag.contains(&DagEdge {
                parent: Some(path[path.len() - 2]),
                child: path[path.len() - 1],
            })
        {
            return Err(NativeError::InvalidInstancePath);
        }
        Ok(path[path.len() - 1])
    }
    pub fn geometry_view_admission(
        &self,
        path: &[NativeId],
    ) -> Result<GeometryViewAdmission, NativeError> {
        let id = self.instance_geometry_id(path)?;
        let facts = self
            .shapes
            .iter()
            .find(|shape| shape.id == id)
            .map(|shape| &shape.view)
            .or_else(|| {
                self.curves
                    .iter()
                    .find(|curve| curve.id == id)
                    .map(|curve| &curve.view)
            })
            .ok_or(NativeError::InvalidInstancePath)?;
        let mut visible = facts.visible()?;
        for id in &path[..path.len() - 1] {
            let node = self
                .nodes
                .iter()
                .find(|node| node.transform_id == *id)
                .ok_or(NativeError::InvalidInstancePath)?;
            if !node.unsupported_transform.is_empty() {
                return Err(NativeError::UnsupportedView("unproved transform effects"));
            }
            visible &= node.view.visible()?;
        }
        Ok(GeometryViewAdmission {
            visible,
            appearance_incomplete: self.appearance_incomplete(),
        })
    }
    pub fn restore_authoring(&mut self, mut snapshot: Self) {
        snapshot.next_id = snapshot.next_id.max(self.next_id);
        snapshot.next_native_id = snapshot.next_native_id.max(self.next_native_id);
        *self = snapshot;
    }

    pub fn create(&mut self, primitive: Primitive, name: &str) -> Result<NodeId, NativeError> {
        self.create_with_options(primitive, &PrimitiveOptions::default_for(primitive), name)
    }

    /// Validates `options` against `primitive` before any mutation. An explicit name gives the
    /// shape `<name>Shape`, as Maya's `polyCube -n` does.
    pub fn create_with_options(
        &mut self,
        primitive: Primitive,
        options: &PrimitiveOptions,
        name: &str,
    ) -> Result<NodeId, NativeError> {
        self.create_named(primitive, options, name, format!("{name}Shape"))
    }

    /// Creates with Maya's default names: the transform takes the lowest free `pCube<N>` and
    /// the shape, numbered separately, the lowest free `pCubeShape<M>` (#175). A name is free
    /// when no transform or shape has it, because authored saves need unique names.
    pub fn create_default_named(
        &mut self,
        primitive: Primitive,
        options: &PrimitiveOptions,
    ) -> Result<NodeId, NativeError> {
        let base = match primitive {
            Primitive::Cube => "pCube",
            Primitive::Plane => "pPlane",
            Primitive::Sphere => "pSphere",
            Primitive::Cylinder => "pCylinder",
            Primitive::Cone => "pCone",
        };
        let count = authored::names(self).count();
        let free = |prefix: &str| {
            (1..=count + 1)
                .map(|suffix| format!("{prefix}{suffix}"))
                .find(|name| !authored::names(self).any(|(_, taken)| taken == name))
                .expect("count names cannot take count + 1 suffixes")
        };
        let name = free(base);
        let shape_name = free(&format!("{base}Shape"));
        self.create_named(primitive, options, &name, shape_name)
    }

    fn create_named(
        &mut self,
        primitive: Primitive,
        options: &PrimitiveOptions,
        name: &str,
        shape_name: String,
    ) -> Result<NodeId, NativeError> {
        self.structural_edit()?;
        // The primitive/options pairing is the one check NativeMesh::with_options does not repeat.
        options
            .validate(primitive)
            .map_err(SceneError::InvalidPrimitiveOptions)?;
        self.validate_new_name(None, name)?;
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(SceneError::IdsExhausted)?;
        let id = NodeId::new(self.next_id);
        self.append(
            id,
            name,
            shape_name,
            NativeMesh::with_options(options)?,
            NativeTransform::default(),
        )?;
        self.next_id = next;
        Ok(id)
    }

    fn append(
        &mut self,
        id: NodeId,
        name: &str,
        shape_name: String,
        mesh: NativeMesh,
        transform: NativeTransform,
    ) -> Result<(), NativeError> {
        let next = self
            .next_native_id
            .checked_add(2)
            .ok_or(SceneError::IdsExhausted)?;
        let transform_id = NativeId(self.next_native_id);
        let shape_id = NativeId(self.next_native_id + 1);
        self.nodes.push(NativeTransformNode {
            id,
            transform_id,
            transform_uuid: Some(Uuid::new_v4()),
            name: name.into(),
            channels: transform.values().map(Scalar::new),
            rotate_pivot: [0.0; 3].map(Scalar::new),
            scale_pivot: [0.0; 3].map(Scalar::new),
            restrictions: Vec::new(),
            unsupported_transform: Vec::new(),
            extras: TransformExtras::default(),
            view: GeometryViewFacts::authored(),
        });
        self.shapes.push(NativeMeshShape {
            id: shape_id,
            uuid: Some(Uuid::new_v4()),
            name: shape_name,
            mesh,
            restrictions: Vec::new(),
            view: GeometryViewFacts::authored(),
        });
        self.dag.extend([
            DagEdge {
                parent: None,
                child: transform_id,
            },
            DagEdge {
                parent: Some(transform_id),
                child: shape_id,
            },
        ]);
        self.next_native_id = next;
        Ok(())
    }

    fn editable(&self, id: NodeId) -> Result<usize, NativeError> {
        self.authoring_preflight()?;
        self.validate()?;
        let index = self
            .nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or(SceneError::MissingNode(id))?;
        self.check_effects(self.nodes[index].transform_id)?;
        Ok(index)
    }
    fn structural_edit(&self) -> Result<(), NativeError> {
        self.authoring_preflight()?;
        self.validate()?;
        if self.source.is_some() {
            return Err(NativeError::UnsupportedEdit);
        }
        Ok(())
    }
    fn authoring_preflight(&self) -> Result<(), NativeError> {
        if let Some(reason) = self.authoring_read_only_reason() {
            return Err(reason);
        }
        Ok(())
    }
    pub fn authoring_read_only_reason(&self) -> Option<NativeError> {
        self.source
            .as_ref()
            .is_some_and(|source| source.format == SourceFormat::Ascii)
            .then_some(NativeError::ReadOnlyFormat)
    }
    pub fn retained_source_bytes(&self) -> usize {
        self.source.as_ref().map_or(0, |source| source.bytes.len())
    }
    pub fn retained_source_records(&self) -> usize {
        self.source
            .as_ref()
            .map_or(0, |source| source.records.len())
    }
    fn check_effects(&self, target: NativeId) -> Result<(), NativeError> {
        let mut descendants = vec![target];
        let mut ancestors = vec![target];
        let mut index = 0;
        while index < descendants.len() {
            let current = descendants[index];
            for edge in self.dag.iter().filter(|edge| edge.parent == Some(current)) {
                if !descendants.contains(&edge.child) {
                    descendants.push(edge.child);
                }
            }
            index += 1;
        }
        index = 0;
        while index < ancestors.len() {
            let current = ancestors[index];
            for edge in self.dag.iter().filter(|edge| edge.child == current) {
                if let Some(parent) = edge.parent
                    && !ancestors.contains(&parent)
                {
                    ancestors.push(parent);
                }
            }
            index += 1;
        }
        for id in descendants.iter().chain(&ancestors) {
            if self.nodes.iter().any(|node| {
                node.transform_id == *id
                    && (!node.restrictions.is_empty() || !node.unsupported_transform.is_empty())
            }) || self
                .shapes
                .iter()
                .any(|shape| shape.id == *id && !shape.restrictions.is_empty())
            {
                return Err(NativeError::UnsupportedEdit);
            }
        }
        Ok(())
    }
    fn supported_shape(&self, node: &NativeTransformNode) -> Result<&NativeMeshShape, NativeError> {
        let incoming: Vec<_> = self
            .dag
            .iter()
            .filter(|edge| edge.child == node.transform_id)
            .collect();
        let children: Vec<_> = self
            .dag
            .iter()
            .filter(|edge| edge.parent == Some(node.transform_id))
            .collect();
        if incoming.len() != 1 || incoming[0].parent.is_some() || children.len() != 1 {
            return Err(NativeError::UnsupportedView("DAG hierarchy or instances"));
        }
        let shape = self
            .shape(children[0].child)
            .map_err(|_| NativeError::UnsupportedView("transform-only group"))?;
        if self
            .dag
            .iter()
            .filter(|edge| edge.child == shape.id)
            .count()
            != 1
        {
            return Err(NativeError::UnsupportedView("shared mesh instances"));
        }
        Ok(shape)
    }
    fn validate_new_name(&self, id: Option<NodeId>, name: &str) -> Result<(), NativeError> {
        let mut chars = name.bytes();
        if !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
            || chars.any(|c| !c.is_ascii_alphanumeric() && c != b'_')
        {
            return Err(NativeError::InvalidName);
        }
        if self
            .nodes
            .iter()
            .any(|n| Some(n.id) != id && n.name == name)
        {
            return Err(NativeError::NameCollision);
        }
        Ok(())
    }
    pub fn rename(&mut self, id: NodeId, name: &str) -> Result<(), NativeError> {
        self.structural_edit()?;
        let index = self.editable(id)?;
        self.validate_new_name(Some(id), name)?;
        self.set_transform_name(index, name);
        Ok(())
    }
    /// Renames the transform and, as Maya's `rename` does, each direct shape whose name is
    /// still derived from it: `<old>Shape` becomes `<new>Shape`, and `<oldBase>Shape<digits>`
    /// becomes `<newBase>Shape<newDigits>`, where a base is the name without trailing digits.
    /// Other shape names are kept (#175). A derived name that another transform or shape
    /// already has is not taken either: Maya allows that duplicate under another parent, but
    /// authored saves need unique names.
    fn set_transform_name(&mut self, index: usize, name: &str) {
        let old = std::mem::replace(&mut self.nodes[index].name, name.into());
        if old == name {
            return;
        }
        let transform = self.nodes[index].transform_id;
        let children: Vec<_> = self
            .dag
            .iter()
            .filter(|edge| edge.parent == Some(transform))
            .map(|edge| edge.child)
            .collect();
        let renames: Vec<_> = self
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| children.contains(&shape.id))
            .filter_map(|(at, shape)| Some((at, derived_shape_name(&shape.name, &old, name)?)))
            .collect();
        for (at, renamed) in renames {
            if !authored::names(self).any(|(_, taken)| taken == renamed) {
                self.shapes[at].name = renamed;
            }
        }
    }
    pub fn remove(&mut self, id: NodeId) -> Result<(), NativeError> {
        self.authoring_preflight()?;
        self.validate()?;
        self.remove_validated(id)
    }
    /// Removes every listed node, in order, after one preflight and one validation; a repeated id
    /// is removed once. The first refusal is returned unchanged and earlier removals stay applied,
    /// so callers run it on a candidate clone, as `History::execute` does.
    pub fn remove_many(&mut self, ids: &[NodeId]) -> Result<(), NativeError> {
        self.authoring_preflight()?;
        self.validate()?;
        let mut seen = std::collections::HashSet::new();
        for id in ids {
            if seen.insert(*id) {
                self.remove_validated(*id)?;
            }
        }
        Ok(())
    }
    fn remove_validated(&mut self, id: NodeId) -> Result<(), NativeError> {
        let index = self
            .nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or(SceneError::MissingNode(id))?;
        let transform = self.nodes[index].transform_id;
        let has_children = self.dag.iter().any(|edge| {
            edge.parent == Some(transform)
                && self
                    .nodes
                    .iter()
                    .any(|node| node.transform_id == edge.child)
        });
        if has_children {
            return Err(NativeError::UnsupportedDelete(
                DeleteUnsupported::HasChildren,
            ));
        }
        let incoming: Vec<_> = self
            .dag
            .iter()
            .filter(|edge| edge.child == transform)
            .collect();
        if incoming.iter().any(|edge| edge.parent.is_some()) {
            return Err(NativeError::UnsupportedDelete(DeleteUnsupported::IsChild));
        }
        self.check_effects(transform)?;
        if self.source.is_some() || incoming.len() != 1 {
            return Err(NativeError::UnsupportedEdit);
        }
        let exclusive: Vec<_> = self
            .dag
            .iter()
            .filter(|edge| edge.parent == Some(transform))
            .filter(|edge| {
                self.dag
                    .iter()
                    .filter(|link| link.child == edge.child)
                    .count()
                    == 1
            })
            .map(|edge| edge.child)
            .collect();
        let node = self.nodes.remove(index);
        self.shapes.retain(|shape| !exclusive.contains(&shape.id));
        self.session_materials.prune(&self.shapes);
        self.dag.retain(|edge| {
            edge.child != node.transform_id && edge.parent != Some(node.transform_id)
        });
        Ok(())
    }
    pub fn set_transform(
        &mut self,
        id: NodeId,
        transform: NativeTransform,
    ) -> Result<(), NativeError> {
        self.edit_node(id, None, transform.values().map(Some))
    }
    pub fn set_channel(
        &mut self,
        id: NodeId,
        channel: Channel,
        value: f64,
    ) -> Result<(), NativeError> {
        let mut channels = [None; 9];
        channels[channel as usize] = Some(value);
        self.edit_node(id, None, channels)
    }
    pub fn edit_node(
        &mut self,
        id: NodeId,
        name: Option<&str>,
        channels: [Option<f64>; 9],
    ) -> Result<(), NativeError> {
        if channels.iter().flatten().any(|value| !value.is_finite()) {
            return Err(NativeError::NonFinite);
        }
        let index = self.editable(id)?;
        let name = name.filter(|name| *name != self.nodes[index].name);
        if let Some(name) = name {
            if self.source.is_some() {
                return Err(NativeError::UnsupportedEdit);
            }
            self.validate_new_name(Some(id), name)?;
        }
        let node = &self.nodes[index];
        let mut replacement = node.channels.clone();
        for (slot, value) in channels.into_iter().enumerate() {
            if let Some(value) = value {
                let field = [
                    TransformField::Translation,
                    TransformField::Rotation,
                    TransformField::Scale,
                ][slot / 3];
                replacement[slot] = replacement[slot].replacement(
                    value,
                    self.source.as_ref(),
                    &self.units,
                    node.transform_id,
                    field,
                    slot % 3,
                )?;
            }
        }
        self.nodes[index].channels = replacement;
        if let Some(name) = name {
            self.set_transform_name(index, name);
        }
        Ok(())
    }
    pub fn extrude_face(
        &mut self,
        id: NativeId,
        face: usize,
        distance: f64,
    ) -> Result<(), NativeError> {
        self.structural_edit()?;
        let index = self
            .shapes
            .iter()
            .position(|shape| shape.id == id)
            .ok_or(NativeError::MissingShape(id))?;
        self.check_effects(id)?;
        let distance = distance * self.units.linear_factor()?;
        let mesh = self.shapes[index].mesh.extruded_face(face, distance)?;
        self.shapes[index].mesh = mesh;
        Ok(())
    }
    pub fn supported_view(&self) -> Result<Scene, NativeError> {
        if !self.curves.is_empty() {
            return Err(NativeError::UnsupportedView("curves in legacy mesh view"));
        }
        let (linear, angular) = self.units.factors()?;
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            if !node.unsupported_transform.is_empty()
                || !node.extras.is_default()
                || !node.restrictions.is_empty()
                || node
                    .rotate_pivot()
                    .into_iter()
                    .chain(node.scale_pivot())
                    .any(|value| value != 0.0)
            {
                return Err(NativeError::UnsupportedView(
                    "DAG, transform or dependencies",
                ));
            }
            let shape = self.supported_shape(node)?;
            if !shape.restrictions.is_empty() {
                return Err(NativeError::UnsupportedView("shape dependencies"));
            }
            let t = node.transform();
            let transform = Transform {
                translation: convert(t.translation, linear)?,
                rotation_degrees: convert(t.rotation, angular)?,
                scale: convert(t.scale, 1.0)?,
            };
            nodes.push((node.id, node.name.clone(), shape.mesh.view(1.0)?, transform));
        }
        Scene::from_view(nodes, self.next_id)
            .map_err(|_| NativeError::UnsupportedView("f32 world coordinates"))
    }
    pub fn validate(&self) -> Result<(), NativeError> {
        self.validate_materials()?;
        self.session_materials
            .validate(&self.shapes, self.source.is_some())?;
        #[cfg(test)]
        FULL_VALIDATIONS.with(|count| count.set(count.get() + 1));
        let mut ids = std::collections::HashSet::new();
        let mut native_ids = std::collections::HashSet::new();
        ids.try_reserve(self.nodes.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        native_ids
            .try_reserve(
                self.nodes
                    .len()
                    .checked_add(self.shapes.len())
                    .and_then(|count| count.checked_add(self.curves.len()))
                    .ok_or(NativeError::ResourceLimit)?,
            )
            .map_err(|_| NativeError::ResourceLimit)?;
        if self.next_id == 0 || self.next_native_id == 0 {
            return Err(NativeError::InvalidIdentity);
        }
        for node in &self.nodes {
            let id = node.id.value();
            if id == 0 || id >= self.next_id || !ids.insert(node.id) {
                return Err(NativeError::InvalidIdentity);
            }
            let native_id = node.transform_id;
            if native_id.0 == 0
                || native_id.0 >= self.next_native_id
                || !native_ids.insert(native_id)
            {
                return Err(NativeError::InvalidIdentity);
            }
            if node
                .channels
                .iter()
                .chain(&node.rotate_pivot)
                .chain(&node.scale_pivot)
                .any(|v| !v.value.is_finite())
            {
                return Err(NativeError::NonFinite);
            }
            node.extras.validate()?;
        }
        for shape in &self.shapes {
            if shape.id.0 == 0 || shape.id.0 >= self.next_native_id || !native_ids.insert(shape.id)
            {
                return Err(NativeError::InvalidIdentity);
            }
            shape.mesh.validate()?;
        }
        for curve in &self.curves {
            if curve.id.0 == 0 || curve.id.0 >= self.next_native_id || !native_ids.insert(curve.id)
            {
                return Err(NativeError::InvalidIdentity);
            }
            curve.validate(self.source.as_ref())?;
        }
        self.validate_dag(&native_ids)?;
        if let Some(source) = &self.source {
            source.validate()?;
            if let Some(appearance) = &source.appearance {
                appearance.validate(self, source)?;
            }
            for (index, leaf) in source.leaves.iter().enumerate() {
                let node = self
                    .nodes
                    .iter()
                    .find(|node| node.transform_id == leaf.owner)
                    .ok_or(NativeError::InvalidRetention)?;
                for (component, scalar) in node.field(leaf.field).iter().enumerate() {
                    if scalar.origin
                        != Some(ScalarOrigin::Binary {
                            leaf: index,
                            component,
                        })
                    {
                        return Err(NativeError::InvalidRetention);
                    }
                }
            }
        }
        for node in &self.nodes {
            for field in [
                TransformField::Translation,
                TransformField::Rotation,
                TransformField::Scale,
                TransformField::RotatePivot,
                TransformField::ScalePivot,
            ] {
                for (component, scalar) in node.field(field).iter().enumerate() {
                    scalar.validate_origin(
                        self.source.as_ref(),
                        &self.units,
                        node.transform_id,
                        field,
                        component,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn validate_dag(
        &self,
        identities: &std::collections::HashSet<NativeId>,
    ) -> Result<(), NativeError> {
        let mut transforms = std::collections::HashSet::new();
        transforms
            .try_reserve(self.nodes.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        transforms.extend(self.nodes.iter().map(|node| node.transform_id));
        let mut edges = std::collections::HashSet::new();
        edges
            .try_reserve(self.dag.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        let mut incoming = std::collections::HashMap::new();
        incoming
            .try_reserve(identities.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        incoming.extend(identities.iter().map(|id| (*id, 0usize)));
        for edge in &self.dag {
            if !identities.contains(&edge.child)
                || !edges.insert((edge.parent, edge.child))
                || edge
                    .parent
                    .is_some_and(|parent| !transforms.contains(&parent))
                || (edge.parent.is_none() && !transforms.contains(&edge.child))
            {
                return Err(NativeError::InvalidIdentity);
            }
            if edge.parent.is_some() {
                *incoming
                    .get_mut(&edge.child)
                    .ok_or(NativeError::InvalidIdentity)? += 1;
            }
        }
        if identities
            .iter()
            .any(|id| !self.dag.iter().any(|edge| edge.child == *id))
        {
            return Err(NativeError::InvalidIdentity);
        }
        let mut pending = std::collections::VecDeque::new();
        pending
            .try_reserve(identities.len())
            .map_err(|_| NativeError::ResourceLimit)?;
        pending.extend(
            incoming
                .iter()
                .filter(|(_, count)| **count == 0)
                .map(|(id, _)| *id),
        );
        let mut visited = 0;
        while let Some(parent) = pending.pop_front() {
            visited += 1;
            for edge in self.dag.iter().filter(|edge| edge.parent == Some(parent)) {
                let count = incoming
                    .get_mut(&edge.child)
                    .ok_or(NativeError::InvalidIdentity)?;
                *count -= 1;
                if *count == 0 {
                    pending.push_back(edge.child);
                }
            }
        }
        if visited != identities.len() {
            return Err(NativeError::InvalidIdentity);
        }
        Ok(())
    }
}

/// The shape name for a shape whose transform is renamed from `old` to `new`, or `None` when
/// the shape name is not derived from `old` and stays as it is. The rule is inferred from
/// Maya 2025.1 `rename` observations (PROVISIONAL, #175).
fn derived_shape_name(shape: &str, old: &str, new: &str) -> Option<String> {
    fn split(name: &str) -> (&str, &str) {
        name.split_at(name.trim_end_matches(|c: char| c.is_ascii_digit()).len())
    }
    if shape.strip_suffix("Shape") == Some(old) {
        return Some(format!("{new}Shape"));
    }
    let digits = shape.strip_prefix(split(old).0)?.strip_prefix("Shape")?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (base, digits) = split(new);
    Some(format!("{base}Shape{digits}"))
}

fn convert(values: [f64; 3], factor: f64) -> Result<Vec3, NativeError> {
    let value = Vec3::from_array(values.map(|v| (v * factor) as f32));
    if !value.is_finite() {
        return Err(NativeError::UnsupportedView("f32 coordinates"));
    }
    Ok(value)
}

/// Why Delete refuses a node whose hierarchy it cannot yet remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteUnsupported {
    HasChildren,
    IsChild,
}
impl fmt::Display for DeleteUnsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::HasChildren => "Deleting a node with children is not supported yet",
            Self::IsChild => "Deleting a node that has a parent is not supported yet",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeError {
    Legacy(SceneError),
    NonFinite,
    InvalidIdentity,
    InvalidRetention,
    InvalidTopology,
    InvalidCurve,
    InvalidTransformExtras,
    ResourceLimit,
    MissingShape(NativeId),
    InvalidInstancePath,
    UnsupportedLinearUnit,
    InvalidName,
    NameCollision,
    UnsupportedEdit,
    UnsupportedDelete(DeleteUnsupported),
    ReadOnlyFormat,
    UnknownSessionMaterial(SessionMaterialId),
    SessionColorOutOfRange,
    InitialSessionMaterialLocked,
    InvalidSessionMaterials,
    UnsupportedMove(WorldMoveUnsupported),
    UnsupportedRotate(RotateUnsupported),
    UnsupportedScale(ScaleUnsupported),
    AbsentChannelLeaf {
        channel: ChannelLeaf,
        component: usize,
    },
    BusyManipulation,
    UnsupportedView(&'static str),
}
impl From<SceneError> for NativeError {
    fn from(value: SceneError) -> Self {
        Self::Legacy(value)
    }
}
impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Legacy(error) => error.fmt(f),
            Self::NonFinite => f.write_str("native authoring values must be finite"),
            Self::InvalidIdentity => f.write_str("invalid native identities or DAG references"),
            Self::InvalidRetention => f.write_str("invalid retained source ranges"),
            Self::InvalidTopology => f.write_str("invalid native polygon connectivity"),
            Self::InvalidCurve => f.write_str("invalid supported native cubic curve"),
            Self::InvalidTransformExtras => {
                f.write_str("rotate order must be 0 through 5 for supported transform extras")
            }
            Self::ResourceLimit => f.write_str("native validation resource limit exceeded"),
            Self::MissingShape(id) => {
                write!(f, "native mesh shape {} does not exist", id.to_decimal())
            }
            Self::InvalidInstancePath => {
                f.write_str("face target does not match a valid full instance path")
            }
            Self::UnsupportedLinearUnit => {
                f.write_str("declared linear unit conversion is unsupported")
            }
            Self::InvalidName => {
                f.write_str("new names require the supported ASCII identifier subset")
            }
            Self::NameCollision => f.write_str("root transform name already exists"),
            Self::UnsupportedEdit => {
                f.write_str("native ownership or edit effects are unsupported")
            }
            Self::UnsupportedDelete(reason) => write!(f, "{reason}"),
            Self::ReadOnlyFormat => f.write_str("Maya ASCII scenes are read-only in this version"),
            Self::UnknownSessionMaterial(id) => {
                write!(f, "session material {} does not exist", id.index())
            }
            Self::InitialSessionMaterialLocked => {
                f.write_str("the initial session material keeps the viewport base fill")
            }
            Self::SessionColorOutOfRange => {
                f.write_str("session material colour channels must be within 0 through 1")
            }
            Self::InvalidSessionMaterials => {
                f.write_str("invalid session material table references")
            }
            Self::AbsentChannelLeaf { channel, component } => write!(
                f,
                "{} is absent from the Maya binary source and cannot be edited; leaf insertion is unsupported",
                channel.name(*component)
            ),
            Self::UnsupportedMove(reason) => write!(f, "World-axis move unsupported: {reason}"),
            Self::UnsupportedRotate(reason) => write!(f, "Rotate unsupported: {reason}"),
            Self::UnsupportedScale(reason) => write!(f, "Scale unsupported: {reason}"),
            Self::BusyManipulation => f.write_str("finish or cancel the active manipulation"),
            Self::UnsupportedView(reason) => {
                write!(f, "native viewport representation unsupported: {reason}")
            }
        }
    }
}
impl std::error::Error for NativeError {}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod edit_policy_tests;
#[cfg(test)]
mod shape_naming_tests;
#[cfg(test)]
pub(crate) mod test_fixtures;

#[cfg(test)]
std::thread_local! {
    static FULL_VALIDATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
