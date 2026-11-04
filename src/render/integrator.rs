use super::image::{Image, RenderError};
use super::ray::intersect;
use crate::view_scene::ViewScene;
use crate::viewport::Camera;
use glam::{Vec2, Vec3};
use std::thread;

#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_depth: u32,
}

fn rand_f32(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32) / (u32::MAX as f32 + 1.0)
}

fn random_unit_vector(seed: &mut u32) -> Vec3 {
    let theta = 2.0 * std::f32::consts::PI * rand_f32(seed);
    let z = 2.0 * rand_f32(seed) - 1.0;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * theta.cos(), r * theta.sin(), z)
}

fn trace_path(scene: &ViewScene, ray: &super::ray::Ray, depth: u32, seed: &mut u32) -> Vec3 {
    if depth == 0 {
        return Vec3::ZERO;
    }

    if let Some(hit) = intersect(scene, ray) {
        let mut color = Vec3::ZERO;

        // 1. Correct the normal to always point against the ray
        let normal = if hit.normal.dot(ray.direction) < 0.0 {
            hit.normal
        } else {
            -hit.normal
        };

        // 2. Direct Lighting (with attenuation and normalization)
        let light_pos = Vec3::new(3.0, 5.0, 3.0);
        let light_color = Vec3::new(1.0, 1.0, 1.0);
        let to_light = light_pos - hit.point;
        let distance_sq = to_light.length_squared();
        let light_dir = to_light / distance_sq.sqrt();

        let shadow_ray = super::ray::Ray {
            origin: hit.point + normal * 0.001,
            direction: light_dir,
        };
        let in_shadow = if let Some(shadow_hit) = intersect(scene, &shadow_ray) {
            (shadow_hit.point - hit.point).length_squared() < distance_sq
        } else {
            false
        };

        if !in_shadow {
            // Lambertian reflectance: L = (albedo / pi) * (N . L) * (1 / d^2)
            let lambert = normal.dot(light_dir).max(0.0) / std::f32::consts::PI;
            let attenuation = 1.0 / distance_sq;
            color += hit.albedo * light_color * lambert * attenuation;
        }

        // 3. Indirect Lighting (Cosine-weighted Sampling)
        let mut next_dir = (normal + random_unit_vector(seed)).normalize();
        if next_dir.dot(normal) < 0.0 {
            next_dir = -next_dir;
        }

        let next_ray = super::ray::Ray {
            origin: hit.point + normal * 0.001,
            direction: next_dir,
        };

        let p = hit.albedo.max_element().clamp(0.1, 1.0);
        if rand_f32(seed) < p {
            color += hit.albedo * trace_path(scene, &next_ray, depth - 1, seed) / p;
        }

        return color;
    }

    Vec3::new(0.05, 0.05, 0.1)
}

pub fn render(
    scene: &ViewScene,
    camera: &Camera,
    settings: RenderSettings,
) -> Result<Image, RenderError> {
    if settings.samples == 0 {
        return Err(RenderError::ZeroSamples);
    }
    if settings.max_depth == 0 {
        return Err(RenderError::InvalidDimensions);
    }

    let mut image = Image::new(settings.width, settings.height)?;
    let aspect = settings.width as f32 / settings.height as f32;
    let threads = thread::available_parallelism().map_or(1, |n| n.get());

    let mut rows: Vec<Vec<Vec3>> =
        vec![vec![Vec3::ZERO; settings.width as usize]; settings.height as usize];

    thread::scope(|s| {
        let chunk_size = (settings.height as usize).div_ceil(threads);
        for (chunk_index, chunk) in rows.chunks_mut(chunk_size).enumerate() {
            let start_y = chunk_index * chunk_size;
            s.spawn(move || {
                for (local_y, row) in chunk.iter_mut().enumerate() {
                    let y = (start_y + local_y) as u32;
                    let mut seed = 12345u32.wrapping_add(y.wrapping_mul(2654435761));
                    for _ in 0..settings.samples {
                        for x in 0..settings.width {
                            let jitter_x = rand_f32(&mut seed) - 0.5;
                            let jitter_y = rand_f32(&mut seed) - 0.5;

                            let ndc = Vec2::new(
                                2.0 * (x as f32 + 0.5 + jitter_x) / settings.width as f32 - 1.0,
                                1.0 - 2.0 * (y as f32 + 0.5 + jitter_y) / settings.height as f32,
                            );

                            if let Ok(ray) = camera.ray(ndc, aspect) {
                                row[x as usize] +=
                                    trace_path(scene, &ray, settings.max_depth, &mut seed);
                            }
                        }
                    }
                }
            });
        }
    });

    for (y, row) in rows.into_iter().enumerate() {
        for (x, color) in row.into_iter().enumerate() {
            image.add_sample(x as u32, y as u32, color);
        }
    }

    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_scene::NativeScene;
    use crate::scene::Primitive;

    #[test]
    fn render_demo_cube() {
        let mut scene = NativeScene::default();

        let floor_id = scene.create(Primitive::Plane, "floor").unwrap();
        let floor_transform = crate::native_scene::NativeTransform {
            scale: [10.0, 1.0, 10.0],
            ..Default::default()
        };
        scene.set_transform(floor_id, floor_transform).unwrap();

        let cube_id = scene.create(Primitive::Cube, "my_cube").unwrap();
        let cube_transform = crate::native_scene::NativeTransform {
            translation: [0.0, 0.5, 0.0],
            ..Default::default()
        };
        scene.set_transform(cube_id, cube_transform).unwrap();

        let view_scene = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
        let mut camera = Camera::default();
        camera
            .set_pose(Vec3::new(0.0, 1.0, 0.0), 6.0, 0.5, 0.3)
            .unwrap();

        let settings = RenderSettings {
            width: 64,
            height: 64,
            samples: 4,
            max_depth: 2,
        };

        let image = render(&view_scene, &camera, settings).unwrap();

        let sky_color = Vec3::new(0.05, 0.05, 0.1);
        // Verify that the top-left pixel is close to the sky color
        assert!(image.pixels[0].distance(sky_color * 4.0) < 0.01);

        // Verify that at least one pixel hit the geometry
        let hit_geometry = image
            .pixels
            .iter()
            .any(|&p| p.distance(sky_color * 4.0) > 0.1);
        assert!(hit_geometry, "The render should hit the cube or floor");
    }
}
