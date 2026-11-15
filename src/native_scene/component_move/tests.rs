use super::*;
use crate::native_scene::test_fixtures;

fn target_of(scene: &NativeScene, node: NodeId) -> TransformTarget {
    scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.node == node)
        .unwrap()
}

fn transform(translation: [f64; 3], rotation: [f64; 3], scale: [f64; 3]) -> NativeTransform {
    NativeTransform {
        translation,
        rotation,
        scale,
    }
}

fn near(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).abs().max_element() < 1e-9,
        "{actual} != {expected}"
    );
}

/// The world displacement of `point` when it moves by the local delta for `world_vector`.
fn world_displacement(frame: &ComponentMoveFrame, point: [f64; 3], world_vector: DVec3) -> DVec3 {
    let delta = frame.local_delta_cm(world_vector).unwrap();
    let moved = [0, 1, 2].map(|k| point[k] + delta[k]);
    frame.world_point_cm(moved) - frame.world_point_cm(point)
}

const POINT: [f64; 3] = [0.5, -1.5, 2.0];
const DRAG: DVec3 = DVec3::new(3.0, -2.0, 5.0);

fn check_displacement(scene: &NativeScene, target: &TransformTarget) {
    let frame = scene.component_move_frame(target).unwrap();
    near(world_displacement(&frame, POINT, DRAG), DRAG);
}

#[test]
fn identity_and_translated_frames_pass_the_vector_through() {
    let (mut scene, node) = test_fixtures::cube();
    let target = target_of(&scene, node);
    let frame = scene.component_move_frame(&target).unwrap();
    assert_eq!(frame.local_delta_cm(DRAG).unwrap(), DRAG.to_array());
    scene
        .set_transform(node, transform([10.0, 20.0, 30.0], [0.0; 3], [1.0; 3]))
        .unwrap();
    let frame = scene.component_move_frame(&target).unwrap();
    assert_eq!(frame.local_delta_cm(DRAG).unwrap(), DRAG.to_array());
    near(
        frame.world_point_cm([1.0, 2.0, 3.0]),
        DVec3::new(11.0, 22.0, 33.0),
    );
}

#[test]
fn rotated_uniform_and_non_uniform_frames_map_a_world_drag_exactly() {
    for (rotation, scale) in [
        ([0.0, 0.0, 90.0], [1.0; 3]),
        ([30.0, 45.0, 60.0], [1.0; 3]),
        ([30.0, 45.0, 60.0], [2.0; 3]),
        ([30.0, 45.0, 60.0], [2.0, 1.0, 0.5]),
        ([0.0, 0.0, 0.0], [-1.0, 1.0, 1.0]),
    ] {
        let (mut scene, node) = test_fixtures::cube();
        scene
            .set_transform(node, transform([1.0, 2.0, 3.0], rotation, scale))
            .unwrap();
        check_displacement(&scene, &target_of(&scene, node));
    }
}

#[test]
fn a_rotated_quarter_turn_maps_axes_to_the_known_local_vector() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(node, transform([0.0; 3], [0.0, 0.0, 90.0], [1.0; 3]))
        .unwrap();
    let frame = scene
        .component_move_frame(&target_of(&scene, node))
        .unwrap();
    // Local X maps to world Y, so a world +Y drag is local +X.
    let local = frame.local_delta_cm(DVec3::Y).unwrap();
    near(DVec3::from_array(local), DVec3::X);
}

#[test]
fn parent_chains_compose_and_non_uniform_parents_stay_exact() {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            transform([10.0, 0.0, 0.0], [20.0, 0.0, 70.0], [2.0, 1.0, 0.5]),
        )
        .unwrap();
    scene
        .set_transform(
            node,
            transform([1.0, 2.0, 3.0], [0.0, 40.0, 0.0], [3.0, 3.0, 3.0]),
        )
        .unwrap();
    check_displacement(&scene, &target_of(&scene, node));
}

#[test]
fn each_instance_path_uses_its_own_world_matrix() {
    let (mut scene, x1, x2, _, b) = test_fixtures::instanced_chain();
    scene
        .set_transform(x1, transform([5.0, 0.0, 0.0], [0.0, 90.0, 0.0], [1.0; 3]))
        .unwrap();
    scene
        .set_transform(
            x2,
            transform([0.0, 7.0, 0.0], [0.0, 0.0, 0.0], [4.0, 1.0, 1.0]),
        )
        .unwrap();
    let paths: Vec<_> = scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .filter(|path| path.node == b)
        .collect();
    assert_eq!(paths.len(), 2);
    let deltas: Vec<_> = paths
        .iter()
        .map(|path| {
            check_displacement(&scene, path);
            scene
                .component_move_frame(path)
                .unwrap()
                .local_delta_cm(DRAG)
                .unwrap()
        })
        .collect();
    assert_ne!(deltas[0], deltas[1]);
}

#[test]
fn object_axes_are_unit_and_follow_the_node_rotation() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(node, transform([0.0; 3], [0.0, 0.0, 90.0], [5.0, 6.0, 7.0]))
        .unwrap();
    let axes = scene
        .component_move_object_axes(&target_of(&scene, node))
        .unwrap();
    near(axes[0], DVec3::Y);
    near(axes[1], -DVec3::X);
    near(axes[2], DVec3::Z);
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(root, transform([0.0; 3], [0.0, 0.0, 90.0], [2.0, 3.0, 4.0]))
        .unwrap();
    let axes = scene
        .component_move_object_axes(&target_of(&scene, node))
        .unwrap();
    // Sheared by the non-uniform parent but each axis is normalised.
    assert!(axes.iter().all(|axis| (axis.length() - 1.0).abs() < 1e-12));
}

#[test]
fn a_singular_or_overflowing_world_is_refused() {
    let refused = NativeError::UnsupportedMove(WorldMoveUnsupported::ParentTransform);
    for scale in [[0.0, 1.0, 1.0], [1e200, 1e200, 1e200], [1e-8, 1.0, 1e8]] {
        let (mut scene, node) = test_fixtures::cube();
        scene
            .set_transform(node, transform([0.0; 3], [0.0; 3], scale))
            .unwrap();
        assert_eq!(
            scene.component_move_frame(&target_of(&scene, node)),
            Err(refused.clone()),
            "{scale:?}"
        );
    }
}

#[test]
fn an_invalid_target_is_refused() {
    let (scene, node) = test_fixtures::cube();
    let mut bad = target_of(&scene, node);
    bad.path.clear();
    assert!(scene.component_move_frame(&bad).is_err());
    assert!(scene.component_move_object_axes(&bad).is_err());
}

#[test]
fn local_delta_refuses_non_finite_input_and_overflow() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(node, transform([0.0; 3], [0.0; 3], [1e-8, 1.0, 1.0]))
        .unwrap();
    let frame = scene
        .component_move_frame(&target_of(&scene, node))
        .unwrap();
    assert_eq!(
        frame.local_delta_cm(DVec3::new(f64::NAN, 0.0, 0.0)),
        Err(NativeError::NonFinite)
    );
    assert_eq!(
        frame.local_delta_cm(DVec3::new(f64::MAX, 0.0, 0.0)),
        Err(NativeError::NonFinite)
    );
}

#[test]
fn the_bbox_centre_is_the_centre_of_the_world_box_not_the_mean() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(node, transform([10.0, 0.0, 0.0], [0.0; 3], [2.0, 2.0, 2.0]))
        .unwrap();
    let frame = scene
        .component_move_frame(&target_of(&scene, node))
        .unwrap();
    // Three points: the mean is (1, 1, 0) locally but the box centre is (1.5, 1.5, 0).
    let points = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [3.0, 3.0, 0.0]];
    near(
        frame.bbox_center_world_cm(&points).unwrap(),
        DVec3::new(13.0, 3.0, 0.0),
    );
    assert_eq!(
        frame.bbox_center_world_cm(&[]),
        Err(NativeError::InvalidTopology)
    );
    assert_eq!(
        frame.bbox_center_world_cm(&[[f64::NAN, 0.0, 0.0]]),
        Err(NativeError::NonFinite)
    );
}

fn first_shape(scene: &NativeScene) -> NativeId {
    scene.shapes[0].id
}

fn points_of(scene: &NativeScene) -> Points {
    scene.shapes[0].mesh.points().clone()
}

fn bits(points: &Points) -> Vec<u64> {
    match points {
        Points::Float3(p) => p.iter().flatten().map(|v| u64::from(v.to_bits())).collect(),
        Points::Double3(p) => p.iter().flatten().map(|v| v.to_bits()).collect(),
    }
}

#[test]
fn check_vertex_move_reports_every_refusal_without_changing_anything() {
    let (scene, _) = test_fixtures::cube();
    let shape = first_shape(&scene);
    let before = points_of(&scene);
    assert_eq!(scene.vertex_move_refusal(shape, &[0, 7]), None);
    for (vertices, expected) in [
        (vec![], NativeError::InvalidTopology),
        (vec![8], NativeError::InvalidTopology),
        (vec![1, 1], NativeError::InvalidTopology),
    ] {
        assert_eq!(
            scene.vertex_move_refusal(shape, &vertices),
            Some(expected),
            "{vertices:?}"
        );
    }
    assert!(matches!(
        scene.vertex_move_refusal(NativeId::from_decimal("999999").unwrap(), &[0]),
        Some(NativeError::MissingShape(_))
    ));
    assert_eq!(bits(&points_of(&scene)), bits(&before));
    let (offsets, _) = test_fixtures::offset_triangle_shape();
    assert_eq!(
        offsets.vertex_move_refusal(first_shape(&offsets), &[0]),
        Some(NativeError::UnsupportedEdit)
    );
}

#[test]
fn preview_then_restore_is_byte_identical_and_not_cumulative() {
    for double in [false, true] {
        let (mut scene, _) = if double {
            test_fixtures::polygon_shape(
                vec![[-0.0, 1.0, 2.0], [3.0, -0.0, 5.0], [6.0, 7.0, -0.0]],
                &[vec![0, 1, 2]],
            )
        } else {
            let (mut scene, id) = test_fixtures::cube();
            scene.shapes[0].mesh = NativeMesh::from_loops(
                vec![[-0.0, 1.0, 2.0], [3.0, -0.0, 5.0], [6.0, 7.0, -0.0]],
                vec![vec![0, 1, 2]],
            );
            (scene, id)
        };
        let shape = first_shape(&scene);
        let original = bits(&points_of(&scene));
        let baseline = scene.capture_vertices(shape, &[0, 1]).unwrap();
        scene
            .preview_vertices(&baseline, [1.5, -2.25, 0.1])
            .unwrap();
        let once = bits(&points_of(&scene));
        assert_ne!(once, original);
        scene
            .preview_vertices(&baseline, [1.5, -2.25, 0.1])
            .unwrap();
        assert_eq!(bits(&points_of(&scene)), once, "not cumulative");
        // The unlisted vertex never changes.
        assert_eq!(once[6..], original[6..]);
        scene.preview_vertices(&baseline, [0.0; 3]).unwrap();
        assert_eq!(bits(&points_of(&scene)), original, "zero keeps -0.0");
        scene.preview_vertices(&baseline, [4.0, 4.0, 4.0]).unwrap();
        scene.restore_vertices(&baseline).unwrap();
        assert_eq!(bits(&points_of(&scene)), original);
    }
}

#[test]
fn a_refused_preview_leaves_the_mesh_alone() {
    let (mut scene, _) = test_fixtures::cube();
    let shape = first_shape(&scene);
    let baseline = scene.capture_vertices(shape, &[0, 1]).unwrap();
    scene.preview_vertices(&baseline, [1.0, 1.0, 1.0]).unwrap();
    let held = bits(&points_of(&scene));
    for delta in [[f64::NAN, 0.0, 0.0], [1e300, 0.0, 0.0]] {
        assert!(scene.preview_vertices(&baseline, delta).is_err());
        assert_eq!(bits(&points_of(&scene)), held);
    }
}

#[test]
fn preview_equals_translate_points_bit_for_bit() {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as f64 / u64::MAX as f64 - 0.5) * 2000.0
    };
    for case in 0..40 {
        let points: Vec<[f64; 3]> = (0..6).map(|_| [next(), next(), next()]).collect();
        let delta = [next() * 0.37, next() / 7.0, next() * 1e-3];
        let (mut previewed, _) = if case % 2 == 0 {
            test_fixtures::polygon_shape(points.clone(), &[vec![0, 1, 2, 3, 4, 5]])
        } else {
            let (mut scene, id) = test_fixtures::cube();
            scene.shapes[0].mesh = NativeMesh::from_loops(
                points.iter().map(|p| p.map(|v| v as f32)).collect(),
                vec![vec![0, 1, 2, 3, 4, 5]],
            );
            (scene, id)
        };
        let mut translated = previewed.clone();
        let shape = first_shape(&previewed);
        let set = [4, 0, 2];
        let baseline = previewed.capture_vertices(shape, &set).unwrap();
        // A first preview at another delta must not leak into the second.
        previewed
            .preview_vertices(&baseline, [9.0, 9.0, 9.0])
            .unwrap();
        previewed.preview_vertices(&baseline, delta).unwrap();
        translated.shapes[0]
            .mesh
            .translate_points(&set, delta)
            .unwrap();
        assert_eq!(bits(&points_of(&previewed)), bits(&points_of(&translated)));
    }
}
