use crate::native_scene::{
    InternalTransform, NativeCurveShape, NativeError, NativeFace, NativeId, NativeMesh,
    NativeMeshShape, NativeScene, NativeTransformNode,
};
use crate::scene::NodeId;
use crate::shading::SURFACE_BASE_RGB;
use glam::{DMat3, DMat4, DVec3, Mat3, Vec3};
use std::fmt;
use std::mem::size_of;

pub const FACE_CENTER_MARKER_PIXELS: f32 = 4.0;
pub const FACE_CENTER_CLICK_PIXELS: f32 = 8.0;
/// Provisional vertex marker size; the click box below is twice it. Policy value, not a Maya default.
pub const VERTEX_MARKER_PIXELS: f32 = 5.0;
/// Provisional width of the selected-edge highlight quad.
pub const SELECTED_EDGE_PIXELS: f32 = 3.0;
/// Provisional click box edge for a vertex marker (5 px marker), a policy value not a Maya default.
pub const VERTEX_CLICK_PIXELS: f32 = 10.0;
/// Provisional largest screen distance from the pointer to a picked edge.
pub const EDGE_CLICK_PIXELS: f32 = 6.0;
/// Allowed distance of a polygon's turning sum from a whole number of turns. A quarter turn
/// separates integer windings; it is not a planarity check, so lifted vertices still display.
/// Strongly skew (non-planar) n-gons may be accepted yet overlap in projection; display only.
const WINDING_RESIDUAL: f64 = 0.25;

mod curves;

fn channel_to_u8(channel: f64) -> u8 {
    (channel * 255.0).round().clamp(0.0, 255.0) as u8
}

#[derive(Debug)]
/// Readonly geometry for one prepared authoring state; rebuild after any scene edit.
pub struct ViewScene {
    instances: Vec<ViewInstance>,
    curve_instances: Vec<ViewCurveInstance>,
}

pub struct ViewCurveInstance {
    path: Vec<NativeId>,
    transform: NodeId,
    shape: NativeId,
    segments: Vec<[Vec3; 2]>,
    bounds: [DVec3; 2],
    appearance_incomplete: bool,
}

impl fmt::Debug for ViewCurveInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ViewCurveInstance")
            .field("shape", &self.shape)
            .field("segment_count", &self.segments.len())
            .finish()
    }
}

impl ViewCurveInstance {
    pub fn path(&self) -> &[NativeId] {
        &self.path
    }
    pub fn transform(&self) -> NodeId {
        self.transform
    }
    pub fn shape(&self) -> NativeId {
        self.shape
    }
    pub fn segments(&self) -> &[[Vec3; 2]] {
        &self.segments
    }
    pub fn bounds(&self) -> [DVec3; 2] {
        self.bounds
    }
    pub fn appearance_incomplete(&self) -> bool {
        self.appearance_incomplete
    }
}

#[derive(Debug)]
pub struct ViewInstance {
    path: Vec<NativeId>,
    transform: NodeId,
    shape: NativeId,
    world_vertices: Vec<Vec3>,
    /// Exact f64 world matrix (centimeters) of this instance path.
    world: DMat4,
    /// Inverse transpose of the world matrix's linear part; zero if it is singular.
    normal_matrix: Mat3,
    faces: Vec<ViewFace>,
    /// Endpoints of every native edge in `NativeMesh::edges()` order.
    edges: Vec<[usize; 2]>,
    appearance_incomplete: bool,
    base_rgb: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ViewCorners {
    Triangle([usize; 3]),
    Quad([usize; 4]),
    /// Five or more corners; display-only, fanned from corner 0.
    Polygon(Box<[usize]>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewFace {
    source_index: usize,
    corners: ViewCorners,
    /// Unit flat normal in the shape's local space; zero when the face has no usable normal.
    normal: Vec3,
}

impl ViewScene {
    /// Applies the caller's derived-view byte budget before expanding instance paths.
    pub fn from_native(scene: &NativeScene, byte_budget: usize) -> Result<Self, ViewError> {
        let graph = Graph::prepare(scene, byte_budget)?;
        let requirements = graph.requirements(byte_budget)?;
        let mut paths = reserve(requirements.paths)?;
        for root in graph.children(None) {
            paths.push(PathRecord {
                node: root.child,
                parent: None,
                world: DMat4::IDENTITY,
                depth: 1,
            });
        }
        let mut instances = reserve(requirements.instances)?;
        let mut curve_instances = reserve(requirements.curve_instances)?;
        let curve_capacity_extra = buffer_bytes::<ViewCurveInstance>(
            curve_instances
                .capacity()
                .checked_sub(requirements.curve_instances)
                .ok_or(ViewError::ResourceLimit)?,
        )?;
        let mut remaining_bytes = byte_budget
            .checked_sub(checked_add(requirements.bytes, curve_capacity_extra)?)
            .ok_or(ViewError::ResourceLimit)?;
        let mut curve_work = curves::CurveWork::default();
        let mut cursor = 0;
        while cursor < paths.len() {
            let parent_world = paths[cursor]
                .parent
                .map_or(DMat4::IDENTITY, |parent| paths[parent].world);
            let record = &graph.records[paths[cursor].node];
            let world = match record {
                Record::Transform(node) => {
                    if !node.unsupported_transform_effects().is_empty() {
                        return Err(ViewError::UnsupportedTransform);
                    }
                    parent_world
                        * local_matrix(
                            scene
                                .internal_transform(node.id())
                                .map_err(ViewError::Native)?,
                        )?
                }
                Record::Shape(_) | Record::Curve(_) => parent_world,
            };
            if !world.is_finite() {
                return Err(ViewError::NonFiniteWorld);
            }
            paths[cursor].world = world;
            if !matches!(record, Record::Transform(_)) {
                let mut path = reserve(paths[cursor].depth)?;
                let mut ancestor = Some(cursor);
                while let Some(index) = ancestor {
                    path.push(graph.records[paths[index].node].id());
                    ancestor = paths[index].parent;
                }
                path.reverse();
                let admission = scene
                    .geometry_view_admission(&path)
                    .map_err(ViewError::Native)?;
                if admission.visible() {
                    let parent = paths[cursor].parent.ok_or(ViewError::InvalidGeometry)?;
                    let Record::Transform(transform) = graph.records[paths[parent].node] else {
                        return Err(ViewError::InvalidGeometry);
                    };
                    match record {
                        Record::Shape(shape) => {
                            let mesh = shape.mesh();
                            let count = mesh.point_count();
                            let mut vertices = reserve(count)?;
                            for index in 0..count {
                                vertices.push(world_point(
                                    world,
                                    DVec3::from_array(
                                        mesh.effective_point_cm(index)
                                            .map_err(ViewError::Native)?,
                                    ),
                                )?);
                            }
                            instances.push(ViewInstance {
                                path,
                                transform: transform.id(),
                                shape: shape.id(),
                                world_vertices: vertices,
                                world,
                                normal_matrix: normal_matrix(&world),
                                faces: view_faces(mesh)?,
                                edges: view_edges(mesh)?,
                                appearance_incomplete: admission.appearance_incomplete(),
                                base_rgb: scene
                                    .session_material(shape)
                                    .map_or(SURFACE_BASE_RGB, |material| {
                                        material.color().map(channel_to_u8)
                                    }),
                            });
                        }
                        Record::Curve(shape) => {
                            let prepared =
                                curves::prepare(shape, &world, remaining_bytes, &mut curve_work)?;
                            remaining_bytes = remaining_bytes
                                .checked_sub(buffer_bytes::<[Vec3; 2]>(
                                    prepared.segments.capacity(),
                                )?)
                                .ok_or(ViewError::ResourceLimit)?;
                            curve_instances.push(ViewCurveInstance {
                                path,
                                transform: transform.id(),
                                shape: shape.id(),
                                segments: prepared.segments,
                                bounds: prepared.bounds,
                                appearance_incomplete: admission.appearance_incomplete(),
                            });
                        }
                        Record::Transform(_) => return Err(ViewError::InvalidGeometry),
                    }
                }
            }
            let depth = checked_add(paths[cursor].depth, 1)?;
            for child in graph.children(Some(paths[cursor].node)) {
                paths.push(PathRecord {
                    node: child.child,
                    parent: Some(cursor),
                    world: DMat4::IDENTITY,
                    depth,
                });
            }
            cursor += 1;
        }
        instances.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        curve_instances.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        Ok(Self {
            instances,
            curve_instances,
        })
    }

    pub fn instances(&self) -> &[ViewInstance] {
        &self.instances
    }
    pub fn curve_instances(&self) -> &[ViewCurveInstance] {
        &self.curve_instances
    }
    /// Whether any visible mesh or curve instance is a direct shape of one of `prefixes`.
    pub fn has_direct_geometry(&self, prefixes: &[&[NativeId]]) -> bool {
        let direct = |path: &[NativeId]| {
            prefixes
                .iter()
                .any(|prefix| is_direct_instance(path, prefix))
        };
        self.instances.iter().any(|instance| direct(&instance.path))
            || self
                .curve_instances
                .iter()
                .any(|instance| direct(&instance.path))
    }
}

/// True when `instance_path` is `prefix` plus exactly one more element (the shape), so the
/// instance belongs to the transform at `prefix` itself and not to a descendant of it.
pub fn is_direct_instance(instance_path: &[NativeId], prefix: &[NativeId]) -> bool {
    instance_path.len().checked_sub(1) == Some(prefix.len()) && instance_path.starts_with(prefix)
}

impl ViewInstance {
    pub fn is_direct_shape_of(&self, transform: NodeId, path: &[NativeId]) -> bool {
        self.transform == transform
            && is_direct_instance(&self.path, path)
            && self.path.last() == Some(&self.shape)
    }

    pub fn path(&self) -> &[NativeId] {
        &self.path
    }
    pub fn transform(&self) -> NodeId {
        self.transform
    }
    pub fn shape(&self) -> NativeId {
        self.shape
    }
    /// Unit world-space normal of `face`: its local normal through the inverse transpose of the
    /// world matrix, so non-uniform scale and mirrored (negative-determinant) parents keep the
    /// normal outward. Zero when the face or transform has no usable normal.
    pub fn world_normal(&self, face: &ViewFace) -> Vec3 {
        let normal = (self.normal_matrix * face.normal()).normalize_or_zero();
        if normal.is_finite() {
            normal
        } else {
            Vec3::ZERO
        }
    }
    /// Unlit surface colour from the shape's session material, shared by every instance of the
    /// shape; the initial group (the viewport base fill) when none is assigned or the scene is
    /// sourced. Linear 0 through 1 is shown as sRGB bytes directly: no colour management yet.
    pub fn base_rgb(&self) -> [u8; 3] {
        self.base_rgb
    }
    pub fn world_vertices(&self) -> &[Vec3] {
        &self.world_vertices
    }
    /// The f64 world matrix (centimeters) the f32 display vertices were derived from, so
    /// exporters can recompute exact positions from the authoring scene.
    pub fn world_matrix(&self) -> DMat4 {
        self.world
    }
    pub fn faces(&self) -> &[ViewFace] {
        &self.faces
    }
    pub fn edges(&self) -> &[[usize; 2]] {
        &self.edges
    }
    pub fn appearance_incomplete(&self) -> bool {
        self.appearance_incomplete
    }
}

impl ViewFace {
    pub fn center(&self, vertices: &[Vec3]) -> Result<Vec3, ViewError> {
        let mut sum = DVec3::ZERO;
        for &index in self.corners() {
            let vertex = vertices.get(index).ok_or(ViewError::InvalidGeometry)?;
            if !vertex.is_finite() {
                return Err(ViewError::NonFiniteWorld);
            }
            sum += vertex.as_dvec3();
        }
        let center = (sum / self.corners().len() as f64).as_vec3();
        center
            .is_finite()
            .then_some(center)
            .ok_or(ViewError::NonFiniteWorld)
    }

    pub fn source_index(&self) -> usize {
        self.source_index
    }
    /// Unit Newell normal of the winding in local space, or zero for a degenerate or
    /// non-finite face. Not yet transformed to world space.
    pub fn normal(&self) -> Vec3 {
        self.normal
    }
    pub fn corners(&self) -> &[usize] {
        match &self.corners {
            ViewCorners::Triangle(indices) => indices,
            ViewCorners::Quad(indices) => indices,
            ViewCorners::Polygon(indices) => indices,
        }
    }
    /// Fans every face from its first corner; a quad yields `[0,1,2]` then `[0,2,3]`.
    pub fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
        let corners = self.corners();
        corners
            .iter()
            .skip(1)
            .zip(corners.iter().skip(2))
            .map(|(&b, &c)| [corners[0], b, c])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewError {
    Native(NativeError),
    Camera(crate::viewport::InvalidCameraInput),
    ResourceLimit,
    InvalidGeometry,
    UnsupportedPolygon,
    UnsupportedTransform,
    NonFiniteWorld,
}

impl fmt::Display for ViewError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Native(error) => return write!(f, "native geometry view denied: {error}"),
            Self::Camera(error) => {
                return write!(f, "native view camera input is invalid: {error}");
            }
            Self::ResourceLimit => {
                "derived view exceeds its resource budget or allocation capacity"
            }
            Self::InvalidGeometry => "native geometry or instance path is invalid",
            Self::UnsupportedPolygon => "view requires faces without holes; faces with five or more corners must also be simple and strictly convex",
            Self::UnsupportedTransform => "native transform or declared units are unsupported",
            Self::NonFiniteWorld => "world geometry cannot be represented by the viewport",
        })
    }
}
impl std::error::Error for ViewError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Native(error) => Some(error),
            Self::Camera(error) => Some(error),
            _ => None,
        }
    }
}

fn normal_matrix(world: &DMat4) -> Mat3 {
    let linear = DMat3::from_mat4(*world);
    let determinant = linear.determinant();
    if !determinant.is_finite() || determinant == 0.0 {
        return Mat3::ZERO;
    }
    let matrix = linear.inverse().transpose().as_mat3();
    if matrix.is_finite() {
        matrix
    } else {
        Mat3::ZERO
    }
}

fn world_point(world: DMat4, point: DVec3) -> Result<Vec3, ViewError> {
    let point = world.transform_point3(point).as_vec3();
    point
        .is_finite()
        .then_some(point)
        .ok_or(ViewError::NonFiniteWorld)
}

/// Cheap structural test shared by the budget preflight and `view_faces`.
fn face_is_viewable_shape(face: &NativeFace) -> bool {
    face.holes().is_empty() && face.outer().len() >= 3
}

fn view_edges(mesh: &NativeMesh) -> Result<Vec<[usize; 2]>, ViewError> {
    let count = mesh.point_count();
    let mut result = reserve(mesh.edges().len())?;
    for edge in mesh.edges() {
        let endpoints = edge.endpoints();
        if endpoints.iter().any(|&point| point >= count) {
            return Err(ViewError::InvalidGeometry);
        }
        result.push(endpoints);
    }
    Ok(result)
}

fn view_faces(mesh: &NativeMesh) -> Result<Vec<ViewFace>, ViewError> {
    let mut result = reserve(mesh.faces().len())?;
    let count = mesh.point_count();
    for (source_index, face) in mesh.faces().iter().enumerate() {
        if !face_is_viewable_shape(face) {
            return Err(ViewError::UnsupportedPolygon);
        }
        let length = face.outer().len();
        let mut small = [0; 4];
        let mut large: Vec<usize> = if length > 4 {
            reserve(length)?
        } else {
            Vec::new()
        };
        let mut previous_end = None;
        for (index, directed) in face.outer().iter().enumerate() {
            let edge = mesh
                .edges()
                .get(directed.edge)
                .ok_or(ViewError::InvalidGeometry)?;
            let [a, b] = edge.endpoints();
            let [start, end] = if directed.reversed { [b, a] } else { [a, b] };
            if start >= count
                || end >= count
                || previous_end.is_some_and(|previous| previous != start)
            {
                return Err(ViewError::InvalidGeometry);
            }
            if length > 4 {
                large.push(start);
            } else {
                small[index] = start;
            }
            previous_end = Some(end);
        }
        let first = if length > 4 { large[0] } else { small[0] };
        if previous_end != Some(first) {
            return Err(ViewError::InvalidGeometry);
        }
        let normal = flat_normal(mesh, if length > 4 { &large } else { &small[..length] })?;
        let corners = match length {
            3 => ViewCorners::Triangle([small[0], small[1], small[2]]),
            4 => ViewCorners::Quad(small),
            _ => {
                check_convex_polygon(mesh, &large)?;
                ViewCorners::Polygon(large.into_boxed_slice())
            }
        };
        result.push(ViewFace {
            source_index,
            corners,
            normal,
        });
    }
    Ok(result)
}

/// Newell normal relative to the first corner, in f64 on the local points; its length is twice
/// the face area and it may be zero or non-finite.
fn newell_normal(mesh: &NativeMesh, corners: &[usize]) -> Result<DVec3, ViewError> {
    let point = |index: usize| {
        mesh.effective_point_cm(index)
            .map(DVec3::from_array)
            .map_err(ViewError::Native)
    };
    let origin = point(*corners.first().ok_or(ViewError::UnsupportedPolygon)?)?;
    let mut normal = DVec3::ZERO;
    let mut previous = DVec3::ZERO;
    for &index in corners {
        let current = point(index)? - origin;
        normal += previous.cross(current);
        previous = current;
    }
    Ok(normal)
}

/// Unit f32 flat normal, or zero when the winding is degenerate or the value is not finite.
fn flat_normal(mesh: &NativeMesh, corners: &[usize]) -> Result<Vec3, ViewError> {
    let normal = newell_normal(mesh, corners)?;
    let length = normal.length();
    let unit = (normal / length).as_vec3();
    Ok(if length > 0.0 && length.is_finite() && unit.is_finite() {
        unit
    } else {
        Vec3::ZERO
    })
}

/// Accepts only simple, strictly convex, once-wound polygons; judged in f64 on the local points.
fn check_convex_polygon(mesh: &NativeMesh, corners: &[usize]) -> Result<(), ViewError> {
    let point = |index: usize| {
        mesh.effective_point_cm(index)
            .map(DVec3::from_array)
            .map_err(ViewError::Native)
    };
    let normal = newell_normal(mesh, corners)?;
    if !normal.is_finite() {
        return Err(ViewError::NonFiniteWorld);
    }
    let length = normal.length();
    if !length.is_finite() {
        return Err(ViewError::UnsupportedPolygon);
    }
    let unit = normal / length;
    let mut turning = 0.0;
    let mut before = point(corners[corners.len() - 2])?;
    let mut current = point(corners[corners.len() - 1])?;
    for &index in corners {
        let after = point(index)?;
        let incoming = current - before;
        let outgoing = after - current;
        let cross = incoming.cross(outgoing);
        if !cross.is_finite() || cross.dot(normal) <= 0.0 {
            return Err(ViewError::UnsupportedPolygon);
        }
        turning += cross.dot(unit).atan2(incoming.dot(outgoing));
        before = current;
        current = after;
    }
    let turns = turning / std::f64::consts::TAU;
    let winding = turns.round();
    if !turns.is_finite() || winding.abs() != 1.0 || (turns - winding).abs() > WINDING_RESIDUAL {
        return Err(ViewError::UnsupportedPolygon);
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Record<'a> {
    Transform(&'a NativeTransformNode),
    Shape(&'a NativeMeshShape),
    Curve(&'a NativeCurveShape),
}

impl Record<'_> {
    fn id(&self) -> NativeId {
        match self {
            Self::Transform(node) => node.transform_id(),
            Self::Shape(shape) => shape.id(),
            Self::Curve(shape) => shape.id(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    parent: Option<usize>,
    child: usize,
}

struct Graph<'a> {
    records: Vec<Record<'a>>,
    edges: Vec<Edge>,
}

#[derive(Clone, Copy, Default)]
struct Counts {
    incoming: usize,
    paths: usize,
    path_elements: usize,
    max_depth: usize,
}

struct PathRecord {
    node: usize,
    parent: Option<usize>,
    world: DMat4,
    depth: usize,
}

struct Requirements {
    paths: usize,
    instances: usize,
    curve_instances: usize,
    bytes: usize,
}

fn checked_add(a: usize, b: usize) -> Result<usize, ViewError> {
    a.checked_add(b).ok_or(ViewError::ResourceLimit)
}

fn buffer_bytes<T>(count: usize) -> Result<usize, ViewError> {
    count
        .checked_mul(size_of::<T>())
        .ok_or(ViewError::ResourceLimit)
}

fn reserve<T>(count: usize) -> Result<Vec<T>, ViewError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| ViewError::ResourceLimit)?;
    Ok(values)
}

impl<'a> Graph<'a> {
    fn prepare(scene: &'a NativeScene, byte_budget: usize) -> Result<Self, ViewError> {
        let count = checked_add(
            checked_add(scene.nodes().len(), scene.shapes().len())?,
            scene.curves().len(),
        )?;
        let graph_bytes = checked_add(
            buffer_bytes::<Record<'_>>(count)?,
            buffer_bytes::<Edge>(scene.dag_edges().len())?,
        )?;
        let scratch = checked_add(
            buffer_bytes::<Counts>(count)?,
            buffer_bytes::<usize>(count)?,
        )?;
        let overhead = size_of::<Self>() + size_of::<Vec<Counts>>() + size_of::<Vec<usize>>();
        if checked_add(checked_add(graph_bytes, scratch)?, overhead)? > byte_budget {
            return Err(ViewError::ResourceLimit);
        }
        let mut records = reserve(count)?;
        records.extend(scene.nodes().iter().map(Record::Transform));
        records.extend(scene.shapes().iter().map(Record::Shape));
        records.extend(scene.curves().iter().map(Record::Curve));
        records.sort_unstable_by_key(Record::id);
        if records.windows(2).any(|pair| pair[0].id() == pair[1].id()) {
            return Err(ViewError::InvalidGeometry);
        }
        let find = |id| {
            records
                .binary_search_by_key(&id, Record::id)
                .map_err(|_| ViewError::InvalidGeometry)
        };
        let mut edges = reserve(scene.dag_edges().len())?;
        for edge in scene.dag_edges() {
            let child = find(edge.child())?;
            let parent = edge.parent().map(find).transpose()?;
            if parent.is_some_and(|index| !matches!(records[index], Record::Transform(_)))
                || (parent.is_none() && !matches!(records[child], Record::Transform(_)))
            {
                return Err(ViewError::InvalidGeometry);
            }
            edges.push(Edge { parent, child });
        }
        edges.sort_unstable();
        if edges.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ViewError::InvalidGeometry);
        }
        Ok(Self { records, edges })
    }

    fn children(&self, parent: Option<usize>) -> &[Edge] {
        let start = self.edges.partition_point(|edge| edge.parent < parent);
        let end = self.edges.partition_point(|edge| edge.parent <= parent);
        &self.edges[start..end]
    }

    fn requirements(&self, byte_budget: usize) -> Result<Requirements, ViewError> {
        let mut counts = reserve(self.records.len())?;
        counts.resize(self.records.len(), Counts::default());
        for edge in &self.edges {
            let target = &mut counts[edge.child];
            if edge.parent.is_some() {
                target.incoming = checked_add(target.incoming, 1)?;
            } else {
                target.paths = 1;
                target.path_elements = 1;
                target.max_depth = 1;
            }
        }
        let mut queue = reserve(self.records.len())?;
        queue.extend(
            counts
                .iter()
                .enumerate()
                .filter_map(|(index, count)| (count.incoming == 0).then_some(index)),
        );
        let mut visited = 0;
        while visited < queue.len() {
            let parent = queue[visited];
            let current = counts[parent];
            if current.paths == 0 {
                return Err(ViewError::InvalidGeometry);
            }
            for edge in self.children(Some(parent)) {
                let child = &mut counts[edge.child];
                child.paths = checked_add(child.paths, current.paths)?;
                child.path_elements = checked_add(
                    child.path_elements,
                    checked_add(current.path_elements, current.paths)?,
                )?;
                child.max_depth = child.max_depth.max(checked_add(current.max_depth, 1)?);
                if buffer_bytes::<PathRecord>(child.paths)? > byte_budget {
                    return Err(ViewError::ResourceLimit);
                }
                child.incoming -= 1;
                if child.incoming == 0 {
                    queue.push(edge.child);
                }
            }
            visited += 1;
        }
        if visited != self.records.len() {
            return Err(ViewError::InvalidGeometry);
        }
        let mut requirements = Requirements {
            paths: 0,
            instances: 0,
            curve_instances: 0,
            bytes: 0,
        };
        let graph_bytes = checked_add(
            buffer_bytes::<Record<'_>>(self.records.len())?,
            buffer_bytes::<Edge>(self.edges.len())?,
        )?;
        let scratch = checked_add(
            buffer_bytes::<Counts>(counts.len())?,
            buffer_bytes::<usize>(queue.capacity())?,
        )?;
        let preflight_bytes = checked_add(
            graph_bytes,
            checked_add(
                scratch,
                size_of::<Self>() + size_of::<Vec<Counts>>() + size_of::<Vec<usize>>(),
            )?,
        )?;
        let mut output_bytes =
            size_of::<Self>() + size_of::<ViewScene>() + size_of::<Vec<PathRecord>>();
        let mut max_depth = 0;
        for (record, count) in self.records.iter().zip(counts) {
            requirements.paths = checked_add(requirements.paths, count.paths)?;
            let per_instance = match record {
                Record::Shape(shape) => {
                    let points = shape.mesh().point_count();
                    let mut corner_bytes = 0;
                    for face in shape.mesh().faces() {
                        if !face_is_viewable_shape(face) {
                            return Err(ViewError::UnsupportedPolygon);
                        }
                        if face.outer().len() > 4 {
                            corner_bytes = checked_add(
                                corner_bytes,
                                buffer_bytes::<usize>(face.outer().len())?,
                            )?;
                        }
                    }
                    requirements.instances = checked_add(requirements.instances, count.paths)?;
                    checked_add(
                        size_of::<ViewInstance>(),
                        checked_add(
                            buffer_bytes::<Vec3>(points)?,
                            checked_add(
                                buffer_bytes::<ViewFace>(shape.mesh().faces().len())?,
                                checked_add(
                                    corner_bytes,
                                    buffer_bytes::<[usize; 2]>(shape.mesh().edges().len())?,
                                )?,
                            )?,
                        )?,
                    )?
                }
                Record::Curve(_) => {
                    requirements.curve_instances =
                        checked_add(requirements.curve_instances, count.paths)?;
                    size_of::<ViewCurveInstance>()
                }
                Record::Transform(_) => continue,
            };
            output_bytes = checked_add(
                output_bytes,
                per_instance
                    .checked_mul(count.paths)
                    .ok_or(ViewError::ResourceLimit)?,
            )?;
            output_bytes =
                checked_add(output_bytes, buffer_bytes::<NativeId>(count.path_elements)?)?;
            max_depth = max_depth.max(count.max_depth);
        }
        output_bytes = checked_add(
            output_bytes,
            checked_add(graph_bytes, buffer_bytes::<PathRecord>(requirements.paths)?)?,
        )?;
        output_bytes = checked_add(
            output_bytes,
            checked_add(
                size_of::<Vec<NativeId>>(),
                buffer_bytes::<NativeId>(max_depth)?,
            )?,
        )?;
        requirements.bytes = preflight_bytes.max(output_bytes);
        if requirements.bytes > byte_budget {
            return Err(ViewError::ResourceLimit);
        }
        Ok(requirements)
    }
}

fn local_matrix(transform: InternalTransform) -> Result<DMat4, ViewError> {
    transform.matrix().ok_or(ViewError::NonFiniteWorld)
}

#[cfg(test)]
#[path = "view_scene/tests.rs"]
mod tests;
