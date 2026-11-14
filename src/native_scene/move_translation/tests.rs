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
        has_removals: false,
        sites: Vec::new(),
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
fn admission_refuses_drivers_source_gaps_and_bad_paths_without_mutation() {
    let (mut scene, node) = test_fixtures::cube();
    let selected = target(&scene, node);
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
        has_removals: false,
        sites: Vec::new(),
        bytes: [1].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    let frame = scene.world_move_frame(&selected).unwrap();
    assert!(matches!(
        frame.require_leaves(WorldAxis::X),
        Err(NativeError::AbsentChannelLeaf {
            channel: ChannelLeaf::Translate,
            component: 0
        })
    ));
    scene.source.as_mut().unwrap().format = SourceFormat::Ascii;
    assert!(matches!(
        scene.world_move_frame(&selected),
        Err(NativeError::ReadOnlyFormat)
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
            // Ordinary inch rounding at 1000 cm is within the storage precision, not a refusal.
            let nudged = frame.translation(WorldAxis::X, 2.54).unwrap();
            assert!(((nudged[0] - 1000.0 / factor) * factor - 2.54).abs() < 1e-12);
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
fn absent_translate_leaf_in_a_binary_source_is_refused_on_every_axis_without_mutation() {
    let (mut scene, node) = test_fixtures::cube();
    scene.source = Some(SourceRetention {
        has_removals: false,
        sites: Vec::new(),
        bytes: [1].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    let selected = target(&scene, node);
    let before = scene.clone();
    for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
        let result = scene.begin_move_translation(&selected, axis);
        assert!(matches!(
            result,
            Err(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Translate,
                ..
            })
        ));
        assert_eq!(scene, before);
    }
}

#[test]
fn source_free_scene_without_leaves_is_not_refused() {
    let (scene, node) = test_fixtures::cube();
    assert!(scene.source.is_none());
    let selected = target(&scene, node);
    for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
        scene.begin_move_translation(&selected, axis).unwrap();
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

#[test]
fn seeded_sweep_of_large_translations_and_distances_stays_representable_on_every_axis() {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1_u64 << 53) as f64
    };
    for rotated_parent in [false, true] {
        let (mut scene, node) = if rotated_parent {
            let (mut scene, root, child, _) = test_fixtures::grouped_shape();
            scene
                .set_transform(
                    root,
                    NativeTransform {
                        rotation: [20.0, 35.0, -15.0],
                        scale: [2.0, 0.5, 1.5],
                        ..NativeTransform::default()
                    },
                )
                .unwrap();
            (scene, child)
        } else {
            test_fixtures::cube()
        };
        for sweep in 0..300 {
            let extent = if sweep == 0 { 696.64 } else { 1e5 };
            let channels = [
                Channel::TranslateX,
                Channel::TranslateY,
                Channel::TranslateZ,
            ];
            for channel in channels {
                let value = if sweep == 0 {
                    extent
                } else {
                    (next() * 2.0 - 1.0) * extent
                };
                scene.set_channel(node, channel, value).unwrap();
            }
            let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
            for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
                let distance = if sweep == 0 {
                    0.41
                } else {
                    (0.01 + next() * 19.99) * if next() < 0.5 { -1.0 } else { 1.0 }
                };
                let translation = frame.translation(axis, distance).unwrap_or_else(|error| {
                    panic!(
                        "T={value:?} d={distance} {axis:?}: {error}",
                        value = frame.translation
                    )
                });
                let moved = DVec3::from_array(translation) * frame.linear_factor;
                let world = frame.parent_linear * (moved - frame.translation_cm);
                let expected = axis.direction() * distance;
                assert!(
                    (world - expected).abs().max_element() <= 1e-6 * distance.abs(),
                    "{rotated_parent} {axis:?} {distance}: {world:?}"
                );
            }
        }
    }
}

#[test]
fn move_origin_is_the_world_pivot_including_rotate_pivot_translate() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
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
            child,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    test_fixtures::set_extras(
        &mut scene,
        child,
        TransformExtras {
            rotate_pivot_translate: Some([0.0, 3.0, 0.0]),
            ..TransformExtras::default()
        },
    );
    let selected = target(&scene, child);
    // Parent maps (x, y, z) to (10 - 2y, 2x, 2z); the pivot is the local (1, 3, 0).
    let expected = [4.0, 2.0, 0.0];
    near(
        scene
            .world_move_frame(&selected)
            .unwrap()
            .origin_cm()
            .to_array(),
        expected,
    );
    near(
        scene
            .manipulator_frame(&selected)
            .unwrap()
            .origin_cm
            .to_array(),
        expected,
    );
}

fn two_baselines() -> (NativeScene, [NodeId; 2], [ChannelBaseline; 2]) {
    let (mut scene, first) = test_fixtures::cube();
    let second = scene.create(Primitive::Cube, "second").unwrap();
    let baseline = |node| {
        scene
            .begin_move_translation(&target(&scene, node), WorldAxis::X)
            .unwrap()
            .1
    };
    let baselines = [baseline(first), baseline(second)];
    (scene, [first, second], baselines)
}

#[test]
fn preview_channels_many_writes_every_member_or_none() {
    let (mut scene, nodes, baselines) = two_baselines();
    let before = scene.clone();
    let items = || {
        baselines
            .iter()
            .zip([[1.0, 0.0, 0.0], [2.0, 0.0, f64::NAN]])
    };
    assert_eq!(
        scene.preview_channels_many(items()).err(),
        Some(NativeError::NonFinite)
    );
    assert_eq!(scene, before);
    let (previous, changed) = scene
        .preview_channels_many(baselines.iter().zip([[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]))
        .unwrap();
    assert!(changed);
    assert_eq!(previous.map_values(), [[0.0; 3]; 2]);
    assert_eq!(
        scene.node(nodes[0]).unwrap().transform().translation[0],
        1.0
    );
    assert_eq!(
        scene.node(nodes[1]).unwrap().transform().translation[0],
        2.0
    );
}

trait MapValues {
    fn map_values(&self) -> Vec<[f64; 3]>;
}

impl MapValues for Vec<ChannelBaseline> {
    fn map_values(&self) -> Vec<[f64; 3]> {
        self.iter().map(ChannelBaseline::values).collect()
    }
}

#[test]
fn channel_changes_many_omits_unchanged_members_and_restore_many_restores_all() {
    let (mut scene, nodes, baselines) = two_baselines();
    let before = scene.clone();
    scene
        .preview_channels_many(baselines.iter().zip([[3.0, 0.0, 0.0], [0.0; 3]]))
        .unwrap();
    let changes = scene.channel_changes_many(baselines.iter()).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, nodes[0]);
    assert_eq!(changes[0].1[0], Some(3.0));
    scene.restore_channels_many(baselines.iter()).unwrap();
    assert_eq!(scene, before);
}

#[test]
fn restore_channels_many_checks_every_owner_before_writing() {
    let (mut scene, nodes, baselines) = two_baselines();
    scene
        .preview_channels_many(baselines.iter().zip([[3.0, 0.0, 0.0], [4.0, 0.0, 0.0]]))
        .unwrap();
    scene.nodes.retain(|node| node.id != nodes[1]);
    let before = scene.clone();
    assert_eq!(
        scene.restore_channels_many(baselines.iter()).unwrap_err(),
        NativeError::InvalidIdentity
    );
    assert_eq!(scene, before);
}

#[test]
fn world_move_frames_reports_a_read_only_scene_once_and_unnamed() {
    let (mut scene, first) = test_fixtures::cube();
    let second = scene.create(Primitive::Cube, "second").unwrap();
    let targets = [target(&scene, first), target(&scene, second)];
    let refs: Vec<&TransformTarget> = targets.iter().collect();
    crate::native_scene::tests::retain(
        &mut scene,
        b"setAttr .tx +0.000;",
        std::iter::once(0..19).collect(),
    );
    scene.nodes[0].channels[0].origin = Some(ScalarOrigin::Ascii { literal: 12..18 });
    assert!(matches!(
        scene.world_move_frames(&refs, Some(WorldAxis::X)),
        Err(MoveBeginError::Scene(NativeError::ReadOnlyFormat))
    ));
}

#[test]
fn world_move_frames_names_the_index_of_the_refusing_target() {
    let (mut scene, pivoted) = test_fixtures::pivoted_shape();
    scene.nodes[0].restrictions.push("driver".into());
    let plain = scene.create(Primitive::Cube, "plain").unwrap();
    let targets = [target(&scene, plain), target(&scene, pivoted)];
    let refs: Vec<&TransformTarget> = targets.iter().collect();
    assert!(matches!(
        scene.world_move_frames(&refs, Some(WorldAxis::X)),
        Err(MoveBeginError::Member {
            index: 1,
            source: NativeError::UnsupportedEdit
        })
    ));
}

/// Baseline: N single frames validate N times; the batch builder validates once.
#[test]
fn world_move_frames_validates_once_for_the_whole_set() {
    let mut scene = NativeScene::default();
    for index in 0..8 {
        scene
            .create(Primitive::Cube, &format!("cube{index}"))
            .unwrap();
    }
    let targets = scene.transform_paths().unwrap();
    let refs: Vec<&TransformTarget> = targets.iter().collect();
    let validations = |action: &mut dyn FnMut()| {
        FULL_VALIDATIONS.with(|count| count.set(0));
        action();
        FULL_VALIDATIONS.with(|count| count.get())
    };
    let singles = validations(&mut || {
        for target in &targets {
            scene.world_move_frame(target).unwrap();
        }
    });
    assert_eq!(singles, 8);
    let batch = validations(&mut || {
        scene.world_move_frames(&refs, Some(WorldAxis::X)).unwrap();
    });
    assert_eq!(batch, 1);
}

fn world_image(scene: &NativeScene, selected: &TransformTarget, point: DVec3) -> DVec3 {
    test_fixtures::world_image(scene, selected, point)
}

fn assert_vec_near(actual: DVec3, expected: DVec3, context: &str) {
    assert!(
        (actual - expected).abs().max_element() <= 1e-9 * (1.0 + expected.abs().max_element()),
        "{context}: {actual} != {expected}"
    );
}

fn pivoted_move_scenes() -> Vec<(&'static str, NativeScene, NodeId)> {
    test_fixtures::pivoted_family([20.0, 35.0, -15.0], [30.0, -20.0, 50.0], true)
}

#[test]
fn a_pivoted_node_is_moved_by_exactly_the_requested_world_displacement() {
    for (name, scene, node) in pivoted_move_scenes() {
        let selected = target(&scene, node);
        let frame = scene.world_move_frame(&selected).unwrap();
        let local = scene.internal_transform(node).unwrap();
        let rp = DVec3::from_array(local.rotate_pivot_cm);
        let sp = DVec3::from_array(local.scale_pivot_cm);
        let parent = scene.ancestor_world(&selected).unwrap().unwrap();
        let rotate_pivot = parent.transform_point3(local.origin_cm() + rp);
        assert_vec_near(frame.origin_cm(), rotate_pivot, name);
        let probes = [rp, sp, DVec3::ZERO, DVec3::new(1.5, -2.0, 4.0)];
        let before: Vec<_> = probes
            .iter()
            .map(|point| world_image(&scene, &selected, *point))
            .collect();
        for (axis, direction) in [
            (WorldAxis::X, DVec3::X),
            (WorldAxis::Y, DVec3::Y),
            (WorldAxis::Z, DVec3::Z),
        ] {
            let translation = frame.translation(axis, 3.5).unwrap();
            let mut moved = scene.clone();
            let mut channels = [None; 9];
            for (slot, value) in channels.iter_mut().zip(translation) {
                *slot = Some(value);
            }
            moved.edit_node(node, None, channels).unwrap();
            for (point, before) in probes.iter().zip(&before) {
                assert_vec_near(
                    world_image(&moved, &selected, *point),
                    *before + direction * 3.5,
                    name,
                );
            }
            // #370 owns Move Object/Parent orientation: a move never turns the axes.
            assert_eq!(
                moved.manipulator_frame(&selected).unwrap().object_axes,
                scene.manipulator_frame(&selected).unwrap().object_axes,
                "{name}"
            );
            assert_vec_near(
                moved.world_move_frame(&selected).unwrap().origin_cm(),
                rotate_pivot + direction * 3.5,
                name,
            );
            let index = scene.nodes.iter().position(|n| n.id == node).unwrap();
            let (was, now) = (&scene.nodes[index], &moved.nodes[index]);
            for slot in 3..9 {
                assert_eq!(
                    was.channels[slot].value.to_bits(),
                    now.channels[slot].value.to_bits(),
                    "{name}"
                );
            }
            assert_eq!(was.rotate_pivot, now.rotate_pivot, "{name}");
            assert_eq!(was.scale_pivot, now.scale_pivot, "{name}");
            assert_eq!(was.extras, now.extras, "{name}");
        }
    }
}

#[test]
fn the_pivoted_frame_origin_is_the_rotate_pivot_not_the_translate_plus_pivot_translate() {
    let (scene, node) = test_fixtures::pivoted_extras_shape();
    // T [3,-2,5] + RT [0.5,-1.5,2] + RP [1,0.25,-2]
    let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
    near(frame.origin_cm().to_array(), [4.5, -3.25, 5.0]);
}

#[test]
fn a_pivot_only_shifts_the_origin_and_never_the_object_axes_or_other_scenes() {
    for (name, scene, node) in pivoted_move_scenes() {
        let selected = target(&scene, node);
        let mut zeroed = scene.clone();
        let index = zeroed.nodes.iter().position(|n| n.id == node).unwrap();
        zeroed.nodes[index].rotate_pivot = [0.0; 3].map(Scalar::new);
        zeroed.nodes[index].scale_pivot = [0.0; 3].map(Scalar::new);
        let with = scene.manipulator_frame(&selected).unwrap();
        let without = zeroed.manipulator_frame(&selected).unwrap();
        assert_eq!(with.object_axes, without.object_axes, "{name}");
        let local = scene.internal_transform(node).unwrap();
        let parent = scene.ancestor_world(&selected).unwrap().unwrap();
        let shift = DMat3::from_mat4(parent) * DVec3::from_array(local.rotate_pivot_cm);
        assert_vec_near(
            scene.world_move_frame(&selected).unwrap().origin_cm()
                - zeroed.world_move_frame(&selected).unwrap().origin_cm(),
            shift,
            name,
        );
    }
}

#[test]
fn large_and_signed_zero_pivots_are_accepted_and_other_refusals_still_win() {
    for value in [1e6, -0.0, 1e-300] {
        let (mut scene, node) = test_fixtures::cube();
        scene.nodes[0].rotate_pivot = [value; 3].map(Scalar::new);
        scene.nodes[0].scale_pivot = [value; 3].map(Scalar::new);
        let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        near(frame.origin_cm().to_array(), [value; 3]);
    }
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    let index = scene.nodes.iter().position(|n| n.id == child).unwrap();
    scene.nodes[index].rotate_pivot = [1e6; 3].map(Scalar::new);
    scene.set_channel(root, Channel::ScaleX, 0.0).unwrap();
    assert!(matches!(
        scene.world_move_frame(&target(&scene, child)),
        Err(NativeError::UnsupportedMove(
            WorldMoveUnsupported::ParentTransform
        ))
    ));
}

mod move_space {
    use super::*;

    fn posed(scene: &mut NativeScene, node: NodeId, rotation: [f64; 3]) {
        scene
            .set_transform(
                node,
                NativeTransform {
                    rotation,
                    ..NativeTransform::default()
                },
            )
            .unwrap();
    }

    // World displacement of a translation change, through the parent's linear map.
    fn world_delta(frame: &WorldMoveFrame, translation: [f64; 3]) -> DVec3 {
        frame.parent_linear
            * (DVec3::from_array(translation) - DVec3::from_array(frame.translation))
    }

    fn near_v(actual: DVec3, expected: DVec3) {
        near(actual.to_array(), expected.to_array());
    }

    #[test]
    fn world_moves_along_the_world_axes_whatever_the_rotations() {
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        posed(&mut scene, root, [30.0, -20.0, 50.0]);
        posed(&mut scene, child, [11.0, 19.0, -8.0]);
        let frame = scene.world_move_frame(&target(&scene, child)).unwrap();
        for (axis, expected) in [
            (WorldAxis::X, DVec3::X),
            (WorldAxis::Y, DVec3::Y),
            (WorldAxis::Z, DVec3::Z),
        ] {
            assert_eq!(frame.direction(MoveSpace::World, axis).unwrap(), expected);
            let moved = frame.translation_in(MoveSpace::World, axis, 4.25).unwrap();
            near_v(world_delta(&frame, moved), expected * 4.25);
        }
    }

    #[test]
    fn a_zero_length_or_non_finite_axis_is_refused() {
        let (mut scene, node) = test_fixtures::cube();
        posed(&mut scene, node, [0.0, 0.0, 90.0]);
        let good = scene.world_move_frame(&target(&scene, node)).unwrap();
        let broken = [
            DMat3::ZERO,
            DMat3::from_diagonal(DVec3::new(f64::NAN, 1.0, 1.0)),
            DMat3::from_diagonal(DVec3::new(f64::INFINITY, 1.0, 1.0)),
        ];
        for linear in broken {
            let mut frame = good;
            frame.parent_linear = linear;
            for space in [MoveSpace::Parent, MoveSpace::Object] {
                assert!(matches!(
                    frame.direction(space, WorldAxis::X),
                    Err(NativeError::UnsupportedMove(
                        WorldMoveUnsupported::ParentTransform
                    ))
                ));
            }
        }
        let mut frame = good;
        frame.rotation_axes = DMat3::ZERO;
        assert!(frame.direction(MoveSpace::Object, WorldAxis::X).is_err());
        assert!(
            frame
                .direction(MoveSpace::AlongRotationAxis, WorldAxis::X)
                .is_ok()
        );
        assert!(frame.direction(MoveSpace::Parent, WorldAxis::X).is_ok());
    }

    #[test]
    fn object_moves_along_the_nodes_own_rotated_axes() {
        let (mut scene, node) = test_fixtures::cube();
        posed(&mut scene, node, [0.0, 0.0, 90.0]);
        let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        let moved = frame
            .translation_in(MoveSpace::Object, WorldAxis::X, 5.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(0.0, 5.0, 0.0));
        let moved = frame
            .translation_in(MoveSpace::Object, WorldAxis::Y, 5.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(-5.0, 0.0, 0.0));
        let moved = frame
            .translation_in(MoveSpace::Object, WorldAxis::Z, 5.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(0.0, 0.0, 5.0));
    }

    #[test]
    fn object_follows_the_rotate_order_and_includes_the_rotate_axis() {
        let (mut scene, node) = test_fixtures::cube();
        let r = [23.0, -41.0, 67.0];
        posed(&mut scene, node, r);
        test_fixtures::set_extras(
            &mut scene,
            node,
            TransformExtras {
                rotate_order: Some(1),
                rotate_axis: Some([17.0, -33.0, 52.0]),
                ..TransformExtras::default()
            },
        );
        let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        let rad = r.map(f64::to_radians);
        // yzx order: Y is applied first, then Z, then X (rightmost factor first).
        let own = DMat3::from_rotation_x(rad[0])
            * DMat3::from_rotation_z(rad[2])
            * DMat3::from_rotation_y(rad[1]);
        let ra = DMat3::from_rotation_z((52.0f64).to_radians())
            * DMat3::from_rotation_y((-33.0f64).to_radians())
            * DMat3::from_rotation_x((17.0f64).to_radians());
        let own = own * ra;
        let moved = frame
            .translation_in(MoveSpace::Object, WorldAxis::Y, 3.0)
            .unwrap();
        near_v(world_delta(&frame, moved), own * DVec3::new(0.0, 3.0, 0.0));
    }

    fn with_extras(scene: &mut NativeScene, node: NodeId, order: u8, axis: [f64; 3]) {
        test_fixtures::set_extras(
            scene,
            node,
            TransformExtras {
                rotate_order: Some(order),
                rotate_axis: Some(axis),
                ..TransformExtras::default()
            },
        );
    }

    // Independent reference: R(ro) written from raw angles, with `ro` given as the
    // application sequence (first axis applied first, rightmost factor).
    fn reference_axes(sequence: [usize; 3], r: [f64; 3], ra: [f64; 3]) -> DMat3 {
        let turn = |axis: usize, degrees: f64| match axis {
            0 => DMat3::from_rotation_x(degrees.to_radians()),
            1 => DMat3::from_rotation_y(degrees.to_radians()),
            _ => DMat3::from_rotation_z(degrees.to_radians()),
        };
        let own = turn(sequence[2], r[sequence[2]])
            * turn(sequence[1], r[sequence[1]])
            * turn(sequence[0], r[sequence[0]]);
        // The Rotate Axis is deliberately not part of this frame (PROVISIONAL, see MoveSpace).
        let _ = ra;
        own
    }

    fn assert_along_rotation_axis(frame: &WorldMoveFrame, parent: DMat3, axes: DMat3) {
        for (axis, unit) in [
            (WorldAxis::X, DVec3::X),
            (WorldAxis::Y, DVec3::Y),
            (WorldAxis::Z, DVec3::Z),
        ] {
            let expected = (parent * (axes * unit)).normalize();
            near_v(
                frame.direction(MoveSpace::AlongRotationAxis, axis).unwrap(),
                expected,
            );
            let moved = frame
                .translation_in(MoveSpace::AlongRotationAxis, axis, 3.0)
                .unwrap();
            near_v(world_delta(frame, moved), expected * 3.0);
        }
    }

    #[test]
    fn along_rotation_axis_ignores_the_rotate_axis_unparented_with_a_non_default_order() {
        let (mut scene, node) = test_fixtures::cube();
        let (r, ra) = ([23.0, -41.0, 67.0], [17.0, -33.0, 52.0]);
        posed(&mut scene, node, r);
        with_extras(&mut scene, node, 1, ra);
        let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        assert_along_rotation_axis(&frame, DMat3::IDENTITY, reference_axes([1, 2, 0], r, ra));
        // The Rotate Axis is ignored: the same frame as with a zero Rotate Axis.
        with_extras(&mut scene, node, 1, [0.0; 3]);
        let zero = scene.world_move_frame(&target(&scene, node)).unwrap();
        for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
            assert_eq!(
                zero.direction(MoveSpace::AlongRotationAxis, axis).unwrap(),
                frame.direction(MoveSpace::AlongRotationAxis, axis).unwrap()
            );
        }
    }

    #[test]
    fn along_rotation_axis_is_parented_by_the_parents_map_and_normalised() {
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        scene
            .set_transform(
                root,
                NativeTransform {
                    rotation: [30.0, -20.0, 50.0],
                    scale: [2.0, 2.0, 2.0],
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        let (r, ra) = ([11.0, 19.0, -8.0], [0.0, 0.0, 90.0]);
        posed(&mut scene, child, r);
        with_extras(&mut scene, child, 3, ra);
        let frame = scene.world_move_frame(&target(&scene, child)).unwrap();
        let parent = DMat3::from_rotation_z(50.0f64.to_radians())
            * DMat3::from_rotation_y((-20.0f64).to_radians())
            * DMat3::from_rotation_x(30.0f64.to_radians())
            * DMat3::from_diagonal(DVec3::splat(2.0));
        assert_along_rotation_axis(&frame, parent, reference_axes([0, 2, 1], r, ra));
        // Normalised despite the parent's scale 2: a 3 cm move is 3 cm in the world.
        let moved = frame
            .translation_in(MoveSpace::AlongRotationAxis, WorldAxis::Y, 3.0)
            .unwrap();
        assert!((world_delta(&frame, moved).length() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn along_rotation_axis_equals_object_bit_for_bit_when_the_rotate_axis_is_zero() {
        for order in [None, Some(0u8), Some(1), Some(5)] {
            let (mut scene, node) = test_fixtures::cube();
            posed(&mut scene, node, [23.0, -41.0, 67.0]);
            if let Some(order) = order {
                with_extras(&mut scene, node, order, [0.0; 3]);
            }
            let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
            for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
                let object = frame.direction(MoveSpace::Object, axis).unwrap();
                let along = frame.direction(MoveSpace::AlongRotationAxis, axis).unwrap();
                assert_eq!(
                    object.to_array().map(f64::to_bits),
                    along.to_array().map(f64::to_bits),
                    "order {order:?}"
                );
            }
        }
    }

    #[test]
    fn along_rotation_axis_refuses_like_object() {
        let (mut scene, node) = test_fixtures::cube();
        posed(&mut scene, node, [0.0, 0.0, 90.0]);
        with_extras(&mut scene, node, 0, [0.0, 0.0, 90.0]);
        let good = scene.world_move_frame(&target(&scene, node)).unwrap();
        let mut frame = good;
        frame.object_rotation = DMat3::ZERO;
        assert!(
            frame
                .direction(MoveSpace::AlongRotationAxis, WorldAxis::X)
                .is_err()
        );
        assert!(frame.direction(MoveSpace::Parent, WorldAxis::X).is_ok());
        let mut frame = good;
        frame.parent_linear = DMat3::ZERO;
        assert!(matches!(
            frame.translation_in(MoveSpace::AlongRotationAxis, WorldAxis::X, 1.0),
            Err(NativeError::UnsupportedMove(
                WorldMoveUnsupported::ParentTransform
            ))
        ));
        assert!(matches!(
            good.translation_in(MoveSpace::AlongRotationAxis, WorldAxis::X, f64::NAN),
            Err(NativeError::NonFinite)
        ));
    }

    #[test]
    fn move_object_and_scale_object_handles_agree_when_the_rotate_axis_is_non_zero() {
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        scene
            .set_transform(
                root,
                NativeTransform {
                    rotation: [30.0, -20.0, 50.0],
                    scale: [2.0, 2.0, 2.0],
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        posed(&mut scene, child, [11.0, 19.0, -8.0]);
        with_extras(&mut scene, child, 4, [17.0, -33.0, 52.0]);
        let selected = target(&scene, child);
        let frame = scene.world_move_frame(&selected).unwrap();
        let handles = scene.manipulator_frame(&selected).unwrap().object_axes;
        for (axis, handle) in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z]
            .into_iter()
            .zip(handles)
        {
            near_v(
                frame.direction(MoveSpace::Object, axis).unwrap(),
                handle.normalize(),
            );
            let along = frame.direction(MoveSpace::AlongRotationAxis, axis).unwrap();
            assert!((along - handle.normalize()).length() > 0.05);
        }
    }

    #[test]
    fn object_composes_the_parents_rotation_with_the_nodes_own() {
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        posed(&mut scene, root, [90.0, 0.0, 0.0]);
        posed(&mut scene, child, [0.0, 0.0, 90.0]);
        let frame = scene.world_move_frame(&target(&scene, child)).unwrap();
        // The child's X is its parent's Y, which a 90 degree turn about world X sends to Z.
        let moved = frame
            .translation_in(MoveSpace::Object, WorldAxis::X, 2.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(0.0, 0.0, 2.0));
    }

    #[test]
    fn parent_uses_the_parents_rotation_not_the_nodes_own() {
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        posed(&mut scene, root, [0.0, 90.0, 0.0]);
        posed(&mut scene, child, [0.0, 0.0, 90.0]);
        let frame = scene.world_move_frame(&target(&scene, child)).unwrap();
        // Parent X is the world -Z (Ry(90) maps X to -Z); the child's own Z rotation is ignored.
        let moved = frame
            .translation_in(MoveSpace::Parent, WorldAxis::X, 2.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(0.0, 0.0, -2.0));
        // and the move is a pure child-local X translation
        near(
            [
                moved[1] - frame.translation[1],
                moved[2] - frame.translation[2],
                0.0,
            ],
            [0.0; 3],
        );
    }

    #[test]
    fn parent_of_a_root_is_the_world_and_unit_length_ignores_parent_scale() {
        let (mut scene, node) = test_fixtures::cube();
        posed(&mut scene, node, [10.0, 20.0, 30.0]);
        let frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        for axis in [WorldAxis::X, WorldAxis::Y, WorldAxis::Z] {
            assert_eq!(
                frame.translation_in(MoveSpace::Parent, axis, 1.5).unwrap(),
                frame.translation(axis, 1.5).unwrap()
            );
        }
        let (mut scene, root, child, _) = test_fixtures::grouped_shape();
        scene
            .set_transform(
                root,
                NativeTransform {
                    scale: [4.0, 1.0, 1.0],
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        let frame = scene.world_move_frame(&target(&scene, child)).unwrap();
        let moved = frame
            .translation_in(MoveSpace::Parent, WorldAxis::X, 2.0)
            .unwrap();
        near_v(world_delta(&frame, moved), DVec3::new(2.0, 0.0, 0.0));
    }

    #[test]
    fn several_objects_each_move_the_same_distance_along_their_own_axes() {
        let (mut scene, first) = test_fixtures::cube();
        let second = scene.create(Primitive::Cube, "second").unwrap();
        posed(&mut scene, first, [0.0, 0.0, 90.0]);
        posed(&mut scene, second, [0.0, 90.0, 0.0]);
        let targets = [target(&scene, first), target(&scene, second)];
        let frames = scene
            .world_move_frames(&[&targets[0], &targets[1]], None)
            .unwrap();
        let deltas: Vec<_> = frames
            .iter()
            .map(|(frame, _)| {
                let moved = frame
                    .translation_in(MoveSpace::Object, WorldAxis::X, 5.0)
                    .unwrap();
                world_delta(frame, moved)
            })
            .collect();
        near_v(deltas[0], DVec3::new(0.0, 5.0, 0.0));
        near_v(deltas[1], DVec3::new(0.0, 0.0, -5.0));
    }

    #[test]
    fn an_absent_leaf_refuses_the_space_that_needs_it() {
        let (mut scene, node) = test_fixtures::cube();
        posed(&mut scene, node, [0.0, 0.0, 90.0]);
        let mut frame = scene.world_move_frame(&target(&scene, node)).unwrap();
        frame.absent_leaf = [false, true, false];
        // Object X is world Y here, so it needs translateY.
        assert!(matches!(
            frame.require_leaves_in(MoveSpace::Object, WorldAxis::X),
            Err(NativeError::AbsentChannelLeaf { component: 1, .. })
        ));
        // A rotation about Z leaves translateZ exactly untouched in every space.
        frame.absent_leaf = [false, false, true];
        for axis in [WorldAxis::X, WorldAxis::Y] {
            assert!(frame.require_leaves_in(MoveSpace::Object, axis).is_ok());
        }
        assert!(
            frame
                .require_leaves_in(MoveSpace::World, WorldAxis::X)
                .is_ok()
        );
    }
}
