use super::*;
use glam::{DMat3, DQuat, DVec3, EulerRot};

fn quat(degrees: [f64; 3]) -> DQuat {
    let [x, y, z] = degrees.map(f64::to_radians);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

fn begin(
    scene: &NativeScene,
    node: NodeId,
    handle: ScaleHandle,
    space: ScaleSpace,
) -> Result<(ScaleFrame, ChannelBaseline), NativeError> {
    scene.begin_scale_frame(&target(scene, node), handle, space)
}

fn parent_scale(scene: &NativeScene, node: NodeId, handle: ScaleHandle, factor: f64) -> [f64; 3] {
    begin(scene, node, handle, ScaleSpace::Parent)
        .unwrap()
        .0
        .scale(handle, factor)
        .unwrap()
}

fn place(scene: &mut NativeScene, node: NodeId, rotation: [f64; 3], scale: [f64; 3]) {
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation,
                scale,
                ..NativeTransform::default()
            },
        )
        .unwrap();
}

// Independent reference: the node's local linear map (the space its parent sees) is R * S.
// Scaling by `factor` along parent axis `axis` is D * R * S; the new channels S' must give R * S'
// equal to it.
fn assert_parent_scale(rotation: [f64; 3], before: [f64; 3], after: [f64; 3], axis: usize, f: f64) {
    let r = DMat3::from_quat(quat(rotation));
    let old = r * DMat3::from_diagonal(DVec3::from_array(before));
    let new = r * DMat3::from_diagonal(DVec3::from_array(after));
    let mut d = DVec3::ONE;
    d[axis] = f;
    let expected = DMat3::from_diagonal(d) * old;
    let drift = (new - expected).to_cols_array().map(f64::abs);
    let drift = drift.into_iter().fold(0.0, f64::max);
    assert!(drift <= 1e-12, "axis {axis} factor {f}: drift {drift}");
}

#[test]
fn quarter_turns_map_each_parent_axis_to_the_independent_local_channel() {
    let (mut scene, node) = test_fixtures::cube();
    for rotation in [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [90.0, 0.0, 0.0],
        [0.0, 90.0, 90.0],
        [180.0, 90.0, 270.0],
    ] {
        let before = [2.0, 3.0, 5.0];
        place(&mut scene, node, rotation, before);
        for (axis, handle) in [ScaleHandle::X, ScaleHandle::Y, ScaleHandle::Z]
            .into_iter()
            .enumerate()
        {
            let after = parent_scale(&scene, node, handle, 1.75);
            assert_parent_scale(rotation, before, after, axis, 1.75);
        }
    }
}

#[test]
fn a_z_quarter_turn_swaps_parent_x_and_local_y() {
    let (mut scene, node) = test_fixtures::cube();
    place(&mut scene, node, [0.0, 0.0, 90.0], [2.0, 3.0, 5.0]);
    let out = parent_scale(&scene, node, ScaleHandle::X, 2.0);
    for (got, want) in out.into_iter().zip([2.0, 6.0, 5.0]) {
        assert!((got - want).abs() < 1e-12, "{out:?}");
    }
}

#[test]
fn a_root_node_without_an_offset_parent_matrix_has_parent_equal_to_world() {
    let (mut scene, node) = test_fixtures::cube();
    place(&mut scene, node, [0.0, 0.0, 90.0], [2.0, 3.0, 5.0]);
    for handle in [
        ScaleHandle::X,
        ScaleHandle::Y,
        ScaleHandle::Z,
        ScaleHandle::Uniform,
    ] {
        let world = begin(&scene, node, handle, ScaleSpace::World).unwrap().0;
        assert_eq!(
            parent_scale(&scene, node, handle, 1.3).map(f64::to_bits),
            world.scale(handle, 1.3).unwrap().map(f64::to_bits)
        );
    }
}

#[test]
fn parent_space_ignores_the_parent_that_world_space_applies() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    place(&mut scene, root, [0.0, 0.0, 90.0], [2.0, 2.0, 2.0]);
    place(&mut scene, child, [0.0, 0.0, 0.0], [2.0, 3.0, 5.0]);
    // World X is the child's local Y under the turned parent; Parent X is its own local X.
    let world = begin(&scene, child, ScaleHandle::X, ScaleSpace::World)
        .unwrap()
        .0
        .scale(ScaleHandle::X, 2.0)
        .unwrap();
    assert_eq!(world, [2.0, 6.0, 5.0]);
    assert_eq!(
        parent_scale(&scene, child, ScaleHandle::X, 2.0),
        [4.0, 3.0, 5.0]
    );
}

#[test]
fn a_non_uniform_parent_does_not_refuse_parent_space() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    place(&mut scene, root, [10.0, 20.0, 30.0], [2.0, 3.0, 4.0]);
    place(&mut scene, child, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    assert!(matches!(
        begin(&scene, child, ScaleHandle::X, ScaleSpace::World),
        Err(NativeError::UnsupportedScale(
            ScaleUnsupported::ParentTransform
        ))
    ));
    assert_eq!(
        parent_scale(&scene, child, ScaleHandle::X, 2.0),
        [2.0, 1.0, 1.0]
    );
}

#[test]
fn oblique_rotation_refuses_the_misaligned_axes_and_keeps_the_scene() {
    let (mut scene, node) = test_fixtures::cube();
    place(&mut scene, node, [0.0, 0.0, 30.0], [2.0, 3.0, 5.0]);
    let before = scene.clone();
    for handle in [ScaleHandle::X, ScaleHandle::Y] {
        assert!(matches!(
            begin(&scene, node, handle, ScaleSpace::Parent),
            Err(NativeError::UnsupportedScale(
                ScaleUnsupported::NotAxisAligned
            ))
        ));
    }
    assert!(begin(&scene, node, ScaleHandle::Z, ScaleSpace::Parent).is_ok());
    assert_eq!(
        parent_scale(&scene, node, ScaleHandle::Uniform, 2.0),
        [4.0, 6.0, 10.0]
    );
    assert_eq!(scene, before);
}

#[test]
fn shear_is_refused_in_parent_space_before_any_other_check() {
    let (mut scene, node) = test_fixtures::cube();
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            shear: Some([0.5, 0.0, 0.0]),
            ..TransformExtras::default()
        },
    );
    let before = scene.clone();
    for handle in [ScaleHandle::X, ScaleHandle::Uniform] {
        assert!(matches!(
            begin(&scene, node, handle, ScaleSpace::Parent),
            Err(NativeError::UnsupportedScale(ScaleUnsupported::Shear))
        ));
        assert!(begin(&scene, node, handle, ScaleSpace::Object).is_ok());
    }
    assert_eq!(scene, before);
}

#[test]
fn factor_one_is_bit_exact_in_parent_space() {
    let (mut scene, node) = test_fixtures::cube();
    place(
        &mut scene,
        node,
        [0.0, 0.0, 90.0],
        [0.1 + 0.2, -0.0, 1e-300],
    );
    for handle in [ScaleHandle::X, ScaleHandle::Y, ScaleHandle::Uniform] {
        assert_eq!(
            parent_scale(&scene, node, handle, 1.0).map(f64::to_bits),
            [0.1 + 0.2, -0.0, 1e-300].map(f64::to_bits)
        );
    }
}
