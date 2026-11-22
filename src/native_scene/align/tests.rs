//! Align Objects: world boxes of the aligned objects, read without the production box code.
use super::*;
use crate::native_scene::group::tests::{bits, cube, set};
use crate::native_scene::{
    ALL_CHANNELS, DEFAULT_JOINT_RADIUS, JointRequest, LinearUnit, ReparentKeep, ReparentRefusal,
    test_fixtures,
};
use glam::DVec3;

const EPS: f64 = 1e-9;
const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;

fn modes(x: AlignMode, y: AlignMode, z: AlignMode) -> AlignOptions {
    AlignOptions {
        modes: [x, y, z],
        to: AlignTo::SelectionAverage,
    }
}

fn chain(scene: &NativeScene, transform: NativeId) -> Vec<NativeId> {
    let mut path = vec![transform];
    while let Some(parent) = scene
        .dag
        .iter()
        .find(|edge| edge.child == *path.last().unwrap())
        .and_then(|edge| edge.parent)
    {
        path.push(parent);
    }
    path.reverse();
    path
}

/// World box of the node's own shape, from the stored points and the chain of world matrices.
fn world_box(scene: &NativeScene, id: NodeId) -> (DVec3, DVec3) {
    let transform = scene.node(id).unwrap().transform_id();
    let world = scene.path_world(&chain(scene, transform)).unwrap();
    let edge = scene
        .dag
        .iter()
        .find(|e| e.parent == Some(transform))
        .unwrap();
    let shape = scene.shapes.iter().find(|s| s.id == edge.child).unwrap();
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for i in 0..shape.mesh.point_count() {
        let p =
            world.transform_point3(DVec3::from_array(shape.mesh.effective_point_cm(i).unwrap()));
        lo = lo.min(p);
        hi = hi.max(p);
    }
    (lo, hi)
}

fn side(scene: &NativeScene, id: NodeId, axis: usize, which: i8) -> f64 {
    let (lo, hi) = world_box(scene, id);
    match which {
        -1 => lo[axis],
        0 => 0.5 * (lo[axis] + hi[axis]),
        _ => hi[axis],
    }
}

/// Three cubes: a rotated and scaled root, a child cube below a rotated, non-uniformly scaled
/// parent, and a plain one.
fn trio() -> (NativeScene, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let (a, b, c, holder) = (
        cube(&mut scene, "a"),
        cube(&mut scene, "b"),
        cube(&mut scene, "c"),
        cube(&mut scene, "holder"),
    );
    set(
        &mut scene,
        a,
        [5.0, 1.0, -3.0],
        [10.0, 20.0, 30.0],
        [1.0, 2.0, 3.0],
    );
    set(
        &mut scene,
        holder,
        [-4.0, 2.0, 1.0],
        [0.0, 40.0, 15.0],
        [2.0, 0.5, 3.0],
    );
    set(
        &mut scene,
        b,
        [1.0, 2.0, 0.5],
        [30.0, 0.0, 10.0],
        [1.5, 1.0, 0.7],
    );
    set(&mut scene, c, [-9.0, 7.0, 4.0], [0.0; 3], [0.5, 4.0, 2.0]);
    scene
        .reparent(b, Some(holder), ReparentKeep::Local)
        .unwrap();
    (scene, a, b, c)
}

#[test]
fn min_mid_max_line_the_boxes_up_in_world_space_per_axis() {
    for (mode, which) in [
        (AlignMode::Min, -1),
        (AlignMode::Mid, 0),
        (AlignMode::Max, 1),
    ] {
        for axis in [X, Y, Z] {
            let (mut scene, a, b, c) = trio();
            let before = scene.clone();
            let average = [a, b, c]
                .iter()
                .map(|n| side(&before, *n, axis, which))
                .sum::<f64>()
                / 3.0;
            let mut m = [AlignMode::None; 3];
            m[axis] = mode;
            scene
                .align_objects(
                    &[a, b, c],
                    AlignOptions {
                        modes: m,
                        to: AlignTo::SelectionAverage,
                    },
                )
                .unwrap();
            for n in [a, b, c] {
                assert!(
                    (side(&scene, n, axis, which) - average).abs() < EPS,
                    "{mode:?} {axis}"
                );
                for other in (0..3).filter(|o| *o != axis) {
                    // an untouched axis keeps the box exactly
                    let (l0, _) = world_box(&before, n);
                    let (l1, _) = world_box(&scene, n);
                    assert!((l0[other] - l1[other]).abs() < EPS);
                }
            }
        }
    }
}

#[test]
fn last_selected_is_the_reference_and_does_not_move() {
    let (mut scene, a, b, c) = trio();
    let (before_c, hold) = (side(&scene, c, Y, 1), bits(&scene, c));
    scene
        .align_objects(
            &[a, b, c],
            AlignOptions {
                modes: [AlignMode::None, AlignMode::Max, AlignMode::Min],
                to: AlignTo::LastSelected,
            },
        )
        .unwrap();
    assert_eq!(bits(&scene, c), hold);
    for n in [a, b, c] {
        assert!((side(&scene, n, Y, 1) - before_c).abs() < EPS);
    }
    let z = side(&scene, c, Z, -1);
    for n in [a, b] {
        assert!((side(&scene, n, Z, -1) - z).abs() < EPS);
    }
}

#[test]
fn combined_axes_and_only_translate_channels_change() {
    let (mut scene, a, b, c) = trio();
    let before = scene.clone();
    scene
        .align_objects(
            &[a, b, c],
            modes(AlignMode::Mid, AlignMode::None, AlignMode::Max),
        )
        .unwrap();
    for n in [a, b, c] {
        let (old, new) = (bits(&before, n), bits(&scene, n));
        assert_eq!(old[3..], new[3..], "rotate and scale stay bit-exact");
    }
    let mids: Vec<f64> = [a, b, c].iter().map(|n| side(&scene, *n, X, 0)).collect();
    assert!(mids.iter().all(|m| (m - mids[0]).abs() < EPS));
    // the unchosen Y axis moved nothing: the translate Y slot of the plain root is bit-exact
    assert_eq!(bits(&before, c)[1], bits(&scene, c)[1]);
}

#[test]
fn dist_equalises_the_gaps_and_keeps_the_outer_boxes() {
    let mut scene = NativeScene::default();
    let ids: Vec<NodeId> = (0..4).map(|i| cube(&mut scene, &format!("d{i}"))).collect();
    let xs = [0.0, 1.0, 2.0, 20.0];
    let sizes = [1.0, 3.0, 2.0, 1.0];
    for ((id, x), s) in ids.iter().zip(xs).zip(sizes) {
        set(&mut scene, *id, [x, 0.0, 0.0], [0.0; 3], [s, 1.0, 1.0]);
    }
    let (first, last) = (world_box(&scene, ids[0]), world_box(&scene, ids[3]));
    // select out of order: the order comes from the box centres
    scene
        .align_objects(
            &[ids[2], ids[0], ids[3], ids[1]],
            modes(AlignMode::Dist, AlignMode::None, AlignMode::None),
        )
        .unwrap();
    assert_eq!(world_box(&scene, ids[0]), first);
    assert_eq!(world_box(&scene, ids[3]), last);
    let boxes: Vec<_> = ids.iter().map(|i| world_box(&scene, *i)).collect();
    let gaps: Vec<f64> = boxes.windows(2).map(|w| w[1].0.x - w[0].1.x).collect();
    assert!(gaps.iter().all(|g| (g - gaps[0]).abs() < EPS), "{gaps:?}");
}

#[test]
fn aligning_twice_is_stable_to_rounding() {
    let (mut scene, a, b, c) = trio();
    let before = scene.clone();
    scene
        .align_objects(
            &[a, b, c],
            modes(AlignMode::Min, AlignMode::Min, AlignMode::Min),
        )
        .unwrap();
    assert_ne!(scene, before);
    let once = scene.clone();
    scene
        .align_objects(
            &[a, b, c],
            modes(AlignMode::Min, AlignMode::Min, AlignMode::Min),
        )
        .unwrap();
    for n in [a, b, c] {
        let (x, y) = (world_box(&once, n), world_box(&scene, n));
        assert!((x.0 - y.0).abs().max_element() < EPS && (x.1 - y.1).abs().max_element() < EPS);
    }
}

#[test]
fn a_refusal_on_the_third_node_changes_nothing() {
    let (mut scene, a, b, c) = trio();
    scene.set_keys(&[c], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    let err = scene
        .align_objects(
            &[a, b, c],
            modes(AlignMode::Min, AlignMode::None, AlignMode::None),
        )
        .unwrap_err();
    assert_eq!(
        err,
        NativeError::InvalidTransformEdit(TransformRefusal::Keyed)
    );
    assert_eq!(scene, before);
}

#[test]
fn typed_refusals() {
    let (scene, a, b, c) = trio();
    let min = modes(AlignMode::Min, AlignMode::None, AlignMode::None);
    let none = modes(AlignMode::None, AlignMode::None, AlignMode::None);
    let mut nested = scene.clone();
    nested.reparent(c, Some(a), ReparentKeep::Local).unwrap();
    let (mut pivoted, pivoted_id) = test_fixtures::pivoted_shape();
    let extra = cube(&mut pivoted, "extra");
    let (hollow, cube_id, empty_id) = test_fixtures::cube_and_empty_transform();
    let cases: Vec<(NativeScene, Vec<NodeId>, AlignOptions, TransformRefusal)> = vec![
        (scene.clone(), vec![a, b], none, TransformRefusal::EmptySet),
        (scene.clone(), vec![a], min, TransformRefusal::AlignTooFew),
        (scene.clone(), vec![], min, TransformRefusal::AlignTooFew),
        (nested, vec![a, c], min, TransformRefusal::Nested),
        (
            pivoted,
            vec![pivoted_id, extra],
            min,
            TransformRefusal::NotPlain,
        ),
        (
            hollow,
            vec![cube_id, empty_id],
            min,
            TransformRefusal::NoGeometry,
        ),
    ];
    for (mut scene, ids, options, refusal) in cases {
        let before = scene.clone();
        assert_eq!(
            scene.align_objects(&ids, options),
            Err(NativeError::InvalidTransformEdit(refusal)),
            "{refusal:?}"
        );
        assert_eq!(scene, before);
    }
    let mut dup = scene.clone();
    assert!(dup.align_objects(&[a, a], min).is_err());
}

fn holder_of(scene: &NativeScene, id: NodeId) -> NodeId {
    let t = scene.node(id).unwrap().transform_id();
    let p = scene
        .dag
        .iter()
        .find(|e| e.child == t)
        .unwrap()
        .parent
        .unwrap();
    scene.nodes.iter().find(|n| n.transform_id == p).unwrap().id
}

fn refused(scene: &mut NativeScene, ids: &[NodeId], expected: NativeError) {
    let before = scene.clone();
    let min = modes(AlignMode::Min, AlignMode::None, AlignMode::None);
    assert_eq!(scene.align_objects(ids, min), Err(expected));
    assert_eq!(*scene, before);
}

#[test]
fn a_collapsing_parent_is_refused_as_degenerate() {
    // without the keeps_volume guard the singular inverse surfaces as NonFinite (zero) or the
    // huge local translate is attempted (tiny scale)
    for scale in [0.0, 1e-12] {
        let (mut scene, a, b, _) = trio();
        let holder = holder_of(&scene, b);
        set(&mut scene, holder, [0.0; 3], [0.0; 3], [scale, 1.0, 1.0]);
        refused(
            &mut scene,
            &[a, b],
            NativeError::InvalidTransformEdit(TransformRefusal::Degenerate),
        );
    }
}

#[test]
fn a_translate_the_channels_cannot_hold_exactly_is_refused_as_not_representable() {
    // the parent maps local 1 to world 0 through a 1e10 scale, so the rounding of the local
    // translate (1e-16) becomes 1e-6 in the world: beyond the 1e-9 re-measure tolerance
    let (mut scene, a, b, _) = trio();
    let holder = holder_of(&scene, b);
    set(&mut scene, holder, [-1e10; 3], [0.0; 3], [1e10; 3]);
    set(&mut scene, b, [1.0; 3], [0.0; 3], [1e-10; 3]);
    set(&mut scene, a, [3.1, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    refused(
        &mut scene,
        &[a, b],
        NativeError::InvalidTransformEdit(TransformRefusal::NotRepresentable),
    );
}

#[test]
fn a_units_mismatch_is_refused() {
    for linear in [
        LinearUnit::Meter,
        LinearUnit::Inch,
        LinearUnit::Unsupported("furlong".into()),
    ] {
        let (mut scene, a, b, _) = trio();
        scene.units.linear = linear;
        refused(&mut scene, &[a, b], NativeError::UnsupportedEdit);
    }
}

#[test]
fn joints_and_instanced_objects_are_refused() {
    let (mut scene, a, _, _) = trio();
    let joint = scene
        .create_joint(&JointRequest {
            parent: None,
            position: [1.0, 0.0, 0.0],
            relative: false,
            radius: DEFAULT_JOINT_RADIUS,
            name: None,
        })
        .unwrap()
        .node;
    refused(
        &mut scene,
        &[a, joint],
        NativeError::InvalidTransformEdit(TransformRefusal::Joint),
    );
    let (mut instanced, x1, _, a, _) = test_fixtures::instanced_chain();
    refused(
        &mut instanced,
        &[x1, a],
        NativeError::InvalidReparent(ReparentRefusal::Instanced),
    );
}

#[test]
fn a_curve_below_the_object_is_refused_not_skipped() {
    // a curve exists only in a sourced scene, which the public entry refuses first; the box
    // reader is reached directly, where skipping the curve would report NoGeometry instead
    let (scene, node) = crate::native_scene::curve::tests::candidate();
    let targets = scene.transform_paths().unwrap();
    let path = scene.sole_target(node).unwrap().path;
    assert_eq!(
        scene.world_box(&path, &targets),
        Err(NativeError::InvalidTransformEdit(TransformRefusal::Curve))
    );
}

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}
mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[test]
fn a_sourced_scene_is_refused() {
    let scenes = [
        (
            crate::native_scene::ascii::decode(std::sync::Arc::from(ascii_fixture::specimen()))
                .unwrap(),
            NativeError::ReadOnlyFormat,
        ),
        (
            crate::native_scene::binary::decode(std::sync::Arc::from(binary_fixture::specimen()))
                .unwrap(),
            NativeError::UnsupportedEdit,
        ),
    ];
    for (mut scene, expected) in scenes {
        let id = scene.nodes[0].id;
        refused(&mut scene, &[id, id], expected);
    }
}

#[test]
fn dist_of_two_objects_is_a_no_op() {
    let (mut scene, a, b, _) = trio();
    let before = scene.clone();
    scene
        .align_objects(
            &[a, b],
            modes(AlignMode::Dist, AlignMode::Dist, AlignMode::Dist),
        )
        .unwrap();
    assert_eq!(scene, before);
}

/// Three cubes scaled along X to `sizes`, centered at `centers`, selected in the given order.
fn x_row(centers: [f64; 3], sizes: [f64; 3]) -> (NativeScene, Vec<NodeId>) {
    let mut scene = NativeScene::default();
    let ids: Vec<NodeId> = (0..3).map(|i| cube(&mut scene, &format!("r{i}"))).collect();
    for ((id, c), s) in ids.iter().zip(centers).zip(sizes) {
        set(&mut scene, *id, [c, 0.0, 0.0], [0.0; 3], [s, 1.0, 1.0]);
    }
    (scene, ids)
}

#[test]
fn dist_with_overlapping_boxes_spreads_over_a_negative_gap() {
    // outer boxes overlap: the gap is negative, the inner box is placed overlapping both
    let (mut scene, ids) = x_row([0.0, 0.5, 1.0], [4.0, 1.0, 4.0]);
    let (first, last) = (world_box(&scene, ids[0]), world_box(&scene, ids[2]));
    let width = {
        let b = world_box(&scene, ids[1]);
        b.1.x - b.0.x
    };
    scene
        .align_objects(
            &ids,
            modes(AlignMode::Dist, AlignMode::None, AlignMode::None),
        )
        .unwrap();
    assert_eq!(world_box(&scene, ids[0]), first);
    assert_eq!(world_box(&scene, ids[2]), last);
    let mid = world_box(&scene, ids[1]);
    let (g1, g2) = (mid.0.x - first.1.x, last.0.x - mid.1.x);
    let expected = (last.0.x - first.1.x - width) / 2.0;
    assert!(expected < 0.0, "the fixture must overlap");
    assert!(
        (g1 - expected).abs() < EPS && (g2 - expected).abs() < EPS,
        "{g1} {g2}"
    );
}

#[test]
fn dist_with_equal_centers_orders_by_selection_and_keeps_the_outer_two() {
    let (mut scene, ids) = x_row([3.0; 3], [1.0, 3.0, 2.0]);
    let (first, last) = (world_box(&scene, ids[0]), world_box(&scene, ids[2]));
    scene
        .align_objects(
            &ids,
            modes(AlignMode::Dist, AlignMode::None, AlignMode::None),
        )
        .unwrap();
    // equal centers tie-break by selection order: the first and last selected stay put
    assert_eq!(world_box(&scene, ids[0]), first);
    assert_eq!(world_box(&scene, ids[2]), last);
    let mid = world_box(&scene, ids[1]);
    let (g1, g2) = (mid.0.x - first.1.x, last.0.x - mid.1.x);
    assert!((g1 - g2).abs() < EPS, "{g1} {g2}");
}
