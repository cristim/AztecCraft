use super::*;
use crate::native_scene::source::{SourceLeaf, Storage};

fn target(scene: &NativeScene, node: NodeId) -> TransformTarget {
    scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.node == node)
        .unwrap()
}

fn near(actual: [f64; 3], expected: [f64; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
    }
}

fn rotate_x([x, y, z]: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [x, c * y - s * z, s * y + c * z]
}

fn rotate_y([x, y, z]: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [c * x + s * z, y, -s * x + c * z]
}

fn rotate_z([x, y, z]: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = angle.sin_cos();
    [c * x - s * y, s * x + c * y, z]
}

fn retain_translation(scene: &mut NativeScene, node: NodeId) {
    let index = scene
        .nodes
        .iter()
        .position(|entry| entry.id == node)
        .unwrap();
    let owner = scene.nodes[index].transform_id;
    let values = scene.internal_transform(node).unwrap().translation_cm;
    let bytes: Vec<_> = values.into_iter().flat_map(f64::to_be_bytes).collect();
    scene.source = Some(SourceRetention {
        bytes: bytes.into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..24).collect(),
        leaves: vec![SourceLeaf {
            payload: 0..24,
            numeric: 0..24,
            owner,
            field: TransformField::Translation,
            storage: Storage::Centimeters,
        }],
        appearance: None,
    });
    for component in 0..3 {
        scene.nodes[index].channels[component].origin =
            Some(ScalarOrigin::Binary { leaf: 0, component });
        scene.nodes[index].channels[component].replaced = false;
    }
    scene.validate().unwrap();
}

#[test]
fn world_axes_use_exact_parent_path_and_declared_units() {
    for (unit, factor) in [
        (LinearUnit::Centimeter, 1.0),
        (LinearUnit::Millimeter, 0.1),
        (LinearUnit::Meter, 100.0),
        (LinearUnit::Inch, 2.54),
        (LinearUnit::Foot, 30.48),
    ] {
        for angular in [AngularUnit::Degree, AngularUnit::Radian] {
            let (mut scene, root, child, _) = test_fixtures::grouped_shape();
            scene.units.linear = unit.clone();
            scene.units.angular = angular.clone();
            let angles = [30f64.to_radians(), -20f64.to_radians(), 50f64.to_radians()];
            let rotation = if angular == AngularUnit::Degree {
                [30.0, -20.0, 50.0]
            } else {
                angles
            };
            scene
                .set_transform(
                    root,
                    NativeTransform {
                        translation: [7.0, -11.0, 13.0],
                        rotation,
                        scale: [2.0, -3.0, 4.0],
                    },
                )
                .unwrap();
            scene
                .set_transform(
                    child,
                    NativeTransform {
                        translation: [1.25, -2.5, 3.75],
                        rotation: [11.0, 19.0, -8.0],
                        scale: [0.5, 2.0, 3.0],
                    },
                )
                .unwrap();
            let selected = target(&scene, child);
            let frame = scene.world_move_frame(&selected).unwrap();
            let rotated = rotate_z(
                rotate_y(
                    rotate_x([2.5 * factor, 7.5 * factor, 15.0 * factor], angles[0]),
                    angles[1],
                ),
                angles[2],
            );
            let expected_origin = [
                rotated[0] + 7.0 * factor,
                rotated[1] - 11.0 * factor,
                rotated[2] + 13.0 * factor,
            ];
            near(frame.origin_cm().to_array(), expected_origin);
            let baseline = scene.clone();
            for (axis, world) in [
                (WorldAxis::X, [7.5, 0.0, 0.0]),
                (WorldAxis::Y, [0.0, 7.5, 0.0]),
                (WorldAxis::Z, [0.0, 0.0, 7.5]),
            ] {
                let inverse = rotate_x(
                    rotate_y(rotate_z(world, -angles[2]), -angles[1]),
                    -angles[0],
                );
                let expected = [
                    1.25 + inverse[0] / (2.0 * factor),
                    -2.5 + inverse[1] / (-3.0 * factor),
                    3.75 + inverse[2] / (4.0 * factor),
                ];
                let translation = frame.translation(axis, 7.5).unwrap();
                near(translation, expected);
                let mut moved = baseline.clone();
                let mut channels = [None; 9];
                for component in 0..3 {
                    channels[component] = Some(translation[component]);
                }
                moved.edit_node(child, None, channels).unwrap();
                let origin = moved
                    .world_move_frame(&selected)
                    .unwrap()
                    .origin_cm()
                    .to_array();
                near(
                    origin,
                    [
                        expected_origin[0] + world[0],
                        expected_origin[1] + world[1],
                        expected_origin[2] + world[2],
                    ],
                );
                assert_eq!(moved.node(root).unwrap(), baseline.node(root).unwrap());
                assert_eq!(moved.shapes, baseline.shapes);
            }
            assert_eq!(scene, baseline);
        }
    }
}

#[test]
fn zero_and_unchanged_axes_preserve_signed_zero_and_source_intent() {
    let (mut scene, node) = test_fixtures::cube();
    scene.set_channel(node, Channel::TranslateY, -0.0).unwrap();
    retain_translation(&mut scene, node);
    scene.set_channel(node, Channel::TranslateX, 2.0).unwrap();
    let baseline = scene.clone();
    let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
    for zero in [0.0, -0.0] {
        assert_eq!(
            frame
                .translation(WorldAxis::X, zero)
                .unwrap()
                .map(f64::to_bits),
            [2.0, -0.0, 0.0].map(f64::to_bits)
        );
    }
    assert_eq!(
        frame
            .translation(WorldAxis::X, -3.5)
            .unwrap()
            .map(f64::to_bits),
        [-1.5, -0.0, 0.0].map(f64::to_bits)
    );
    assert_eq!(scene, baseline);
}

#[test]
fn transform_instances_use_the_selected_parent_and_share_the_authoring_node() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    scene.set_channel(root, Channel::RotateZ, 90.0).unwrap();
    scene.set_channel(root, Channel::ScaleX, 2.0).unwrap();
    let child_id = scene.node(child).unwrap().transform_id;
    scene.dag.push(DagEdge {
        parent: None,
        child: child_id,
    });
    scene.validate().unwrap();
    let paths: Vec<_> = scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .filter(|path| path.node == child)
        .collect();
    let nested = paths.iter().find(|path| path.path.len() == 2).unwrap();
    let direct = paths.iter().find(|path| path.path.len() == 1).unwrap();
    near(
        scene
            .world_move_frame(nested)
            .unwrap()
            .translation(WorldAxis::Y, 6.0)
            .unwrap(),
        [3.0, 0.0, 0.0],
    );
    near(
        scene
            .world_move_frame(direct)
            .unwrap()
            .translation(WorldAxis::Y, 6.0)
            .unwrap(),
        [0.0, 6.0, 0.0],
    );
    scene.set_channel(child, Channel::TranslateX, 3.0).unwrap();
    near(
        scene
            .world_move_frame(nested)
            .unwrap()
            .origin_cm()
            .to_array(),
        [0.0, 6.0, 0.0],
    );
    near(
        scene
            .world_move_frame(direct)
            .unwrap()
            .origin_cm()
            .to_array(),
        [3.0, 0.0, 0.0],
    );
    assert!(scene.world_move_frame(&target(&scene, root)).is_ok());
    let (multi, first, _, _) = test_fixtures::shared_shape();
    assert!(multi.world_move_frame(&target(&multi, first)).is_ok());
}

#[test]
fn admission_refuses_pivots_drivers_source_gaps_and_bad_paths_without_mutation() {
    let (mut scene, node) = test_fixtures::cube();
    let selected = target(&scene, node);
    for scale_pivot in [false, true] {
        let mut candidate = scene.clone();
        if scale_pivot {
            candidate.nodes[0].scale_pivot[1] = Scalar::new(0.25);
        } else {
            candidate.nodes[0].rotate_pivot[2] = Scalar::new(0.25);
        }
        assert!(matches!(
            candidate.world_move_frame(&selected),
            Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::SelectedPivot
            ))
        ));
    }
    for unsupported in [false, true] {
        let mut candidate = scene.clone();
        if unsupported {
            candidate.nodes[0]
                .unsupported_transform
                .push("driver".into());
        } else {
            candidate.nodes[0].restrictions.push("driver".into());
        }
        assert!(matches!(
            candidate.world_move_frame(&selected),
            Err(NativeError::UnsupportedEdit)
        ));
    }
    scene.source = Some(SourceRetention {
        bytes: [1].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    assert!(matches!(
        scene.world_move_frame(&selected),
        Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::TranslationSource
        ))
    ));
    scene.source.as_mut().unwrap().format = SourceFormat::Ascii;
    assert!(matches!(
        scene.world_move_frame(&selected),
        Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::TranslationSource
        ))
    ));
    let before = scene.clone();
    for path in [
        vec![],
        vec![NativeId(999)],
        vec![selected.path[0], selected.path[0]],
    ] {
        assert!(matches!(
            scene.world_move_frame(&TransformTarget { node, path }),
            Err(NativeError::InvalidInstancePath)
        ));
    }
    assert!(matches!(
        scene.world_move_frame(&TransformTarget {
            node,
            path: vec![selected.path[0]; MAX_TRANSFORM_DEPTH + 1]
        }),
        Err(NativeError::ResourceLimit)
    ));
    assert_eq!(scene, before);
}

#[test]
fn singular_unstable_overflow_and_unrepresentable_motion_are_refused() {
    let (scene, root, child, _) = test_fixtures::grouped_shape();
    for scale in [
        [0.0, 1.0, 1.0],
        [1e-13, 1.0, 1.0],
        [f64::MAX, f64::MAX, f64::MAX],
    ] {
        let mut candidate = scene.clone();
        candidate
            .set_transform(
                root,
                NativeTransform {
                    scale,
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        assert!(matches!(
            candidate.world_move_frame(&target(&candidate, child)),
            Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform
            ))
        ));
    }
    let (mut scene, node) = test_fixtures::cube();
    scene.set_channel(node, Channel::TranslateX, 1e30).unwrap();
    let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
    for distance in [1.0, -1.0] {
        assert!(matches!(
            frame.translation(WorldAxis::X, distance),
            Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::NumericalDisplacement
            ))
        ));
    }
    let (mut tiny, root, child, _) = test_fixtures::grouped_shape();
    tiny.set_transform(
        root,
        NativeTransform {
            scale: [0.5; 3],
            ..NativeTransform::default()
        },
    )
    .unwrap();
    let tiny_frame = tiny.world_move_frame(&target(&tiny, child)).unwrap();
    assert!(matches!(
        tiny_frame.translation(WorldAxis::X, f64::MAX),
        Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::NumericalDisplacement
        ))
    ));
    for distance in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            frame.translation(WorldAxis::Y, distance),
            Err(NativeError::NonFinite)
        ));
    }
}

#[test]
fn binary_nonbinary_units_check_actual_stored_displacement() {
    for (unit, factor, distance, expected) in [
        (LinearUnit::Inch, 2.54, 5.08, 1005.08_f64),
        (LinearUnit::Foot, 30.48, 30.48, 1030.48_f64),
    ] {
        let (mut scene, node) = test_fixtures::cube();
        scene.units.linear = unit;
        scene
            .set_channel(node, Channel::TranslateX, 1000.0 / factor)
            .unwrap();
        retain_translation(&mut scene, node);
        let selected = target(&scene, node);
        let frame = scene.world_move_frame(&selected).unwrap();
        let before = scene.clone();
        if factor == 2.54 {
            assert!(matches!(
                frame.translation(WorldAxis::X, 2.54),
                Err(NativeError::UnsupportedMove(
                    WorldMoveUnsupported::NumericalDisplacement
                ))
            ));
            assert_eq!(scene, before);
        }
        let translation = frame.translation(WorldAxis::X, distance).unwrap();
        scene
            .edit_node(
                node,
                None,
                [
                    Some(translation[0]),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                ],
            )
            .unwrap();
        assert_eq!(
            scene.internal_transform(node).unwrap().translation_cm[0].to_bits(),
            expected.to_bits()
        );
        let stored_distance = scene.internal_transform(node).unwrap().translation_cm[0] - 1000.0;
        assert!((stored_distance - distance).abs() <= 128.0 * f64::EPSILON * distance);
        assert_eq!(scene.source, before.source);
        assert_eq!(scene.nodes[0].channels[1..], before.nodes[0].channels[1..]);
    }
}

#[test]
fn deep_noncommuting_ancestors_use_root_to_leaf_composition() {
    let (mut scene, root, parent, _) = test_fixtures::grouped_shape();
    let child = scene.create(Primitive::Cube, "grandchild").unwrap();
    let child_id = scene.node(child).unwrap().transform_id;
    scene
        .dag
        .retain(|edge| !(edge.child == child_id && edge.parent.is_none()));
    scene.dag.push(DagEdge {
        parent: Some(scene.node(parent).unwrap().transform_id),
        child: child_id,
    });
    for (node, translation, rotation, scale) in [
        (root, [5.0, 7.0, -2.0], [0.0, 0.0, 90.0], [2.0, 3.0, -1.0]),
        (parent, [1.0, 2.0, 3.0], [90.0, 0.0, 0.0], [4.0, -2.0, 0.5]),
        (child, [-1.0, 3.0, 2.0], [0.0; 3], [1.0; 3]),
    ] {
        scene
            .set_transform(
                node,
                NativeTransform {
                    translation,
                    rotation,
                    scale,
                },
            )
            .unwrap();
    }
    let selected = target(&scene, child);
    assert_eq!(selected.path.len(), 3);
    let frame = scene.world_move_frame(&selected).unwrap();
    near(frame.origin_cm().to_array(), [2.0, 1.0, 1.0]);
    let translated = frame.translation(WorldAxis::X, 6.0).unwrap();
    near(translated, [-1.0, 3.0, 6.0]);
    scene
        .edit_node(
            child,
            None,
            [
                Some(translated[0]),
                Some(translated[1]),
                Some(translated[2]),
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        )
        .unwrap();
    near(
        scene
            .world_move_frame(&selected)
            .unwrap()
            .origin_cm()
            .to_array(),
        [8.0, 1.0, 1.0],
    );
}

#[test]
fn moving_selected_group_with_multiple_shapes_updates_every_derived_instance() {
    use crate::view_scene::ViewScene;
    let (mut scene, group, child, child_shape) = test_fixtures::grouped_shape();
    let other = scene.create(Primitive::Cube, "otherShapeOwner").unwrap();
    let other_id = scene.node(other).unwrap().transform_id;
    let other_shape = scene.shapes.last().unwrap().id;
    let group_id = scene.node(group).unwrap().transform_id;
    scene
        .dag
        .retain(|edge| !(edge.child == other_shape && edge.parent == Some(other_id)));
    scene.dag.push(DagEdge {
        parent: Some(group_id),
        child: other_shape,
    });
    scene.dag.push(DagEdge {
        parent: Some(group_id),
        child: child_shape,
    });
    scene.set_channel(child, Channel::TranslateZ, 3.0).unwrap();
    scene.validate().unwrap();
    let selected = target(&scene, group);
    let before = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    assert_eq!(before.instances().len(), 3);
    let translation = scene
        .world_move_frame(&selected)
        .unwrap()
        .translation(WorldAxis::Y, -7.0)
        .unwrap();
    scene
        .edit_node(
            group,
            None,
            [
                None,
                Some(translation[1]),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        )
        .unwrap();
    let after = ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    for (before, after) in before.instances().iter().zip(after.instances()) {
        assert_eq!(before.path(), after.path());
        for (before, after) in before.world_vertices().iter().zip(after.world_vertices()) {
            near(
                after.as_dvec3().to_array(),
                [
                    f64::from(before.x),
                    f64::from(before.y) - 7.0,
                    f64::from(before.z),
                ],
            );
        }
    }
}
