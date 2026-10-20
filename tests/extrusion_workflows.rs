use azteccraft::{
    history::{Command, History},
    native_scene::{NativeError, NativeScene},
    primitive_options::SINGLE_QUAD_PLANE,
    scene::{NodeId, Primitive, Scene, SceneError, Transform},
};
use glam::Vec3;
use serde_json::json;
use std::collections::{HashMap, HashSet};
mod common;

fn cube() -> (Scene, NodeId) {
    let mut scene = Scene::default();
    let id = scene.create(Primitive::Cube, "cube").unwrap();
    (scene, id)
}

fn scaled_cube(size: f32) -> (Scene, NodeId) {
    let (scene, id) = cube();
    let mut value = serde_json::to_value(&scene).unwrap();
    value["nodes"][0]["mesh"]["vertices"] = json!(
        scene
            .node(id)
            .unwrap()
            .mesh()
            .vertices()
            .iter()
            .map(|v| *v * size)
            .collect::<Vec<_>>()
    );
    (serde_json::from_value(value).unwrap(), id)
}

#[test]
fn cube_face_replaces_cap_and_adds_four_closed_consistently_wound_sides() {
    let (mut scene, id) = cube();
    let other = scene.create(Primitive::Plane, "other").unwrap();
    let before = scene.clone();
    scene.extrude_face(id, 0, 1.0).unwrap();
    scene.validate().unwrap();
    let mesh = scene.node(id).unwrap().mesh();
    assert_eq!(mesh.vertices().len(), 12);
    assert_eq!(mesh.faces().len(), 10);
    assert_eq!(
        &mesh.vertices()[..8],
        before.node(id).unwrap().mesh().vertices()
    );
    assert_eq!(
        &mesh.faces()[1..6],
        &before.node(id).unwrap().mesh().faces()[1..]
    );
    assert_eq!(scene.node(other).unwrap(), before.node(other).unwrap());
    assert_eq!(mesh.faces()[0], [8, 9, 10, 11]);
    assert_eq!(
        &mesh.vertices()[8..],
        &[
            Vec3::new(-0.5, -0.5, -1.5),
            Vec3::new(-0.5, 0.5, -1.5),
            Vec3::new(0.5, 0.5, -1.5),
            Vec3::new(0.5, -0.5, -1.5)
        ]
    );
    assert!(!mesh.faces().contains(&[0, 3, 2, 1]));
    let mut edges = HashMap::new();
    for face in mesh.faces() {
        let [a, b, c, d] = face.map(|i| mesh.vertices()[i]);
        assert!((b - a).cross(c - a).dot((a + b + c + d) / 4.0) > 0.0);
        for i in 0..4 {
            *edges.entry((face[i], face[(i + 1) % 4])).or_insert(0) += 1;
        }
    }
    for ((a, b), count) in &edges {
        assert_eq!(*count, 1);
        assert_eq!(edges.get(&(*b, *a)), Some(&1));
    }
}

#[test]
fn repeated_inward_zero_and_rounded_displacement_keep_the_selected_cap_index() {
    let (mut scene, id) = cube();
    for (distance, z, vertex_count) in [
        (1.0, -1.5, 12),
        (0.25, -1.75, 16),
        (-0.5, -1.25, 20),
        (0.0, -1.25, 24),
        (1e-10, -1.25, 28),
    ] {
        scene.extrude_face(id, 0, distance).unwrap();
        let mesh = scene.node(id).unwrap().mesh();
        assert_eq!(mesh.vertices().len(), vertex_count);
        assert_eq!(mesh.faces().len(), 6 + (vertex_count - 8));
        for index in mesh.faces()[0] {
            assert_eq!(mesh.vertices()[index].z, z);
        }
        scene.validate().unwrap();
    }
}

#[test]
fn normals_and_displacement_are_computed_before_float32_overflow_or_underflow() {
    for size in [1e-30_f32, 1e30_f32] {
        let (mut scene, id) = scaled_cube(size);
        scene.extrude_face(id, 0, size).unwrap();
        for index in scene.node(id).unwrap().mesh().faces()[0] {
            let z = scene.node(id).unwrap().mesh().vertices()[index].z;
            assert!((z / size + 1.5).abs() < 0.00001);
        }
    }
    let (scene, id) = cube();
    let mut value = serde_json::to_value(scene).unwrap();
    value["nodes"][0]["mesh"]["faces"][0] = json!([1, 2, 3, 0]);
    let mut scene: Scene = serde_json::from_value(value).unwrap();
    scene.extrude_face(id, 0, 1.0).unwrap();
    for index in scene.node(id).unwrap().mesh().faces()[0] {
        assert_eq!(scene.node(id).unwrap().mesh().vertices()[index].z, 0.5);
    }
}

#[test]
fn extrusion_uses_object_space_and_preserves_reflected_nonuniform_and_zero_transforms() {
    for scale in [Vec3::new(-2.0, 3.0, 4.0), Vec3::ZERO] {
        let (mut scene, id) = cube();
        let transform = Transform {
            translation: Vec3::new(10.0, 20.0, 30.0),
            rotation_degrees: Vec3::new(0.0, 0.0, 90.0),
            scale,
        };
        scene.set_transform(id, transform).unwrap();
        scene.extrude_face(id, 0, 1.0).unwrap();
        assert_eq!(scene.node(id).unwrap().transform(), transform);
        let world = scene.node(id).unwrap().world_vertices()[8];
        let expected = if scale == Vec3::ZERO {
            transform.translation
        } else {
            Vec3::new(11.5, 21.0, 24.0)
        };
        assert!(world.distance(expected) < 0.00001);
    }
}

#[test]
fn failed_extrusions_preserve_scene_and_pending_redo() {
    let mut scene = NativeScene::default();
    let id = scene.create(Primitive::Cube, "cube").unwrap();
    let cube_shape = scene.shapes()[0].id();
    scene
        .create_with_options(Primitive::Plane, &SINGLE_QUAD_PLANE, "plane")
        .unwrap();
    let plane_shape = scene.shapes()[1].id();
    let removed = scene.create(Primitive::Cube, "removed").unwrap();
    let removed_shape = scene.shapes()[2].id();
    scene.remove(removed).unwrap();
    let mut history = History::new(scene);
    history
        .execute(Command::Rename {
            id,
            name: "redo_name".into(),
        })
        .unwrap();
    assert!(history.undo());
    let before = history.scene().clone();
    for (shape, face_index, distance, expected) in [
        (
            removed_shape,
            0,
            1.0,
            NativeError::MissingShape(removed_shape),
        ),
        (
            cube_shape,
            99,
            1.0,
            NativeError::Legacy(SceneError::MissingFace(99)),
        ),
        (
            plane_shape,
            0,
            1.0,
            NativeError::Legacy(SceneError::UnsupportedAllFacesExtrusion),
        ),
        (
            plane_shape,
            0,
            0.0,
            NativeError::Legacy(SceneError::UnsupportedAllFacesExtrusion),
        ),
        (
            cube_shape,
            0,
            f64::NAN,
            NativeError::Legacy(SceneError::InvalidExtrusionDistance),
        ),
        (
            cube_shape,
            0,
            f64::INFINITY,
            NativeError::Legacy(SceneError::InvalidExtrusionDistance),
        ),
    ] {
        assert_eq!(
            history.execute(Command::ExtrudeFace {
                shape,
                face_index,
                distance
            }),
            Err(expected)
        );
        assert_eq!(history.scene(), &before);
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }
    assert!(history.redo());
    assert_eq!(history.scene().node(id).unwrap().name(), "redo_name");
}

#[test]
fn default_plane_face_extrudes_and_undo_restores_the_grid() {
    // The default plane is 10x10 since #176, so one face is no longer every face.
    let mut scene = NativeScene::default();
    scene.create(Primitive::Plane, "plane").unwrap();
    let shape = scene.shapes()[0].id();
    let counts = |scene: &NativeScene| {
        let mesh = scene.shapes()[0].mesh();
        (mesh.point_count(), mesh.edges().len(), mesh.faces().len())
    };
    assert_eq!(counts(&scene), (121, 220, 100));
    let before = scene.clone();
    let mut history = History::new(scene);
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 0,
            distance: 1.0,
        })
        .unwrap();
    assert_eq!(counts(history.scene()), (125, 228, 104));
    history.scene().validate().unwrap();
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
}

#[test]
fn undefined_normal_and_local_or_world_overflow_do_not_publish_candidates() {
    let (scene, id) = cube();
    let mut value = serde_json::to_value(scene).unwrap();
    value["nodes"][0]["mesh"]["vertices"] = json!(vec![[0.0; 3]; 8]);
    let collapsed: Scene = serde_json::from_value(value).unwrap();
    let (large, _) = scaled_cube(2e38);
    let (mut world_overflow, _) = cube();
    world_overflow
        .set_transform(
            id,
            Transform {
                scale: Vec3::splat(f32::MAX),
                ..Transform::default()
            },
        )
        .unwrap();
    for (scene, distance, error) in [
        (collapsed, 0.0, SceneError::UndefinedFaceNormal),
        (large, f32::MAX, SceneError::InvalidMesh),
        (world_overflow, 1.0, SceneError::InvalidTransform),
    ] {
        let mut scene = scene;
        let before = scene.clone();
        assert_eq!(scene.extrude_face(id, 0, distance), Err(error));
        assert_eq!(scene, before);
    }
}

#[test]
fn committed_legacy_example_matches_closed_corner_topology_and_export_journey() {
    let (mut scene, id) = common::extruded_corner();
    let expected = scene.clone();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/extruded-cube.aztec");
    let loaded = azteccraft::document::load(&path).unwrap();
    assert_eq!(loaded, expected);
    let mesh = loaded.node(id).unwrap().mesh();
    assert_eq!(mesh.vertices().len(), 16);
    assert_eq!(mesh.faces().len(), 14);
    let mut edges = HashMap::new();
    for face in mesh.faces() {
        for i in 0..4 {
            *edges.entry((face[i], face[(i + 1) % 4])).or_insert(0) += 1;
        }
    }
    for ((a, b), count) in &edges {
        assert_eq!(*count, 1);
        assert_eq!(edges.get(&(*b, *a)), Some(&1));
    }
    for index in mesh.faces()[1] {
        assert_eq!(mesh.vertices()[index].z, 1.0);
    }
    for index in mesh.faces()[7] {
        assert_eq!(mesh.vertices()[index].x, 1.0);
    }
    for vertex in 0..mesh.vertices().len() {
        let mut links: HashMap<usize, Vec<usize>> = HashMap::new();
        for face in mesh.faces() {
            if let Some(i) = face.iter().position(|index| *index == vertex) {
                let (a, b) = (face[(i + 3) % 4], face[(i + 1) % 4]);
                links.entry(a).or_default().push(b);
                links.entry(b).or_default().push(a);
            }
        }
        assert!(links.values().all(|neighbors| neighbors.len() == 2));
        let mut pending = vec![*links.keys().next().unwrap()];
        let mut seen = HashSet::new();
        while let Some(neighbor) = pending.pop() {
            if seen.insert(neighbor) {
                pending.extend(&links[&neighbor]);
            }
        }
        assert_eq!(seen.len(), links.len());
    }
    scene
        .set_transform(
            id,
            Transform {
                translation: Vec3::new(4.0, 5.0, 6.0),
                rotation_degrees: Vec3::new(0.0, 0.0, 90.0),
                scale: Vec3::new(-2.0, 3.0, 0.5),
            },
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let native = dir.path().join("journey.aztec");
    azteccraft::document::save_new(&native, &scene).unwrap();
    let loaded = azteccraft::document::load(&native).unwrap();
    assert_eq!(loaded, scene);
    let obj = dir.path().join("journey.obj");
    azteccraft::obj::export_new(&obj, &loaded).unwrap();
    assert!(std::fs::metadata(obj).unwrap().len() > 0);
    let mut camera = azteccraft::viewport::Camera::default();
    camera.frame(loaded.node(id).unwrap(), 1.0).unwrap();
    assert_eq!(
        camera.pick(&loaded, glam::Vec2::ZERO, 1.0).unwrap(),
        Some(id)
    );
}
