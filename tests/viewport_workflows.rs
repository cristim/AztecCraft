use azteccraft::scene;
use azteccraft::viewport::Camera;
use glam::{Vec2, Vec3};
use scene::{Primitive, Scene, Transform};
mod common;

fn front_camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 90_f32.to_radians(), 0.1, 20.0).unwrap()
}

fn close(actual: Vec3, expected: Vec3) {
    assert!(
        actual.distance(expected) < 0.0001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn analytic_projection_and_ray_agree_off_center() {
    let camera = front_camera();
    close(camera.eye(), Vec3::new(0.0, 0.0, 5.0));
    close(
        camera.view_matrix().transform_point3(Vec3::ZERO),
        Vec3::new(0.0, 0.0, -5.0),
    );
    let world = Vec3::new(2.0, 1.0, 0.0);
    let projected = camera.project(world, 2.0).unwrap().unwrap();
    assert!((projected.x - 0.2).abs() < 0.0001);
    assert!((projected.y - 0.2).abs() < 0.0001);
    let ray = camera.ray(projected.truncate(), 2.0).unwrap();
    close(ray.origin, camera.eye());
    close(ray.direction, (world - ray.origin).normalize());
    let mut rotated = Camera::default();
    rotated.pan(Vec2::new(1.0, -2.0)).unwrap();
    // Off center but inside the default film-fit frustum, which is narrower than 45 degrees.
    let world = rotated.target() + Vec3::new(0.5, 0.25, 0.0);
    let projected = rotated.project(world, 1.0).unwrap().unwrap();
    assert!(projected.truncate().abs().max_element() <= 1.0);
    close(
        rotated.ray(projected.truncate(), 1.0).unwrap().direction,
        (world - rotated.eye()).normalize(),
    );
}

#[test]
fn nearest_geometry_wins_independent_of_creation_order() {
    let camera = front_camera();
    let mut scene = Scene::default();
    let farther = scene.create(Primitive::Cube, "farther").unwrap();
    let plane = scene.create(Primitive::Plane, "plane").unwrap();
    scene
        .set_transform(
            plane,
            Transform {
                translation: Vec3::new(0.0, 0.0, 1.0),
                rotation_degrees: Vec3::new(90.0, 0.0, 0.0),
                scale: Vec3::splat(3.0),
            },
        )
        .unwrap();
    assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), Some(plane));
    scene
        .set_transform(
            farther,
            Transform {
                translation: Vec3::new(0.0, 0.0, 2.0),
                scale: Vec3::new(-1.0, 1.0, 1.0),
                ..Transform::default()
            },
        )
        .unwrap();
    assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), Some(farther));
    assert_eq!(
        camera.pick(&scene, Vec2::new(0.95, 0.95), 1.0).unwrap(),
        None
    );
    scene
        .set_transform(
            farther,
            Transform {
                scale: Vec3::ZERO,
                ..Transform::default()
            },
        )
        .unwrap();
    assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), Some(plane));
}

#[test]
fn projection_and_picking_respect_depth_clipping() {
    let camera = front_camera();
    for point in [
        Vec3::new(0.0, 0.0, 6.0),
        Vec3::new(0.0, 0.0, 4.95),
        Vec3::new(0.0, 0.0, -16.0),
    ] {
        assert_eq!(camera.project(point, 1.0).unwrap(), None);
    }
    assert!(
        camera
            .project(Vec3::new(100.0, 0.0, 0.0), 1.0)
            .unwrap()
            .is_some()
    );
    let mut scene = Scene::default();
    let plane = scene.create(Primitive::Plane, "plane").unwrap();
    for z in [6.0, 4.95, -16.0] {
        scene
            .set_transform(
                plane,
                Transform {
                    translation: Vec3::new(0.0, 0.0, z),
                    rotation_degrees: Vec3::new(90.0, 0.0, 0.0),
                    ..Transform::default()
                },
            )
            .unwrap();
        assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), None);
    }
}

#[test]
fn framing_fits_translated_rotated_nonuniform_and_collapsed_geometry() {
    let mut scene = Scene::default();
    let cube = scene.create(Primitive::Cube, "cube").unwrap();
    scene
        .set_transform(
            cube,
            Transform {
                translation: Vec3::new(50.0, -20.0, 80.0),
                rotation_degrees: Vec3::new(25.0, 50.0, 10.0),
                scale: Vec3::new(-4.0, 2.0, 10.0),
            },
        )
        .unwrap();
    let mut camera = Camera::default();
    for aspect in [0.25, 1.0, 4.0] {
        camera.frame(scene.node(cube).unwrap(), aspect).unwrap();
        close(camera.target(), Vec3::new(50.0, -20.0, 80.0));
        for vertex in scene.node(cube).unwrap().world_vertices() {
            let projected = camera.project(vertex, aspect).unwrap().unwrap();
            assert!(projected.truncate().abs().max_element() < 1.0);
        }
        assert_eq!(camera.pick(&scene, Vec2::ZERO, aspect).unwrap(), Some(cube));
    }
    scene
        .set_transform(
            cube,
            Transform {
                translation: Vec3::new(3.0, 2.0, 1.0),
                scale: Vec3::ZERO,
                ..Transform::default()
            },
        )
        .unwrap();
    camera.frame(scene.node(cube).unwrap(), 1.0).unwrap();
    assert!(camera.project(camera.target(), 1.0).unwrap().is_some());
    assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), None);
}

#[test]
fn orbit_pan_and_dolly_preserve_camera_relationships() {
    let mut camera = front_camera();
    camera
        .orbit(Vec2::new(std::f32::consts::FRAC_PI_2, 0.0))
        .unwrap();
    close(camera.eye(), Vec3::new(5.0, 0.0, 0.0));
    camera.pan(Vec2::new(2.0, 3.0)).unwrap();
    close(camera.target(), Vec3::new(0.0, 3.0, -2.0));
    camera.dolly(0.5).unwrap();
    assert_eq!(camera.distance(), 2.5);
    close(camera.eye() - camera.target(), Vec3::new(2.5, 0.0, 0.0));
    close(camera.ray(Vec2::ZERO, 1.0).unwrap().direction, Vec3::NEG_X);
    camera.orbit(Vec2::new(0.0, 100.0)).unwrap();
    assert!(camera.view_matrix().is_finite());
}

#[test]
fn invalid_inputs_reject_without_changing_camera() {
    let mut camera = front_camera();
    let before = camera.clone();
    assert!(camera.orbit(Vec2::splat(f32::NAN)).is_err());
    assert!(camera.pan(Vec2::splat(f32::INFINITY)).is_err());
    for factor in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(camera.dolly(factor).is_err());
    }
    assert_eq!(camera, before);
    for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(camera.projection_matrix(aspect).is_err());
    }
    assert!(camera.ray(Vec2::new(1.1, 0.0), 1.0).is_err());
    assert!(camera.project(Vec3::splat(f32::NAN), 1.0).is_err());
    assert!(
        Camera::new(
            Vec3::ZERO,
            0.0,
            std::f32::consts::FRAC_PI_2,
            1.0,
            1.0,
            0.1,
            10.0
        )
        .is_err()
    );
    assert!(Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0, 0.0, 0.1, 10.0).is_err());
    assert!(Camera::new(Vec3::ZERO, 0.0, 0.0, 1.0, 1.0, 10.0, 1.0).is_err());
}

#[test]
fn projection_rejects_finite_world_point_when_ndc_division_overflows() {
    assert!(
        front_camera()
            .project(Vec3::new(f32::MAX, 0.0, 4.75), 2.0)
            .is_err()
    );
}

#[test]
fn reloaded_extrusion_frames_and_picks_the_new_side_outside_original_cube() {
    let dir = tempfile::tempdir().unwrap();
    let (scene, id) = common::extruded_corner();
    let path = dir.path().join("corner.aztec");
    azteccraft::document::save_new(&path, &scene).unwrap();
    let loaded = azteccraft::document::load(&path).unwrap();
    let mut camera = front_camera();
    camera.frame(loaded.node(id).unwrap(), 1.0).unwrap();
    for vertex in loaded.node(id).unwrap().world_vertices() {
        let projected = camera.project(vertex, 1.0).unwrap().unwrap();
        assert!(projected.truncate().abs().max_element() < 1.0);
    }
    let projected = camera
        .project(Vec3::new(0.75, 0.0, 1.0), 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(
        camera.pick(&loaded, projected.truncate(), 1.0).unwrap(),
        Some(id)
    );
    let mut original = Scene::default();
    original.create(Primitive::Cube, "original").unwrap();
    assert_eq!(
        camera.pick(&original, projected.truncate(), 1.0).unwrap(),
        None
    );
}

#[test]
fn empty_mesh_is_unpickable_and_framing_returns_error_without_camera_mutation() {
    let mut scene = Scene::default();
    let id = scene.create(Primitive::Cube, "empty").unwrap();
    let mut value = serde_json::to_value(scene).unwrap();
    value["nodes"][0]["mesh"] = serde_json::json!({"vertices": [], "faces": []});
    let scene: Scene = serde_json::from_value(value).unwrap();
    let mut camera = front_camera();
    let before = camera.clone();
    assert_eq!(camera.pick(&scene, Vec2::ZERO, 1.0).unwrap(), None);
    assert!(camera.frame(scene.node(id).unwrap(), 1.0).is_err());
    assert_eq!(camera, before);
}
