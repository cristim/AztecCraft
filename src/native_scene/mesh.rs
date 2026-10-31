use super::{NativeError, convert};
#[cfg(test)]
use crate::scene::Primitive;
use crate::{mesh::Mesh, primitive_options::PrimitiveOptions, scene::SceneError};
use glam::DVec3;
use std::collections::HashMap;
use std::fmt;

#[derive(Clone)]
pub enum Points {
    Float3(Vec<[f32; 3]>),
    Double3(Vec<[f64; 3]>),
}

impl PartialEq for Points {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Float3(a), Self::Float3(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits))
            }
            (Self::Double3(a), Self::Double3(b)) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b)
                        .all(|(a, b)| a.map(f64::to_bits) == b.map(f64::to_bits))
            }
            _ => false,
        }
    }
}
impl Points {
    fn len(&self) -> usize {
        match self {
            Self::Float3(p) => p.len(),
            Self::Double3(p) => p.len(),
        }
    }
    fn get(&self, index: usize) -> Option<[f64; 3]> {
        match self {
            Self::Float3(p) => p.get(index).map(|v| v.map(f64::from)),
            Self::Double3(p) => p.get(index).copied(),
        }
    }
    fn values(&self) -> Vec<DVec3> {
        match self {
            Self::Float3(points) => points
                .iter()
                .map(|p| DVec3::from_array(p.map(f64::from)))
                .collect(),
            Self::Double3(points) => points.iter().copied().map(DVec3::from_array).collect(),
        }
    }
    fn append(&mut self, added: [DVec3; 4]) -> Result<(), NativeError> {
        match self {
            Self::Float3(points) => {
                let converted = added.map(|p| p.as_vec3());
                if converted.iter().any(|p| !p.is_finite()) {
                    return Err(NativeError::NonFinite);
                }
                points.extend(converted.map(|p| p.to_array()));
            }
            Self::Double3(points) => points.extend(added.map(|p| p.to_array())),
        }
        Ok(())
    }
}

impl fmt::Debug for Points {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(match self {
            Self::Float3(_) => "Float3",
            Self::Double3(_) => "Double3",
        })
        .field("count", &self.len())
        .finish()
    }
}

#[derive(Clone)]
pub(super) struct PointOffset {
    pub(super) vertex: usize,
    pub(super) delta_cm: [f32; 3],
}
impl PartialEq for PointOffset {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex
            && self.delta_cm.map(f32::to_bits) == other.delta_cm.map(f32::to_bits)
    }
}
impl fmt::Debug for PointOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PointOffset")
            .field("vertex", &self.vertex)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectedEdge {
    pub edge: usize,
    pub reversed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeEdge {
    pub(super) endpoints: [usize; 2],
    pub(super) smooth: Option<bool>,
}
impl NativeEdge {
    pub fn endpoints(&self) -> [usize; 2] {
        self.endpoints
    }
    pub fn smooth(&self) -> Option<bool> {
        self.smooth
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct NativeFace {
    pub(super) outer: Vec<DirectedEdge>,
    pub(super) holes: Vec<Vec<DirectedEdge>>,
}
impl fmt::Debug for NativeFace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeFace")
            .field("outer_edges", &self.outer.len())
            .field("hole_count", &self.holes.len())
            .finish()
    }
}
impl NativeFace {
    pub fn outer(&self) -> &[DirectedEdge] {
        &self.outer
    }
    pub fn holes(&self) -> &[Vec<DirectedEdge>] {
        &self.holes
    }
}

/// First edge index per unordered endpoint pair, local to one build.
#[derive(Default)]
struct EdgeIndex(HashMap<[usize; 2], usize>);
impl EdgeIndex {
    fn key(endpoints: [usize; 2]) -> [usize; 2] {
        if endpoints[0] <= endpoints[1] {
            endpoints
        } else {
            [endpoints[1], endpoints[0]]
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct NativeMesh {
    points: Points,
    point_offsets: Vec<PointOffset>,
    edges: Vec<NativeEdge>,
    faces: Vec<NativeFace>,
    unsupported_channels: Vec<String>,
}
impl fmt::Debug for NativeMesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeMesh")
            .field("points", &self.points)
            .field("offset_count", &self.point_offsets.len())
            .field("edge_count", &self.edges.len())
            .field("face_count", &self.faces.len())
            .field("restricted_channels", &self.unsupported_channels.len())
            .finish()
    }
}
impl NativeMesh {
    pub(super) fn has_point_offsets(&self) -> bool {
        !self.point_offsets.is_empty()
    }
    pub(super) fn has_unsupported_channels(&self) -> bool {
        !self.unsupported_channels.is_empty()
    }
    #[cfg(test)]
    pub(super) fn offset_triangle_fixture() -> Self {
        Self::from_binary_parts(
            vec![
                [16777216.0, 0.0, 0.0],
                [16777218.0, 0.0, 0.0],
                [16777216.0, 1.0, 0.0],
            ],
            [[0, 1], [1, 2], [2, 0]]
                .map(|endpoints| NativeEdge {
                    endpoints,
                    smooth: None,
                })
                .into(),
            vec![NativeFace {
                outer: (0..3)
                    .map(|edge| DirectedEdge {
                        edge,
                        reversed: false,
                    })
                    .collect(),
                holes: Vec::new(),
            }],
            vec![PointOffset {
                vertex: 0,
                delta_cm: [0.25, -0.0, 0.0],
            }],
        )
        .unwrap()
    }
    pub(super) fn from_binary_parts(
        base: Vec<[f32; 3]>,
        edges: Vec<NativeEdge>,
        faces: Vec<NativeFace>,
        offsets: Vec<PointOffset>,
    ) -> Result<Self, NativeError> {
        if edges.iter().any(|e| e.smooth.is_some()) || faces.iter().any(|f| !f.holes.is_empty()) {
            return Err(NativeError::UnsupportedEdit);
        }
        let mesh = Self {
            points: Points::Float3(base),
            point_offsets: offsets,
            edges,
            faces,
            unsupported_channels: Vec::new(),
        };
        mesh.validate()?;
        Ok(mesh)
    }
    pub fn point_count(&self) -> usize {
        self.points.len()
    }
    pub fn effective_point_cm(&self, index: usize) -> Result<[f64; 3], NativeError> {
        let mut point = self.points.get(index).ok_or(NativeError::InvalidTopology)?;
        if let Ok(offset) = self
            .point_offsets
            .binary_search_by_key(&index, |v| v.vertex)
        {
            for (value, delta) in point.iter_mut().zip(self.point_offsets[offset].delta_cm) {
                *value += f64::from(delta);
            }
        }
        if point.iter().any(|v| !v.is_finite()) {
            return Err(NativeError::NonFinite);
        }
        Ok(point)
    }
    /// Test mesh of independent faces; each loop gets its own edges so large loops stay linear.
    #[cfg(test)]
    pub(super) fn polygon_fixture(points: Vec<[f64; 3]>, loops: &[Vec<usize>]) -> Self {
        let mut mesh = Self {
            point_offsets: Vec::new(),
            points: Points::Double3(points),
            edges: Vec::new(),
            faces: Vec::new(),
            unsupported_channels: Vec::new(),
        };
        for corners in loops {
            let outer = (0..corners.len())
                .map(|i| mesh.append_edge([corners[i], corners[(i + 1) % corners.len()]]))
                .collect();
            mesh.faces.push(NativeFace {
                outer,
                holes: Vec::new(),
            });
        }
        mesh
    }
    /// Gives the first face a hole loop with its own edges.
    #[cfg(test)]
    pub(super) fn add_hole_fixture(&mut self, hole: &[usize]) {
        let loop_edges = (0..hole.len())
            .map(|i| self.append_edge([hole[i], hole[(i + 1) % hole.len()]]))
            .collect();
        self.faces[0].holes.push(loop_edges);
    }
    /// Appends an edge no face uses, without validating its endpoints.
    #[cfg(test)]
    pub(super) fn add_loose_edge_fixture(&mut self, endpoints: [usize; 2]) {
        self.append_edge(endpoints);
    }
    #[cfg(test)]
    pub(super) fn triangle_double_fixture() -> Self {
        Self {
            point_offsets: Vec::new(),
            points: Points::Double3(vec![
                [16777216.25, 0.0, 0.0],
                [16777217.25, 0.0, 0.0],
                [16777216.25, 1.0, 0.0],
            ]),
            edges: [[0, 1], [1, 2], [2, 0]]
                .map(|endpoints| NativeEdge {
                    endpoints,
                    smooth: Some(false),
                })
                .into(),
            faces: vec![NativeFace {
                outer: (0..3)
                    .map(|edge| DirectedEdge {
                        edge,
                        reversed: false,
                    })
                    .collect(),
                holes: Vec::new(),
            }],
            unsupported_channels: Vec::new(),
        }
    }
    pub fn points(&self) -> &Points {
        &self.points
    }
    pub fn edges(&self) -> &[NativeEdge] {
        &self.edges
    }
    pub fn faces(&self) -> &[NativeFace] {
        &self.faces
    }
    #[cfg(test)]
    pub(super) fn primitive(primitive: Primitive) -> Result<Self, NativeError> {
        Self::with_options(&PrimitiveOptions::default_for(primitive))
    }
    pub(super) fn with_options(options: &PrimitiveOptions) -> Result<Self, NativeError> {
        options
            .validate(options.primitive())
            .map_err(SceneError::InvalidPrimitiveOptions)?;
        match *options {
            PrimitiveOptions::Cube {
                width,
                height,
                depth,
                sub_width,
                sub_height,
                sub_depth,
                axis,
            } => Ok(Self::cube_grid(&grid::CubeGrid {
                size: [width, height, depth],
                divisions: [sub_width, sub_height, sub_depth].map(|n| n as usize),
                axis,
            })),
            PrimitiveOptions::Plane {
                width,
                height,
                sub_width,
                sub_height,
                axis,
            } => Ok(Self::plane_grid(
                [width, height],
                [sub_width, sub_height].map(|n| n as usize),
                axis,
            )),
            PrimitiveOptions::Sphere {
                radius,
                sub_axis,
                sub_height,
                axis,
            } => Ok(Self::polygon_sphere(&revolve(
                radius, sub_axis, sub_height, axis,
            ))),
            PrimitiveOptions::Cylinder {
                radius,
                height,
                sub_axis,
                sub_height,
                axis,
            } => Ok(Self::polygon_cylinder(
                &revolve(radius, sub_axis, sub_height, axis),
                height,
            )),
            PrimitiveOptions::Cone {
                radius,
                height,
                sub_axis,
                sub_height,
                axis,
            } => Ok(Self::polygon_cone(
                &revolve(radius, sub_axis, sub_height, axis),
                height,
            )),
        }
    }
    pub(super) fn from_legacy(mesh: &Mesh) -> Self {
        let mut native = Self {
            point_offsets: Vec::new(),
            points: Points::Float3(mesh.vertices().iter().map(|v| v.to_array()).collect()),
            edges: Vec::new(),
            faces: Vec::new(),
            unsupported_channels: Vec::new(),
        };
        let mut index = EdgeIndex::default();
        for face in mesh.faces() {
            let outer = (0..4)
                .map(|i| native.edge(&mut index, [face[i], face[(i + 1) % 4]]))
                .collect();
            native.faces.push(NativeFace {
                outer,
                holes: Vec::new(),
            });
        }
        native
    }
    /// Closed polygon mesh from `f32` points and vertex loops; edges are shared and hard.
    fn from_loops(points: Vec<[f32; 3]>, loops: Vec<Vec<usize>>) -> Self {
        let mut mesh = Self {
            point_offsets: Vec::new(),
            points: Points::Float3(points),
            edges: Vec::new(),
            faces: Vec::new(),
            unsupported_channels: Vec::new(),
        };
        let mut index = EdgeIndex::default();
        for corners in loops {
            let outer: Vec<DirectedEdge> = (0..corners.len())
                .map(|i| mesh.edge(&mut index, [corners[i], corners[(i + 1) % corners.len()]]))
                .collect();
            mesh.faces.push(NativeFace {
                outer,
                holes: Vec::new(),
            });
        }
        mesh
    }
    fn edge(&mut self, index: &mut EdgeIndex, endpoints: [usize; 2]) -> DirectedEdge {
        let key = EdgeIndex::key(endpoints);
        if let Some(&edge) = index.0.get(&key) {
            return DirectedEdge {
                edge,
                reversed: self.edges[edge].endpoints != endpoints,
            };
        }
        let appended = self.append_edge(endpoints);
        index.0.insert(key, appended.edge);
        appended
    }
    /// Reference lookup the indexed `edge` must match exactly.
    #[cfg(test)]
    fn edge_linear(&mut self, endpoints: [usize; 2]) -> DirectedEdge {
        for (edge, existing) in self.edges.iter().enumerate() {
            if existing.endpoints == endpoints {
                return DirectedEdge {
                    edge,
                    reversed: false,
                };
            }
            if existing.endpoints == [endpoints[1], endpoints[0]] {
                return DirectedEdge {
                    edge,
                    reversed: true,
                };
            }
        }
        self.append_edge(endpoints)
    }
    fn append_edge(&mut self, endpoints: [usize; 2]) -> DirectedEdge {
        let edge = self.edges.len();
        self.edges.push(NativeEdge {
            endpoints,
            smooth: Some(false),
        });
        DirectedEdge {
            edge,
            reversed: false,
        }
    }
    /// The vertex a face loop reaches this corner at.
    pub(super) fn directed_start(&self, edge: DirectedEdge) -> Result<usize, NativeError> {
        Ok(self.endpoints(edge)?[0])
    }
    fn endpoints(&self, edge: DirectedEdge) -> Result<[usize; 2], NativeError> {
        let points = self
            .edges
            .get(edge.edge)
            .ok_or(NativeError::InvalidTopology)?
            .endpoints;
        Ok(if edge.reversed {
            [points[1], points[0]]
        } else {
            points
        })
    }
    fn validate_loop(&self, edges: &[DirectedEdge]) -> Result<(), NativeError> {
        if edges.len() < 3 {
            return Err(NativeError::InvalidTopology);
        }
        for i in 0..edges.len() {
            if self.endpoints(edges[i])?[1] != self.endpoints(edges[(i + 1) % edges.len()])?[0] {
                return Err(NativeError::InvalidTopology);
            }
        }
        Ok(())
    }
    fn loop_vertices(&self, edges: &[DirectedEdge]) -> Result<Vec<usize>, NativeError> {
        self.validate_loop(edges)?;
        edges
            .iter()
            .map(|e| self.endpoints(*e).map(|p| p[0]))
            .collect()
    }
    pub(super) fn validate(&self) -> Result<(), NativeError> {
        for index in 0..self.point_count() {
            if self
                .points
                .get(index)
                .is_none_or(|p| p.iter().any(|v| !v.is_finite()))
            {
                return Err(NativeError::NonFinite);
            }
        }
        for (index, offset) in self.point_offsets.iter().enumerate() {
            if offset.vertex >= self.point_count()
                || (index > 0 && self.point_offsets[index - 1].vertex >= offset.vertex)
            {
                return Err(NativeError::InvalidTopology);
            }
            if offset.delta_cm.iter().any(|v| !v.is_finite()) {
                return Err(NativeError::NonFinite);
            }
        }
        if self
            .edges
            .iter()
            .any(|e| e.endpoints.iter().any(|i| *i >= self.point_count()))
        {
            return Err(NativeError::InvalidTopology);
        }
        for face in &self.faces {
            self.validate_loop(&face.outer)?;
            for hole in &face.holes {
                self.validate_loop(hole)?;
            }
        }
        Ok(())
    }
    pub(super) fn view(&self, factor: f64) -> Result<Mesh, NativeError> {
        if !self.unsupported_channels.is_empty() {
            return Err(NativeError::UnsupportedView("mesh channels"));
        }
        let vertices = (0..self.point_count())
            .map(|i| convert(self.effective_point_cm(i)?, factor))
            .collect::<Result<Vec<_>, _>>()?;
        let mut faces = Vec::new();
        for face in &self.faces {
            if !face.holes.is_empty() {
                return Err(NativeError::UnsupportedView("polygon holes"));
            }
            let vertices = self.loop_vertices(&face.outer)?;
            let quad: [usize; 4] = vertices
                .try_into()
                .map_err(|_| NativeError::UnsupportedView("non-quad polygon"))?;
            faces.push(quad);
        }
        Mesh::from_view(vertices, faces)
            .map_err(|_| NativeError::UnsupportedView("quad connectivity"))
    }
    pub(super) fn extruded_face(
        &self,
        face_index: usize,
        distance: f64,
    ) -> Result<Self, NativeError> {
        if !distance.is_finite() {
            return Err(SceneError::InvalidExtrusionDistance.into());
        }
        let face = self
            .faces
            .get(face_index)
            .ok_or(SceneError::MissingFace(face_index))?;
        if !self.unsupported_channels.is_empty()
            || !face.holes.is_empty()
            || !self.point_offsets.is_empty()
            || self.edges.iter().any(|e| e.smooth.is_none())
        {
            return Err(NativeError::UnsupportedEdit);
        }
        if self.faces.len() == 1 {
            return Err(SceneError::UnsupportedAllFacesExtrusion.into());
        }
        let vertices: [usize; 4] = self
            .loop_vertices(&face.outer)?
            .try_into()
            .map_err(|_| NativeError::UnsupportedEdit)?;
        if vertices
            .iter()
            .enumerate()
            .any(|(i, v)| vertices[..i].contains(v))
        {
            return Err(NativeError::UnsupportedEdit);
        }
        let points = self.points.values();
        let delta = crate::mesh::quad_extrusion(
            vertices,
            vertices.map(|i| points[i]),
            points.len(),
            distance,
        )?;
        let mut result = self.clone();
        result.points.append(delta.added)?;
        let vertical: [_; 4] =
            std::array::from_fn(|i| result.append_edge([vertices[i], delta.cap[i]]));
        let top: [_; 4] =
            std::array::from_fn(|i| result.append_edge([delta.cap[i], delta.cap[(i + 1) % 4]]));
        result.faces[face_index].outer = top.to_vec();
        for i in 0..4 {
            let next = (i + 1) % 4;
            result.faces.push(NativeFace {
                outer: vec![
                    face.outer[i],
                    vertical[next],
                    DirectedEdge {
                        reversed: true,
                        ..top[i]
                    },
                    DirectedEdge {
                        reversed: true,
                        ..vertical[i]
                    },
                ],
                holes: Vec::new(),
            });
        }
        result.validate()?;
        Ok(result)
    }
}

mod axis;
mod cone;
#[cfg(test)]
mod cone_tests;
mod cylinder;
#[cfg(test)]
mod cylinder_tests;
#[cfg(test)]
mod edge_index_tests;
mod grid;
#[cfg(test)]
mod grid_tests;
mod sphere;
#[cfg(test)]
mod sphere_tests;
#[cfg(test)]
mod tests;

fn revolve(
    radius: f64,
    sub_axis: u32,
    sub_height: u32,
    axis: crate::primitive_options::Axis,
) -> grid::RevolveGrid {
    grid::RevolveGrid {
        radius,
        axis_divisions: sub_axis as usize,
        height_divisions: sub_height as usize,
        axis,
    }
}
