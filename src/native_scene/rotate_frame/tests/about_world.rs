//! `RotateFrame::rotation_about_world` (#492): turning about an arbitrary world direction, the core
//! of the Rotate tool's view ring and free rotation.

use super::*;
use glam::{DQuat, DVec3, EulerRot};

fn quat(degrees: [f64; 3]) -> DQuat {
    let [x, y, z] = degrees.map(f64::to_radians);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

fn same_orientation(a: DQuat, b: DQuat) -> bool {
    1.0 - a.dot(b).abs() < 1e-12
}

fn world_frame(scene: &NativeScene, node: NodeId) -> RotateFrame {
    scene
        .begin_rotation(&target(scene, node), RotateAxis::X, RotateSpace::World)
        .unwrap()
        .0
}

fn nested(parent: NativeTransform, child: [f64; 3]) -> (NativeScene, NodeId) {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene.set_transform(root, parent).unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: child,
                ..NativeTransform::default()
            },
        )
        .unwrap();
    (scene, node)
}

/// The world orientation after the turn is the independently composed quaternion
/// `R(axis, angle) * Q_parent * Q_child`, for random parents, children, axes and angles.
#[test]
fn seeded_sweep_matches_an_independent_quaternion_world_orientation() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let (mut checked, mut gimbal) = (0, 0);
    for round in 0..400 {
        let parent_rotation = [
            next() * 720.0 - 360.0,
            next() * 360.0 - 180.0,
            next() * 720.0 - 360.0,
        ];
        let uniform = 0.1 + next() * 9.9;
        let child = [
            next() * 720.0 - 360.0,
            next() * 170.0 - 85.0,
            next() * 720.0 - 360.0,
        ];
        let degrees = next() * 720.0 - 360.0;
        // Unnormalized on purpose: the length must not matter.
        let axis = DVec3::new(next() - 0.5, next() - 0.5, next() - 0.5) * (0.01 + next() * 50.0);
        if axis.length() < 1e-6 {
            continue;
        }
        let (scene, node, parent_q) = if round % 5 == 0 {
            let (scene, node) = rotated(child);
            (scene, node, DQuat::IDENTITY)
        } else {
            let (scene, node) = nested(
                NativeTransform {
                    translation: [next() * 10.0, 0.0, -3.0],
                    rotation: parent_rotation,
                    scale: [uniform; 3],
                },
                child,
            );
            (scene, node, quat(parent_rotation))
        };
        match world_frame(&scene, node).rotation_about_world(axis, degrees) {
            Ok(out) => {
                let expected = DQuat::from_axis_angle(axis.normalize(), degrees.to_radians())
                    * (parent_q * quat(child));
                assert!(
                    same_orientation(parent_q * quat(out), expected),
                    "round {round} {axis:?} {degrees}"
                );
                checked += 1;
            }
            Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal)) => gimbal += 1,
            Err(error) => panic!("round {round} {axis:?}: {error}"),
        }
    }
    assert!(checked > 300, "{checked} checked, {gimbal} gimbal");
}

/// A world basis axis reproduces the World ring to 1e-9 degrees (the two paths normalize the
/// parent-local axis differently, so the last bits may differ).
#[test]
fn a_world_basis_axis_matches_the_world_ring() {
    let (scene, node) = nested(
        NativeTransform {
            rotation: [20.0, 35.0, -15.0],
            scale: [2.5; 3],
            ..NativeTransform::default()
        },
        [13.0, 40.0, -77.0],
    );
    for (axis, direction) in [
        (RotateAxis::X, DVec3::X),
        (RotateAxis::Y, DVec3::Y),
        (RotateAxis::Z, DVec3::Z),
    ] {
        let ring = scene
            .begin_rotation(&target(&scene, node), axis, RotateSpace::World)
            .unwrap()
            .0;
        for degrees in [33.0, -170.0, 1e-7] {
            let expected = ring.rotation(axis, degrees).unwrap();
            let actual = ring.rotation_about_world(direction, degrees).unwrap();
            for component in 0..3 {
                assert!(
                    (actual[component] - expected[component]).abs() <= 1e-9,
                    "{axis:?} {degrees}: {actual:?} vs {expected:?}"
                );
            }
        }
    }
}

#[test]
fn an_axis_through_one_channel_of_an_unrotated_root_changes_only_that_channel() {
    let (scene, node) = rotated([0.0; 3]);
    let frame = world_frame(&scene, node);
    let out = frame.rotation_about_world(DVec3::Y * 7.0, 33.0).unwrap();
    assert_eq!(bits([out[0], out[2], 0.0]), bits([0.0, 0.0, 0.0]));
    assert_eq!(
        out[1], 33.0,
        "a single channel stores baseline plus the step exactly"
    );
}

#[test]
fn zero_angle_is_the_baseline_bit_for_bit() {
    let (scene, node) = rotated([13.0, 120.0, -77.0]);
    let frame = world_frame(&scene, node);
    assert_eq!(
        bits(
            frame
                .rotation_about_world(DVec3::new(1.0, 2.0, 3.0), 0.0)
                .unwrap()
        ),
        bits(frame.declared)
    );
}

#[test]
fn malformed_axes_and_angles_are_refused() {
    let (scene, node) = rotated([10.0, 20.0, 30.0]);
    let frame = world_frame(&scene, node);
    let numerical = NativeError::UnsupportedRotate(RotateUnsupported::NumericalAngle);
    for (axis, degrees, expected) in [
        (DVec3::new(f64::NAN, 0.0, 1.0), 10.0, NativeError::NonFinite),
        (
            DVec3::new(0.0, f64::INFINITY, 1.0),
            10.0,
            NativeError::NonFinite,
        ),
        (DVec3::Z, f64::NAN, NativeError::NonFinite),
        (DVec3::Z, f64::INFINITY, NativeError::NonFinite),
        (DVec3::ZERO, 10.0, numerical.clone()),
        (DVec3::splat(1e-320), 10.0, numerical.clone()),
        (DVec3::Z, 1e308, numerical.clone()),
    ] {
        assert_eq!(
            frame.rotation_about_world(axis, degrees),
            Err(expected),
            "{axis:?} {degrees}"
        );
    }
}

#[test]
fn object_and_gimbal_frames_refuse_a_world_direction() {
    let (scene, node) = rotated([10.0, 20.0, 30.0]);
    for space in [RotateSpace::Object, RotateSpace::Gimbal] {
        let frame = scene
            .begin_rotation(&target(&scene, node), RotateAxis::X, space)
            .unwrap()
            .0;
        assert_eq!(
            frame.rotation_about_world(DVec3::Z, 10.0),
            Err(NativeError::UnsupportedEdit),
            "{space:?}"
        );
    }
}

#[test]
fn a_turn_onto_gimbal_lock_is_refused() {
    let (scene, node) = rotated([0.0; 3]);
    let frame = world_frame(&scene, node);
    assert_eq!(
        frame.rotation_about_world(DVec3::Y, 90.0),
        Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal))
    );
}

#[test]
fn radian_scenes_store_radian_channels() {
    let (mut scene, node) = rotated([0.0; 3]);
    scene.units.angular = AngularUnit::Radian;
    let frame = world_frame(&scene, node);
    let out = frame.rotation_about_world(DVec3::Z, 90.0).unwrap();
    assert!(
        (out[2] - std::f64::consts::FRAC_PI_2).abs() < 1e-12,
        "{out:?}"
    );
}
