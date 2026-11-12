use crate::scene::{Primitive, SceneError};
use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mesh {
    vertices: Vec<Vec3>,
    faces: Vec<[usize; 4]>,
}

impl Mesh {
    pub(crate) fn from_view(
        vertices: Vec<Vec3>,
        faces: Vec<[usize; 4]>,
    ) -> Result<Self, SceneError> {
        let mesh = Self { vertices, faces };
        mesh.validate()?;
        Ok(mesh)
    }
    pub(crate) fn primitive(primitive: Primitive) -> Result<Self, SceneError> {
        Ok(match primitive {
            Primitive::Cube => Self {
                vertices: vec![
                    Vec3::new(-0.5, -0.5, -0.5),
                    Vec3::new(0.5, -0.5, -0.5),
                    Vec3::new(0.5, 0.5, -0.5),
                    Vec3::new(-0.5, 0.5, -0.5),
                    Vec3::new(-0.5, -0.5, 0.5),
                    Vec3::new(0.5, -0.5, 0.5),
                    Vec3::new(0.5, 0.5, 0.5),
                    Vec3::new(-0.5, 0.5, 0.5),
                ],
                faces: vec![
                    [0, 3, 2, 1],
                    [4, 5, 6, 7],
                    [0, 4, 7, 3],
                    [1, 2, 6, 5],
                    [0, 1, 5, 4],
                    [3, 7, 6, 2],
                ],
            },
            Primitive::Plane => Self {
                vertices: vec![
                    Vec3::new(-0.5, 0.0, -0.5),
                    Vec3::new(-0.5, 0.0, 0.5),
                    Vec3::new(0.5, 0.0, 0.5),
                    Vec3::new(0.5, 0.0, -0.5),
                ],
                faces: vec![[0, 1, 2, 3]],
            },
            Primitive::Sphere | Primitive::Cylinder | Primitive::Cone | Primitive::Torus => {
                return Err(SceneError::UnsupportedPrimitive(primitive));
            }
        })
    }

    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }
    pub fn faces(&self) -> &[[usize; 4]] {
        &self.faces
    }

    pub(crate) fn extruded_face(
        &self,
        face_index: usize,
        distance: f32,
    ) -> Result<Self, SceneError> {
        if !distance.is_finite() {
            return Err(SceneError::InvalidExtrusionDistance);
        }
        let face = *self
            .faces
            .get(face_index)
            .ok_or(SceneError::MissingFace(face_index))?;
        if self.faces.len() == 1 {
            return Err(SceneError::UnsupportedAllFacesExtrusion);
        }
        let points = face.map(|i| self.vertices[i].as_dvec3());
        let delta = quad_extrusion(face, points, self.vertices.len(), f64::from(distance))?;
        let mut mesh = self.clone();
        mesh.vertices.extend(delta.added.map(|p| p.as_vec3()));
        mesh.faces[face_index] = delta.cap;
        mesh.faces.extend(delta.sides);
        mesh.validate()?;
        Ok(mesh)
    }

    pub(crate) fn validate(&self) -> Result<(), SceneError> {
        if self.vertices.iter().any(|v| !v.is_finite()) {
            return Err(SceneError::InvalidMesh);
        }
        for face in &self.faces {
            if face.iter().any(|i| *i >= self.vertices.len())
                || face
                    .iter()
                    .enumerate()
                    .any(|(i, index)| face[..i].contains(index))
            {
                return Err(SceneError::InvalidMesh);
            }
        }
        Ok(())
    }
}

pub(crate) struct QuadExtrusion {
    pub added: [DVec3; 4],
    pub cap: [usize; 4],
    pub sides: [[usize; 4]; 4],
}

pub(crate) fn quad_extrusion(
    face: [usize; 4],
    points: [DVec3; 4],
    offset: usize,
    distance: f64,
) -> Result<QuadExtrusion, SceneError> {
    if !distance.is_finite() {
        return Err(SceneError::InvalidExtrusionDistance);
    }
    let mut normal = DVec3::ZERO;
    for i in 0..4 {
        let p = points[i];
        let q = points[(i + 1) % 4];
        normal += DVec3::new(
            (p.y - q.y) * (p.z + q.z),
            (p.z - q.z) * (p.x + q.x),
            (p.x - q.x) * (p.y + q.y),
        );
    }
    let length = normal.length();
    if !length.is_finite() || length == 0.0 {
        return Err(SceneError::UndefinedFaceNormal);
    }
    let displacement = normal / length * distance;
    let added = points.map(|p| p + displacement);
    if added.iter().any(|p| !p.is_finite()) {
        return Err(SceneError::InvalidMesh);
    }
    let cap = [
        offset,
        offset.checked_add(1).ok_or(SceneError::InvalidMesh)?,
        offset.checked_add(2).ok_or(SceneError::InvalidMesh)?,
        offset.checked_add(3).ok_or(SceneError::InvalidMesh)?,
    ];
    let sides = std::array::from_fn(|i| {
        let next = (i + 1) % 4;
        [face[i], face[next], cap[next], cap[i]]
    });
    Ok(QuadExtrusion { added, cap, sides })
}
