use crate::view_scene::ViewScene;
pub use crate::viewport::Ray;
use glam::Vec3;

pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    pub albedo: Vec3,
}

fn ray_triangle(ray: &Ray, v0: Vec3, v1: Vec3, v2: Vec3) -> Option<f32> {
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = ray.direction.cross(edge2);
    let a = edge1.dot(h);

    // Relative epsilon based on the size of the triangle for robustness in f32
    let eps = 1e-8 * edge1.length().max(edge2.length()).max(1e-6);
    if a > -eps && a < eps {
        return None;
    }

    let f = 1.0 / a;
    let s = ray.origin - v0;
    let u = f * s.dot(h);

    if u < 0.0 || u > 1.0 {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray.direction.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);
    if t > eps { Some(t) } else { None }
}

pub fn intersect(scene: &ViewScene, ray: &Ray) -> Option<Hit> {
    let mut closest_t = f32::MAX;
    let mut result = None;

    for instance in scene.instances() {
        let vertices = instance.world_vertices();
        let base_color = instance.base_rgb();
        let albedo = Vec3::new(
            base_color[0] as f32 / 255.0,
            base_color[1] as f32 / 255.0,
            base_color[2] as f32 / 255.0,
        );

        for face in instance.faces() {
            for triangle in face.triangles() {
                let (Some(&v0), Some(&v1), Some(&v2)) = (
                    vertices.get(triangle[0]),
                    vertices.get(triangle[1]),
                    vertices.get(triangle[2]),
                ) else {
                    continue;
                };

                if let Some(t) = ray_triangle(ray, v0, v1, v2)
                    && t < closest_t
                {
                    closest_t = t;
                    result = Some(Hit {
                        point: ray.origin + ray.direction * t,
                        normal: instance.world_normal(face),
                        albedo,
                    });
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_triangle_hits() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, -1.0),
            direction: Vec3::new(0.0, 0.0, 1.0),
        };
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        assert!(ray_triangle(&ray, v0, v1, v2).is_some());
    }

    #[test]
    fn ray_triangle_misses() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, -1.0),
            direction: Vec3::new(0.0, 1.0, 0.0),
        };
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        assert!(ray_triangle(&ray, v0, v1, v2).is_none());
    }

    #[test]
    fn ray_triangle_parallel() {
        let ray = Ray {
            origin: Vec3::new(0.0, 0.0, -1.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
        };
        let v0 = Vec3::new(-1.0, -1.0, 0.0);
        let v1 = Vec3::new(1.0, -1.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);
        assert!(ray_triangle(&ray, v0, v1, v2).is_none());
    }
}
