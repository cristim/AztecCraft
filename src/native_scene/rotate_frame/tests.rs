use super::*;
use crate::native_scene::source::SourceLeaf;

fn target(scene: &NativeScene, node: NodeId) -> TransformTarget {
    scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.node == node)
        .unwrap()
}

fn rotated(rotation: [f64; 3]) -> (NativeScene, NodeId) {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation,
                ..NativeTransform::default()
            },
        )
        .unwrap();
    (scene, node)
}

fn frame(scene: &NativeScene, node: NodeId, axis: RotateAxis) -> RotateFrame {
    scene
        .begin_rotation(&target(scene, node), axis, RotateSpace::Object)
        .unwrap()
        .0
}

mod world;

fn matrix_deg(degrees: [f64; 3]) -> DMat3 {
    rotation_matrix(degrees.map(f64::to_radians))
}

fn assert_same_rotation(actual: DMat3, expected: DMat3, tolerance: f64) {
    let drift = (actual - expected).to_cols_array().map(f64::abs);
    let drift = drift.into_iter().fold(0.0, f64::max);
    assert!(drift <= tolerance, "rotation drift {drift}");
}

fn bits(values: [f64; 3]) -> [u64; 3] {
    values.map(f64::to_bits)
}

fn retain_rotation(scene: &mut NativeScene, node: NodeId, present: [bool; 3]) {
    let index = scene.nodes.iter().position(|n| n.id == node).unwrap();
    let owner = scene.nodes[index].transform_id;
    let values = scene.internal_transform(node).unwrap().rotation_radians;
    let bytes: Vec<_> = values.into_iter().flat_map(f64::to_be_bytes).collect();
    scene.source = Some(SourceRetention {
        bytes: bytes.into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..24).collect(),
        leaves: vec![SourceLeaf {
            payload: 0..24,
            numeric: 0..24,
            owner,
            field: TransformField::Rotation,
            storage: Storage::Radians,
        }],
        appearance: None,
    });
    for component in (0..3).filter(|c| present[*c]) {
        scene.nodes[index].channels[3 + component].origin =
            Some(ScalarOrigin::Binary { leaf: 0, component });
        scene.nodes[index].channels[3 + component].replaced = false;
    }
    scene.validate().unwrap();
}

#[test]
fn y_ring_from_rotate_y_120_takes_the_branch_nearest_the_baseline() {
    let (scene, node) = rotated([0.0, 120.0, 0.0]);
    let frame = frame(&scene, node, RotateAxis::Y);
    for degrees in [30.0, -45.0, 100.0, -200.0] {
        let out = frame.rotation(RotateAxis::Y, degrees).unwrap();
        assert_eq!(out[0].to_bits(), 0.0f64.to_bits());
        assert_eq!(out[2].to_bits(), 0.0f64.to_bits());
        // Representatives are taken nearest the baseline, so compare modulo whole turns.
        let turns = (out[1] - (120.0 + degrees)) / 360.0;
        assert!((turns - turns.round()).abs() < 1e-11, "{out:?}");
        assert!((out[1] - 120.0).abs() <= 180.0 + 1e-9, "{out:?}");
    }
}

#[test]
fn x_ring_from_rotate_x_720_changes_only_rotate_x() {
    let (scene, node) = rotated([720.0, 12.0, -7.0]);
    let frame = frame(&scene, node, RotateAxis::X);
    let out = frame.rotation(RotateAxis::X, 10.0).unwrap();
    assert!((out[0] - 730.0).abs() < 1e-9);
    assert_eq!(bits([out[1], out[2], 0.0]), bits([12.0, -7.0, 0.0]));
    let y = self::frame(&scene, node, RotateAxis::Y);
    let turned = y.rotation(RotateAxis::Y, 20.0).unwrap();
    assert_same_rotation(
        matrix_deg(turned),
        matrix_deg([720.0, 12.0, -7.0]) * DMat3::from_rotation_y(20f64.to_radians()),
        1e-12,
    );
    assert!(
        (turned[0] - 720.0).abs() < 90.0,
        "unwrapped toward the baseline turns: {turned:?}"
    );
}

#[test]
fn out_and_back_is_bit_exact_through_the_scene() {
    let (mut scene, node) = rotated([13.0, 120.0, -77.0]);
    let before = scene.clone();
    for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
        let (frame, baseline) = scene
            .begin_rotation(&target(&scene, node), axis, RotateSpace::Object)
            .unwrap();
        let out = frame.rotation(axis, 37.5).unwrap();
        assert_ne!(bits(out), bits(frame.declared));
        let (_, changed) = scene.preview_channels(&baseline, out).unwrap();
        assert!(changed);
        let back = frame.rotation(axis, 0.0).unwrap();
        scene.preview_channels(&baseline, back).unwrap();
        assert_eq!(scene, before);
        assert_eq!(scene.channel_changes(&baseline).unwrap(), [None; 9]);
    }
}

#[test]
fn seeded_property_composition_matches_the_matrix_product() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let mut checked = 0;
    for _ in 0..400 {
        let base = [
            next() * 1440.0 - 720.0,
            next() * 170.0 - 85.0,
            next() * 1440.0 - 720.0,
        ];
        let degrees = next() * 720.0 - 360.0;
        let (scene, node) = rotated(base);
        for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
            let frame = frame(&scene, node, axis);
            let step = match axis {
                RotateAxis::X => DMat3::from_rotation_x(degrees.to_radians()),
                RotateAxis::Y => DMat3::from_rotation_y(degrees.to_radians()),
                RotateAxis::Z => DMat3::from_rotation_z(degrees.to_radians()),
            };
            match frame.rotation(axis, degrees) {
                Ok(out) => {
                    assert_same_rotation(matrix_deg(out), matrix_deg(base) * step, 1e-11);
                    checked += 1;
                }
                Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal)) => {}
                Err(error) => panic!("{base:?} {axis:?} {degrees}: {error}"),
            }
        }
    }
    assert!(checked > 1000);
}

#[test]
fn gimbal_lock_is_refused() {
    let (scene, node) = rotated([0.0; 3]);
    let frame = frame(&scene, node, RotateAxis::Y);
    assert!(matches!(
        frame.rotation(RotateAxis::Y, 90.0),
        Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal))
    ));
    assert!(matches!(
        frame.rotation(RotateAxis::Y, 90.0 - 1e-9),
        Err(NativeError::UnsupportedRotate(RotateUnsupported::Gimbal))
    ));
    assert!(frame.rotation(RotateAxis::Y, 89.0).is_ok());
}

#[test]
fn non_finite_and_huge_angles_are_refused() {
    let (scene, node) = rotated([0.0; 3]);
    for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
        let frame = frame(&scene, node, axis);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                frame.rotation(axis, bad),
                Err(NativeError::NonFinite)
            ));
        }
        for huge in [1e308, -1e308, 1e9] {
            assert!(matches!(
                frame.rotation(axis, huge),
                Err(NativeError::UnsupportedRotate(
                    RotateUnsupported::NumericalAngle
                ))
            ));
        }
    }
}

#[test]
fn tiny_intended_angle_is_honored_and_unintended_noise_is_snapped() {
    let (scene, node) = rotated([0.0; 3]);
    for (axis, index) in [(RotateAxis::X, 0), (RotateAxis::Y, 1), (RotateAxis::Z, 2)] {
        let frame = frame(&scene, node, axis);
        let out = frame.rotation(axis, 1e-11).unwrap();
        assert!((out[index] - 1e-11).abs() < 1e-23, "{axis:?} {out:?}");
        for other in (0..3).filter(|i| *i != index) {
            assert_eq!(out[other].to_bits(), 0.0f64.to_bits());
        }
    }
    let (scene, node) = rotated([0.0, 0.0, -0.0]);
    let out = frame(&scene, node, RotateAxis::Y)
        .rotation(RotateAxis::Y, 10.0)
        .unwrap();
    assert_eq!(out[2].to_bits(), (-0.0f64).to_bits());
}

#[test]
fn angular_unit_radians_stores_radian_channels() {
    let (mut scene, node) = test_fixtures::cube();
    scene.units.angular = AngularUnit::Radian;
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 1.0, 0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let out = frame(&scene, node, RotateAxis::Y)
        .rotation(RotateAxis::Y, 90.0 - 45.0)
        .unwrap();
    assert!((out[1] - (1.0 + 45f64.to_radians())).abs() < 1e-12);
    scene.units.angular = AngularUnit::Unsupported("grad".into());
    assert!(
        scene
            .begin_rotation(&target(&scene, node), RotateAxis::Y, RotateSpace::Object)
            .is_err()
    );
}

#[test]
fn refusals_leave_the_scene_untouched() {
    let (scene, node) = test_fixtures::cube();
    let selected = target(&scene, node);
    let before = scene.clone();

    let mut pivot = scene.clone();
    pivot.nodes[0].rotate_pivot[1] = Scalar::new(0.25);
    assert!(matches!(
        pivot.begin_rotation(&selected, RotateAxis::X, RotateSpace::Object),
        Err(NativeError::UnsupportedRotate(
            RotateUnsupported::SelectedPivot
        ))
    ));

    for effect in [
        "rotateOrder yzx",
        "rotateAxis 1 0 0",
        "shear 0.5",
        "jointOrient 5",
    ] {
        let mut effects = scene.clone();
        effects.nodes[0].unsupported_transform.push(effect.into());
        assert!(matches!(
            effects.begin_rotation(&selected, RotateAxis::X, RotateSpace::Object),
            Err(NativeError::UnsupportedEdit)
        ));
    }

    let mut ascii = scene.clone();
    ascii.source = Some(SourceRetention {
        bytes: [1].into(),
        format: SourceFormat::Ascii,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    assert!(matches!(
        ascii.begin_rotation(&selected, RotateAxis::X, RotateSpace::Object),
        Err(NativeError::ReadOnlyFormat)
    ));

    for path in [vec![], vec![selected.path[0], selected.path[0]]] {
        let bad = TransformTarget { node, path };
        assert!(
            scene
                .begin_rotation(&bad, RotateAxis::X, RotateSpace::Object)
                .is_err()
        );
    }
    assert_eq!(scene, before);
}

#[test]
fn leaf_rules_follow_the_ring() {
    let (scene, node) = rotated([10.0, 20.0, 30.0]);
    let base = frame(&scene, node, RotateAxis::X);
    let only_x = RotateFrame {
        absent_leaf: [false, true, true],
        ..base.clone()
    };
    assert!(only_x.require_leaves(RotateAxis::X).is_ok());
    for axis in [RotateAxis::Y, RotateAxis::Z] {
        assert!(matches!(
            only_x.require_leaves(axis),
            Err(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Rotate,
                component: 1
            })
        ));
    }
    let no_x = RotateFrame {
        absent_leaf: [true, false, false],
        ..base.clone()
    };
    assert!(matches!(
        no_x.require_leaves(RotateAxis::X),
        Err(NativeError::AbsentChannelLeaf { component: 0, .. })
    ));
    assert!(base.require_leaves(RotateAxis::Z).is_ok());
}

#[test]
fn binary_source_without_rotate_leaves_is_refused_on_every_ring() {
    let (mut scene, node) = test_fixtures::cube();
    scene.source = Some(SourceRetention {
        bytes: [1].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    let selected = target(&scene, node);
    let before = scene.clone();
    for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
        assert!(matches!(
            scene.begin_rotation(&selected, axis, RotateSpace::Object),
            Err(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Rotate,
                component: 0
            })
        ));
        assert_eq!(scene, before);
    }
}

#[test]
fn binary_source_with_rotate_leaves_previews_and_restores_exactly() {
    // Radian units keep declared and stored values bit-identical so the retained leaf validates.
    let (mut scene, node) = test_fixtures::cube();
    scene.units.angular = AngularUnit::Radian;
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.25, 0.5, 0.75],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    retain_rotation(&mut scene, node, [true; 3]);
    let before = scene.clone();
    for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
        let (frame, baseline) = scene
            .begin_rotation(&target(&scene, node), axis, RotateSpace::Object)
            .unwrap();
        let (_, changed) = scene
            .preview_channels(&baseline, frame.rotation(axis, 15.0).unwrap())
            .unwrap();
        assert!(changed);
        assert!(
            scene
                .channel_changes(&baseline)
                .unwrap()
                .iter()
                .any(Option::is_some)
        );
        scene.restore_channels(&baseline).unwrap();
        assert_eq!(scene, before);
    }
}

#[test]
fn source_free_scene_with_default_leaves_is_not_refused() {
    let (scene, node) = test_fixtures::cube();
    assert!(scene.source.is_none());
    for axis in [RotateAxis::X, RotateAxis::Y, RotateAxis::Z] {
        scene
            .begin_rotation(&target(&scene, node), axis, RotateSpace::Object)
            .unwrap();
    }
}

// Near gimbal lock (cos y about 1e-3) the Euler decomposition no longer reproduces the target
// matrix to ROUNDTRIP_EPSILON, though the pitch is far from the 1e-6 gimbal bound. Found by a
// sweep over pitches 80 to 90 degrees that refused 0.2% of the Y and Z rings; this input pins the check.
#[test]
fn near_gimbal_decomposition_drift_is_refused() {
    let (scene, node) = rotated([-5.0, 88.9499105, -53.0]);
    let frame = frame(&scene, node, RotateAxis::Y);
    assert!(matches!(
        frame.rotation(RotateAxis::Y, 0.9),
        Err(NativeError::UnsupportedRotate(
            RotateUnsupported::NumericalAngle
        ))
    ));
    // The same baseline turns fine by a step that keeps the decomposition accurate.
    assert!(frame.rotation(RotateAxis::Y, 0.3).is_ok());
}

// An angle whose radian -> degree -> radian round trip is not the identity, so the old path
// stores a value one ulp off `base + angle`.
fn inexact_radian_angle(base: f64) -> (f64, f64) {
    let factor = 180.0 / PI;
    (1..100_000)
        .map(|i| f64::from(i) / 1000.0)
        .find_map(|angle| {
            let degrees = angle * factor;
            (base + degrees.to_radians() != base + angle).then_some((angle, degrees))
        })
        .expect("some radian angle fails the degree round trip")
}

#[test]
fn radian_scene_single_channel_rotate_stores_baseline_plus_requested_exactly() {
    // Y and Z rings change one channel only when the channels they mix with are zero.
    for (axis, index, baseline) in [
        (RotateAxis::X, 0, [0.25, 0.5, 0.75]),
        (RotateAxis::Y, 1, [0.0, 0.5, 0.0]),
        (RotateAxis::Z, 2, [0.0, 0.0, 0.75]),
    ] {
        let (mut scene, node) = test_fixtures::cube();
        scene.units.angular = AngularUnit::Radian;
        scene
            .set_transform(
                node,
                NativeTransform {
                    rotation: baseline,
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        let frame = frame(&scene, node, axis);
        let (angle, degrees) = inexact_radian_angle(baseline[index]);
        for requested in [angle, -angle] {
            let out = frame
                .rotation_declared(axis, requested * (180.0 / PI), Some(requested))
                .unwrap();
            assert_eq!(
                out[index].to_bits(),
                (baseline[index] + requested).to_bits(),
                "{axis:?} {requested}"
            );
            for other in (0..3).filter(|i| *i != index) {
                assert_eq!(out[other].to_bits(), baseline[other].to_bits());
            }
        }
        // Degrees alone (the old path) cannot reproduce it.
        let old = frame.rotation(axis, degrees).unwrap();
        assert_ne!(old[index].to_bits(), (baseline[index] + angle).to_bits());
    }
}
