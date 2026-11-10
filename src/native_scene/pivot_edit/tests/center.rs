use super::*;
use crate::native_scene::{
    DagEdge,
    test_fixtures::{cube_and_empty_transform, hide_first_shape},
};

/// A cube child of `parent`, `x_cm` along the parent's x (declared units).
fn add_child(scene: &mut NativeScene, parent: NodeId, name: &str, x_cm: f64) -> NodeId {
    let child = scene.create(Primitive::Cube, name).unwrap();
    let (parent_native, child_native) = (
        scene.node(parent).unwrap().transform_id,
        scene.node(child).unwrap().transform_id,
    );
    scene
        .dag
        .retain(|e| !(e.child == child_native && e.parent.is_none()));
    scene.dag.push(DagEdge {
        parent: Some(parent_native),
        child: child_native,
    });
    let x = Storage::Centimeters.declared(x_cm, &scene.units).unwrap();
    scene
        .set_transform(
            child,
            NativeTransform {
                translation: [x, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    scene.validate().unwrap();
    child
}

/// The cube's x extent is `[-half, half]`, so a child at `x` makes the box centre `x / 2`.
fn centre_x_cm(x_cm: f64) -> f64 {
    x_cm / 2.0
}

fn declared(scene: &NativeScene, cm: [f64; 3]) -> [f64; 3] {
    cm.map(|v| Storage::Centimeters.declared(v, &scene.units).unwrap())
}

fn assert_close(label: &str, got: [f64; 3], want: [f64; 3]) {
    let scale = 1.0 + want.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    for (g, w) in got.iter().zip(want) {
        assert!(
            (g - w).abs() <= 1e-9 * scale,
            "{label}: {got:?} vs {want:?}"
        );
    }
}

#[test]
fn center_pivot_moves_both_pivots_to_the_box_centre_and_keeps_the_matrix_over_the_case_table() {
    for c in table() {
        let (mut scene, id) = build(&c);
        add_child(&mut scene, id, "child", 100.0);
        let before = matrix(&scene, id);
        assert_eq!(
            scene.reset_pivots(&[id], PivotReset::Center),
            Ok(()),
            "{}",
            c.name
        );
        let node = scene.node(id).unwrap();
        let want = declared(&scene, [centre_x_cm(100.0), 0.0, 0.0]);
        assert_close(c.name, node.rotate_pivot(), want);
        assert_close(c.name, node.scale_pivot(), want);
        assert_same_matrix(c.name, before, matrix(&scene, id));
    }
}

#[test]
fn center_pivot_compensates_through_the_pivot_translates() {
    // Rz(90 degrees), rp from 0 to (50, 0, 0): rt' = -(I - R) d = (-50, 50, 0); S = 1 so spt stays
    // unset, and the translate channel is untouched.
    let (mut scene, id) = cube();
    scene.nodes[0].channels[5] = Scalar::new(90.0);
    add_child(&mut scene, id, "child", 100.0);
    let translate = bits(scene.node(id).unwrap().transform().translation);
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    let node = scene.node(id).unwrap();
    assert_close(
        "rpt",
        node.extras().rotate_pivot_translate.unwrap(),
        [-50.0, 50.0, 0.0],
    );
    assert_eq!(node.extras().scale_pivot_translate, None);
    assert_eq!(bits(node.transform().translation), translate);
    assert_close("rp", node.rotate_pivot(), [50.0, 0.0, 0.0]);
}

#[test]
fn center_then_zero_restores_the_matrix_and_clears_the_pivots() {
    let (mut scene, id) = build(&base("rotated"));
    add_child(&mut scene, id, "child", 100.0);
    let before = matrix(&scene, id);
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    scene.reset_pivots(&[id], PivotReset::Zero).unwrap();
    let node = scene.node(id).unwrap();
    assert!(
        node.rotate_pivot()
            .iter()
            .chain(&node.scale_pivot())
            .all(|v| *v == 0.0)
    );
    assert_eq!(node.extras().rotate_pivot_translate, None);
    assert_same_matrix("center then zero", before, matrix(&scene, id));
}

#[test]
fn a_call_that_changes_nothing_is_a_no_op() {
    let (mut scene, id) = build(&base("rotated"));
    add_child(&mut scene, id, "child", 100.0);
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    let centred = scene.clone();
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    assert_eq!(scene, centred);
    assert_eq!(scene.reset_pivots(&[], PivotReset::Center), Ok(()));
    assert_eq!(scene, centred);
    // A node without geometry is left as it is, whatever its pivots.
    let (mut scene, _, empty) = cube_and_empty_transform();
    scene.nodes[1].rotate_pivot = [1.0, 2.0, 3.0].map(Scalar::new);
    let before = scene.clone();
    assert_eq!(scene.reset_pivots(&[empty], PivotReset::Center), Ok(()));
    assert_eq!(scene, before);
}

#[test]
fn a_meter_scene_stores_declared_units() {
    let (mut scene, id) = build(&Case {
        linear: LinearUnit::Meter,
        ..base("meter")
    });
    add_child(&mut scene, id, "child", 100.0);
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    assert_close(
        "metres",
        scene.node(id).unwrap().rotate_pivot(),
        [0.5, 0.0, 0.0],
    );
    let again = scene.clone();
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    assert_eq!(scene, again);
}

#[test]
fn each_object_is_centred_on_its_own_hierarchy() {
    let (mut scene, parent) = cube();
    let child = add_child(&mut scene, parent, "child", 100.0);
    let other = scene.create(Primitive::Cube, "other").unwrap();
    let grandchild = add_child(&mut scene, other, "grandchild", -40.0);
    scene
        .reset_pivots(&[parent, child, other], PivotReset::Center)
        .unwrap();
    let pivot = |scene: &NativeScene, id| scene.node(id).unwrap().rotate_pivot();
    assert_close("parent", pivot(&scene, parent), [50.0, 0.0, 0.0]);
    assert_close("child", pivot(&scene, child), [0.0; 3]);
    assert_close("other", pivot(&scene, other), [-20.0, 0.0, 0.0]);
    assert_eq!(pivot(&scene, grandchild), [0.0; 3]);
}

#[test]
fn invisible_children_do_not_move_the_centre_and_unproved_ones_refuse() {
    let (mut scene, parent) = cube();
    add_child(&mut scene, parent, "child", 100.0);
    scene.shapes[1].view.local_visibility = Some(false);
    scene.reset_pivots(&[parent], PivotReset::Center).unwrap();
    assert_close(
        "hidden child",
        scene.node(parent).unwrap().rotate_pivot(),
        [0.0; 3],
    );
    scene.nodes[0].rotate_pivot = [1.0, 0.0, 0.0].map(Scalar::new);
    scene.shapes[1].view.local_visibility = None;
    let before = scene.clone();
    assert_eq!(
        scene.reset_pivots(&[parent], PivotReset::Center),
        Err(NativeError::UnsupportedView("unproved local visibility"))
    );
    assert_eq!(scene, before);
    hide_first_shape(&mut scene);
    scene.shapes[1].view.local_visibility = Some(true);
    scene.reset_pivots(&[parent], PivotReset::Center).unwrap();
    assert_close(
        "own shape hidden",
        scene.node(parent).unwrap().rotate_pivot(),
        [100.0, 0.0, 0.0],
    );
}

#[test]
fn refusals_leave_every_node_untouched() {
    let (mut scene, first) = build(&base("first"));
    add_child(&mut scene, first, "child", 100.0);
    let second = scene.create(Primitive::Cube, "second").unwrap();
    for mode in [PivotReset::Center, PivotReset::Zero] {
        let before = scene.clone();
        assert_eq!(
            scene.reset_pivots(&[first, first], mode),
            Err(SceneError::InvalidIds.into())
        );
        assert_eq!(
            scene.reset_pivots(&[first, NodeId::new(999)], mode),
            Err(SceneError::MissingNode(NodeId::new(999)).into())
        );
        assert_eq!(scene, before);
    }
    scene.set_keys(&[second], 1, ALL_CHANNELS, 1).unwrap();
    let keyed = scene.clone();
    for mode in [PivotReset::Center, PivotReset::Zero] {
        assert_eq!(
            scene.reset_pivots(&[first, second], mode),
            Err(NativeError::PivotOnKeyedNode)
        );
        assert_eq!(scene, keyed);
    }
    let unkeyed = scene.node(first).unwrap().id();
    assert_eq!(scene.reset_pivots(&[unkeyed], PivotReset::Center), Ok(()));
    restrict_edits(&mut scene, second);
    let restricted = scene.clone();
    assert!(
        scene
            .reset_pivots(&[unkeyed, second], PivotReset::Center)
            .is_err()
    );
    assert_eq!(scene, restricted);
}

#[test]
fn zero_mode_is_zero_pivots() {
    let (mut by_mode, id) = build(&base("rotated"));
    let mut direct = by_mode.clone();
    by_mode.reset_pivots(&[id], PivotReset::Zero).unwrap();
    direct.zero_pivots(&[id]).unwrap();
    assert_eq!(by_mode, direct);
}

#[test]
fn an_overflowing_centre_is_refused() {
    let (mut scene, id) = cube();
    add_child(&mut scene, id, "child", 1.7e308);
    // Rotated 180 degrees the compensation is 2d = 1.7e308, which overflows against t.
    scene.nodes[0].channels[0] = Scalar::new(-1.5e308);
    scene.nodes[0].channels[5] = Scalar::new(180.0);
    let before = scene.clone();
    assert_eq!(
        scene.reset_pivots(&[id], PivotReset::Center),
        Err(NativeError::NonFinite)
    );
    assert_eq!(scene, before);
}

#[test]
fn an_opened_binary_takes_only_a_centre_that_needs_no_compensation() {
    // Unrotated, unscaled: the compensation is exactly zero, so only the rp and sp words change.
    let (mut scene, id) = opened([0.0; 3], [1.0; 3]);
    let (lo, hi) = scene.hierarchy_bounds_local(id).unwrap().unwrap();
    let centre = (lo * 0.5 + hi * 0.5).to_array();
    scene.reset_pivots(&[id], PivotReset::Center).unwrap();
    let node = scene.node(id).unwrap();
    assert_eq!(node.rotate_pivot(), centre);
    assert_eq!(node.scale_pivot(), centre);
    assert_eq!(node.extras(), &TransformExtras::default());
    // Rotated: the compensation needs rpt, which an opened binary cannot store.
    let (mut scene, id) = opened([0.0, 0.0, 0.7], [2.0, 1.0, 1.0]);
    let before = scene.clone();
    assert_eq!(
        scene.reset_pivots(&[id], PivotReset::Center),
        Err(NativeError::PivotNeedsTranslateCorrection)
    );
    assert_eq!(scene, before);
}

#[test]
fn an_opened_binary_without_pivot_leaves_cannot_be_centred() {
    let mut scene = binary_scene(&[(b"t", [1.0, 2.0, 3.0])]);
    let id = scene.nodes()[0].id();
    let before = scene.clone();
    assert_eq!(
        scene.reset_pivots(&[id], PivotReset::Center),
        Err(NativeError::AbsentPivotLeaf {
            field: PivotLeaf::Rotate
        })
    );
    assert_eq!(scene, before);
}

#[test]
fn an_opened_ascii_scene_is_read_only() {
    let mut scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let id = scene.nodes()[0].id();
    let before = scene.clone();
    for mode in [PivotReset::Center, PivotReset::Zero] {
        assert_eq!(
            scene.reset_pivots(&[id], mode),
            Err(NativeError::ReadOnlyFormat)
        );
        assert_eq!(scene, before);
    }
}
