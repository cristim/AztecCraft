use super::*;
use crate::native_scene::NativeTransform;
use crate::native_scene::test_fixtures;
use crate::scene::Primitive;
use crate::viewport::Camera;
use glam::Vec2;

const VIEW_BUDGET: usize = 256 * 1024 * 1024;
const CURVE_CC: &str =
    "setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;";

fn curve_native(body: &str) -> NativeScene {
    crate::native_scene::ascii::decode(std::sync::Arc::from(format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; {body}"
    ).into_bytes())).unwrap()
}

fn shared_curve(hidden_second: bool) -> NativeScene {
    let mut scene = curve_native(&format!(
        "createNode transform -n \"first\"; setAttr \".t\" -type \"double3\" 10 0 0; createNode transform -n \"second\"; setAttr \".t\" -type \"double3\" -10 0 0; setAttr \".v\" {}; createNode nurbsCurve -n \"shape\" -p \"first\"; {CURVE_CC}",
        if hidden_second { "no" } else { "yes" }
    ));
    let second = scene.nodes()[1].id();
    test_fixtures::share_curve_with_transform(&mut scene, second);
    scene
}

#[test]
fn curve_instances_keep_exact_paths_bounds_and_object_only_identity() {
    let scene = shared_curve(false);
    let before = scene.clone();
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    assert!(view.instances().is_empty());
    assert_eq!(view.curve_instances().len(), 2);
    let curve = scene.curves()[0].id();
    for (index, x) in [(0, 10.0f32), (1, -10.0)] {
        let instance = &view.curve_instances()[index];
        let node = &scene.nodes()[index];
        assert_eq!(instance.path(), [node.transform_id(), curve]);
        assert_eq!(instance.transform(), node.id());
        assert_eq!(instance.shape(), curve);
        assert_eq!(
            instance.segments().first().unwrap()[0],
            Vec3::new(x, 0.0, 0.0)
        );
        assert_eq!(
            instance.segments().last().unwrap()[1],
            Vec3::new(x + 3.0, 0.0, 0.0)
        );
        assert_eq!(
            instance.bounds(),
            [
                DVec3::new(f64::from(x), -3.0, 0.0),
                DVec3::new(f64::from(x + 3.0), 3.0, 0.0)
            ]
        );
        assert!(
            scene
                .resolve_face_target(&crate::native_scene::FaceTarget {
                    transform: node.id(),
                    shape: curve,
                    path: instance.path().to_vec(),
                    face_index: 0,
                })
                .is_err()
        );
    }
    let camera = front_camera();
    assert!(camera.pick_view(&view, Vec2::ZERO, 1.0).unwrap().is_none());
    assert_eq!(scene, before);
}

#[test]
fn curve_admission_rejects_bad_paths_and_checks_hidden_ancestors() {
    let mut scene = shared_curve(true);
    let a = scene.nodes()[0].transform_id();
    let b = scene.nodes()[1].transform_id();
    let shape = scene.curves()[0].id();
    assert!(
        scene
            .geometry_view_admission(&[a, shape])
            .unwrap()
            .visible()
    );
    assert!(
        !scene
            .geometry_view_admission(&[b, shape])
            .unwrap()
            .visible()
    );
    for path in [
        vec![],
        vec![shape],
        vec![shape, a],
        vec![a, a, shape],
        vec![a, b, shape],
        vec![a, b],
        vec![shape, a, shape],
    ] {
        assert!(scene.geometry_view_admission(&path).is_err());
    }
    assert_eq!(
        ViewScene::from_native(&scene, VIEW_BUDGET)
            .unwrap()
            .curve_instances()
            .len(),
        1
    );
    let hidden = scene.nodes()[1].id();
    test_fixtures::unprove_curve_ancestor(&mut scene, hidden);
    assert!(scene.geometry_view_admission(&[b, shape]).is_err());
    assert!(ViewScene::from_native(&scene, VIEW_BUDGET).is_err());

    let mut nested = curve_native(&format!(
        "createNode transform -n \"root\"; setAttr \".v\" no; createNode transform -n \"child\" -p \"root\"; createNode nurbsCurve -n \"shape\" -p \"child\"; {CURVE_CC}"
    ));
    let root = nested.nodes()[0].transform_id();
    let child = nested.nodes()[1].transform_id();
    let shape = nested.curves()[0].id();
    assert!(
        !nested
            .geometry_view_admission(&[root, child, shape])
            .unwrap()
            .visible()
    );
    assert!(nested.geometry_view_admission(&[child, shape]).is_err());
    assert!(nested.geometry_view_admission(&[root, child]).is_err());
    let child_node = nested.nodes()[1].id();
    test_fixtures::unprove_curve_ancestor(&mut nested, child_node);
    assert!(
        nested
            .geometry_view_admission(&[root, child, shape])
            .is_err()
    );
    assert!(ViewScene::from_native(&nested, VIEW_BUDGET).is_err());
}

#[test]
fn curve_only_camera_frames_conservative_hull_and_exact_selected_prefix() {
    let scene = shared_curve(false);
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    for index in 0..2 {
        let instance = &view.curve_instances()[index];
        let prefix = &instance.path()[..1];
        let mut camera = front_camera();
        camera.frame_view(&view, Some(prefix), 1.0).unwrap();
        let [low, high] = instance.bounds();
        assert_eq!(camera.target(), ((low + high) * 0.5).as_vec3());
        for p in [low, high] {
            let ndc = camera.project(p.as_vec3(), 1.0).unwrap().unwrap();
            assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
        }
    }
    let mut camera = front_camera();
    camera.frame_view(&view, None, 1.0).unwrap();
    assert_eq!(camera.target(), Vec3::new(1.5, 0.0, 0.0));
    let before = camera.clone();
    assert!(
        camera
            .frame_view(&view, Some(&[scene.curves()[0].id()]), 1.0)
            .is_err()
    );
    assert_eq!(camera, before);
}

#[test]
fn complete_curve_view_budget_accounts_all_prepared_instances() {
    let scene = shared_curve(false);
    let mut low = 0;
    let mut high = VIEW_BUDGET;
    while low < high {
        let middle = low + (high - low) / 2;
        if ViewScene::from_native(&scene, middle).is_ok() {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    assert_eq!(
        ViewScene::from_native(&scene, low)
            .unwrap()
            .curve_instances()
            .len(),
        2
    );
    assert!(matches!(
        ViewScene::from_native(&scene, low - 1),
        Err(ViewError::ResourceLimit)
    ));
    let first_only = curve_native(&format!(
        "createNode transform -n \"first\"; createNode nurbsCurve -n \"shape\" -p \"first\"; {CURVE_CC}"
    ));
    let graph = Graph::prepare(&scene, VIEW_BUDGET).unwrap();
    let requirements = graph.requirements(VIEW_BUDGET).unwrap();
    let first_requirements = Graph::prepare(&first_only, VIEW_BUDGET)
        .unwrap()
        .requirements(VIEW_BUDGET)
        .unwrap();
    let mut first_low = first_requirements.bytes;
    let mut first_high = VIEW_BUDGET;
    while first_low < first_high {
        let middle = first_low + (first_high - first_low) / 2;
        if ViewScene::from_native(&first_only, middle).is_ok() {
            first_high = middle;
        } else {
            first_low = middle + 1;
        }
    }
    let only_one_geometry = requirements.bytes + first_low - first_requirements.bytes;
    assert!(matches!(
        ViewScene::from_native(&scene, only_one_geometry),
        Err(ViewError::ResourceLimit)
    ));
}

#[test]
fn curve_subdivision_work_is_shared_across_all_instance_paths() {
    use std::fmt::Write;
    let spans = 65_536;
    let mut body = String::from(
        "createNode transform -n \"first\"; createNode transform -n \"second\"; createNode transform -n \"third\"; createNode transform -n \"fourth\"; createNode nurbsCurve -n \"shape\" -p \"first\"; setAttr \".cc\" -type \"nurbsCurve\" 3 65536 0 no 3 65541 0 0 0",
    );
    for knot in 1..spans {
        write!(body, " {knot}").unwrap();
    }
    body.push_str(" 65536 65536 65536 65539");
    body.push_str(&" 1 2 3".repeat(65_539));
    body.push(';');
    let mut scene = curve_native(&body);
    for index in 1..3 {
        let node = scene.nodes()[index].id();
        test_fixtures::share_curve_with_transform(&mut scene, node);
    }
    assert_eq!(
        ViewScene::from_native(&scene, VIEW_BUDGET)
            .unwrap()
            .curve_instances()
            .len(),
        3
    );
    let fourth = scene.nodes()[3].id();
    test_fixtures::share_curve_with_transform(&mut scene, fourth);
    let before = scene.clone();
    assert!(matches!(
        ViewScene::from_native(&scene, VIEW_BUDGET),
        Err(ViewError::ResourceLimit)
    ));
    assert_eq!(scene, before);
}
const IDENTITY: InternalTransform = InternalTransform {
    translation_cm: [0.0; 3],
    rotation_radians: [0.0; 3],
    scale: [1.0; 3],
    rotate_pivot_cm: [0.0; 3],
    scale_pivot_cm: [0.0; 3],
};

fn front_camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap()
}

#[test]
fn scale_pivot_is_fixed_and_rotation_uses_its_own_pivot() {
    let pivot = DVec3::new(0.02773666381835227, -1.0603328943252563, 1.9979510307312012);
    let transform = InternalTransform {
        scale: [1.2474601320620895; 3],
        scale_pivot_cm: pivot.to_array(),
        ..IDENTITY
    };
    let matrix = local_matrix(transform).unwrap();
    assert!(matrix.transform_point3(pivot).distance(pivot) < 1e-14);
    let rotation = InternalTransform {
        rotation_radians: [0.0, 0.0, std::f64::consts::FRAC_PI_2],
        rotate_pivot_cm: [2.0, 3.0, 0.0],
        ..IDENTITY
    };
    let matrix = local_matrix(rotation).unwrap();
    assert!(
        matrix
            .transform_point3(DVec3::new(3.0, 3.0, 0.0))
            .distance(DVec3::new(2.0, 4.0, 0.0))
            < 1e-14
    );
}

#[test]
fn internal_centimeter_radian_channels_are_not_converted_again() {
    let transform = InternalTransform {
        translation_cm: [100.0, 0.0, 0.0],
        scale: [2.0, 1.0, 1.0],
        scale_pivot_cm: [2.0, 0.0, 0.0],
        ..IDENTITY
    };
    let matrix = local_matrix(transform).unwrap();
    assert_eq!(
        world_point(matrix, DVec3::new(2.5, 0.0, 0.0)).unwrap(),
        Vec3::new(103.0, 0.0, 0.0)
    );
    let transform = InternalTransform {
        rotation_radians: [0.0, 0.0, std::f64::consts::FRAC_PI_2],
        ..IDENTITY
    };
    let matrix = local_matrix(transform).unwrap();
    assert!(matrix.transform_point3(DVec3::X).distance(DVec3::Y) < 1e-14);
}

#[test]
fn parent_composition_retains_double_precision_before_world_conversion() {
    let parent = InternalTransform {
        translation_cm: [16_777_216.25, 0.0, 0.0],
        ..IDENTITY
    };
    let child = InternalTransform {
        translation_cm: [-16_777_216.0, 0.0, 0.0],
        ..IDENTITY
    };
    let world = local_matrix(parent).unwrap() * local_matrix(child).unwrap();
    assert_eq!(
        world_point(world, DVec3::new(0.5, 0.0, 0.0)).unwrap(),
        Vec3::new(0.75, 0.0, 0.0)
    );
}

#[test]
fn grouped_native_shape_composes_the_complete_parent_path_in_double_precision() {
    let (mut native, root, child, shape) = test_fixtures::grouped_shape();
    native
        .set_transform(
            root,
            NativeTransform {
                translation: [16_777_216.25, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    native
        .set_transform(
            child,
            NativeTransform {
                translation: [-16_777_216.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(view.instances().len(), 1);
    let instance = &view.instances()[0];
    assert_eq!(
        instance.path(),
        &[
            native.node(root).unwrap().transform_id(),
            native.node(child).unwrap().transform_id(),
            shape
        ]
    );
    assert_eq!(instance.transform(), child);
    let Points::Float3(source) = native.shape(shape).unwrap().mesh().points() else {
        panic!("canonical cube must use Float3 points");
    };
    for (actual, source) in instance.world_vertices().iter().zip(source) {
        assert_eq!(actual.x, source[0] + 0.25);
    }
    assert_eq!(native, before);
}

#[test]
fn grouped_native_parent_rotation_and_child_scale_compose_in_path_order() {
    let (mut native, root, child, shape) = test_fixtures::grouped_shape();
    native
        .set_transform(
            root,
            NativeTransform {
                translation: [10.0, -3.0, 2.0],
                rotation: [0.0, 0.0, 90.0],
                ..Default::default()
            },
        )
        .unwrap();
    native
        .set_transform(
            child,
            NativeTransform {
                translation: [1.0, 2.0, 3.0],
                scale: [2.0, 3.0, 4.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let Points::Float3(points) = native.shape(shape).unwrap().mesh().points() else {
        panic!("canonical cube must use Float3 points");
    };
    for (actual, [x, y, z]) in view.instances()[0].world_vertices().iter().zip(points) {
        assert!(actual.distance(Vec3::new(8.0 - 3.0 * y, 2.0 * x - 2.0, 4.0 * z + 5.0)) < 1e-6);
    }
}

#[test]
fn shared_native_shape_keeps_distinct_world_instances_and_numeric_path_ties() {
    let (mut native, first, second, shape) = test_fixtures::shared_shape();
    let coincident = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(coincident.instances().len(), 2);
    let hit = front_camera()
        .pick_view(&coincident, Vec2::ZERO, 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.transform, first);
    assert_eq!(hit.shape, shape);
    native.resolve_face_target(&hit).unwrap();
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [3.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let second_view = view
        .instances()
        .iter()
        .find(|instance| instance.transform() == second)
        .unwrap();
    let first_view = view
        .instances()
        .iter()
        .find(|instance| instance.transform() == first)
        .unwrap();
    for (a, b) in first_view
        .world_vertices()
        .iter()
        .zip(second_view.world_vertices())
    {
        assert_eq!(*b, *a + Vec3::new(3.0, 0.0, 0.0));
    }
    let mut camera = front_camera();
    camera
        .frame_points(second_view.world_vertices(), 1.0)
        .unwrap();
    let hit = camera.pick_view(&view, Vec2::ZERO, 1.0).unwrap().unwrap();
    assert_eq!(hit.transform, second);
    assert_eq!(hit.path, second_view.path());
    native.resolve_face_target(&hit).unwrap();
}

#[test]
fn native_visibility_and_unproved_geometry_admission_are_not_guessed() {
    let (hidden, ..) = test_fixtures::hidden_group();
    let view = ViewScene::from_native(&hidden, VIEW_BUDGET).unwrap();
    assert!(view.instances().is_empty());
    assert!(
        front_camera()
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .is_none()
    );
    let (hidden, _) = test_fixtures::hidden_shape();
    assert!(
        ViewScene::from_native(&hidden, VIEW_BUDGET)
            .unwrap()
            .instances()
            .is_empty()
    );
    let (unproved, ..) = test_fixtures::unproved_group();
    assert!(matches!(
        ViewScene::from_native(&unproved, VIEW_BUDGET),
        Err(ViewError::Native(NativeError::UnsupportedView(_)))
    ));
}

#[test]
fn native_pick_obeys_camera_depth_and_nearest_instance() {
    let (mut native, first, second, _) = test_fixtures::shared_shape();
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [0.0, 0.0, 2.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(
        front_camera()
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .unwrap()
            .transform,
        second
    );
    let clipped = Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 4.0, 6.0).unwrap();
    assert_eq!(
        clipped
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .unwrap()
            .transform,
        first
    );
    native
        .set_transform(
            first,
            NativeTransform {
                translation: [0.0, 0.0, 8.0],
                ..Default::default()
            },
        )
        .unwrap();
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [0.0, 0.0, -110.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert!(
        front_camera()
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        front_camera().pick_view(&view, Vec2::ZERO, 0.0),
        Err(ViewError::Camera(_))
    ));
    assert!(matches!(
        front_camera().pick_view(&view, Vec2::splat(f32::NAN), 1.0),
        Err(ViewError::Camera(_))
    ));
}

#[test]
fn empty_native_scene_has_no_draw_or_pick_geometry() {
    let native = NativeScene::default();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert!(view.instances().is_empty());
    assert!(
        front_camera()
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn real_native_cube_maps_front_and_side_hits_to_original_faces_without_mutation() {
    let mut native = NativeScene::default();
    let node = native.create(Primitive::Cube, "cube").unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let front = front_camera()
        .pick_view(&view, Vec2::ZERO, 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(front.transform, node);
    assert_eq!(front.shape, native.shapes()[0].id());
    assert_eq!(
        front.path,
        vec![native.node(node).unwrap().transform_id(), front.shape]
    );
    assert_eq!(front.face_index, 1);
    native.resolve_face_target(&front).unwrap();
    let side = Camera::new(
        Vec3::ZERO,
        std::f32::consts::FRAC_PI_2,
        0.0,
        5.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap()
    .pick_view(&view, Vec2::ZERO, 1.0)
    .unwrap()
    .unwrap();
    assert_eq!(side.face_index, 3);
    assert!(
        front_camera()
            .pick_view(&view, Vec2::new(0.9, 0.9), 1.0)
            .unwrap()
            .is_none()
    );
    assert_eq!(native, before);
}

#[test]
fn native_combined_xyz_rotation_matches_independent_axis_application() {
    let (mut native, node) = test_fixtures::cube();
    native
        .set_transform(
            node,
            NativeTransform {
                rotation: [90.0; 3],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let Points::Float3(points) = native.shapes()[0].mesh().points() else {
        panic!("canonical cube must use Float3 points");
    };
    for (actual, [x, y, z]) in view.instances()[0].world_vertices().iter().zip(points) {
        assert!(actual.distance(Vec3::new(*z, *y, -*x)) < 1e-6);
    }
}

#[test]
fn native_double_triangle_preserves_low_bits_until_world_conversion_and_original_face_pick() {
    let (native, node) = test_fixtures::triangle_double_shape();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    assert_eq!(
        instance.world_vertices(),
        &[
            Vec3::new(0.25, 0.0, 0.0),
            Vec3::new(1.25, 0.0, 0.0),
            Vec3::new(0.25, 1.0, 0.0)
        ]
    );
    assert_eq!(instance.faces()[0].corners(), &[0, 1, 2]);
    assert_eq!(instance.faces()[0].triangles().count(), 1);
    let camera = front_camera();
    let ndc = camera
        .project(Vec3::new(0.5, 0.25, 0.0), 1.0)
        .unwrap()
        .unwrap()
        .truncate();
    let hit = camera.pick_view(&view, ndc, 1.0).unwrap().unwrap();
    assert_eq!(hit.transform, node);
    assert_eq!(hit.face_index, 0);
    native.resolve_face_target(&hit).unwrap();
    let miss = camera
        .project(Vec3::new(1.1, 0.9, 0.0), 1.0)
        .unwrap()
        .unwrap()
        .truncate();
    assert!(camera.pick_view(&view, miss, 1.0).unwrap().is_none());
    assert_eq!(native, before);
}

#[test]
fn native_nonidentity_pivots_follow_independent_scalar_axis_equations() {
    let (native, _) = test_fixtures::pivoted_shape();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let Points::Float3(points) = native.shapes()[0].mesh().points() else {
        panic!("canonical cube must use Float3 points");
    };
    let (sx, cx) = 20_f64.to_radians().sin_cos();
    let (sy, cy) = 35_f64.to_radians().sin_cos();
    let (sz, cz) = (-15_f64).to_radians().sin_cos();
    for (actual, point) in view.instances()[0].world_vertices().iter().zip(points) {
        let [x, y, z] = point.map(f64::from);
        let [x, y, z] = [
            (x + 0.5) * 2.0 - 0.5 - 1.0,
            (y - 1.0) * 0.5 + 1.0 - 0.25,
            -z + 2.0,
        ];
        let (y, z) = (cx * y - sx * z, sx * y + cx * z);
        let (x, z) = (cy * x + sy * z, -sy * x + cy * z);
        let (x, y) = (cz * x - sz * y, sz * x + cz * y);
        let expected = Vec3::new((x + 4.0) as f32, (y - 1.75) as f32, (z + 3.0) as f32);
        assert!(actual.distance(expected) < 1e-6);
    }
    assert_eq!(native, before);
}

#[test]
fn view_budget_rejects_one_byte_below_the_complete_preflight_requirement() {
    let mut native = NativeScene::default();
    native.create(Primitive::Cube, "cube").unwrap();
    let graph = Graph::prepare(&native, VIEW_BUDGET).unwrap();
    let required = graph.requirements(VIEW_BUDGET).unwrap().bytes;
    assert!(ViewScene::from_native(&native, required).is_ok());
    assert!(matches!(
        ViewScene::from_native(&native, required - 1),
        Err(ViewError::ResourceLimit)
    ));
}

#[test]
fn frame_points_preserves_camera_on_empty_or_nonfinite_data() {
    let mut camera = front_camera();
    let before = camera.clone();
    assert!(camera.frame_points(&[], 1.0).is_err());
    assert_eq!(camera, before);
    assert!(camera.frame_points(&[Vec3::splat(f32::NAN)], 1.0).is_err());
    assert_eq!(camera, before);
    let points = [Vec3::new(20.0, -3.0, 0.0), Vec3::new(28.0, 5.0, 2.0)];
    camera.frame_points(&points, 1.5).unwrap();
    assert_eq!(camera.target(), Vec3::new(24.0, 1.0, 1.0));
    for point in points {
        let projected = camera.project(point, 1.5).unwrap().unwrap();
        assert!(projected.x.abs() <= 1.0 && projected.y.abs() <= 1.0);
    }
}

#[test]
fn reflected_nonuniform_native_geometry_keeps_world_bounds_and_zero_scale_stays_degenerate() {
    let mut native = NativeScene::default();
    let node = native.create(Primitive::Cube, "cube").unwrap();
    native
        .set_transform(
            node,
            NativeTransform {
                translation: [5.0, -2.0, 1.0],
                scale: [-2.0, 3.0, 0.5],
                ..Default::default()
            },
        )
        .unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let points = view.instances()[0].world_vertices();
    let min = points
        .iter()
        .copied()
        .fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let max = points
        .iter()
        .copied()
        .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    assert_eq!(min, Vec3::new(4.0, -3.5, 0.75));
    assert_eq!(max, Vec3::new(6.0, -0.5, 1.25));
    assert_eq!(native, before);
    native
        .set_transform(
            node,
            NativeTransform {
                scale: [0.0; 3],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(view.instances()[0].world_vertices(), &[Vec3::ZERO; 8]);
    assert!(
        front_camera()
            .pick_view(&view, Vec2::ZERO, 1.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn transform_only_shared_paths_are_counted_without_exponential_enumeration() {
    let mut native = NativeScene::default();
    for index in 0..40 {
        native
            .create(Primitive::Cube, &format!("group{index}"))
            .unwrap();
    }
    let records = native.nodes().iter().map(Record::Transform).collect();
    let mut edges = vec![
        Edge {
            parent: None,
            child: 0,
        },
        Edge {
            parent: None,
            child: 1,
        },
    ];
    for layer in 1..20 {
        for parent in [(layer - 1) * 2, (layer - 1) * 2 + 1] {
            for child in [layer * 2, layer * 2 + 1] {
                edges.push(Edge {
                    parent: Some(parent),
                    child,
                });
            }
        }
    }
    edges.sort_unstable();
    let graph = Graph { records, edges };
    assert!(matches!(
        graph.requirements(1024 * 1024),
        Err(ViewError::ResourceLimit)
    ));
}

#[test]
fn triangles_keep_three_corners_and_quad_diagonal_has_source_order() {
    let triangle = ViewFace {
        source_index: 7,
        corners: ViewCorners::Triangle([3, 4, 5]),
    };
    assert_eq!(triangle.corners(), &[3, 4, 5]);
    assert_eq!(triangle.triangles().collect::<Vec<_>>(), [[3, 4, 5]]);
    let quad = ViewFace {
        source_index: 8,
        corners: ViewCorners::Quad([0, 1, 2, 3]),
    };
    assert_eq!(quad.triangles().collect::<Vec<_>>(), [[0, 1, 2], [0, 2, 3]]);
}
#[test]
fn sparse_offsets_are_added_in_f64_before_actual_world_transform() {
    let (scene, _) = crate::native_scene::test_fixtures::offset_triangle_shape();
    let view = super::ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    assert_eq!(view.instances()[0].world_vertices()[0].x, 0.25);
    assert_eq!(view.instances()[0].faces()[0].source_index(), 0);
}
use crate::native_scene::Points;
