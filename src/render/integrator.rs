use super::image::{Image, MAX_SAMPLES, RenderError};
use super::ray::{Ray, intersect};
use crate::view_scene::ViewScene;
use crate::viewport::Camera;
use glam::{Vec2, Vec3};
use std::thread;

// PROVISIONAL: the light and the sky are fixed constants, not scene data, and `intersect` tests
// every triangle of every instance per ray (no BVH). This
// is a prototype integrator. No Maya, Arnold or viewport-render parity is claimed.
const LIGHT_POSITION: Vec3 = Vec3::new(3.0, 5.0, 3.0);
const LIGHT_COLOR: Vec3 = Vec3::new(50.0, 50.0, 50.0);
const SKY_COLOR: Vec3 = Vec3::new(0.05, 0.05, 0.1);

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
    let val = (*seed as f32) / (u32::MAX as f32 + 1.0);
    val.min(0.9999999)
}

fn random_unit_vector(seed: &mut u32) -> Vec3 {
    let theta = 2.0 * std::f32::consts::PI * rand_f32(seed);
    let z = 2.0 * rand_f32(seed) - 1.0;
    let r = (1.0 - z * z).sqrt();
    Vec3::new(r * theta.cos(), r * theta.sin(), z)
}

fn trace_path(scene: &ViewScene, ray: &Ray, depth: u32, seed: &mut u32) -> Vec3 {
    if depth == 0 {
        return Vec3::ZERO;
    }

    if let Some(hit) = intersect(scene, ray) {
        let mut color = Vec3::ZERO;

        let normal = if hit.normal.dot(ray.direction) < 0.0 {
            hit.normal
        } else {
            -hit.normal
        };

        // 1. Direct Lighting
        let light_color = LIGHT_COLOR;
        let to_light = LIGHT_POSITION - hit.point;
        let distance_sq = to_light.length_squared();

        if distance_sq > 1e-8 {
            let light_dir = to_light / distance_sq.sqrt();
            let offset = normal * hit.point.length().max(1.0) * 1e-4;

            let shadow_ray = Ray {
                origin: hit.point + offset,
                direction: light_dir,
            };
            let in_shadow = if let Some(shadow_hit) = intersect(scene, &shadow_ray) {
                (shadow_hit.point - hit.point).length_squared() < distance_sq
            } else {
                false
            };

            if !in_shadow {
                let lambert = normal.dot(light_dir).max(0.0) / std::f32::consts::PI;
                let attenuation = 1.0 / distance_sq;
                color += hit.albedo * light_color * lambert * attenuation;
            }
        }

        // 2. Indirect Lighting
        let mut next_dir = (normal + random_unit_vector(seed)).normalize_or_zero();
        if next_dir.dot(normal) < 0.0 {
            next_dir = -next_dir;
        }

        let offset = normal * hit.point.length().max(1.0) * 1e-4;
        let next_ray = Ray {
            origin: hit.point + offset,
            direction: next_dir,
        };

        let p = hit.albedo.max_element().clamp(0.1, 1.0);
        if rand_f32(seed) < p {
            color += hit.albedo * trace_path(scene, &next_ray, depth - 1, seed) / p;
        }

        return color;
    }

    SKY_COLOR
}

/// A valid `Camera` and bounded image sizes make this infallible in practice; a failure still
/// aborts the render with a typed error instead of dropping the sample.
fn sample_ray(camera: &Camera, ndc: Vec2, aspect: f32) -> Result<Ray, RenderError> {
    camera
        .ray(ndc, aspect)
        .map_err(|_| RenderError::InvalidCameraInput)
}

pub fn render(
    scene: &ViewScene,
    camera: &Camera,
    settings: RenderSettings,
) -> Result<Image, RenderError> {
    if settings.samples == 0 {
        return Err(RenderError::ZeroSamples);
    }
    if settings.samples > MAX_SAMPLES {
        return Err(RenderError::TooManySamples);
    }
    if settings.max_depth == 0 || settings.max_depth > 16 {
        return Err(RenderError::InvalidDepth);
    }

    let mut image = Image::new(settings.width, settings.height)?;
    let aspect = settings.width as f32 / settings.height as f32;

    let threads = thread::available_parallelism().map_or(1, |n| n.get());
    let rows_per_thread = (settings.height as usize).div_ceil(threads);
    let row_len = settings.width as usize;

    thread::scope(|s| {
        let mut handles = Vec::new();
        for (chunk_idx, chunk) in image
            .pixels
            .chunks_mut(rows_per_thread * row_len)
            .enumerate()
        {
            let start_y = chunk_idx * rows_per_thread;
            handles.push(s.spawn(move || -> Result<(), RenderError> {
                for (local_y, row) in chunk.chunks_mut(row_len).enumerate() {
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

                            let ray = sample_ray(camera, ndc, aspect)?;
                            row[x as usize] +=
                                trace_path(scene, &ray, settings.max_depth, &mut seed);
                        }
                    }
                }
                Ok(())
            }));
        }
        for handle in handles {
            match handle.join() {
                Ok(result) => result?,
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
        Ok::<(), RenderError>(())
    })?;

    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_scene::NativeScene;
    use crate::scene::Primitive;

    fn setup_demo_scene() -> (ViewScene, Camera) {
        let mut scene = NativeScene::default();
        let floor_id = scene.create(Primitive::Plane, "floor").unwrap();
        scene
            .set_transform(
                floor_id,
                crate::native_scene::NativeTransform {
                    scale: [10.0, 1.0, 10.0],
                    ..Default::default()
                },
            )
            .unwrap();

        let cube_id = scene.create(Primitive::Cube, "my_cube").unwrap();
        scene
            .set_transform(
                cube_id,
                crate::native_scene::NativeTransform {
                    translation: [0.0, 0.5, 0.0],
                    ..Default::default()
                },
            )
            .unwrap();

        let view_scene = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
        let mut camera = Camera::default();
        // Camera pointing at the center of the cube
        camera
            .set_pose(Vec3::new(0.0, 0.5, 0.0), 5.0, 0.8, 0.6)
            .unwrap();
        (view_scene, camera)
    }

    #[test]
    fn render_demo_cube() {
        let (view_scene, camera) = setup_demo_scene();
        let settings = RenderSettings {
            width: 64,
            height: 64,
            samples: 4,
            max_depth: 2,
        };
        let image = render(&view_scene, &camera, settings).unwrap();
        assert!(
            image
                .pixels
                .iter()
                .all(|p| p.is_finite() && p.min_element() >= 0.0)
        );

        let dir = tempfile::tempdir().unwrap();
        let ppm = dir.path().join("demo.ppm");
        image.save_ppm(&ppm, settings.samples).unwrap();
        assert_eq!(
            std::fs::metadata(&ppm).unwrap().len(),
            "P6\n64 64\n255\n".len() as u64 + 64 * 64 * 3
        );

        let sky_color = SKY_COLOR * 4.0;

        assert!(
            image.pixels[0].distance(sky_color) < 0.01,
            "Top-left pixel should be sky"
        );

        let mut darkest_geom = Vec3::splat(f32::MAX);
        let mut brightest_geom = Vec3::ZERO;
        let mut has_geometry = false;

        for &pixel in &image.pixels {
            if pixel.distance(sky_color) > 0.2 {
                has_geometry = true;
                darkest_geom = darkest_geom.min(pixel);
                brightest_geom = brightest_geom.max(pixel);
            }
        }

        assert!(has_geometry, "The render should hit the cube or floor");

        // **REGRESSION TEST**:
        // The lit geometry should be significantly brighter than the shadow. If someone breaks
        // shadows, the darkest pixel would be as bright as the lit one.
        assert!(
            brightest_geom.length() > darkest_geom.length() * 2.0,
            "The lit floor should be brighter than the cube's shadow. Brightest: {:?}, Darkest: {:?}",
            brightest_geom,
            darkest_geom
        );
    }

    fn occluder_scene(with_occluder: bool) -> ViewScene {
        let mut scene = NativeScene::default();
        let floor_id = scene.create(Primitive::Plane, "floor").unwrap();
        scene
            .set_transform(
                floor_id,
                crate::native_scene::NativeTransform {
                    scale: [10.0, 1.0, 10.0],
                    ..Default::default()
                },
            )
            .unwrap();
        if with_occluder {
            // Small cube on the segment from the floor point (1.5, 0, 1.5) to the light.
            let cube_id = scene.create(Primitive::Cube, "occluder").unwrap();
            scene
                .set_transform(
                    cube_id,
                    crate::native_scene::NativeTransform {
                        translation: [1.95, 1.5, 1.95],
                        scale: [0.4, 0.4, 0.4],
                        ..Default::default()
                    },
                )
                .unwrap();
        }
        ViewScene::from_native(&scene, 1024 * 1024).unwrap()
    }

    #[test]
    fn occluder_between_point_and_light_removes_direct_light() {
        let ray = super::Ray {
            origin: Vec3::new(1.5, 1.0, 1.5),
            direction: Vec3::NEG_Y,
        };
        let mut seed = 1;
        // Depth 1 adds only the direct term; the indirect bounce contributes zero at depth 0.
        let lit = trace_path(&occluder_scene(false), &ray, 1, &mut seed);
        let shadowed = trace_path(&occluder_scene(true), &ray, 1, &mut seed);
        assert!(
            lit.min_element() > 0.0,
            "unoccluded point must be lit: {lit:?}"
        );
        assert_eq!(
            shadowed,
            Vec3::ZERO,
            "occluded point must get no direct light"
        );
    }

    #[test]
    fn sample_ray_reports_invalid_camera_input_as_a_typed_error() {
        let camera = Camera::default();
        assert!(sample_ray(&camera, Vec2::ZERO, 1.0).is_ok());
        for (ndc, aspect) in [
            (Vec2::ZERO, f32::NAN),
            (Vec2::ZERO, 0.0),
            (Vec2::new(f32::NAN, 0.0), 1.0),
            (Vec2::new(2.0, 0.0), 1.0),
        ] {
            assert_eq!(
                sample_ray(&camera, ndc, aspect).err(),
                Some(RenderError::InvalidCameraInput),
                "ndc {ndc:?}, aspect {aspect}"
            );
        }
    }

    #[test]
    fn render_rejects_oversized_settings() {
        let (view_scene, camera) = setup_demo_scene();
        let base = RenderSettings {
            width: 8,
            height: 8,
            samples: 1,
            max_depth: 1,
        };
        let too_many = RenderSettings {
            samples: MAX_SAMPLES + 1,
            ..base
        };
        assert_eq!(
            render(&view_scene, &camera, too_many).err(),
            Some(RenderError::TooManySamples)
        );
        let too_wide = RenderSettings {
            width: u32::MAX,
            height: u32::MAX,
            ..base
        };
        assert_eq!(
            render(&view_scene, &camera, too_wide).err(),
            Some(RenderError::DimensionsTooLarge)
        );
    }

    #[test]
    fn render_rejects_invalid_settings() {
        let (view_scene, camera) = setup_demo_scene();
        let mut settings = RenderSettings {
            width: 64,
            height: 64,
            samples: 0,
            max_depth: 2,
        };

        assert!(
            render(&view_scene, &camera, settings).is_err(),
            "Zero samples should error"
        );

        settings.samples = 4;
        settings.max_depth = 0;
        assert!(
            render(&view_scene, &camera, settings).is_err(),
            "Zero depth should error"
        );

        settings.max_depth = 20;
        assert!(
            render(&view_scene, &camera, settings).is_err(),
            "Depth > 16 should error"
        );
    }
}
