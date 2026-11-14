use super::*;
use glam::{DQuat, DVec3, EulerRot};

const AXES: [RotateAxis; 3] = [RotateAxis::X, RotateAxis::Y, RotateAxis::Z];

fn quat(degrees: [f64; 3]) -> DQuat {
    let [x, y, z] = degrees.map(f64::to_radians);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

fn gimbal_frame(scene: &NativeScene, node: NodeId, axis: RotateAxis) -> RotateFrame {
    scene
        .begin_rotation(&target(scene, node), axis, RotateSpace::Gimbal)
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

fn turned_parent() -> NativeTransform {
    NativeTransform {
        translation: [1.0, 2.0, 3.0],
        rotation: [20.0, -40.0, 65.0],
        scale: [2.5; 3],
    }
}

fn same_orientation(a: DQuat, b: DQuat) -> bool {
    1.0 - a.dot(b).abs() < 1e-12
}

#[test]
fn every_ring_edits_exactly_its_own_channel_bit_exactly() {
    let child = [30.0, 50.0, 70.0];
    let (scene, node) = nested(turned_parent(), child);
    for axis in AXES {
        let out = gimbal_frame(&scene, node, axis)
            .rotation(axis, 12.5)
            .unwrap();
        let mut expected = child;
        expected[axis.index()] += 12.5;
        assert_eq!(bits(out), bits(expected), "{axis:?}");
    }
}

#[test]
fn ring_axes_turn_the_world_orientation_like_one_channel_of_an_independent_reference() {
    let child = [30.0, 50.0, 70.0];
    let (scene, node) = nested(turned_parent(), child);
    let parent_q = quat([20.0, -40.0, 65.0]);
    let axes = scene.gimbal_axes(&target(&scene, node)).unwrap();
    for (axis, ring) in AXES.into_iter().zip(axes) {
        for degrees in [25.0_f64, -140.0] {
            let turned = DQuat::from_axis_angle(ring.normalize(), degrees.to_radians())
                * parent_q
                * quat(child);
            let mut edited = child;
            edited[axis.index()] += degrees;
            assert!(
                same_orientation(turned, parent_q * quat(edited)),
                "{axis:?} by {degrees}"
            );
        }
        // The parent scale is 2.5, so the axes are scaled, not normalized.
        assert!((ring.length() - 2.5).abs() < 1e-12, "{axis:?}");
    }
}

#[test]
fn ring_axes_differ_from_object_and_world_axes_where_the_rings_should() {
    let child = [30.0, 50.0, 70.0];
    let (scene, node) = nested(turned_parent(), child);
    let axes = scene.gimbal_axes(&target(&scene, node)).unwrap();
    let object = scene
        .manipulator_frame(&target(&scene, node))
        .unwrap()
        .object_axes;
    let parent = DMat3::from_mat4(
        scene
            .ancestor_world(&target(&scene, node))
            .unwrap()
            .unwrap(),
    );
    // Object X equals gimbal X, so the X ring cannot tell the spaces apart.
    assert!((axes[0] - object[0]).length() < 1e-12);
    assert!((axes[1] - object[1]).length() > 0.1);
    assert!((axes[2] - object[2]).length() > 0.1);
    // The Z ring is the parent's own Z, which a world Z ring is not.
    assert!((axes[2] - parent * DVec3::Z).length() < 1e-12);
    assert!((axes[2].normalize() - DVec3::Z).length() > 0.1);
}

#[test]
fn root_node_axes_match_the_euler_product_columns() {
    let (scene, node) = rotated([30.0, 50.0, 70.0]);
    let [x, y, z] = scene.gimbal_axes(&target(&scene, node)).unwrap();
    let rz = quat([0.0, 0.0, 70.0]);
    let rzy = quat([0.0, 50.0, 70.0]);
    assert!((x - rzy * DVec3::X).length() < 1e-12);
    assert!((y - rz * DVec3::Y).length() < 1e-12);
    assert!((z - DVec3::Z).length() < 1e-12);
}

#[test]
fn near_and_exact_pitch_ninety_are_not_refused() {
    for pitch in [90.0, 89.999_999, -90.0] {
        let (scene, node) = rotated([10.0, pitch, 20.0]);
        let axes = scene.gimbal_axes(&target(&scene, node)).unwrap();
        for axis in AXES {
            let out = gimbal_frame(&scene, node, axis)
                .rotation(axis, 15.0)
                .unwrap();
            assert!((out[axis.index()] - [10.0, pitch, 20.0][axis.index()] - 15.0).abs() < 1e-12);
        }
        // X and Z collapse to one line at lock but stay finite and unit length.
        assert!(axes.iter().all(|axis| (axis.length() - 1.0).abs() < 1e-12));
    }
}

#[test]
fn parents_that_are_not_rotation_times_uniform_scale_are_refused_by_the_ring_axes() {
    for parent in [
        NativeTransform {
            scale: [1.0, 2.0, 1.0],
            ..turned_parent()
        },
        NativeTransform {
            scale: [-1.0, 1.0, 1.0],
            ..turned_parent()
        },
    ] {
        let (scene, node) = nested(parent, [30.0, 50.0, 70.0]);
        let refused = Some(NativeError::UnsupportedRotate(
            RotateUnsupported::ParentTransform,
        ));
        assert_eq!(scene.gimbal_axes(&target(&scene, node)).err(), refused);
    }
}
