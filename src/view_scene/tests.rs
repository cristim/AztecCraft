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
fn frame_view_many_includes_curve_members_in_the_union() {
    let scene = shared_curve(false);
    let view = ViewScene::from_native(&scene, VIEW_BUDGET).unwrap();
    let prefixes: Vec<&[_]> = view
        .curve_instances()
        .iter()
        .map(|instance| &instance.path()[..1])
        .collect();
    assert_eq!(prefixes.len(), 2);
    let mut union = front_camera();
    union.frame_view_many(&view, &prefixes, 1.0).unwrap();
    let mut all = front_camera();
    all.frame_view(&view, None, 1.0).unwrap();
    assert_eq!(union, all);
    assert_eq!(union.target(), Vec3::new(1.5, 0.0, 0.0));
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
    extras: None,
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
        extras: None,
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
        extras: None,
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
        normal: Vec3::ZERO,
    };
    assert_eq!(triangle.corners(), &[3, 4, 5]);
    assert_eq!(triangle.triangles().collect::<Vec<_>>(), [[3, 4, 5]]);
    let quad = ViewFace {
        source_index: 8,
        corners: ViewCorners::Quad([0, 1, 2, 3]),
        normal: Vec3::ZERO,
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

fn center_fixture(vertices: Vec<Vec3>, corners: ViewCorners) -> (ViewScene, NodeId, Vec<NativeId>) {
    let (native, owner) = test_fixtures::triangle_double_shape();
    let mut view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &mut view.instances[0];
    instance.world_vertices = vertices;
    instance.faces = vec![ViewFace {
        source_index: 0,
        corners,
        normal: Vec3::ZERO,
    }];
    let path = instance.path[..instance.path.len() - 1].to_vec();
    (view, owner, path)
}

fn center_quad(z: f32) -> Vec<Vec3> {
    vec![
        Vec3::new(-1.0, -1.0, z),
        Vec3::new(1.0, -1.0, z),
        Vec3::new(1.0, 1.0, z),
        Vec3::new(-1.0, 1.0, z),
    ]
}

fn center_pointer(camera: &Camera, point: Vec3, size: [u32; 2], scale: f32) -> Vec2 {
    let ndc = camera
        .project(point, size[0] as f32 / size[1] as f32)
        .unwrap()
        .unwrap();
    Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * Vec2::new(size[0] as f32, size[1] as f32) / scale
}

#[test]
fn center_mean_uses_all_ordered_corners_without_f32_accumulation_overflow() {
    let face = ViewFace {
        source_index: 0,
        corners: ViewCorners::Triangle([0, 1, 2]),
        normal: Vec3::ZERO,
    };
    assert_eq!(
        face.center(&[Vec3::splat(f32::MAX); 3]).unwrap(),
        Vec3::splat(f32::MAX)
    );
    assert_eq!(
        face.center(&[Vec3::ZERO, Vec3::X * 3.0, Vec3::Y * 6.0])
            .unwrap(),
        Vec3::new(1.0, 2.0, 0.0)
    );
    assert_eq!(face.center(&[Vec3::ZERO]), Err(ViewError::InvalidGeometry));
    assert_eq!(
        face.center(&[Vec3::splat(f32::NAN); 3]),
        Err(ViewError::NonFiniteWorld)
    );
}

#[test]
fn center_click_hits_triangle_and_quad_centers_but_not_the_face_body() {
    for (vertices, corners) in [
        (
            vec![
                Vec3::new(-1.0, -1.0, 0.0),
                Vec3::new(1.0, -1.0, 0.0),
                Vec3::Y,
            ],
            ViewCorners::Triangle([0, 1, 2]),
        ),
        (center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3])),
    ] {
        let (view, owner, path) = center_fixture(vertices, corners);
        let center = view.instances[0].faces[0]
            .center(&view.instances[0].world_vertices)
            .unwrap();
        let camera = front_camera();
        for scale in [1.0, 2.0] {
            let size = [800, 600];
            let pointer = center_pointer(&camera, center, size, scale);
            let hit = camera
                .pick_face_center(&view, Some((owner, &path)), pointer, size, scale)
                .unwrap()
                .unwrap();
            assert_eq!(hit.transform, owner);
            assert_eq!(hit.path, view.instances[0].path);
            assert_eq!(hit.face_index, 0);
            for (offset, expected) in [(3.9, true), (4.1, false)] {
                assert_eq!(
                    camera
                        .pick_face_center(
                            &view,
                            Some((owner, &path)),
                            pointer + Vec2::X * offset / scale,
                            size,
                            scale
                        )
                        .unwrap()
                        .is_some(),
                    expected
                );
            }
        }
        let body = center_pointer(&camera, Vec3::new(0.25, -0.5, 0.0), [800, 600], 1.0);
        assert!(
            camera
                .pick_view(
                    &view,
                    camera
                        .project(Vec3::new(0.25, -0.5, 0.0), 800.0 / 600.0)
                        .unwrap()
                        .unwrap()
                        .truncate(),
                    800.0 / 600.0
                )
                .unwrap()
                .is_some()
        );
        assert!(
            camera
                .pick_face_center(&view, Some((owner, &path)), body, [800, 600], 1.0)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn center_pick_preserves_real_shared_instance_and_selected_group_scope() {
    let (native, first, second, shape) = test_fixtures::shared_shape();
    let before = native.clone();
    let mut view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let camera = front_camera();
    for owner in [first, second] {
        let path = vec![native.node(owner).unwrap().transform_id()];
        let hit = camera
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0,
            )
            .unwrap()
            .unwrap();
        assert_eq!(hit.transform, owner);
        assert_eq!(hit.shape, shape);
        assert_eq!(&hit.path[..hit.path.len() - 1], path);
        native.resolve_face_target(&hit).unwrap();
    }
    view.instances.reverse();
    let path = vec![native.node(second).unwrap().transform_id()];
    assert_eq!(
        camera
            .pick_face_center(
                &view,
                Some((second, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .unwrap()
            .transform,
        second
    );
    assert_eq!(native, before);
    let (native, root, child, _) = test_fixtures::grouped_shape();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let root_path = vec![native.node(root).unwrap().transform_id()];
    let child_path = vec![root_path[0], native.node(child).unwrap().transform_id()];
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((root, &root_path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .is_none()
    );
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((child, &child_path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .is_some()
    );
    assert!(
        camera
            .pick_face_center(&view, None, Vec2::splat(300.0), [600, 600], 1.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn center_visibility_uses_center_ray_even_for_offset_clicks_and_all_faces_occlude() {
    let (mut view, owner, path) = center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
    let instance = &mut view.instances[0];
    instance.world_vertices.extend([
        Vec3::new(-0.01, -0.01, 1.0),
        Vec3::new(0.01, -0.01, 1.0),
        Vec3::new(0.01, 0.01, 1.0),
        Vec3::new(-0.01, 0.01, 1.0),
    ]);
    instance.faces.push(ViewFace {
        source_index: 1,
        corners: ViewCorners::Quad([4, 5, 6, 7]),
        normal: Vec3::ZERO,
    });
    let camera = front_camera();
    let hit = camera
        .pick_face_center(
            &view,
            Some((owner, &path)),
            Vec2::new(303.0, 300.0),
            [600, 600],
            1.0,
        )
        .unwrap()
        .unwrap();
    assert_eq!(hit.face_index, 1);
    // The offset mouse ray misses the small blocker, but the center ray must not.
    assert_eq!(
        camera
            .pick_view(&view, Vec2::new(0.01, 0.0), 1.0)
            .unwrap()
            .unwrap()
            .face_index,
        0
    );
    view.instances[0].faces[1].source_index = 7;
    assert_eq!(
        camera
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .unwrap()
            .face_index,
        7
    );
}

#[test]
fn center_planar_equality_is_visible_but_nonplanar_quad_can_occlude_its_own_center() {
    let (mut view, owner, path) = center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
    let camera = front_camera();
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .is_some()
    );
    view.instances[0].world_vertices[0].z = 1.0;
    view.instances[0].world_vertices[2].z = 1.0;
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .is_none()
    );
}

#[test]
fn center_pick_rejects_invalid_viewports_and_clips_before_dividing() {
    let (mut view, owner, path) = center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
    let camera = front_camera();
    for (size, pointer, scale) in [
        ([0, 600], Vec2::ZERO, 1.0),
        ([600, 0], Vec2::ZERO, 1.0),
        ([600, 600], Vec2::splat(f32::NAN), 1.0),
        ([600, 600], Vec2::ZERO, 0.0),
        ([600, 600], Vec2::ZERO, f32::INFINITY),
    ] {
        assert!(matches!(
            camera.pick_face_center(&view, Some((owner, &path)), pointer, size, scale),
            Err(ViewError::Camera(_))
        ));
    }
    for displacement in [
        Vec3::new(100.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 6.0),
        Vec3::new(0.0, 0.0, -110.0),
    ] {
        view.instances[0].world_vertices = center_quad(0.0)
            .into_iter()
            .map(|v| v + displacement)
            .collect();
        assert!(
            camera
                .pick_face_center(
                    &view,
                    Some((owner, &path)),
                    Vec2::splat(300.0),
                    [600, 600],
                    1.0
                )
                .unwrap()
                .is_none()
        );
    }
    view.instances[0].world_vertices = vec![Vec3::splat(f32::MAX); 4];
    assert!(matches!(
        camera.pick_face_center(
            &view,
            Some((owner, &path)),
            Vec2::splat(300.0),
            [600, 600],
            1.0
        ),
        Err(ViewError::NonFiniteWorld)
    ));
}

#[test]
fn center_work_admission_counts_only_click_eligible_faces_and_checks_exact_product() {
    let (mut view, owner, path) = center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
    for index in 1..3000 {
        let start = view.instances[0].world_vertices.len();
        view.instances[0].world_vertices.extend(
            center_quad(0.0)
                .into_iter()
                .map(|v| v + Vec3::X * index as f32 * 3.0),
        );
        view.instances[0].faces.push(ViewFace {
            source_index: index,
            corners: ViewCorners::Quad([start, start + 1, start + 2, start + 3]),
            normal: Vec3::ZERO,
        });
    }
    assert_eq!(
        front_camera()
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0
            )
            .unwrap()
            .unwrap()
            .face_index,
        0
    );
    assert!(crate::viewport::face_center_work(1, 1_000_000).is_ok());
    assert!(crate::viewport::face_center_work(1000, 1000).is_ok());
    assert_eq!(
        crate::viewport::face_center_work(1, 1_000_001),
        Err(ViewError::ResourceLimit)
    );
    assert_eq!(
        crate::viewport::face_center_work(usize::MAX, 2),
        Err(ViewError::ResourceLimit)
    );
    assert!(crate::viewport::face_center_work(0, usize::MAX).is_ok());
    let face = view.instances[0].faces[0].clone();
    view.instances[0].faces = vec![face; 1000];
    assert_eq!(
        front_camera().pick_face_center(
            &view,
            Some((owner, &path)),
            Vec2::splat(300.0),
            [600, 600],
            1.0
        ),
        Err(ViewError::ResourceLimit)
    );
}

#[test]
fn center_visibility_uses_rendered_near_far_clipping_for_positive_occluder_hits() {
    let camera = Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 4.0, 6.0).unwrap();
    for (occluder_z, expected) in [(2.0, true), (1.0, false), (-2.0, true)] {
        let (mut view, owner, path) =
            center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
        let (mut blocker, _, _) =
            center_fixture(center_quad(occluder_z), ViewCorners::Quad([0, 1, 2, 3]));
        blocker.instances[0].transform = NodeId::new(999);
        view.instances.append(&mut blocker.instances);
        assert_eq!(
            camera
                .pick_face_center(
                    &view,
                    Some((owner, &path)),
                    Vec2::splat(300.0),
                    [600, 600],
                    1.0
                )
                .unwrap()
                .is_some(),
            expected,
            "occluder z={occluder_z}"
        );
    }
}

#[test]
fn center_work_boundary_is_enforced_on_the_actual_picker() {
    let (mut view, owner, path) = center_fixture(
        vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::Y,
        ],
        ViewCorners::Triangle([0, 1, 2]),
    );
    let face = view.instances[0].faces[0].clone();
    view.instances[0].faces = (0..1000)
        .map(|index| ViewFace {
            source_index: index,
            ..face.clone()
        })
        .collect();
    let camera = front_camera();
    let pointer = center_pointer(&camera, Vec3::new(0.0, -1.0 / 3.0, 0.0), [600, 600], 1.0);
    assert_eq!(
        camera
            .pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0)
            .unwrap()
            .unwrap()
            .face_index,
        0
    );
    view.instances[0].faces.push(face);
    assert_eq!(
        camera.pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0),
        Err(ViewError::ResourceLimit)
    );
}

#[test]
fn center_pick_handles_perspective_and_reflected_world_geometry_without_scene_edits() {
    let (mut native, owner) = test_fixtures::cube();
    native
        .set_transform(
            owner,
            NativeTransform {
                translation: [0.7, -0.4, 0.5],
                rotation: [17.0, 31.0, 12.0],
                scale: [-2.0, 1.5, 0.75],
            },
        )
        .unwrap();
    let before = native.clone();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let path = [native.node(owner).unwrap().transform_id()];
    let camera = Camera::default();
    let mut hits = 0;
    for face in view.instances[0].faces() {
        let center = face.center(view.instances[0].world_vertices()).unwrap();
        let pointer = center_pointer(&camera, center, [1200, 800], 2.0);
        if let Some(target) = camera
            .pick_face_center(&view, Some((owner, &path)), pointer, [1200, 800], 2.0)
            .unwrap()
        {
            assert_eq!(target.face_index, face.source_index());
            native.resolve_face_target(&target).unwrap();
            hits += 1;
        }
    }
    assert!(hits > 0 && hits < 6);
    assert_eq!(native, before);
}

mod center_wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[test]
fn center_degenerate_geometry_needs_no_own_surface_hit_but_obeys_occlusion() {
    let (mut view, owner, path) =
        center_fixture(vec![Vec3::ZERO; 3], ViewCorners::Triangle([0, 1, 2]));
    let camera = front_camera();
    let pointer = Vec2::splat(300.0);
    assert!(
        camera
            .pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0)
            .unwrap()
            .is_some()
    );
    let (mut blocker, _, _) = center_fixture(center_quad(1.0), ViewCorners::Quad([0, 1, 2, 3]));
    blocker.instances[0].transform = NodeId::new(999);
    view.instances.append(&mut blocker.instances);
    assert!(
        camera
            .pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0)
            .unwrap()
            .is_none()
    );
}

#[test]
fn center_pick_keeps_two_direct_imported_shapes_and_excludes_descendant_shapes() {
    use center_wire::{chunk, creation, group, mesh_payload, scene, triple};
    let mesh = |name: &[u8], parent: &[u8]| {
        group(
            b"DMSH",
            &[
                creation(name, Some(parent)),
                chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
            ]
            .concat(),
            4,
        )
    };
    let bytes = scene(
        &[
            group(b"XFRM", &creation(b"root", None), 4),
            mesh(b"first", b"root"),
            mesh(b"second", b"root"),
            group(
                b"XFRM",
                &[
                    creation(b"child", Some(b"root")),
                    triple(b"t", [1.25, 0.0, 0.0]),
                ]
                .concat(),
                4,
            ),
            mesh(b"descendant", b"child"),
        ]
        .concat(),
    );
    let native = crate::native_scene::binary::decode(bytes.clone()).unwrap();
    let before = native.clone();
    let mut view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let root = native
        .nodes()
        .iter()
        .find(|node| node.name() == "root")
        .unwrap();
    let path = [root.transform_id()];
    let camera = front_camera();
    let pointer = center_pointer(
        &camera,
        Vec3::new(1.0 / 3.0, 1.0 / 3.0, 0.0),
        [800, 800],
        1.0,
    );
    let expected = view
        .instances()
        .iter()
        .filter(|instance| instance.transform() == root.id())
        .min_by_key(|instance| instance.path())
        .unwrap()
        .shape();
    for _ in 0..2 {
        let target = camera
            .pick_face_center(&view, Some((root.id(), &path)), pointer, [800, 800], 1.0)
            .unwrap()
            .unwrap();
        assert_eq!(target.shape, expected);
        native.resolve_face_target(&target).unwrap();
        view.instances.reverse();
    }
    let descendant_pointer = center_pointer(
        &camera,
        Vec3::new(1.25 + 1.0 / 3.0, 1.0 / 3.0, 0.0),
        [800, 800],
        1.0,
    );
    assert!(
        camera
            .pick_face_center(
                &view,
                Some((root.id(), &path)),
                descendant_pointer,
                [800, 800],
                1.0
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(native, before);
    // Exercise the shared distributed specimen through the same checked decoder.
    let imported = crate::native_scene::binary::decode(center_wire::specimen().into()).unwrap();
    assert_eq!(
        ViewScene::from_native(&imported, VIEW_BUDGET)
            .unwrap()
            .instances()
            .len(),
        1
    );
}

fn default_cube_center_view() -> (ViewScene, NodeId, Vec<NativeId>, Vec3) {
    let mut native = NativeScene::default();
    let owner = native.create(Primitive::Cube, "cube").unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances[0];
    let path = instance.path[..instance.path.len() - 1].to_vec();
    let vertices = &instance.world_vertices;
    let centroid = vertices.iter().copied().sum::<Vec3>() / vertices.len() as f32;
    (view, owner, path, centroid)
}

fn cube_camera(target: Vec3, yaw: f32, pitch: f32, distance: f32) -> Camera {
    Camera::new(
        target,
        yaw,
        pitch,
        distance,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap()
}

#[test]
fn center_pick_returns_own_face_for_reviewed_depth_rounding_pose() {
    let (view, owner, path, centroid) = default_cube_center_view();
    let camera = cube_camera(centroid, 0.1, -0.43, 10.3);
    let instance = &view.instances[0];
    let face = instance.faces.iter().find(|f| f.source_index == 3).unwrap();
    let center = face.center(&instance.world_vertices).unwrap();
    for size in [[600, 600], [800, 600], [1280, 720], [1920, 1080]] {
        let pointer = center_pointer(&camera, center, size, 1.0);
        let hit = camera
            .pick_face_center(&view, Some((owner, &path)), pointer, size, 1.0)
            .unwrap();
        assert_eq!(hit.map(|h| h.face_index), Some(3), "{size:?}");
    }
}

#[test]
fn center_pick_returns_own_face_for_every_visible_cube_center_over_a_pose_sweep() {
    let (view, owner, path, centroid) = default_cube_center_view();
    let instance = &view.instances[0];
    let size = [600, 600];
    let mut visible = 0;
    for yaw_step in 0..24 {
        let yaw = yaw_step as f32 * std::f32::consts::TAU / 24.0 + 0.1;
        for pitch in [-1.2, -0.8, -0.43, 0.0, 0.3, 0.7, 1.2] {
            for distance in [4.0, 6.5, 10.3, 20.0, 40.0, 80.0] {
                let camera = cube_camera(centroid, yaw, pitch, distance);
                for face in &instance.faces {
                    let center = face.center(&instance.world_vertices).unwrap();
                    let front = (center - centroid).dot(camera.eye() - center) > 0.0;
                    if !front || camera.project(center, 1.0).unwrap().is_none() {
                        continue;
                    }
                    visible += 1;
                    let pointer = center_pointer(&camera, center, size, 1.0);
                    let hit = camera
                        .pick_face_center(&view, Some((owner, &path)), pointer, size, 1.0)
                        .unwrap();
                    assert_eq!(
                        hit.map(|h| h.face_index),
                        Some(face.source_index),
                        "yaw {yaw} pitch {pitch} distance {distance} face {}",
                        face.source_index
                    );
                }
            }
        }
    }
    assert!(visible > 1000, "{visible}");
}

#[test]
fn center_pick_still_rejects_back_face_centers_and_centers_behind_another_object() {
    let (view, owner, path, centroid) = default_cube_center_view();
    let instance = &view.instances[0];
    let size = [600, 600];
    let camera = cube_camera(centroid, 0.1, -0.43, 10.3);
    for face in &instance.faces {
        let center = face.center(&instance.world_vertices).unwrap();
        if (center - centroid).dot(camera.eye() - center) >= 0.0 {
            continue;
        }
        let pointer = center_pointer(&camera, center, size, 1.0);
        let hit = camera
            .pick_face_center(&view, Some((owner, &path)), pointer, size, 1.0)
            .unwrap();
        assert!(hit.is_none(), "back face {} was picked", face.source_index);
    }
    // A second object just in front of the cube's front-face center occludes it.
    let (mut view, owner, path, centroid) = default_cube_center_view();
    let camera = cube_camera(centroid, 0.1, -0.43, 10.3);
    let instance = &mut view.instances[0];
    let face = instance.faces.iter().find(|f| f.source_index == 3).unwrap();
    let center = face.center(&instance.world_vertices).unwrap();
    let toward = (camera.eye() - center).normalize();
    let base = instance.world_vertices.len();
    let right = toward.cross(Vec3::Y).normalize() * 0.05;
    let up = toward.cross(right).normalize() * 0.05;
    let plate = center + toward * 0.01;
    instance.world_vertices.extend([
        plate - right - up,
        plate + right - up,
        plate + right + up,
        plate - right + up,
    ]);
    instance.faces.push(ViewFace {
        source_index: 99,
        corners: ViewCorners::Quad([base, base + 1, base + 2, base + 3]),
        normal: Vec3::ZERO,
    });
    let pointer = center_pointer(&camera, center, size, 1.0);
    let hit = camera
        .pick_face_center(&view, Some((owner, &path)), pointer, size, 1.0)
        .unwrap();
    assert_eq!(hit.map(|h| h.face_index), Some(99));
}

#[test]
fn center_depth_tolerance_admits_rounding_gaps_but_not_half_percent_occluders() {
    let camera = front_camera();
    for (gap, expected) in [(0.0001, Some(0)), (0.025, None)] {
        let (mut view, owner, path) =
            center_fixture(center_quad(0.0), ViewCorners::Quad([0, 1, 2, 3]));
        let instance = &mut view.instances[0];
        // The blocker covers the base center ray but its own center is far off-pointer.
        instance.world_vertices.extend(
            center_quad(gap)
                .into_iter()
                .map(|v| v * 0.5 + Vec3::new(0.3, 0.0, 0.0)),
        );
        instance.faces.push(ViewFace {
            source_index: 1,
            corners: ViewCorners::Quad([4, 5, 6, 7]),
            normal: Vec3::ZERO,
        });
        let hit = camera
            .pick_face_center(
                &view,
                Some((owner, &path)),
                Vec2::splat(300.0),
                [600, 600],
                1.0,
            )
            .unwrap()
            .map(|hit| hit.face_index);
        assert_eq!(hit, expected, "gap {gap}");
    }
}

fn ngon_points(count: usize, radius: f64) -> Vec<[f64; 3]> {
    (0..count)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / count as f64;
            [radius * angle.cos(), radius * angle.sin(), 0.0]
        })
        .collect()
}

fn ngon_view(points: Vec<[f64; 3]>) -> Result<(ViewScene, NativeScene, NodeId), ViewError> {
    let corners = (0..points.len()).collect();
    let (native, owner) = test_fixtures::polygon_shape(points, &[corners]);
    ViewScene::from_native(&native, VIEW_BUDGET).map(|view| (view, native, owner))
}

fn refused(points: Vec<[f64; 3]>, order: Vec<usize>) -> ViewError {
    let (native, _) = test_fixtures::polygon_shape(points, &[order]);
    ViewScene::from_native(&native, VIEW_BUDGET).unwrap_err()
}

#[test]
fn convex_pentagon_and_twenty_gon_are_accepted_as_one_face_each() {
    for count in [5, 20] {
        let (view, _, _) = ngon_view(ngon_points(count, 1.0)).unwrap();
        let instance = &view.instances()[0];
        assert_eq!(instance.faces().len(), 1);
        let face = &instance.faces()[0];
        assert!(matches!(face.corners, ViewCorners::Polygon(_)));
        assert_eq!(face.source_index(), 0);
        assert_eq!(face.corners(), (0..count).collect::<Vec<_>>());
        let expected: Vec<[usize; 3]> = (1..count - 1).map(|i| [0, i, i + 1]).collect();
        assert_eq!(face.triangles().collect::<Vec<_>>(), expected);
        assert_eq!(face.triangles().count(), count - 2);
    }
}

#[test]
fn quad_and_triangle_keep_their_variants_and_the_pinned_fan() {
    let quad = ViewFace {
        source_index: 0,
        corners: ViewCorners::Quad([4, 5, 6, 7]),
        normal: Vec3::ZERO,
    };
    assert_eq!(quad.triangles().collect::<Vec<_>>(), [[4, 5, 6], [4, 6, 7]]);
    let (view, _, _) = ngon_view(ngon_points(4, 1.0)).unwrap();
    assert!(matches!(
        view.instances()[0].faces()[0].corners,
        ViewCorners::Quad(_)
    ));
}

#[test]
fn twenty_gon_picks_every_fan_triangle_as_its_one_source_face() {
    let (view, _, _) = ngon_view(ngon_points(20, 1.0)).unwrap();
    let instance = &view.instances()[0];
    let camera = front_camera();
    for triangle in instance.faces()[0].triangles() {
        let [a, b, c] = triangle.map(|i| instance.world_vertices()[i]);
        let ndc = camera.project((a + b + c) / 3.0, 1.0).unwrap().unwrap();
        let hit = camera
            .pick_view(&view, Vec2::new(ndc.x, ndc.y), 1.0)
            .unwrap()
            .unwrap();
        assert_eq!(hit.face_index, 0);
    }
}

#[test]
fn twenty_gon_center_marker_is_the_corner_average_and_clickable() {
    let (view, _, owner) = ngon_view(ngon_points(20, 1.0)).unwrap();
    let instance = &view.instances()[0];
    let center = instance.faces()[0]
        .center(instance.world_vertices())
        .unwrap();
    assert!(center.abs_diff_eq(Vec3::ZERO, 1e-6));
    let path = instance.path()[..instance.path().len() - 1].to_vec();
    let camera = front_camera();
    let pointer = center_pointer(&camera, center, [600, 600], 1.0);
    let hit = camera
        .pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.face_index, 0);
}

#[test]
fn frame_face_frames_a_twenty_gon_like_its_points() {
    let (view, _, owner) = ngon_view(ngon_points(20, 1.0)).unwrap();
    let instance = &view.instances()[0];
    let target = crate::native_scene::FaceTarget {
        transform: owner,
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 0,
    };
    let mut camera = front_camera();
    camera.frame_face(&view, &target, 1.5).unwrap();
    let mut expected = front_camera();
    expected
        .frame_points(instance.world_vertices(), 1.5)
        .unwrap();
    assert_eq!(camera, expected);
    let mut missing = target.clone();
    missing.face_index = 1;
    let before = camera.clone();
    assert!(camera.frame_face(&view, &missing, 1.5).is_err());
    assert_eq!(camera, before);
}

#[test]
fn frame_vertex_and_edge_use_world_points_and_reject_missing_targets() {
    use crate::native_scene::{EdgeTarget, VertexTarget};
    let (view, _, owner) = ngon_view(ngon_points(5, 1.0)).unwrap();
    let instance = &view.instances()[0];
    let vertex = VertexTarget {
        transform: owner,
        shape: instance.shape(),
        path: instance.path().to_vec(),
        vertex_index: 2,
    };
    let mut camera = front_camera();
    camera.frame_vertex(&view, &vertex, 1.5).unwrap();
    let mut expected = front_camera();
    expected
        .frame_points(&[instance.world_vertices()[2]], 1.5)
        .unwrap();
    assert_eq!(camera, expected);
    let edge = EdgeTarget {
        transform: owner,
        shape: instance.shape(),
        path: instance.path().to_vec(),
        edge_index: 1,
    };
    let [a, b] = instance.edges()[1];
    let mut camera = front_camera();
    camera.frame_edge(&view, &edge, 1.5).unwrap();
    let mut expected = front_camera();
    expected
        .frame_points(
            &[instance.world_vertices()[a], instance.world_vertices()[b]],
            1.5,
        )
        .unwrap();
    assert_eq!(camera, expected);
    let before = camera.clone();
    let mut bad_vertex = vertex.clone();
    bad_vertex.vertex_index = instance.world_vertices().len();
    let mut bad_edge = edge.clone();
    bad_edge.edge_index = instance.edges().len();
    let mut wrong_path = vertex.clone();
    wrong_path.path.clear();
    assert!(camera.frame_vertex(&view, &bad_vertex, 1.5).is_err());
    assert!(camera.frame_vertex(&view, &wrong_path, 1.5).is_err());
    assert!(camera.frame_edge(&view, &bad_edge, 1.5).is_err());
    assert!(camera.frame_vertex(&view, &vertex, f32::NAN).is_err());
    assert_eq!(camera, before);
}

#[test]
fn frame_vertex_and_edge_are_keyed_to_their_own_instance() {
    use crate::native_scene::{EdgeTarget, VertexTarget};
    let (mut shared, _, second, _) = test_fixtures::shared_shape();
    shared
        .set_transform(
            second,
            NativeTransform {
                translation: [3.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let (mut grouped, root, child, _) = test_fixtures::grouped_shape();
    grouped
        .set_transform(
            root,
            NativeTransform {
                translation: [0.0, 4.0, 0.0],
                scale: [2.0, 2.0, 2.0],
                ..Default::default()
            },
        )
        .unwrap();
    grouped
        .set_transform(
            child,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    for native in [shared, grouped] {
        let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
        assert!(!view.instances().is_empty());
        let mut targets = Vec::new();
        for instance in view.instances() {
            let [a, b] = instance.edges()[1];
            let vertices = instance.world_vertices();
            let (vertex, edge) = (
                VertexTarget {
                    transform: instance.transform(),
                    shape: instance.shape(),
                    path: instance.path().to_vec(),
                    vertex_index: 3,
                },
                EdgeTarget {
                    transform: instance.transform(),
                    shape: instance.shape(),
                    path: instance.path().to_vec(),
                    edge_index: 1,
                },
            );
            let mut camera = front_camera();
            camera.frame_vertex(&view, &vertex, 1.0).unwrap();
            assert_eq!(camera.target(), vertices[3]);
            let mut camera = front_camera();
            camera.frame_edge(&view, &edge, 1.0).unwrap();
            assert!(
                camera
                    .target()
                    .abs_diff_eq((vertices[a] + vertices[b]) * 0.5, 1e-5)
            );
            targets.push(vertices[3]);
        }
        assert!(
            targets.windows(2).all(|pair| pair[0] != pair[1]),
            "instances must differ in world space"
        );
    }
}

#[test]
fn degenerate_and_non_convex_polygons_stay_unsupported() {
    fn hexagon_with_vertex3_at_vertex0() -> Vec<[f64; 3]> {
        let mut points = ngon_points(6, 1.0);
        points[3] = points[0];
        points
    }
    // Every corner turns the same way, but the turning sum is 1.44 turns: only the
    // winding residual refuses it.
    fn twisted_hexagon() -> Vec<[f64; 3]> {
        vec![
            [0.8950132017937551, 0.0, -1.291038920799824],
            [0.37030449220567174, 0.480610265934816, 0.14929456192631818],
            [-0.6930633823964827, 0.9551455910595742, 0.2611957344943887],
            [-0.5035827461915017, 0.0, -0.41636237197246473],
            [-0.365708081106396, -0.6692425642592846, -0.8132736901807922],
            [
                0.43060902699800346,
                -1.2488456432197155,
                -0.043411437531646335,
            ],
        ]
    }
    let pentagram = {
        let points = ngon_points(5, 1.0);
        (points, vec![0, 2, 4, 1, 3])
    };
    let mut collinear = ngon_points(5, 1.0);
    collinear[1] = [
        (collinear[0][0] + collinear[2][0]) / 2.0,
        (collinear[0][1] + collinear[2][1]) / 2.0,
        0.0,
    ];
    let mut concave = ngon_points(5, 1.0);
    concave[2] = [0.1, 0.1, 0.0];
    let zero_area = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
        [4.0, 0.0, 0.0],
    ];
    for (points, order) in [
        pentagram,
        (collinear, (0..5).collect()),
        (concave, (0..5).collect()),
        (zero_area, (0..5).collect()),
        (ngon_points(6, 1.0), vec![0, 1, 2, 3, 1, 4]),
        (hexagon_with_vertex3_at_vertex0(), (0..6).collect()),
        (ngon_points(5, 1.0), vec![0, 1, 2, 3, 4, 0, 1, 2, 3, 4]),
        (twisted_hexagon(), (0..6).collect()),
    ] {
        assert_eq!(refused(points, order), ViewError::UnsupportedPolygon);
    }
}

#[test]
fn polygon_with_a_hole_stays_unsupported() {
    let mut points = ngon_points(5, 1.0);
    points.extend(ngon_points(3, 0.2));
    let (mut native, _) = test_fixtures::polygon_shape(points, &[(0..5).collect()]);
    test_fixtures::add_hole_to_first_face(&mut native, &[5, 6, 7]);
    assert_eq!(
        ViewScene::from_native(&native, VIEW_BUDGET).unwrap_err(),
        ViewError::UnsupportedPolygon
    );
}

#[test]
fn reversed_winding_is_judged_by_the_faces_own_normal() {
    let mut points = ngon_points(5, 1.0);
    points.reverse();
    let (view, _, _) = ngon_view(points).unwrap();
    assert_eq!(view.instances()[0].faces()[0].corners().len(), 5);
    let pentagram_reversed = refused(ngon_points(5, 1.0), vec![0, 3, 1, 4, 2]);
    assert_eq!(pentagram_reversed, ViewError::UnsupportedPolygon);
}

#[test]
fn large_polygon_is_budgeted_per_corner_and_refuses_below_the_minimum_budget() {
    let count = 20_000;
    let (view, native, _) = ngon_view(ngon_points(count, 1.0)).unwrap();
    assert_eq!(
        view.instances()[0].faces()[0].triangles().count(),
        count - 2
    );
    let mut order: Vec<usize> = (0..count).collect();
    order.insert(count / 2, 5);
    assert_eq!(
        refused(ngon_points(count, 1.0), order),
        ViewError::UnsupportedPolygon
    );

    let needed = count * (size_of::<Vec3>() + size_of::<usize>());
    let measure = |budget| ViewScene::from_native(&native, budget);
    let mut low = 0;
    let mut high = VIEW_BUDGET;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if measure(mid).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    assert!(
        high >= needed,
        "budget {high} ignores {needed} corner bytes"
    );
    assert!(measure(high).is_ok());
    assert_eq!(measure(high - 1).unwrap_err(), ViewError::ResourceLimit);
}

#[test]
fn cylinder_shaped_scene_picks_a_face_center_within_the_work_bound() {
    let mut points = Vec::new();
    for y in [1.0, -1.0] {
        points.extend(ngon_points(20, 1.0).into_iter().map(|[x, z, _]| [x, y, z]));
    }
    let top: Vec<usize> = (0..20).rev().collect();
    let bottom: Vec<usize> = (20..40).collect();
    let mut loops = vec![top, bottom];
    for i in 0..20 {
        let j = (i + 1) % 20;
        loops.push(vec![i, j, 20 + j, 20 + i]);
    }
    let (native, owner) = test_fixtures::polygon_shape(points, &loops);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    assert_eq!(instance.faces().len(), 22);
    let triangles: usize = instance.faces().iter().map(|f| f.triangles().count()).sum();
    assert_eq!(triangles, 2 * 18 + 20 * 2);
    let path = instance.path()[..instance.path().len() - 1].to_vec();
    let camera = front_camera();
    let face = &instance.faces()[6];
    let center = face.center(instance.world_vertices()).unwrap();
    let pointer = center_pointer(&camera, center, [600, 600], 1.0);
    let hit = camera
        .pick_face_center(&view, Some((owner, &path)), pointer, [600, 600], 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(hit.face_index, 6);
}

fn lifted_ngon(count: usize, lift: f64) -> Vec<[f64; 3]> {
    let mut points = ngon_points(count, 1.0);
    points[1][2] = lift;
    points
}

#[test]
fn slightly_non_planar_convex_polygons_display_best_effort() {
    for count in [5, 20] {
        for lift in [1e-2, 0.1] {
            let (view, _, _) = ngon_view(lifted_ngon(count, lift)).unwrap();
            assert_eq!(view.instances()[0].faces()[0].corners().len(), count);
        }
    }
}

#[test]
fn far_from_origin_polygon_is_judged_relative_to_its_first_corner() {
    let points = ngon_points(5, 1.0)
        .into_iter()
        .map(|[x, y, z]| [x + 1e9, y + 1e9, z + 1e9])
        .collect();
    let (view, _, _) = ngon_view(points).unwrap();
    assert_eq!(view.instances()[0].faces()[0].corners().len(), 5);
}

#[test]
fn skew_convex_pentagon_with_residual_between_a_tenth_and_a_quarter_is_accepted() {
    // Cross products all agree, but the projected turning sum is 1.2 turns: best-effort display.
    let points = vec![
        [0.8261, 0.0, 0.1856],
        [0.2476, 1.0187, -0.2918],
        [-0.7475, 0.6627, -0.0347],
        [-0.7494, -0.5834, 0.3684],
        [0.2543, -1.1318, -0.8588],
    ];
    let (view, _, _) = ngon_view(points).unwrap();
    assert_eq!(view.instances()[0].faces()[0].corners().len(), 5);
}

#[test]
fn skew_loop_with_a_repeated_corner_is_currently_accepted() {
    // Documented limitation (known-issues.md): non-planar input can pass the convexity and
    // winding checks while visiting corner 0 twice. Planar input cannot.
    let points = vec![
        [-0.00106, 0.90400, 0.22367],
        [1.65935, -3.12242, 0.25331],
        [0.69473, -0.28290, 1.19833],
        [-0.62327, 0.64238, 0.30704],
        [-1.02529, 0.98906, 0.40631],
    ];
    let (native, _) = test_fixtures::polygon_shape(points, &[vec![0, 1, 2, 0, 3, 4]]);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(view.instances()[0].faces()[0].corners(), [0, 1, 2, 0, 3, 4]);
}

fn outward_unit_faces(primitive: Primitive) -> Vec<(Vec3, Vec3, usize)> {
    let mut native = NativeScene::default();
    native.create(primitive, "shape").unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    instance
        .faces()
        .iter()
        .map(|face| {
            let center = face.center(instance.world_vertices()).unwrap();
            (face.normal(), center, face.corners().len())
        })
        .collect()
}

#[test]
fn cube_face_normals_are_unit_axis_aligned_and_outward() {
    let faces = outward_unit_faces(Primitive::Cube);
    assert_eq!(faces.len(), 6);
    for (normal, center, _) in &faces {
        assert!((normal.length() - 1.0).abs() < 1e-6, "{normal}");
        assert!(normal.abs().max_element() > 0.999, "{normal}");
        assert!(normal.dot(*center) > 0.0, "{normal} vs {center}");
    }
    let mut axes: Vec<_> = faces.iter().map(|(n, ..)| n.round().as_ivec3()).collect();
    axes.sort_by_key(|n| n.to_array());
    axes.dedup();
    assert_eq!(axes.len(), 6);
}

#[test]
fn sphere_pole_triangle_normal_points_up_and_every_face_is_outward() {
    let faces = outward_unit_faces(Primitive::Sphere);
    let triangles: Vec<_> = faces.iter().filter(|face| face.2 == 3).collect();
    assert!(!triangles.is_empty());
    for (normal, center, _) in &faces {
        assert!((normal.length() - 1.0).abs() < 1e-5, "{normal}");
        assert!(normal.dot(center.normalize()) > 0.5, "{normal} vs {center}");
    }
    let top = triangles
        .iter()
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .unwrap();
    assert!(top.0.y > 0.9, "{}", top.0);
}

#[test]
fn twenty_gon_cap_normal_follows_the_winding() {
    let (view, _, _) = ngon_view(ngon_points(20, 1.0)).unwrap();
    assert!(
        view.instances()[0].faces()[0]
            .normal()
            .abs_diff_eq(Vec3::Z, 1e-6)
    );
    let reversed = (0..20).rev().collect();
    let (native, _) = test_fixtures::polygon_shape(ngon_points(20, 1.0), &[reversed]);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert!(
        view.instances()[0].faces()[0]
            .normal()
            .abs_diff_eq(-Vec3::Z, 1e-6)
    );
}

#[test]
fn degenerate_faces_get_a_zero_normal_not_nan() {
    let collinear = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    let (view, ..) = ngon_view(collinear).unwrap();
    assert_eq!(view.instances()[0].faces()[0].normal(), Vec3::ZERO);
    let coincident = vec![[1.0, 2.0, 3.0]; 3];
    let (view, ..) = ngon_view(coincident).unwrap();
    assert_eq!(view.instances()[0].faces()[0].normal(), Vec3::ZERO);
    // Largest coordinates the f32 world accepts: the f64 Newell sum stays finite, so the
    // normal is a valid unit vector. The f64 overflow branch in `flat_normal` is unreachable
    // through ViewScene (1e200 is refused as NonFiniteWorld first).
    let huge = vec![[0.0; 3], [1e30, 0.0, 0.0], [0.0, 1e30, 0.0]];
    let (view, ..) = ngon_view(huge).unwrap();
    assert!(
        view.instances()[0].faces()[0]
            .normal()
            .abs_diff_eq(Vec3::Z, 1e-6)
    );
    assert_eq!(
        ngon_view(vec![[0.0; 3], [1e200, 0.0, 0.0], [0.0, 1e200, 0.0]]).unwrap_err(),
        ViewError::NonFiniteWorld
    );
}

fn world_newell(instance: &ViewInstance, face: &ViewFace) -> Vec3 {
    let vertices = instance.world_vertices();
    let corners = face.corners();
    let origin = vertices[corners[0]];
    let mut normal = Vec3::ZERO;
    let mut previous = Vec3::ZERO;
    for &index in corners {
        let current = vertices[index] - origin;
        normal += previous.cross(current);
        previous = current;
    }
    normal.normalize()
}

#[test]
fn world_normals_stay_outward_and_perpendicular_under_shear_and_mirroring() {
    let cases = [
        ([0.0, 0.0, 45.0], [1.0; 3], [4.0, 1.0, 1.0], [1.0; 3]),
        (
            [0.0, 0.0, 30.0],
            [1.0, 2.0, 3.0],
            [-1.0, 1.0, 1.0],
            [1.0; 3],
        ),
        (
            [20.0, 0.0, 30.0],
            [1.0; 3],
            [2.0, 1.0, 0.5],
            [1.0, -1.0, 1.0],
        ),
        (
            [0.0, 40.0, 0.0],
            [1.0; 3],
            [-1.0, -1.0, -1.0],
            [3.0, 1.0, 1.0],
        ),
    ];
    for (rotation, child_scale, root_scale, root_child_scale) in cases {
        let (mut native, root, child, _) = test_fixtures::grouped_shape();
        native
            .set_transform(
                root,
                NativeTransform {
                    scale: root_scale,
                    ..Default::default()
                },
            )
            .unwrap();
        native
            .set_transform(
                child,
                NativeTransform {
                    rotation,
                    scale: [
                        child_scale[0] * root_child_scale[0],
                        child_scale[1] * root_child_scale[1],
                        child_scale[2] * root_child_scale[2],
                    ],
                    ..Default::default()
                },
            )
            .unwrap();
        let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
        let instance = &view.instances()[0];
        let vertices = instance.world_vertices();
        let centroid = vertices.iter().sum::<Vec3>() / vertices.len() as f32;
        for face in instance.faces() {
            let normal = instance.world_normal(face);
            let center = face.center(vertices).unwrap();
            assert!(
                (normal.length() - 1.0).abs() < 1e-5,
                "{rotation:?} {normal}"
            );
            assert!(normal.dot(center - centroid) > 0.0, "{rotation:?} {normal}");
            let surface = world_newell(instance, face);
            assert!(
                normal.cross(surface).length() < 1e-4,
                "{rotation:?} {normal} {surface}"
            );
        }
    }
}

#[test]
fn singular_transform_gives_zero_world_normals() {
    let mut native = NativeScene::default();
    let node = native.create(Primitive::Cube, "flat").unwrap();
    native
        .set_transform(
            node,
            NativeTransform {
                scale: [1.0, 0.0, 1.0],
                ..Default::default()
            },
        )
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    assert!(
        instance
            .faces()
            .iter()
            .all(|f| instance.world_normal(f) == Vec3::ZERO)
    );
}

fn base_colors(native: &NativeScene) -> Vec<[u8; 3]> {
    ViewScene::from_native(native, VIEW_BUDGET)
        .unwrap()
        .instances()
        .iter()
        .map(ViewInstance::base_rgb)
        .collect()
}

#[test]
fn instances_default_to_the_pinned_base_fill() {
    assert_eq!(SURFACE_BASE_RGB, [83, 83, 83]);
    let (native, ..) = test_fixtures::shared_shape();
    assert_eq!(base_colors(&native), [SURFACE_BASE_RGB; 2]);
}

#[test]
fn every_instance_of_an_assigned_shape_takes_its_colour_and_undo_restores_grey() {
    use crate::history::{Command, History};
    use crate::native_scene::MaterialChoice;
    let (native, _, _, shape) = test_fixtures::shared_shape();
    let mut history = History::new(native);
    history
        .execute(Command::AssignSessionMaterial {
            shape,
            material: MaterialChoice::New {
                color: [1.0, 0.0, 0.2],
            },
        })
        .unwrap();
    assert_eq!(base_colors(history.scene()), [[255, 0, 51]; 2]);
    assert!(history.undo());
    assert_eq!(base_colors(history.scene()), [SURFACE_BASE_RGB; 2]);
}

#[test]
fn recolouring_a_material_updates_its_instances_and_other_shapes_keep_grey() {
    let (mut native, _, _, shape) = test_fixtures::shared_shape();
    native.create(Primitive::Cube, "other").unwrap();
    let material = native
        .assign_session_material(
            shape,
            crate::native_scene::MaterialChoice::New { color: [1.0; 3] },
        )
        .unwrap();
    native
        .set_session_material_color(material, [0.0, 1.0, 0.0])
        .unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    for instance in view.instances() {
        let expected = if instance.shape() == shape {
            [0, 255, 0]
        } else {
            SURFACE_BASE_RGB
        };
        assert_eq!(instance.base_rgb(), expected);
    }
}

fn native_edges(native: &NativeScene) -> Vec<[usize; 2]> {
    native.shapes()[0]
        .mesh()
        .edges()
        .iter()
        .map(|edge| edge.endpoints())
        .collect()
}

#[test]
fn instance_edges_equal_native_edge_order_for_primitives_and_mixed_polygons() {
    for primitive in [Primitive::Cube, Primitive::Sphere] {
        let mut native = NativeScene::default();
        native.create(primitive, "shape").unwrap();
        let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
        let edges = native_edges(&native);
        assert!(!edges.is_empty());
        assert_eq!(view.instances()[0].edges(), edges);
    }
    let points = vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [2.0, 0.5, 0.0],
        [1.5, 1.6, 0.0],
        [0.5, 2.2, 0.0],
        [-0.5, 1.6, 0.0],
    ];
    let loops = [vec![0, 1, 2, 3], vec![1, 4, 2], vec![3, 2, 5, 6, 7]];
    let (native, _) = test_fixtures::polygon_shape(points, &loops);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(view.instances()[0].edges().len(), 12);
    assert_eq!(view.instances()[0].edges(), native_edges(&native));
}

#[test]
fn loose_edge_is_carried_and_a_bad_endpoint_is_invalid_geometry() {
    let points = ngon_points(5, 1.0);
    let (mut native, _) = test_fixtures::polygon_shape(points.clone(), &[(0..5).collect()]);
    test_fixtures::add_loose_edge_unvalidated(&mut native, [0, 2]);
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    assert_eq!(view.instances()[0].edges().last(), Some(&[0, 2]));
    for endpoints in [[0, 5], [5, 0], [usize::MAX, 1]] {
        let (mut native, _) = test_fixtures::polygon_shape(points.clone(), &[(0..5).collect()]);
        test_fixtures::add_loose_edge_unvalidated(&mut native, endpoints);
        assert!(matches!(
            ViewScene::from_native(&native, VIEW_BUDGET),
            Err(ViewError::InvalidGeometry)
        ));
    }
}

#[test]
fn view_budget_charges_every_source_edge_per_instance() {
    let required = |loose: usize| {
        let (mut native, _) =
            test_fixtures::polygon_shape(ngon_points(5, 1.0), &[(0..5).collect()]);
        for _ in 0..loose {
            test_fixtures::add_loose_edge_unvalidated(&mut native, [0, 2]);
        }
        let graph = Graph::prepare(&native, VIEW_BUDGET).unwrap();
        graph.requirements(VIEW_BUDGET).unwrap().bytes
    };
    assert_eq!(required(100) - required(0), 100 * size_of::<[usize; 2]>());
}

#[test]
fn native_extras_reach_the_view_through_the_shared_matrix() {
    use crate::native_scene::TransformExtras;
    let (mut native, root, child, _) = test_fixtures::grouped_shape();
    native
        .set_transform(
            root,
            NativeTransform {
                translation: [10.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    native
        .set_transform(
            child,
            NativeTransform {
                translation: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 90.0],
                scale: [2.0, 3.0, 4.0],
            },
        )
        .unwrap();
    test_fixtures::set_extras(
        &mut native,
        child,
        TransformExtras {
            shear: Some([0.5, 0.0, 0.0]),
            rotate_pivot_translate: Some([0.0, 0.0, 5.0]),
            ..TransformExtras::default()
        },
    );
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let Points::Float3(points) = native.shapes()[0].mesh().points() else {
        panic!("canonical cube must use Float3 points");
    };
    // Row-vector reference for R(z 90) * SH(xy 0.5) * S then T + RT, under a translated parent.
    for (actual, [x, y, z]) in view.instances()[0].world_vertices().iter().zip(points) {
        let (sx, sy, sz) = (2.0 * x, 3.0 * y, 4.0 * z);
        let (hx, hy) = (sx + 0.5 * sy, sy);
        let expected = Vec3::new(10.0 + 1.0 - hy, 2.0 + hx, 3.0 + 5.0 + sz);
        assert!(actual.distance(expected) < 1e-5, "{actual} != {expected}");
    }
}

#[test]
fn face_center_work_budget_accepts_exactly_the_limit_and_rejects_one_more() {
    for (eligible, triangles) in [(1, 1_000_000), (1000, 1000), (500, 2000), (1_000_000, 1)] {
        assert!(
            crate::viewport::face_center_work(eligible, triangles).is_ok(),
            "{eligible} x {triangles}"
        );
    }
    for (eligible, triangles) in [(1, 1_000_001), (1_000_001, 1), (1000, 1001), (1001, 1000)] {
        assert_eq!(
            crate::viewport::face_center_work(eligible, triangles),
            Err(ViewError::ResourceLimit),
            "{eligible} x {triangles}"
        );
    }
}

#[test]
fn marker_sizes_are_pinned_to_their_provisional_pixel_values() {
    assert_eq!(FACE_CENTER_MARKER_PIXELS, 4.0);
    assert_eq!(VERTEX_MARKER_PIXELS, 5.0);
    assert_eq!(SELECTED_EDGE_PIXELS, 3.0);
}
