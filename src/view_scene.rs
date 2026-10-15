use crate::native_scene::{
    InternalTransform, NativeError, NativeId, NativeMesh, NativeMeshShape, NativeScene,
    NativeTransformNode,
};
use crate::scene::NodeId;
use glam::{DMat4, DVec3, Vec3};
use std::fmt;
use std::mem::size_of;

pub const FACE_CENTER_MARKER_PIXELS: f32 = 4.0;
pub const FACE_CENTER_CLICK_PIXELS: f32 = 8.0;

#[derive(Debug)]
/// Readonly geometry for one prepared authoring state; rebuild after any scene edit.
pub struct ViewScene {
    instances: Vec<ViewInstance>,
}

#[derive(Debug)]
pub struct ViewInstance {
    path: Vec<NativeId>,
    transform: NodeId,
    shape: NativeId,
    world_vertices: Vec<Vec3>,
    faces: Vec<ViewFace>,
    appearance_incomplete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewCorners {
    Triangle([usize; 3]),
    Quad([usize; 4]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewFace {
    source_index: usize,
    corners: ViewCorners,
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
                Record::Shape(_) => parent_world,
            };
            if !world.is_finite() {
                return Err(ViewError::NonFiniteWorld);
            }
            paths[cursor].world = world;
            if let Record::Shape(shape) = record {
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
                    let mesh = shape.mesh();
                    let count = mesh.point_count();
                    let mut vertices = reserve(count)?;
                    for index in 0..count {
                        vertices.push(world_point(
                            world,
                            DVec3::from_array(
                                mesh.effective_point_cm(index).map_err(ViewError::Native)?,
                            ),
                        )?);
                    }
                    instances.push(ViewInstance {
                        path,
                        transform: transform.id(),
                        shape: shape.id(),
                        world_vertices: vertices,
                        faces: view_faces(mesh)?,
                        appearance_incomplete: admission.appearance_incomplete(),
                    });
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
        Ok(Self { instances })
    }

    pub fn instances(&self) -> &[ViewInstance] {
        &self.instances
    }
}

impl ViewInstance {
    pub fn is_direct_shape_of(&self, transform: NodeId, path: &[NativeId]) -> bool {
        self.transform == transform
            && self.path.len().checked_sub(1) == Some(path.len())
            && self.path.starts_with(path)
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
    pub fn world_vertices(&self) -> &[Vec3] {
        &self.world_vertices
    }
    pub fn faces(&self) -> &[ViewFace] {
        &self.faces
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
    pub fn corners(&self) -> &[usize] {
        match &self.corners {
            ViewCorners::Triangle(indices) => indices,
            ViewCorners::Quad(indices) => indices,
        }
    }
    pub fn triangles(&self) -> impl Iterator<Item = [usize; 3]> {
        match self.corners {
            ViewCorners::Triangle(indices) => [Some(indices), None],
            ViewCorners::Quad([a, b, c, d]) => [Some([a, b, c]), Some([a, c, d])],
        }
        .into_iter()
        .flatten()
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
            Self::UnsupportedPolygon => "view requires triangle or quad faces without holes",
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

fn world_point(world: DMat4, point: DVec3) -> Result<Vec3, ViewError> {
    let point = world.transform_point3(point).as_vec3();
    point
        .is_finite()
        .then_some(point)
        .ok_or(ViewError::NonFiniteWorld)
}

fn view_faces(mesh: &NativeMesh) -> Result<Vec<ViewFace>, ViewError> {
    let mut result = reserve(mesh.faces().len())?;
    let count = mesh.point_count();
    for (source_index, face) in mesh.faces().iter().enumerate() {
        if !face.holes().is_empty() || !matches!(face.outer().len(), 3 | 4) {
            return Err(ViewError::UnsupportedPolygon);
        }
        let mut corners = [0; 4];
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
            corners[index] = start;
            previous_end = Some(end);
        }
        if previous_end != Some(corners[0]) {
            return Err(ViewError::InvalidGeometry);
        }
        let corners = if face.outer().len() == 3 {
            ViewCorners::Triangle([corners[0], corners[1], corners[2]])
        } else {
            ViewCorners::Quad(corners)
        };
        result.push(ViewFace {
            source_index,
            corners,
        });
    }
    Ok(result)
}

#[derive(Clone, Copy)]
enum Record<'a> {
    Transform(&'a NativeTransformNode),
    Shape(&'a NativeMeshShape),
}

impl Record<'_> {
    fn id(&self) -> NativeId {
        match self {
            Self::Transform(node) => node.transform_id(),
            Self::Shape(shape) => shape.id(),
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
        let count = checked_add(scene.nodes().len(), scene.shapes().len())?;
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
            if parent.is_some_and(|index| matches!(records[index], Record::Shape(_)))
                || (parent.is_none() && matches!(records[child], Record::Shape(_)))
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
            if let Record::Shape(shape) = record {
                let points = shape.mesh().point_count();
                for face in shape.mesh().faces() {
                    if !face.holes().is_empty() || !matches!(face.outer().len(), 3 | 4) {
                        return Err(ViewError::UnsupportedPolygon);
                    }
                }
                requirements.instances = checked_add(requirements.instances, count.paths)?;
                let per_instance = checked_add(
                    size_of::<ViewInstance>(),
                    checked_add(
                        buffer_bytes::<Vec3>(points)?,
                        buffer_bytes::<ViewFace>(shape.mesh().faces().len())?,
                    )?,
                )?;
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
    let translate = DVec3::from_array(transform.translation_cm);
    let rp = DVec3::from_array(transform.rotate_pivot_cm);
    let sp = DVec3::from_array(transform.scale_pivot_cm);
    let rotation = DVec3::from_array(transform.rotation_radians);
    let matrix = DMat4::from_translation(translate)
        * DMat4::from_translation(rp)
        * DMat4::from_rotation_z(rotation.z)
        * DMat4::from_rotation_y(rotation.y)
        * DMat4::from_rotation_x(rotation.x)
        * DMat4::from_translation(-rp)
        * DMat4::from_translation(sp)
        * DMat4::from_scale(DVec3::from_array(transform.scale))
        * DMat4::from_translation(-sp);
    matrix
        .is_finite()
        .then_some(matrix)
        .ok_or(ViewError::NonFiniteWorld)
}

#[cfg(test)]
#[path = "view_scene/tests.rs"]
mod tests;
