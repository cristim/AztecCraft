use super::*;
use crate::native_scene::NativeTransform;
use crate::native_scene::test_fixtures;
use crate::scene::Primitive;
use crate::viewport::Camera;
use glam::Vec2;

const VIEW_BUDGET: usize = 256 * 1024 * 1024;
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

fn center_fixture(vertices: Vec<Vec3>, corners: ViewCorners) -> (ViewScene, NodeId, Vec<NativeId>) {
    let (native, owner) = test_fixtures::triangle_double_shape();
    let mut view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &mut view.instances[0];
    instance.world_vertices = vertices;
    instance.faces = vec![ViewFace {
        source_index: 0,
        corners,
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
    let face = view.instances[0].faces[0];
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
    let face = view.instances[0].faces[0];
    view.instances[0].faces = (0..1000)
        .map(|index| ViewFace {
            source_index: index,
            ..face
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
