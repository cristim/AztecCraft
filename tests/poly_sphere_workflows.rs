use azteccraft::{
    history::{Command, EditOutcome, History},
    native_scene::{NativeError, NativeMesh, NativeScene, NativeTransform, ascii, binary},
    primitive_options::SINGLE_QUAD_PLANE,
    scene::{NodeId, Primitive, Scene, SceneError},
    view_scene::ViewScene,
    viewport::Camera,
};
use glam::{DVec3, Vec2, Vec3};
use std::{collections::HashMap, sync::Arc};

mod binary_fixture {
    include!("support/maya_binary.rs");
}
mod ascii_fixture {
    include!("support/maya_ascii.rs");
}

const BUDGET: usize = 64 * 1024 * 1024;

fn sphere() -> (NativeScene, NodeId) {
    let mut scene = NativeScene::default();
    let id = scene.create(Primitive::Sphere, "pSphere1").unwrap();
    (scene, id)
}

fn mesh_of(scene: &NativeScene, id: NodeId) -> &NativeMesh {
    let transform = scene.node(id).unwrap().transform_id();
    let shape = scene
        .dag_edges()
        .iter()
        .find(|edge| edge.parent() == Some(transform))
        .unwrap()
        .child();
    scene.shape(shape).unwrap().mesh()
}

fn face_vertices(mesh: &NativeMesh, face: usize) -> Vec<usize> {
    mesh.faces()[face]
        .outer()
        .iter()
        .map(|directed| {
            let [a, b] = mesh.edges()[directed.edge].endpoints();
            if directed.reversed { b } else { a }
        })
        .collect()
}

fn point(mesh: &NativeMesh, index: usize) -> DVec3 {
    DVec3::from_array(mesh.effective_point_cm(index).unwrap())
}

#[test]
fn counts_topology_poles_and_precision_are_the_documented_uv_sphere() {
    let (scene, id) = sphere();
    let mesh = mesh_of(&scene, id);
    assert_eq!(
        (mesh.point_count(), mesh.edges().len(), mesh.faces().len()),
        (382, 780, 400)
    );
    let sizes: Vec<usize> = mesh.faces().iter().map(|f| f.outer().len()).collect();
    assert_eq!(sizes.iter().filter(|&&n| n == 3).count(), 40);
    assert_eq!(sizes.iter().filter(|&&n| n == 4).count(), 360);
    assert_eq!(382 - 780 + 400, 2);
    let mut uses: HashMap<usize, usize> = HashMap::new();
    for face in mesh.faces() {
        for directed in face.outer() {
            *uses.entry(directed.edge).or_default() += 1;
        }
    }
    assert_eq!(uses.len(), 780);
    assert!(uses.values().all(|&n| n == 2));
    // Closed and consistently oriented: each undirected edge is traversed once per direction.
    let mut directions: HashMap<(usize, usize), usize> = HashMap::new();
    for face in 0..mesh.faces().len() {
        let vertices = face_vertices(mesh, face);
        for i in 0..vertices.len() {
            *directions
                .entry((vertices[i], vertices[(i + 1) % vertices.len()]))
                .or_default() += 1;
        }
    }
    assert_eq!(directions.len(), 1560);
    assert!(directions.values().all(|&n| n == 1));
    for index in 0..mesh.point_count() {
        let p = point(mesh, index);
        assert!(p.is_finite());
        assert!((p.length() - 1.0).abs() < 1e-6, "{index}: {p:?}");
    }
    assert_eq!(point(mesh, 0).to_array(), [0.0, 1.0, 0.0]);
    assert_eq!(point(mesh, 381).to_array(), [0.0, -1.0, 0.0]);
    assert!(mesh.edges().iter().all(|e| e.smooth() == Some(false)));
}

#[test]
fn every_face_normal_points_outward() {
    let (scene, id) = sphere();
    let mesh = mesh_of(&scene, id);
    for face in 0..mesh.faces().len() {
        let vertices = face_vertices(mesh, face);
        let points: Vec<DVec3> = vertices.iter().map(|&i| point(mesh, i)).collect();
        let mut normal = DVec3::ZERO;
        for i in 0..points.len() {
            let (p, q) = (points[i], points[(i + 1) % points.len()]);
            normal += DVec3::new(
                (p.y - q.y) * (p.z + q.z),
                (p.z - q.z) * (p.x + q.x),
                (p.x - q.x) * (p.y + q.y),
            );
        }
        let centroid = points.iter().sum::<DVec3>() / points.len() as f64;
        assert!(normal.dot(centroid) > 1e-6, "face {face} faces inward");
    }
}

#[test]
fn view_scene_round_trips_triangles_and_quads() {
    let (scene, id) = sphere();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    assert_eq!(view.instances().len(), 1);
    let instance = &view.instances()[0];
    assert_eq!(instance.world_vertices().len(), 382);
    let faces = instance.faces();
    assert_eq!(faces.len(), 400);
    assert_eq!(faces.iter().filter(|f| f.corners().len() == 3).count(), 40);
    assert_eq!(faces.iter().filter(|f| f.corners().len() == 4).count(), 360);
    let mesh = mesh_of(&scene, id);
    for (index, face) in faces.iter().enumerate() {
        assert_eq!(face.source_index(), index);
        assert_eq!(face.corners(), face_vertices(mesh, index));
    }
}

#[test]
fn pick_face_select_and_framing_work_on_a_triangle_face() {
    let (scene, _) = sphere();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let instance = &view.instances()[0];
    let triangle = instance
        .faces()
        .iter()
        .find(|f| f.corners().len() == 3)
        .unwrap();
    let center = triangle.center(instance.world_vertices()).unwrap();
    let mut camera = Camera::default();
    camera.frame_view(&view, None, 1.0).unwrap();
    let ndc = camera.project(center, 1.0).unwrap().unwrap();
    let target = camera
        .pick_view(&view, ndc.truncate(), 1.0)
        .unwrap()
        .unwrap();
    // Picking resolves to the nearest face along the ray; the top fan is the face whose center
    // was projected, unless a nearer body quad covers it.
    scene.resolve_face_target(&target).unwrap();
    let mut shot = Camera::new(Vec3::ZERO, 0.0, 1.4, 6.0, 1.0, 0.1, 50.0).unwrap();
    let top = azteccraft::native_scene::FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: triangle.source_index(),
    };
    scene.resolve_face_target(&top).unwrap();
    shot.frame_face(&view, &top, 1.0).unwrap();
    assert!(shot.target().distance(center) < 0.5);
    assert!(shot.distance() < 6.0);
    // The pick path used by the viewport: the face center marker of that triangle is hit at its
    // projected point.
    let pixels = [800u32, 800u32];
    let mut camera = Camera::new(Vec3::ZERO, 0.0, 1.0, 4.0, 1.0, 0.1, 50.0).unwrap();
    if camera.eye().y < 0.0 {
        camera = Camera::new(Vec3::ZERO, 0.0, -1.0, 4.0, 1.0, 0.1, 50.0).unwrap();
    }
    assert!(camera.eye().y > 0.0);
    camera.frame_view(&view, None, 1.0).unwrap();
    let ndc = camera.project(center, 1.0).unwrap().unwrap();
    let pointer = Vec2::new((ndc.x * 0.5 + 0.5) * 800.0, (0.5 - ndc.y * 0.5) * 800.0);
    let hit = camera
        .pick_face_center(
            &view,
            Some((
                instance.transform(),
                &instance.path()[..instance.path().len() - 1],
            )),
            pointer,
            pixels,
            1.0,
        )
        .unwrap();
    assert_eq!(hit.map(|h| h.face_index), Some(triangle.source_index()));
}

#[test]
fn pole_triangle_extrusion_is_refused_and_body_quad_extrusion_stays_viewable() {
    let (mut scene, id) = sphere();
    let transform = scene.node(id).unwrap().transform_id();
    let shape = scene
        .dag_edges()
        .iter()
        .find(|e| e.parent() == Some(transform))
        .unwrap()
        .child();
    let before = scene.clone();
    for pole_face in [0, 19, 380, 399] {
        assert_eq!(
            scene.extrude_face(shape, pole_face, 0.5),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(scene, before);
    }
    let body = 20 + 5 * 20 + 3;
    scene.extrude_face(shape, body, 0.25).unwrap();
    let mesh = scene.shape(shape).unwrap().mesh();
    assert_eq!(
        (mesh.point_count(), mesh.edges().len(), mesh.faces().len()),
        (386, 788, 404)
    );
    scene.validate().unwrap();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    assert_eq!(view.instances()[0].faces().len(), 404);
}

#[test]
fn undo_redo_restores_the_exact_mesh_and_move_edit_stays_valid() {
    let mut history = History::default();
    let outcome = history
        .execute(Command::Create {
            primitive: Primitive::Sphere,
            options: None,
            name: "pSphere1".into(),
        })
        .unwrap();
    let EditOutcome::Created(id) = outcome else {
        panic!("expected a created node")
    };
    let created = history.scene().clone();
    assert!(history.undo());
    assert!(history.scene().nodes().is_empty());
    assert!(history.redo());
    assert_eq!(history.scene(), &created);
    assert_eq!(mesh_of(history.scene(), id), mesh_of(&created, id));
    history
        .execute(Command::SetTransform {
            id,
            transform: NativeTransform {
                translation: [3.0, 0.0, -2.0],
                ..NativeTransform::default()
            },
            label: None,
        })
        .unwrap();
    history.scene().validate().unwrap();
    let view = ViewScene::from_native(history.scene(), BUDGET).unwrap();
    assert!(
        view.instances()[0]
            .world_vertices()
            .iter()
            .all(|v| v.is_finite())
    );
    assert_eq!(view.instances()[0].world_vertices()[0].x, 3.0);
    assert!(history.undo());
    assert_eq!(history.scene(), &created);
}

#[test]
fn legacy_scene_refuses_sphere_without_touching_state() {
    let mut legacy = Scene::default();
    let cube = legacy.create(Primitive::Cube, "pCube1").unwrap();
    let before = legacy.clone();
    assert_eq!(
        legacy.create(Primitive::Sphere, "pSphere1"),
        Err(SceneError::UnsupportedPrimitive(Primitive::Sphere))
    );
    assert_eq!(legacy, before);
    assert_eq!(
        legacy.create(Primitive::Cube, "next").unwrap().to_decimal(),
        "2"
    );
    assert_ne!(cube.to_decimal(), "2");
}

#[test]
fn id_exhaustion_is_unchanged_for_spheres() {
    let mut legacy = Scene::default();
    legacy.create(Primitive::Cube, "a").unwrap();
    let mut native = NativeScene::from_legacy(legacy).unwrap();
    native.create(Primitive::Sphere, "b").unwrap();
    let second = native.create(Primitive::Sphere, "c").unwrap();
    assert_eq!(second.to_decimal(), "3");
}

#[test]
fn sphere_is_independent_of_cube_and_plane_builders() {
    let mut scene = NativeScene::default();
    let cube = scene.create(Primitive::Cube, "c").unwrap();
    // The single-quad plane: the default plane is 10x10 since #176.
    let plane = scene
        .create_with_options(Primitive::Plane, &SINGLE_QUAD_PLANE, "p")
        .unwrap();
    let cube_mesh = mesh_of(&scene, cube);
    assert_eq!(
        (
            cube_mesh.point_count(),
            cube_mesh.edges().len(),
            cube_mesh.faces().len()
        ),
        (8, 12, 6)
    );
    let expected: [[f64; 3]; 8] = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    for (i, e) in expected.iter().enumerate() {
        assert_eq!(cube_mesh.effective_point_cm(i).unwrap(), *e);
    }
    assert_eq!(face_vertices(cube_mesh, 0), [0, 3, 2, 1]);
    assert_eq!(face_vertices(cube_mesh, 5), [3, 7, 6, 2]);
    let plane_mesh = mesh_of(&scene, plane);
    assert_eq!(face_vertices(plane_mesh, 0), [0, 1, 2, 3]);
    assert_eq!(plane_mesh.effective_point_cm(2).unwrap(), [0.5, 0.0, 0.5]);
}

#[test]
fn sourced_scenes_refuse_sphere_creation_without_mutation() {
    let binary_bytes: Arc<[u8]> = Arc::from(binary_fixture::specimen());
    let mut opened = binary::decode(binary_bytes).unwrap();
    let before = opened.clone();
    assert_eq!(
        opened.create(Primitive::Sphere, "pSphere1"),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(opened, before);
    let mut ascii_scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let before = ascii_scene.clone();
    assert_eq!(
        ascii_scene.create(Primitive::Sphere, "pSphere1"),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(ascii_scene, before);
}
