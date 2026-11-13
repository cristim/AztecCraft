use super::*;
use glam::{DMat3, DQuat, DVec3, EulerRot};

const HANDLES: [ScaleHandle; 3] = [ScaleHandle::X, ScaleHandle::Y, ScaleHandle::Z];

fn world_frame(scene: &NativeScene, node: NodeId, handle: ScaleHandle) -> ScaleFrame {
    scene
        .begin_scale_frame(&target(scene, node), handle, ScaleSpace::World)
        .unwrap()
        .0
}

fn begin_world(
    scene: &NativeScene,
    node: NodeId,
    handle: ScaleHandle,
) -> Result<(ScaleFrame, ChannelBaseline), NativeError> {
    scene.begin_scale_frame(&target(scene, node), handle, ScaleSpace::World)
}

fn quat(degrees: [f64; 3]) -> DQuat {
    let [x, y, z] = degrees.map(f64::to_radians);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

// Maya rotate channels (degrees) of a quaternion, as the xyz order Rz * Ry * Rx.
fn degrees_of(q: DQuat) -> [f64; 3] {
    let (z, y, x) = q.to_euler(EulerRot::ZYX);
    [x, y, z].map(f64::to_degrees)
}

fn axis_of(handle: ScaleHandle) -> DVec3 {
    match handle {
        ScaleHandle::X => DVec3::X,
        ScaleHandle::Y => DVec3::Y,
        _ => DVec3::Z,
    }
}

// Independent reference: scale the node's world linear map P * R * S by D along a world axis (or
// uniformly) and compare with P * R * S' built from the stored channels.
fn assert_world_scale(
    parents: DMat3,
    rotation: [f64; 3],
    before: [f64; 3],
    after: [f64; 3],
    handle: ScaleHandle,
    factor: f64,
) {
    let node = DMat3::from_quat(quat(rotation));
    let old = parents * node * DMat3::from_diagonal(DVec3::from_array(before));
    let new = parents * node * DMat3::from_diagonal(DVec3::from_array(after));
    let step = if handle == ScaleHandle::Uniform {
        DMat3::from_diagonal(DVec3::splat(factor))
    } else {
        let axis = axis_of(handle);
        DMat3::IDENTITY + outer(axis) * (factor - 1.0)
    };
    let expected = step * old;
    let scale = old
        .to_cols_array()
        .into_iter()
        .fold(1.0, |a, v| f64::max(a, v.abs()));
    let drift = (new - expected).to_cols_array().map(f64::abs);
    let drift = drift.into_iter().fold(0.0, f64::max);
    assert!(
        drift <= 1e-11 * scale * factor.max(1.0),
        "{handle:?} {factor}: drift {drift}"
    );
}

fn outer(axis: DVec3) -> DMat3 {
    DMat3::from_cols(axis * axis.x, axis * axis.y, axis * axis.z)
}

#[test]
fn seeded_sweep_matches_an_independent_world_linear_map() {
    let mut state = 0x1234_5678_9abc_def1_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 11) as f64) / ((1u64 << 53) as f64)
    };
    let mut checked = 0;
    for round in 0..300 {
        let parent_rotation = [
            next() * 720.0 - 360.0,
            next() * 170.0 - 85.0,
            next() * 720.0 - 360.0,
        ];
        let uniform = 0.1 + next() * 9.9;
        // The node rotation cancels the parent's and adds a random quarter-turn permutation, so
        // every world axis is a local axis of the node (within the tolerance).
        let quarter = quat([
            90.0 * (next() * 4.0).floor(),
            90.0 * (next() * 4.0).floor(),
            90.0 * (next() * 4.0).floor(),
        ]);
        let (parent_q, rotation) = if round % 5 == 0 {
            (DQuat::IDENTITY, degrees_of(quarter))
        } else {
            let parent_q = quat(parent_rotation);
            (parent_q, degrees_of(parent_q.inverse() * quarter))
        };
        let signs = [1.0, if round % 3 == 0 { -1.0 } else { 1.0 }, 1.0];
        let baseline = [
            signs[0] * (0.1 + next() * 4.0),
            signs[1] * (0.1 + next() * 4.0),
            signs[2] * (0.1 + next() * 4.0),
        ];
        let (mut scene, root, node, _) = test_fixtures::grouped_shape();
        let (node, parents) = if round % 5 == 0 {
            (root, DMat3::IDENTITY)
        } else {
            scene
                .set_transform(
                    root,
                    NativeTransform {
                        translation: [next() * 10.0, 1.0, 2.0],
                        rotation: parent_rotation,
                        scale: [uniform; 3],
                    },
                )
                .unwrap();
            (node, DMat3::from_quat(parent_q) * uniform)
        };
        scene
            .set_transform(
                node,
                NativeTransform {
                    rotation,
                    scale: baseline,
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        for handle in [
            ScaleHandle::X,
            ScaleHandle::Y,
            ScaleHandle::Z,
            ScaleHandle::Uniform,
        ] {
            let factor = 0.05 + next() * 6.0;
            let out = world_frame(&scene, node, handle)
                .scale(handle, factor)
                .unwrap();
            assert_world_scale(parents, rotation, baseline, out, handle, factor);
            checked += 1;
        }
    }
    assert_eq!(checked, 1200);
}

#[test]
fn world_axes_map_to_the_matching_local_channel_after_a_quarter_turn() {
    // Rotated 90 degrees about Z, world X is local Y and world Y is local X.
    let (scene, node) = test_fixtures::cube();
    let mut scene = scene;
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                scale: [2.0, 3.0, 5.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    for (handle, expected) in [
        (ScaleHandle::X, [2.0, 6.0, 5.0]),
        (ScaleHandle::Y, [4.0, 3.0, 5.0]),
        (ScaleHandle::Z, [2.0, 3.0, 10.0]),
    ] {
        let out = world_frame(&scene, node, handle)
            .scale(handle, 2.0)
            .unwrap();
        for component in 0..3 {
            assert!(
                (out[component] - expected[component]).abs() < 1e-12,
                "{handle:?} {out:?}"
            );
        }
    }
}

#[test]
fn three_level_chain_uses_the_product_of_both_ancestors() {
    let root = NativeTransform {
        translation: [4.0, -2.0, 1.0],
        rotation: [35.0, -50.0, 80.0],
        scale: [2.0; 3],
    };
    let middle = NativeTransform {
        translation: [0.5, 3.0, -7.0],
        rotation: [-110.0, 25.0, 40.0],
        scale: [0.5; 3],
    };
    let parent_q = quat(root.rotation) * quat(middle.rotation);
    let rotation = degrees_of(parent_q.inverse() * quat([90.0, 0.0, 180.0]));
    let leaf = NativeTransform {
        rotation,
        scale: [1.5, 2.5, 3.5],
        ..NativeTransform::default()
    };
    let (scene, node) = test_fixtures::three_level_chain(root, middle, leaf);
    assert_eq!(scene.transform_paths().unwrap().len(), 3);
    let parents = DMat3::from_quat(parent_q);
    for handle in HANDLES.into_iter().chain([ScaleHandle::Uniform]) {
        let out = world_frame(&scene, node, handle)
            .scale(handle, 1.75)
            .unwrap();
        assert_world_scale(parents, rotation, leaf.scale, out, handle, 1.75);
    }
    let (scene, node) = test_fixtures::three_level_chain(
        root,
        NativeTransform {
            scale: [0.5, 0.5, 0.6],
            ..middle
        },
        leaf,
    );
    assert!(matches!(
        begin_world(&scene, node, ScaleHandle::X),
        Err(NativeError::UnsupportedScale(
            ScaleUnsupported::ParentTransform
        ))
    ));
}

#[test]
fn oblique_rotation_is_refused_per_axis_without_mutation() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 0.0, 30.0],
                scale: [2.0, 3.0, 5.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let before = scene.clone();
    // Spinning about Z leaves world Z aligned; X and Y would shear.
    for handle in [ScaleHandle::X, ScaleHandle::Y] {
        assert!(matches!(
            begin_world(&scene, node, handle),
            Err(NativeError::UnsupportedScale(
                ScaleUnsupported::NotAxisAligned
            ))
        ));
        let object = scene
            .begin_scale_frame(&target(&scene, node), handle, ScaleSpace::Object)
            .unwrap()
            .0;
        assert!(object.scale(handle, 2.0).is_ok());
    }
    assert!(begin_world(&scene, node, ScaleHandle::Z).is_ok());
    // The center box multiplies every channel, whatever the rotation.
    let (uniform, _) = begin_world(&scene, node, ScaleHandle::Uniform).unwrap();
    assert_eq!(
        bits(uniform.scale(ScaleHandle::Uniform, 1.5).unwrap()),
        bits([3.0, 4.5, 7.5])
    );
    assert_eq!(scene, before);
}

#[test]
fn alignment_tolerance_is_one_nanoradian_whatever_the_parent_scale() {
    // The parent's uniform scale must not change what counts as aligned.
    for parent_scale in [1.0, 1e-3, 1e3] {
        for (tilt, aligned) in [(5e-10, true), (2e-9, false)] {
            let (mut scene, root, node, _) = test_fixtures::grouped_shape();
            scene
                .set_transform(
                    root,
                    NativeTransform {
                        scale: [parent_scale; 3],
                        ..NativeTransform::default()
                    },
                )
                .unwrap();
            scene
                .set_transform(
                    node,
                    NativeTransform {
                        rotation: [0.0, 0.0, f64::to_degrees(tilt)],
                        ..NativeTransform::default()
                    },
                )
                .unwrap();
            // Tilt about Z mixes world X into local Y by about `tilt`.
            assert_eq!(
                begin_world(&scene, node, ScaleHandle::X).is_ok(),
                aligned,
                "{parent_scale} {tilt}"
            );
        }
    }
}

#[test]
fn non_uniform_parent_pivot_and_factor_refusals_leave_the_scene_unchanged() {
    let (mut scene, root, child, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                scale: [2.0, 3.0, 4.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let before = scene.clone();
    for handle in [ScaleHandle::X, ScaleHandle::Uniform] {
        assert!(matches!(
            begin_world(&scene, child, handle),
            Err(NativeError::UnsupportedScale(
                ScaleUnsupported::ParentTransform
            ))
        ));
    }
    assert_eq!(scene, before);

    let (cube, node) = test_fixtures::cube();
    let frame = world_frame(&cube, node, ScaleHandle::X);
    for bad in [0.0, -1.0, -0.0] {
        assert!(matches!(
            frame.scale(ScaleHandle::X, bad),
            Err(NativeError::UnsupportedScale(
                ScaleUnsupported::InvalidFactor
            ))
        ));
    }
    for bad in [f64::NAN, f64::INFINITY] {
        assert!(matches!(
            frame.scale(ScaleHandle::X, bad),
            Err(NativeError::NonFinite)
        ));
    }
}

#[test]
fn factor_one_and_out_and_back_are_bit_exact_in_world_space() {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    scene
        .set_transform(
            root,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                scale: [3.0; 3],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 0.0, -90.0],
                scale: [0.3, 7.0, 1.0 / 3.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let before = scene.clone();
    for handle in [
        ScaleHandle::X,
        ScaleHandle::Y,
        ScaleHandle::Z,
        ScaleHandle::Uniform,
    ] {
        let (frame, baseline) = begin_world(&scene, node, handle).unwrap();
        assert_eq!(
            bits(frame.scale(handle, 1.0).unwrap()),
            bits([0.3, 7.0, 1.0 / 3.0])
        );
        let (_, changed) = scene
            .preview_channels(&baseline, frame.scale(handle, 2.7).unwrap())
            .unwrap();
        assert!(changed);
        scene
            .preview_channels(&baseline, frame.scale(handle, 1.0).unwrap())
            .unwrap();
        assert_eq!(scene, before);
    }
}

#[test]
fn world_axis_leaf_need_follows_the_mapped_channel() {
    // Rotated a quarter turn about Z: world X edits local Y, world Y edits local X.
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let base = world_frame(&scene, node, ScaleHandle::X);
    let only_y = ScaleFrame {
        absent_leaf: [true, false, true],
        ..base
    };
    assert!(only_y.require_leaves(ScaleHandle::X).is_ok());
    for (handle, component) in [
        (ScaleHandle::Y, 0),
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
}

fn with_extras(scene: &mut NativeScene, node: NodeId, extras: TransformExtras) {
    test_fixtures::set_extras(scene, node, extras);
}

#[test]
fn rotate_order_yzx_decides_which_local_channel_a_world_axis_edits() {
    // Rx(90) * Rz(90) (yzx) sends local X, Y, Z to world Z, -X, -Y; the xyz order sends them to
    // world Y, Z, X, so a world X drag edits local Y (channel 1) here and local Z under xyz.
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [90.0, 0.0, 90.0],
                scale: [1.0, 1.0, 1.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let plain = world_frame(&scene, node, ScaleHandle::X);
    assert_eq!(plain.scale(ScaleHandle::X, 2.0).unwrap(), [1.0, 1.0, 2.0]);
    with_extras(
        &mut scene,
        node,
        TransformExtras {
            rotate_order: Some(1),
            ..TransformExtras::default()
        },
    );
    let yzx = world_frame(&scene, node, ScaleHandle::X);
    assert_eq!(yzx.scale(ScaleHandle::X, 2.0).unwrap(), [1.0, 2.0, 1.0]);
    let yzx = world_frame(&scene, node, ScaleHandle::Y);
    assert_eq!(yzx.scale(ScaleHandle::Y, 2.0).unwrap(), [1.0, 1.0, 2.0]);
    let yzx = world_frame(&scene, node, ScaleHandle::Z);
    assert_eq!(yzx.scale(ScaleHandle::Z, 2.0).unwrap(), [2.0, 1.0, 1.0]);
}

#[test]
fn rotate_pivot_translate_leaves_world_scale_channels_unchanged() {
    // RT is a translation left of R, so it cannot change which channel a world drag edits.
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    let parent = NativeTransform {
        rotation: [0.0, 0.0, 90.0],
        scale: [2.0, 2.0, 2.0],
        ..NativeTransform::default()
    };
    scene.set_transform(root, parent).unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                rotation: [0.0, 0.0, 90.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let run = |scene: &NativeScene| {
        world_frame(scene, node, ScaleHandle::X)
            .scale(ScaleHandle::X, 1.5)
            .unwrap()
    };
    // Parent Rz90 then node Rz90 send local X to world -X, so world X edits channel 0.
    assert_eq!(run(&scene), [1.5, 1.0, 1.0]);
    with_extras(
        &mut scene,
        node,
        TransformExtras {
            rotate_pivot_translate: Some([4.0, -5.0, 6.0]),
            ..TransformExtras::default()
        },
    );
    assert_eq!(run(&scene), [1.5, 1.0, 1.0]);
}

struct Spec {
    t: DVec3,
    r: [f64; 3],
    s: DVec3,
    rp: DVec3,
    ro: u8,
    ra: [f64; 3],
    rpt: DVec3,
    spt: DVec3,
}

fn rot(axis: usize, deg: f64, p: DVec3) -> DVec3 {
    let a = deg.to_radians();
    match axis {
        0 => DMat3::from_rotation_x(a) * p,
        1 => DMat3::from_rotation_y(a) * p,
        _ => DMat3::from_rotation_z(a) * p,
    }
}

// Maya row-vector chain, applied to a point left to right: SP^-1 S SP ST RP^-1 RA R RP RT T (sp = 0).
fn node_point(sp: &Spec, p: DVec3) -> DVec3 {
    let mut p = p * sp.s + sp.spt - sp.rp;
    for axis in 0..3 {
        p = rot(axis, sp.ra[axis], p);
    }
    let order: [usize; 3] = match sp.ro {
        0 => [0, 1, 2],
        1 => [1, 2, 0],
        2 => [2, 0, 1],
        3 => [0, 2, 1],
        4 => [1, 0, 2],
        _ => [2, 1, 0],
    };
    for axis in order {
        p = rot(axis, sp.r[axis], p);
    }
    p + sp.rp + sp.rpt + sp.t
}

fn install(scene: &mut NativeScene, node: NodeId, sp: &Spec) {
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: sp.t.to_array(),
                rotation: sp.r,
                scale: sp.s.to_array(),
            },
        )
        .unwrap();
    let entry = scene.nodes.iter_mut().find(|n| n.id == node).unwrap();
    entry.rotate_pivot = sp.rp.to_array().map(Scalar::new);
    test_fixtures::set_extras(
        scene,
        node,
        TransformExtras {
            rotate_order: Some(sp.ro),
            rotate_axis: Some(sp.ra),
            rotate_pivot_translate: Some(sp.rpt.to_array()),
            scale_pivot_translate: Some(sp.spt.to_array()),
            ..TransformExtras::default()
        },
    );
}

// Scale pivot is zero, so the fixed point is the world image of the origin. Independent of
// `matrix()`: points go through the Maya row-vector chain one factor at a time.
#[test]
fn world_scale_with_rotate_axis_and_non_xyz_order_scales_about_the_fixed_point() {
    let turns = [[90.0, 0.0, 90.0], [180.0, 90.0, 270.0]];
    let axes = [[90.0, 0.0, 0.0], [90.0, 90.0, 0.0], [0.0, 90.0, 90.0]];
    let parent = Spec {
        t: DVec3::new(5.0, -3.0, 2.0),
        r: [0.0, 0.0, 90.0],
        s: DVec3::splat(2.0),
        rp: DVec3::new(1.0, 2.0, -1.0),
        ro: 2,
        ra: [0.0; 3],
        rpt: DVec3::new(0.5, 0.25, 4.0),
        spt: DVec3::new(-2.0, 1.0, 0.5),
    };
    let mut checked = 0;
    for ro in 0..6u8 {
        for r in turns {
            for ra in axes {
                let child = Spec {
                    t: DVec3::new(1.0, 2.0, 3.0),
                    r,
                    s: DVec3::new(2.0, 3.0, 4.0),
                    rp: DVec3::new(1.0, -2.0, 3.0),
                    ro,
                    ra,
                    rpt: DVec3::new(0.7, -0.3, 0.9),
                    spt: DVec3::new(0.5, -1.0, 2.0),
                };
                let (mut scene, root, node, _) = test_fixtures::grouped_shape();
                install(&mut scene, root, &parent);
                install(&mut scene, node, &child);
                let selected = target(&scene, node);
                let world_old = |p: DVec3| node_point(&parent, node_point(&child, p));
                let fixed = world_old(DVec3::ZERO);
                let frame = scene.manipulator_frame(&selected).unwrap();
                assert!((frame.scale_origin_cm - fixed).length() < 1e-9, "ro={ro}");
                for (handle, axis) in [
                    (ScaleHandle::X, DVec3::X),
                    (ScaleHandle::Y, DVec3::Y),
                    (ScaleHandle::Z, DVec3::Z),
                ] {
                    let Ok((scale_frame, _)) =
                        scene.begin_scale_frame(&selected, handle, ScaleSpace::World)
                    else {
                        continue;
                    };
                    let factor = 1.7;
                    let scaled = scale_frame.scale(handle, factor).unwrap();
                    let moved = Spec {
                        s: DVec3::from_array(scaled),
                        ..child_clone(&child)
                    };
                    let world_new = |p: DVec3| node_point(&parent, node_point(&moved, p));
                    for p in [
                        DVec3::new(1.0, 0.0, 0.0),
                        DVec3::new(0.3, -2.0, 1.1),
                        DVec3::new(-1.0, 1.0, 5.0),
                    ] {
                        let old = world_old(p) - fixed;
                        let want = old + axis * axis.dot(old) * (factor - 1.0) + fixed;
                        let got = world_new(p);
                        assert!(
                            (got - want).length() < 1e-8,
                            "ro={ro} r={r:?} ra={ra:?} {handle:?}: {got:?} != {want:?}"
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 20, "{checked} cases reached an aligned edit");
}

fn child_clone(c: &Spec) -> Spec {
    Spec {
        t: c.t,
        r: c.r,
        s: c.s,
        rp: c.rp,
        ro: c.ro,
        ra: c.ra,
        rpt: c.rpt,
        spt: c.spt,
    }
}

// Parent space scales along the axes the node's local matrix lives in. Reference: the local linear
// map of the Maya row-vector chain, compared with the same chain built from the new channels.
#[test]
fn parent_space_follows_a_non_default_rotate_order_and_rotate_axis() {
    for (ro, r, ra) in [
        (1u8, [90.0, 0.0, 90.0], [0.0, 90.0, 0.0]),
        (2, [0.0, 90.0, 90.0], [90.0, 0.0, 90.0]),
        (5, [90.0, 90.0, 0.0], [0.0, 0.0, 90.0]),
    ] {
        let before = [2.0, 3.0, 5.0];
        let spec = |s: [f64; 3]| Spec {
            t: DVec3::ZERO,
            r,
            s: DVec3::from_array(s),
            rp: DVec3::ZERO,
            ro,
            ra,
            rpt: DVec3::ZERO,
            spt: DVec3::ZERO,
        };
        let linear = |sp: &Spec| {
            DMat3::from_cols(
                node_point(sp, DVec3::X),
                node_point(sp, DVec3::Y),
                node_point(sp, DVec3::Z),
            )
        };
        let old = linear(&spec(before));
        for (axis, handle) in [ScaleHandle::X, ScaleHandle::Y, ScaleHandle::Z]
            .into_iter()
            .enumerate()
        {
            let (mut scene, node) = test_fixtures::cube();
            install(&mut scene, node, &spec(before));
            let out = scene
                .begin_scale_frame(&target(&scene, node), handle, ScaleSpace::Parent)
                .unwrap()
                .0
                .scale(handle, 1.5)
                .unwrap();
            let mut d = DVec3::ONE;
            d[axis] = 1.5;
            let expected = DMat3::from_diagonal(d) * old;
            let drift = (linear(&spec(out)) - expected)
                .to_cols_array()
                .map(f64::abs)
                .into_iter()
                .fold(0.0, f64::max);
            assert!(drift < 1e-12, "ro {ro} axis {axis}: drift {drift}");
        }
    }
}

#[test]
fn parent_space_ignores_an_offset_parent_matrix_that_world_space_applies() {
    let (mut scene, node) = test_fixtures::cube();
    scene
        .set_transform(
            node,
            NativeTransform {
                scale: [2.0, 3.0, 5.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    // Row-vector opm of a 90 degree turn about Z: world X is local Y.
    let mut opm = [0.0; 16];
    opm[1] = 1.0;
    opm[4] = -1.0;
    opm[10] = 1.0;
    opm[15] = 1.0;
    with_extras(
        &mut scene,
        node,
        TransformExtras {
            offset_parent_matrix: Some(opm),
            ..TransformExtras::default()
        },
    );
    let scaled = |space| {
        scene
            .begin_scale_frame(&target(&scene, node), ScaleHandle::X, space)
            .unwrap()
            .0
            .scale(ScaleHandle::X, 2.0)
            .unwrap()
    };
    assert_eq!(scaled(ScaleSpace::World), [2.0, 6.0, 5.0]);
    assert_eq!(scaled(ScaleSpace::Parent), [4.0, 3.0, 5.0]);
}
