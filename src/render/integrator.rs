use super::image::Image;
use super::ray::intersect;
use crate::view_scene::ViewScene;
use crate::viewport::Camera;
use glam::{Vec2, Vec3};
use rayon::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub samples: u32,
    pub max_depth: u32, // Max number of bounces for indirect lighting
}

// Simple pseudo-random number generator (PRNG) using xorshift
fn rand_f32(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32) / (u32::MAX as f32)
}

// A random unit vector in 3D space
fn random_unit_vector(seed: &mut u32) -> Vec3 {
    let theta = 2.0 * std::f32::consts::PI * rand_f32(seed);
    let z = 2.0 * rand_f32(seed) - 1.0;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * theta.cos(), r * theta.sin(), z)
}

// Recursive function for Path Tracing
fn trace_path(scene: &ViewScene, ray: &super::ray::Ray, depth: u32, seed: &mut u32) -> Vec3 {
    if depth == 0 {
        return Vec3::ZERO;
    }

    if let Some(hit) = intersect(scene, ray) {
        let mut color = Vec3::ZERO;

        // 1. Direct lighting (hardcoded point light)
        let light_pos = Vec3::new(3.0, 5.0, 3.0);
        let light_color = Vec3::new(1.0, 1.0, 1.0);
        let to_light = light_pos - hit.point;
        let distance_to_light = to_light.length();
        let light_dir = to_light / distance_to_light;

        // Shadow ray: check if there is something between the point and the light
        let shadow_ray = super::ray::Ray {
            origin: hit.point + hit.normal * 0.001, // Offset for self-intersection
            direction: light_dir,
        };
        let in_shadow = if let Some(shadow_hit) = intersect(scene, &shadow_ray) {
            (shadow_hit.point - hit.point).length() < distance_to_light
        } else {
            false
        };

        if !in_shadow {
            let diffuse = hit.normal.dot(light_dir).max(0.0);
            color += hit.albedo * light_color * diffuse;
        }

        // 2. Indirect lighting (bounces)
        let normal = if hit.normal.dot(ray.direction) < 0.0 {
            hit.normal
        } else {
            -hit.normal
        };

        // Generate a random direction in the hemisphere oriented by the normal
        let mut next_dir = random_unit_vector(seed);
        if next_dir.dot(normal) < 0.0 {
            next_dir = -next_dir;
        }

        let next_ray = super::ray::Ray {
            origin: hit.point + normal * 0.001,
            direction: next_dir,
        };

        // Russian Roulette: probabilistically terminate the path to avoid infinite recursion
        let p = hit.albedo.max_element().clamp(0.1, 1.0);
        if rand_f32(seed) < p {
            // For cosine-weighted sampling, the contribution is albedo * L (without dividing by PI)
            color += hit.albedo * trace_path(scene, &next_ray, depth - 1, seed) / p;
        }

        return color;
    }

    // Sky color background when the ray doesn't hit anything
    Vec3::new(0.05, 0.05, 0.1)
}

pub fn render(scene: &ViewScene, camera: &Camera, settings: RenderSettings) -> Image {
    let mut image = Image::new(settings.width, settings.height);
    let aspect = settings.width as f32 / settings.height as f32;

    // Parallelise the rows of the image using Rayon
    let rows: Vec<Vec<Vec3>> = (0..settings.height)
        .into_par_iter()
        .map(|y| {
            let mut row = vec![Vec3::ZERO; settings.width as usize];
            let mut seed = 12345 + y; // Unique seed per row

            for _ in 0..settings.samples {
                for x in 0..settings.width {
                    let jitter_x = rand_f32(&mut seed) - 0.5;
                    let jitter_y = rand_f32(&mut seed) - 0.5;

                    let ndc = Vec2::new(
                        2.0 * (x as f32 + 0.5 + jitter_x) / settings.width as f32 - 1.0,
                        1.0 - 2.0 * (y as f32 + 0.5 + jitter_y) / settings.height as f32,
                    );

                    let ray = match camera.ray(ndc, aspect) {
                        Ok(ray) => ray,
                        Err(_) => continue,
                    };

                    let color = trace_path(scene, &ray, settings.max_depth, &mut seed);
                    row[x as usize] += color;
                }
            }
            row
        })
        .collect();

    // Knock over the rows into the image pixels
    for (y, row) in rows.into_iter().enumerate() {
        for (x, color) in row.into_iter().enumerate() {
            image.pixels[y * settings.width as usize + x] = color;
        }
    }

    image
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
            width: 256,
            height: 256,
            samples: 10,
            max_depth: 3,
        };

        let image = render(&view_scene, &camera, settings);

        // If the environment variable is defined, we save there to be able to inspect it.
        // If not, we use a temporary directory to avoid cluttering the repository (CI).
        if let Ok(path_str) = std::env::var("AZTECCRAFT_RENDER_OUTPUT") {
            let path = std::path::PathBuf::from(path_str);
            image.save_ppm(&path, settings.samples).unwrap();
            println!(
                "Imagen Path Traced guardada en: {:?}",
                path.canonicalize().unwrap()
            );
        } else {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("demo_render_path_traced.ppm");
            image.save_ppm(&path, settings.samples).unwrap();
            let metadata = std::fs::metadata(&path).unwrap();
            assert!(metadata.len() > 0);
            // Print the path to the temporary file for debugging.
            println!("Imagen temporal Path Traced guardada en: {:?}", path);
        }
    }
}
