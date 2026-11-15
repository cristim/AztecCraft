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
