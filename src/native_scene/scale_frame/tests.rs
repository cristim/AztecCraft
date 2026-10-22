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

fn scaled(scale: [f64; 3]) -> (NativeScene, NodeId) {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                scale,
                ..NativeTransform::default()
            },
        )
        .unwrap();
    (scene, node)
}

fn frame(scene: &NativeScene, node: NodeId, handle: ScaleHandle) -> ScaleFrame {
    scene
        .begin_scale_frame(&target(scene, node), handle, ScaleSpace::Object)
        .unwrap()
        .0
}

fn bits(values: [f64; 3]) -> [u64; 3] {
    values.map(f64::to_bits)
}

fn refusal(error: ScaleUnsupported) -> NativeError {
    NativeError::UnsupportedScale(error)
}

mod world;

const HANDLES: [ScaleHandle; 4] = [
    ScaleHandle::X,
    ScaleHandle::Y,
    ScaleHandle::Z,
    ScaleHandle::Uniform,
];

#[test]
fn axis_handles_scale_one_channel_and_uniform_scales_all_three() {
    let (scene, node) = scaled([2.0, -3.0, 0.5]);
    let base = [2.0, -3.0, 0.5];
    for (handle, index) in [
        (ScaleHandle::X, 0),
        (ScaleHandle::Y, 1),
        (ScaleHandle::Z, 2),
    ] {
        let out = frame(&scene, node, handle).scale(handle, 1.5).unwrap();
        for component in 0..3 {
            let expected: f64 = if component == index {
                base[component] * 1.5
            } else {
                base[component]
            };
            assert_eq!(out[component].to_bits(), expected.to_bits());
        }
    }
    let out = frame(&scene, node, ScaleHandle::Uniform)
        .scale(ScaleHandle::Uniform, 4.0)
        .unwrap();
    assert_eq!(out, [8.0, -12.0, 2.0]);
}

#[test]
fn factor_one_is_bit_exact_for_every_handle_including_negative_zero() {
    let (scene, node) = scaled([0.1 + 0.2, -0.0, 1e-300]);
    for handle in HANDLES {
        let frame = frame(&scene, node, handle);
        assert_eq!(
            bits(frame.scale(handle, 1.0).unwrap()),
            bits([0.1 + 0.2, -0.0, 1e-300])
        );
    }
}

#[test]
fn out_and_back_is_bit_exact_through_the_scene() {
    let (mut scene, node) = scaled([0.3, 7.0, 1.0 / 3.0]);
    let before = scene.clone();
    for handle in HANDLES {
        let (frame, baseline) = scene
            .begin_scale_frame(&target(&scene, node), handle, ScaleSpace::Object)
            .unwrap();
        let (_, changed) = scene
            .preview_channels(&baseline, frame.scale(handle, 2.7).unwrap())
            .unwrap();
        assert!(changed);
        scene
            .preview_channels(&baseline, frame.scale(handle, 1.0).unwrap())
            .unwrap();
        assert_eq!(scene, before);
        assert_eq!(scene.channel_changes(&baseline).unwrap(), [None; 9]);
    }
}

#[test]
fn scale_is_independent_of_the_parent_scale() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                scale: [5.0, 0.25, -3.0],
                rotation: [30.0, 40.0, 50.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene
        .set_transform(
            child,
            NativeTransform {
                scale: [2.0, 3.0, 4.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let (frame, _) = scene
        .begin_scale_frame(&target(&scene, child), ScaleHandle::Y, ScaleSpace::Object)
        .unwrap();
    assert_eq!(frame.scale(ScaleHandle::Y, 2.0).unwrap(), [2.0, 6.0, 4.0]);
}

#[test]
fn invalid_huge_and_tiny_factors_are_refused() {
    let (scene, node) = scaled([1.0, 1.0, 1.0]);
    for handle in HANDLES {
        let frame = frame(&scene, node, handle);
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(matches!(
                frame.scale(handle, bad),
                Err(NativeError::NonFinite)
            ));
        }
        for bad in [0.0, -0.0, -1.0, -1e-300, f64::MIN] {
            assert_eq!(
                frame.scale(handle, bad),
                Err(refusal(ScaleUnsupported::InvalidFactor))
            );
        }
        assert_eq!(
            frame.scale(handle, f64::MAX),
            Ok(match handle {
                ScaleHandle::X => [f64::MAX, 1.0, 1.0],
                ScaleHandle::Y => [1.0, f64::MAX, 1.0],
                ScaleHandle::Z => [1.0, 1.0, f64::MAX],
                ScaleHandle::Uniform => [f64::MAX; 3],
            })
        );
        // Subnormal factors that keep the channel nonzero are representable.
        assert!(frame.scale(handle, f64::from_bits(1)).is_ok());
    }
}

#[test]
fn overflow_and_underflow_to_zero_are_refused() {
    let (scene, node) = scaled([1e300, 1e-300, 2.0]);
    let x = frame(&scene, node, ScaleHandle::X);
    assert_eq!(
        x.scale(ScaleHandle::X, 1e10),
        Err(refusal(ScaleUnsupported::Unrepresentable))
    );
    let y = frame(&scene, node, ScaleHandle::Y);
    assert_eq!(
        y.scale(ScaleHandle::Y, 1e-30),
        Err(refusal(ScaleUnsupported::Unrepresentable))
    );
    // Uniform fails if any one channel overflows.
    let u = frame(&scene, node, ScaleHandle::Uniform);
    assert_eq!(
        u.scale(ScaleHandle::Uniform, 1e10),
        Err(refusal(ScaleUnsupported::Unrepresentable))
    );
    // A zero baseline channel stays zero without being a refusal.
    let (zero, node) = scaled([0.0, 1.0, 1.0]);
    let out = frame(&zero, node, ScaleHandle::X)
        .scale(ScaleHandle::X, 3.0)
        .unwrap();
    assert_eq!(out[0].to_bits(), 0.0f64.to_bits());
}

fn retain_scale(scene: &mut NativeScene, node: NodeId) {
    let index = scene.nodes.iter().position(|n| n.id == node).unwrap();
    let owner = scene.nodes[index].transform_id;
    let values = scene.internal_transform(node).unwrap().scale;
    let bytes: Vec<_> = values.into_iter().flat_map(f64::to_be_bytes).collect();
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: bytes.into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..24).collect(),
        leaves: vec![SourceLeaf {
            payload: 0..24,
            numeric: 0..24,
            owner,
            field: TransformField::Scale,
            storage: Storage::Unitless,
        }],
        appearance: None,
    });
    for component in 0..3 {
        scene.nodes[index].channels[6 + component].origin =
            Some(ScalarOrigin::Binary { leaf: 0, component });
        scene.nodes[index].channels[6 + component].replaced = false;
    }
    scene.validate().unwrap();
}

#[test]
fn refusals_leave_the_scene_untouched() {
    let (scene, node) = test_fixtures::cube();
    let selected = target(&scene, node);
    let before = scene.clone();

    let mut pivot = scene.clone();
    pivot.nodes[0].scale_pivot[2] = Scalar::new(0.25);
    assert!(matches!(
        pivot.begin_scale_frame(&selected, ScaleHandle::X, ScaleSpace::Object),
        Err(NativeError::UnsupportedScale(
            ScaleUnsupported::SelectedPivot
        ))
    ));
    // A rotate pivot does not affect the scale channels.
    let mut rotate_pivot = scene.clone();
    rotate_pivot.nodes[0].rotate_pivot[2] = Scalar::new(0.25);
    assert!(
        rotate_pivot
            .begin_scale_frame(&selected, ScaleHandle::X, ScaleSpace::Object)
            .is_ok()
    );

    for effect in ["shear 0.5", "rotateAxis 1 0 0", "rotateOrder yzx"] {
        let mut effects = scene.clone();
        effects.nodes[0].unsupported_transform.push(effect.into());
        assert!(matches!(
            effects.begin_scale_frame(&selected, ScaleHandle::X, ScaleSpace::Object),
            Err(NativeError::UnsupportedEdit)
        ));
    }

    let mut ascii = scene.clone();
    ascii.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: [1].into(),
        format: SourceFormat::Ascii,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    assert!(matches!(
        ascii.begin_scale_frame(&selected, ScaleHandle::X, ScaleSpace::Object),
        Err(NativeError::ReadOnlyFormat)
    ));

    for path in [vec![], vec![selected.path[0], selected.path[0]]] {
        let bad = TransformTarget { node, path };
        assert!(
            scene
                .begin_scale_frame(&bad, ScaleHandle::X, ScaleSpace::Object)
                .is_err()
        );
    }
    assert_eq!(scene, before);
}

#[test]
fn leaf_rules_follow_the_handle() {
    let (scene, node) = scaled([2.0, 3.0, 4.0]);
    let base = frame(&scene, node, ScaleHandle::X);
    let only_y = ScaleFrame {
        absent_leaf: [true, false, true],
        ..base.clone()
    };
    assert!(only_y.require_leaves(ScaleHandle::Y).is_ok());
    for (handle, component) in [
        (ScaleHandle::X, 0),
        (ScaleHandle::Z, 2),
        (ScaleHandle::Uniform, 0),
    ] {
        assert!(matches!(
            only_y.require_leaves(handle),
            Err(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Scale,
                component: c
            }) if c == component
        ));
    }
    let no_z = ScaleFrame {
        absent_leaf: [false, false, true],
        ..base
    };
    assert!(no_z.require_leaves(ScaleHandle::X).is_ok());
    assert!(no_z.require_leaves(ScaleHandle::Uniform).is_err());
}

#[test]
fn binary_source_without_scale_leaves_is_refused_on_every_handle() {
    let (mut scene, node) = test_fixtures::cube();
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: [1].into(),
        format: SourceFormat::Binary,
        records: std::iter::once(0..1).collect(),
        leaves: vec![],
        appearance: None,
    });
    let selected = target(&scene, node);
    let before = scene.clone();
    for (handle, component) in [
        (ScaleHandle::X, 0),
        (ScaleHandle::Y, 1),
        (ScaleHandle::Z, 2),
        (ScaleHandle::Uniform, 0),
    ] {
        assert!(matches!(
            scene.begin_scale_frame(&selected, handle, ScaleSpace::Object),
            Err(NativeError::AbsentChannelLeaf {
                channel: ChannelLeaf::Scale,
                component: c
            }) if c == component
        ));
        assert_eq!(scene, before);
    }
}

#[test]
fn binary_source_with_scale_leaves_previews_and_restores_exactly() {
    let (mut scene, node) = scaled([2.0, 3.0, 4.0]);
    retain_scale(&mut scene, node);
    let before = scene.clone();
    for handle in HANDLES {
        let (frame, baseline) = scene
            .begin_scale_frame(&target(&scene, node), handle, ScaleSpace::Object)
            .unwrap();
        let (_, changed) = scene
            .preview_channels(&baseline, frame.scale(handle, 1.5).unwrap())
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
    for handle in HANDLES {
        scene
            .begin_scale_frame(&target(&scene, node), handle, ScaleSpace::Object)
            .unwrap();
    }
}
