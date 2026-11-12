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

fn near(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).abs().max_element() < 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn a_root_frame_sits_at_its_translation_with_its_own_rotation() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 90.0],
                scale: [5.0, 6.0, 7.0],
            },
        )
        .unwrap();
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    near(frame.origin_cm, DVec3::new(1.0, 2.0, 3.0));
    // Node scale never reaches the axes; a quarter turn about Z sends X to Y and Y to -X.
    near(frame.object_axes[0], DVec3::Y);
    near(frame.object_axes[1], -DVec3::X);
    near(frame.object_axes[2], DVec3::Z);
}

#[test]
fn parents_move_the_origin_and_turn_and_scale_the_axes() {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                translation: [10.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 90.0],
                scale: [2.0, 2.0, 2.0],
            },
        )
        .unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    near(frame.origin_cm, DVec3::new(10.0, 2.0, 0.0));
    near(frame.object_axes[0], DVec3::new(0.0, 2.0, 0.0));
    near(frame.object_axes[1], DVec3::new(-2.0, 0.0, 0.0));
}

#[test]
fn a_negative_scale_parent_gives_left_handed_axes() {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                scale: [-1.0, 1.0, 1.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    let [x, y, z] = frame.object_axes;
    assert!(x.cross(y).dot(z) < 0.0);
}

#[test]
fn an_invalid_target_is_refused() {
    let (scene, node) = test_fixtures::cube();
    let mut bad = target_of(&scene, node);
    bad.path.clear();
    assert!(scene.manipulator_frame(&bad).is_err());
}

#[test]
fn refusals_report_what_begin_would_refuse() {
    let (scene, node) = test_fixtures::cube();
    let target = target_of(&scene, node);
    assert_eq!(
        scene.rotation_refusal(&target, RotateAxis::Y, RotateSpace::Object),
        None
    );
    assert_eq!(
        scene.scale_refusal(&target, ScaleHandle::Uniform, ScaleSpace::World),
        None
    );
    let (mut uneven, root, child, _) = test_fixtures::grouped_shape();
    uneven
        .set_transform(
            root,
            NativeTransform {
                scale: [1.0, 2.0, 1.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    assert_eq!(
        uneven.scale_refusal(
            &target_of(&uneven, child),
            ScaleHandle::X,
            ScaleSpace::World
        ),
        Some(NativeError::UnsupportedScale(
            ScaleUnsupported::ParentTransform
        ))
    );
    let (pivoted, node) = test_fixtures::pivoted_shape();
    assert_eq!(
        pivoted.rotation_refusal(
            &target_of(&pivoted, node),
            RotateAxis::X,
            RotateSpace::Object
        ),
        None
    );
}

fn assert_orthonormal_right_handed(axes: [DVec3; 3]) {
    for axis in axes {
        assert!((axis.length() - 1.0).abs() < 1e-12, "{axis}");
    }
    assert!(axes[0].dot(axes[1]).abs() < 1e-12);
    assert!(axes[1].dot(axes[2]).abs() < 1e-12);
    assert!(axes[2].dot(axes[0]).abs() < 1e-12);
    assert!(axes[0].cross(axes[1]).dot(axes[2]) > 0.0);
}

#[test]
fn a_sheared_parent_keeps_the_handles_skewed_frame_refusal() {
    use crate::manipulator_handles::HandleFrame;
    use crate::move_handles::HandleError;
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    test_fixtures::set_extras(
        &mut scene,
        root,
        TransformExtras {
            shear: Some([0.5, 0.25, -0.75]),
            ..TransformExtras::default()
        },
    );
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    assert_eq!(
        HandleFrame::new(frame.origin_cm, frame.object_axes).err(),
        Some(HandleError::SkewedFrame)
    );
}

#[test]
fn uneven_scale_above_a_rotation_stays_refused_without_any_extras() {
    use crate::manipulator_handles::HandleFrame;
    use crate::move_handles::HandleError;
    let (mut scene, root, mid, leaf) = test_fixtures::chain_of_three();
    scene
        .set_transform(
            root,
            NativeTransform {
                scale: [2.0, 1.0, 1.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene
        .set_transform(
            mid,
            NativeTransform {
                rotation: [0.0, 0.0, 45.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let frame = scene.manipulator_frame(&target_of(&scene, leaf)).unwrap();
    assert_eq!(
        HandleFrame::new(frame.origin_cm, frame.object_axes).err(),
        Some(HandleError::SkewedFrame)
    );
}

#[test]
fn a_default_translation_keeps_negative_zero_in_the_frame_origin() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [-0.0, 1.0, -0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let internal = scene.internal_transform(node).unwrap();
    assert!(internal.extras.is_none());
    let origin = internal.origin_cm();
    assert!(origin.x.is_sign_negative() && origin.z.is_sign_negative());
}

#[test]
fn move_and_rotate_accept_a_rotate_pivot_and_their_frame_origin_is_the_world_pivot() {
    let (mut scene, node) = test_fixtures::cube();
    scene.nodes[0].rotate_pivot[1] = Scalar::new(0.25);
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            rotate_pivot_translate: Some([0.0, 3.0, 0.0]),
            ..TransformExtras::default()
        },
    );
    let selected = target_of(&scene, node);
    assert!(scene.world_move_frame(&selected).is_ok());
    assert!(
        scene
            .begin_rotation(&selected, RotateAxis::X, RotateSpace::Object)
            .is_ok()
    );
}

#[test]
fn a_nodes_own_shear_and_rotate_axis_orient_the_frame_without_skewing_it() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [10.0, 20.0, 30.0],
                scale: [2.0, 3.0, 4.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let plain = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            shear: Some([0.5, 0.25, -0.75]),
            ..TransformExtras::default()
        },
    );
    let sheared = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    assert_orthonormal_right_handed(sheared.object_axes);
    for (a, b) in plain.object_axes.into_iter().zip(sheared.object_axes) {
        near(a, b);
    }
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            rotate_order: Some(5),
            rotate_axis: Some([0.0, 0.0, 90.0]),
            ..TransformExtras::default()
        },
    );
    let turned = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    assert_orthonormal_right_handed(turned.object_axes);
    // Independent row-vector reference: order zyx (5), RA turns 90 degrees about Z.
    let want = crate::native_scene::transform_extras::tests::row_vector_axes(
        5,
        [10f64.to_radians(), 20f64.to_radians(), 30f64.to_radians()],
        [0.0, 0.0, 90f64.to_radians()],
    );
    for (axis, expected) in turned.object_axes.into_iter().zip(want) {
        near(axis, DVec3::from_array(expected));
    }
}

#[test]
fn multi_axis_rotate_axis_frame_matches_the_matrix_and_the_row_vector_reference() {
    let (mut scene, node) = test_fixtures::cube();
    let r = [23.0, -41.0, 67.0];
    let ra = [17.0, -33.0, 52.0];
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: r,
                ..NativeTransform::default()
            },
        )
        .unwrap();
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            rotate_order: Some(1),
            rotate_axis: Some(ra),
            ..TransformExtras::default()
        },
    );
    let internal = scene.internal_transform(node).unwrap();
    let frame = internal.rotation_frame().unwrap();
    let linear = DMat3::from_mat4(internal.matrix().unwrap());
    let rad = |v: [f64; 3]| v.map(f64::to_radians);
    let want = crate::native_scene::transform_extras::tests::row_vector_axes(1, rad(r), rad(ra));
    for (i, column) in [frame.x_axis, frame.y_axis, frame.z_axis]
        .into_iter()
        .enumerate()
    {
        near(column, [linear.x_axis, linear.y_axis, linear.z_axis][i]);
        near(column, DVec3::from_array(want[i]));
    }
    let handles = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    near(handles.object_axes[0], frame.x_axis);
}

#[test]
fn scale_origin_is_the_scale_fixed_point_not_the_translate() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 90.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let selected = target_of(&scene, node);
    let plain = scene.manipulator_frame(&selected).unwrap();
    near(plain.scale_origin_cm, plain.origin_cm);
    // spt: ST sits right of R, so the fixed point moves by R * spt = (0, 2, 0).
    test_fixtures::set_extras(
        &mut scene,
        node,
        TransformExtras {
            scale_pivot_translate: Some([2.0, 0.0, 0.0]),
            ..TransformExtras::default()
        },
    );
    let frame = scene.manipulator_frame(&selected).unwrap();
    near(frame.origin_cm, DVec3::new(1.0, 0.0, 0.0));
    near(frame.scale_origin_cm, DVec3::new(1.0, 2.0, 0.0));
    // rp = (1, 0, 0) under Rz(90): RP + R * (-RP) = (1, -1, 0), plus T.
    test_fixtures::set_extras(&mut scene, node, TransformExtras::default());
    scene.nodes[0].rotate_pivot[0] = Scalar::new(1.0);
    let frame = scene.manipulator_frame(&selected).unwrap();
    near(frame.scale_origin_cm, DVec3::new(2.0, -1.0, 0.0));
}

#[test]
fn axes_that_overflow_are_refused() {
    // The parent map is finite, but its product with a 45 degree turn sums two 1.2e308 terms.
    let (mut scene, root, leaf, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                scale: [1.7e308; 3],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene
        .set_transform(
            leaf,
            NativeTransform {
                rotation: [0.0, 0.0, 45.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    test_fixtures::set_extras(
        &mut scene,
        root,
        TransformExtras {
            shear: Some([1.0, 0.0, 0.0]),
            ..TransformExtras::default()
        },
    );
    assert!(matches!(
        scene.manipulator_frame(&target_of(&scene, leaf)),
        Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::ParentTransform
        ))
    ));
}

#[test]
fn scale_origin_includes_a_nonzero_scale_pivot() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                scale: [3.0, 3.0, 3.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene.nodes[0].scale_pivot[0] = Scalar::new(2.0);
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    // Scaling about the pivot leaves its image fixed: T + sp.
    near(frame.scale_origin_cm, DVec3::new(3.0, 0.0, 0.0));
    near(frame.origin_cm, DVec3::new(1.0, 0.0, 0.0));
}

#[test]
fn the_frame_pivot_is_the_world_image_of_the_rotate_pivot_including_rp() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [1.0, 2.0, 3.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene.nodes[0].rotate_pivot[0] = Scalar::new(0.5);
    scene.nodes[0].rotate_pivot[2] = Scalar::new(-4.0);
    scene.validate().unwrap();
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    // T + RT + RP in the parent's space; a rotation about RP leaves RP fixed. origin_cm keeps
    // ignoring RP, which Frame Selected and the Rotate and Scale handles rely on.
    near(frame.pivot_cm, DVec3::new(1.5, 2.0, -1.0));
    near(frame.origin_cm, DVec3::new(1.0, 2.0, 3.0));
}

#[test]
fn handle_origin_is_the_rotate_pivot_for_move_and_rotate_and_the_scale_fixed_point_for_scale() {
    for (name, scene, node) in
        test_fixtures::pivoted_family([20.0, 35.0, -15.0], [30.0, -20.0, 50.0], true)
    {
        let selected = target_of(&scene, node);
        let frame = scene.manipulator_frame(&selected).unwrap();
        let rotate_fixed = test_fixtures::world_image(
            &scene,
            &selected,
            test_fixtures::rotate_fixed_point(&scene, node),
        );
        let scale_fixed = test_fixtures::world_image(
            &scene,
            &selected,
            DVec3::from_array(scene.internal_transform(node).unwrap().scale_pivot_cm),
        );
        for tool in [ManipulatorTool::Move, ManipulatorTool::Rotate] {
            near_relative(frame.handle_origin(tool), rotate_fixed);
        }
        near_relative(frame.handle_origin(ManipulatorTool::Scale), scale_fixed);
        // The pivots must be nonzero and distinct from T + RT, or the test proves nothing.
        assert!((frame.pivot_cm - frame.origin_cm).length() > 0.1, "{name}");
        assert!(
            (frame.scale_origin_cm - frame.pivot_cm).length() > 0.1,
            "{name}"
        );
    }
}

fn near_relative(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).abs().max_element() <= 1e-9 * (1.0 + expected.abs().max_element()),
        "{actual} != {expected}"
    );
}

/// A pivot does not hide any other refusal: a restricted node refuses all three tools, and a
/// non-xyz rotate order or a rotate axis still refuses Rotate (Move and Scale accept them).
#[test]
fn other_refusals_still_win_on_a_pivoted_node() {
    let (mut scene, node) = test_fixtures::pivoted_shape();
    let selected = target_of(&scene, node);
    let mut restricted = scene.clone();
    test_fixtures::restrict_edits(&mut restricted, node);
    assert!(restricted.world_move_frame(&selected).is_err());
    assert_eq!(
        restricted.rotation_refusal(&selected, RotateAxis::X, RotateSpace::Object),
        Some(NativeError::UnsupportedEdit)
    );
    assert_eq!(
        restricted.scale_refusal(&selected, ScaleHandle::X, ScaleSpace::Object),
        Some(NativeError::UnsupportedEdit)
    );
    for (extras, expected) in [
        (
            TransformExtras {
                rotate_order: Some(1),
                ..TransformExtras::default()
            },
            RotateUnsupported::RotateOrder,
        ),
        (
            TransformExtras {
                rotate_axis: Some([1.0, 0.0, 0.0]),
                ..TransformExtras::default()
            },
            RotateUnsupported::RotateAxis,
        ),
    ] {
        test_fixtures::set_extras(&mut scene, node, extras);
        assert_eq!(
            scene.rotation_refusal(&selected, RotateAxis::X, RotateSpace::Object),
            Some(NativeError::UnsupportedRotate(expected))
        );
        assert!(scene.world_move_frame(&selected).is_ok());
        assert_eq!(
            scene.scale_refusal(&selected, ScaleHandle::X, ScaleSpace::Object),
            None
        );
    }
}

#[test]
fn ancestor_offsets_and_inherits_off_shape_the_handle_frame() {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    let at = |scene: &mut NativeScene, id, x| {
        scene
            .set_transform(
                id,
                NativeTransform {
                    translation: [x, 0.0, 0.0],
                    ..NativeTransform::default()
                },
            )
            .unwrap();
    };
    at(&mut scene, root, 10.0);
    at(&mut scene, node, 1.0);
    let mut offset = TransformExtras {
        offset_parent_matrix: Some([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 4.0, 0.0, 1.0,
        ]),
        ..TransformExtras::default()
    };
    test_fixtures::set_extras(&mut scene, root, offset.clone());
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    near(frame.pivot_cm, DVec3::new(11.0, 4.0, 0.0));
    // The selected node's own inheritsTransform off drops the whole parent chain.
    test_fixtures::set_extras(&mut scene, root, TransformExtras::default());
    offset.inherits_transform = Some(false);
    test_fixtures::set_extras(&mut scene, node, offset);
    let frame = scene.manipulator_frame(&target_of(&scene, node)).unwrap();
    near(frame.pivot_cm, DVec3::new(1.0, 4.0, 0.0));
}
