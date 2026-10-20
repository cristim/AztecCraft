use azteccraft::{
    history::{Command, EditOutcome, History},
    native_scene::{
        FaceTarget, NativeError, NativeId, NativeMesh, NativeScene, NativeTransform, ascii, binary,
    },
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
const CYLINDER_TOP: usize = 20;
const CYLINDER_BOTTOM: usize = 21;
const CONE_BASE: usize = 20;

fn solid(primitive: Primitive) -> (NativeScene, NodeId) {
    let mut scene = NativeScene::default();
    let id = scene.create(primitive, "solid").unwrap();
    (scene, id)
}

fn shape_of(scene: &NativeScene, id: NodeId) -> NativeId {
    let transform = scene.node(id).unwrap().transform_id();
    scene
        .dag_edges()
        .iter()
        .find(|edge| edge.parent() == Some(transform))
        .unwrap()
        .child()
}

fn mesh_of(scene: &NativeScene, id: NodeId) -> &NativeMesh {
    scene.shape(shape_of(scene, id)).unwrap().mesh()
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

fn counts(mesh: &NativeMesh) -> (usize, usize, usize) {
    (mesh.point_count(), mesh.edges().len(), mesh.faces().len())
}

fn assert_closed_and_consistently_wound(mesh: &NativeMesh) {
    let mut uses: HashMap<usize, usize> = HashMap::new();
    for face in mesh.faces() {
        for directed in face.outer() {
            *uses.entry(directed.edge).or_default() += 1;
        }
    }
    assert_eq!(uses.len(), mesh.edges().len());
    assert!(uses.values().all(|&n| n == 2));
    let mut directions: HashMap<(usize, usize), usize> = HashMap::new();
    for face in 0..mesh.faces().len() {
        let vertices = face_vertices(mesh, face);
        for i in 0..vertices.len() {
            *directions
                .entry((vertices[i], vertices[(i + 1) % vertices.len()]))
                .or_default() += 1;
        }
    }
    assert_eq!(directions.len(), 2 * mesh.edges().len());
    assert!(directions.values().all(|&n| n == 1));
    for edge in mesh.edges() {
        assert_eq!(edge.smooth(), Some(false));
    }
    for index in 0..mesh.point_count() {
        assert!(point(mesh, index).is_finite());
    }
}

fn assert_outward(mesh: &NativeMesh, solid_center: DVec3) {
    for face in 0..mesh.faces().len() {
        let points: Vec<DVec3> = face_vertices(mesh, face)
            .iter()
            .map(|&i| point(mesh, i))
            .collect();
        let mut normal = DVec3::ZERO;
        for i in 0..points.len() {
            let (p, q) = (points[i], points[(i + 1) % points.len()]);
            normal += DVec3::new(
                (p.y - q.y) * (p.z + q.z),
                (p.z - q.z) * (p.x + q.x),
                (p.x - q.x) * (p.y + q.y),
            );
        }
        let center = points.iter().sum::<DVec3>() / points.len() as f64;
        assert!(
            normal.dot(center - solid_center) > 1e-6,
            "face {face} faces inward"
        );
    }
}

#[test]
fn cylinder_counts_caps_and_geometry_are_the_documented_sc0_variant() {
    let (scene, id) = solid(Primitive::Cylinder);
    let mesh = mesh_of(&scene, id);
    assert_eq!(counts(mesh), (40, 60, 22));
    assert_eq!(40 - 60 + 22, 2);
    let sizes: Vec<usize> = mesh.faces().iter().map(|f| f.outer().len()).collect();
    assert_eq!(sizes.iter().filter(|&&n| n == 4).count(), 20);
    assert_eq!(sizes.iter().filter(|&&n| n == 20).count(), 2);
    assert_eq!((sizes[CYLINDER_TOP], sizes[CYLINDER_BOTTOM]), (20, 20));
    assert_closed_and_consistently_wound(mesh);
    for index in 0..40 {
        let p = point(mesh, index);
        assert!((p.x.hypot(p.z) - 1.0).abs() < 1e-6, "{index}: {p:?}");
        assert_eq!(p.y, if index < 20 { 1.0 } else { -1.0 });
    }
    assert_eq!(point(mesh, 0).to_array(), [1.0, 1.0, 0.0]);
}

#[test]
fn cone_counts_apex_base_and_geometry_are_the_documented_variant() {
    let (scene, id) = solid(Primitive::Cone);
    let mesh = mesh_of(&scene, id);
    assert_eq!(counts(mesh), (21, 40, 21));
    assert_eq!(21 - 40 + 21, 2);
    let sizes: Vec<usize> = mesh.faces().iter().map(|f| f.outer().len()).collect();
    assert_eq!(sizes.iter().filter(|&&n| n == 3).count(), 20);
    assert_eq!(sizes[CONE_BASE], 20);
    assert_closed_and_consistently_wound(mesh);
    assert_eq!(point(mesh, 0).to_array(), [0.0, 1.0, 0.0]);
    for index in 1..21 {
        let p = point(mesh, index);
        assert_eq!(p.y, -1.0);
        assert!((p.x.hypot(p.z) - 1.0).abs() < 1e-6, "{index}: {p:?}");
    }
}

#[test]
fn every_face_normal_points_outward_including_caps_and_apex() {
    let (scene, id) = solid(Primitive::Cylinder);
    assert_outward(mesh_of(&scene, id), DVec3::ZERO);
    let (scene, id) = solid(Primitive::Cone);
    // The volume centroid of a cone sits a quarter of the height above its base.
    assert_outward(mesh_of(&scene, id), DVec3::new(0.0, -0.5, 0.0));
}

#[test]
fn view_scene_round_trips_quads_triangles_and_true_caps() {
    for (primitive, faces, polygons, triangles) in [
        (Primitive::Cylinder, 22, 2, 76),
        (Primitive::Cone, 21, 1, 38),
    ] {
        let (scene, id) = solid(primitive);
        let view = ViewScene::from_native(&scene, BUDGET).unwrap();
        let instance = &view.instances()[0];
        assert_eq!(instance.faces().len(), faces);
        assert_eq!(
            instance
                .faces()
                .iter()
                .filter(|f| f.corners().len() == 20)
                .count(),
            polygons
        );
        let mesh = mesh_of(&scene, id);
        for (index, face) in instance.faces().iter().enumerate() {
            assert_eq!(face.source_index(), index);
            assert_eq!(face.corners(), face_vertices(mesh, index));
        }
        assert_eq!(
            instance
                .faces()
                .iter()
                .map(|f| f.triangles().count())
                .sum::<usize>(),
            triangles
        );
    }
}

fn top_camera() -> Camera {
    let camera = Camera::new(Vec3::ZERO, 0.0, 1.0, 4.0, 1.0, 0.1, 50.0).unwrap();
    assert!(camera.eye().y > 0.0);
    camera
}

fn bottom_camera() -> Camera {
    let camera = Camera::new(Vec3::ZERO, 0.0, -1.0, 4.0, 1.0, 0.1, 50.0).unwrap();
    assert!(camera.eye().y < 0.0);
    camera
}

fn pick_at(camera: &Camera, view: &ViewScene, world: Vec3) -> Option<FaceTarget> {
    let ndc = camera.project(world, 1.0).unwrap().unwrap();
    camera.pick_view(view, ndc.truncate(), 1.0).unwrap()
}

#[test]
fn cap_pick_select_marker_and_framing_work_on_a_real_cylinder() {
    let (scene, _) = solid(Primitive::Cylinder);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let instance = &view.instances()[0];
    for (cap, y, camera) in [
        (CYLINDER_TOP, 1.0_f32, top_camera()),
        (CYLINDER_BOTTOM, -1.0, bottom_camera()),
    ] {
        let face = &instance.faces()[cap];
        let center = face.center(instance.world_vertices()).unwrap();
        assert!(center.distance(Vec3::new(0.0, y, 0.0)) < 1e-6, "{center:?}");
        // A hit on any fan triangle selects the whole cap: the centre and two off-centre points.
        for offset in [(0.0, 0.0), (0.5, 0.3), (-0.5, -0.4), (0.1, -0.6)] {
            let target = pick_at(&camera, &view, Vec3::new(offset.0, y, offset.1)).unwrap();
            assert_eq!(target.face_index, cap, "{offset:?}");
            scene.resolve_face_target(&target).unwrap();
        }
        let top = FaceTarget {
            transform: instance.transform(),
            shape: instance.shape(),
            path: instance.path().to_vec(),
            face_index: cap,
        };
        let mut shot = Camera::new(Vec3::ZERO, 0.0, 1.4, 6.0, 1.0, 0.1, 50.0).unwrap();
        shot.frame_face(&view, &top, 1.0).unwrap();
        assert!(shot.target().distance(center) < 0.5);
        assert!(shot.distance() < 6.0);
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
                [800, 800],
                1.0,
            )
            .unwrap();
        assert_eq!(hit.map(|h| h.face_index), Some(cap));
    }
}

#[test]
fn cone_base_cap_pick_marker_and_framing_work() {
    let (scene, _) = solid(Primitive::Cone);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let instance = &view.instances()[0];
    let face = &instance.faces()[CONE_BASE];
    let center = face.center(instance.world_vertices()).unwrap();
    assert!(
        center.distance(Vec3::new(0.0, -1.0, 0.0)) < 1e-6,
        "{center:?}"
    );
    let camera = bottom_camera();
    for offset in [(0.0, 0.0), (0.5, 0.3), (-0.4, -0.5)] {
        let target = pick_at(&camera, &view, Vec3::new(offset.0, -1.0, offset.1)).unwrap();
        assert_eq!(target.face_index, CONE_BASE, "{offset:?}");
    }
    let apex_side = pick_at(&top_camera(), &view, Vec3::new(0.4, -0.2, 0.0)).unwrap();
    assert!(apex_side.face_index < 20);
    let base = FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: CONE_BASE,
    };
    let mut shot = Camera::new(Vec3::ZERO, 0.0, -1.4, 6.0, 1.0, 0.1, 50.0).unwrap();
    shot.frame_face(&view, &base, 1.0).unwrap();
    assert!(shot.target().distance(center) < 0.5);
    assert!(shot.distance() < 6.0);
}

#[test]
fn cap_and_cone_extrusion_are_refused_and_cylinder_side_quad_extrusion_stays_viewable() {
    let (mut cylinder, id) = solid(Primitive::Cylinder);
    let shape = shape_of(&cylinder, id);
    let before = cylinder.clone();
    for cap in [CYLINDER_TOP, CYLINDER_BOTTOM] {
        assert_eq!(
            cylinder.extrude_face(shape, cap, 0.5),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(cylinder, before);
    }
    cylinder.extrude_face(shape, 3, 0.25).unwrap();
    assert_eq!(counts(cylinder.shape(shape).unwrap().mesh()), (44, 68, 26));
    cylinder.validate().unwrap();
    let view = ViewScene::from_native(&cylinder, BUDGET).unwrap();
    assert_eq!(view.instances()[0].faces().len(), 26);

    let (mut cone, id) = solid(Primitive::Cone);
    let shape = shape_of(&cone, id);
    let before = cone.clone();
    for face in [0, 7, 19, CONE_BASE] {
        assert_eq!(
            cone.extrude_face(shape, face, 0.5),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(cone, before);
    }
}

#[test]
fn undo_redo_restores_the_exact_mesh_and_move_edit_stays_valid() {
    for (primitive, name) in [
        (Primitive::Cylinder, "pCylinder1"),
        (Primitive::Cone, "pCone1"),
    ] {
        let mut history = History::default();
        let outcome = history
            .execute(Command::Create {
                primitive,
                options: None,
                name: name.into(),
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
        assert!(history.undo());
        assert_eq!(history.scene(), &created);
    }
}

#[test]
fn legacy_scene_refuses_both_primitives_without_touching_state() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        let mut legacy = Scene::default();
        legacy.create(Primitive::Cube, "pCube1").unwrap();
        let before = legacy.clone();
        assert_eq!(
            legacy.create(primitive, "x"),
            Err(SceneError::UnsupportedPrimitive(primitive))
        );
        assert_eq!(legacy, before);
        assert_eq!(
            legacy.create(Primitive::Cube, "next").unwrap().to_decimal(),
            "2"
        );
    }
}

#[test]
fn id_exhaustion_is_unchanged_for_cylinders_and_cones() {
    let mut legacy = Scene::default();
    legacy.create(Primitive::Cube, "a").unwrap();
    let mut native = NativeScene::from_legacy(legacy).unwrap();
    native.create(Primitive::Cylinder, "b").unwrap();
    let second = native.create(Primitive::Cone, "c").unwrap();
    assert_eq!(second.to_decimal(), "3");
}

#[test]
fn new_builders_leave_cube_plane_and_sphere_unchanged() {
    let mut scene = NativeScene::default();
    let cube = scene.create(Primitive::Cube, "c").unwrap();
    let plane = scene.create(Primitive::Plane, "p").unwrap();
    let sphere = scene.create(Primitive::Sphere, "s").unwrap();
    scene.create(Primitive::Cylinder, "y").unwrap();
    scene.create(Primitive::Cone, "n").unwrap();
    assert_eq!(counts(mesh_of(&scene, cube)), (8, 12, 6));
    assert_eq!(counts(mesh_of(&scene, plane)), (4, 4, 1));
    assert_eq!(counts(mesh_of(&scene, sphere)), (382, 780, 400));
    assert_eq!(face_vertices(mesh_of(&scene, cube), 0), [0, 3, 2, 1]);
    assert_eq!(face_vertices(mesh_of(&scene, plane), 0), [0, 1, 2, 3]);
}

#[test]
fn sourced_scenes_refuse_cylinder_and_cone_creation_without_mutation() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        let mut opened = binary::decode(Arc::from(binary_fixture::specimen())).unwrap();
        let before = opened.clone();
        assert_eq!(
            opened.create(primitive, "pSolid1"),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(opened, before);
        let mut ascii_scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
        let before = ascii_scene.clone();
        assert_eq!(
            ascii_scene.create(primitive, "pSolid1"),
            Err(NativeError::ReadOnlyFormat)
        );
        assert_eq!(ascii_scene, before);
    }
}
