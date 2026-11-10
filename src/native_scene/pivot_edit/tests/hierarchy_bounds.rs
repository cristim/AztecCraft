use super::*;
use crate::native_scene::{
    DagEdge, StaticGeometry,
    test_fixtures::{
        chain_of_three, cube_and_empty_transform, group_with_own_shape_and_child, hide_first_shape,
    },
};

type Box3 = (DVec3, DVec3);

/// The box of the first mesh shape's own points (no transform applied).
fn shape_box(scene: &NativeScene, shape: usize) -> Box3 {
    let mesh = scene.shapes[shape].mesh();
    let points: Vec<DVec3> = (0..mesh.point_count())
        .map(|i| DVec3::from_array(mesh.effective_point_cm(i).unwrap()))
        .collect();
    (
        points.iter().fold(DVec3::MAX, |a, p| a.min(*p)),
        points.iter().fold(DVec3::MIN, |a, p| a.max(*p)),
    )
}

fn assert_box(label: &str, got: Option<Box3>, want: Box3) {
    let (lo, hi) = got.unwrap_or_else(|| panic!("{label}: no bounds"));
    assert!(
        lo.abs_diff_eq(want.0, 1e-9) && hi.abs_diff_eq(want.1, 1e-9),
        "{label}: {lo:?} {hi:?} vs {want:?}"
    );
}

fn moved(b: Box3, by: DVec3) -> Box3 {
    (b.0 + by, b.1 + by)
}

fn union(a: Box3, b: Box3) -> Box3 {
    (a.0.min(b.0), a.1.max(b.1))
}

fn place(scene: &mut NativeScene, id: NodeId, t: [f64; 3], r: [f64; 3], s: [f64; 3]) {
    scene
        .set_transform(
            id,
            NativeTransform {
                translation: t,
                rotation: r,
                scale: s,
            },
        )
        .unwrap();
}

#[test]
fn a_shape_is_boxed_in_its_own_object_space_whatever_the_node_matrix() {
    let (mut scene, id) = cube();
    let own = shape_box(&scene, 0);
    assert_box("plain", scene.hierarchy_bounds_local(id).unwrap(), own);
    place(
        &mut scene,
        id,
        [5.0, -3.0, 9.0],
        [30.0, 40.0, 50.0],
        [2.0, 3.0, 4.0],
    );
    assert_box("placed", scene.hierarchy_bounds_local(id).unwrap(), own);
}

#[test]
fn descendants_are_mapped_into_the_node_object_space() {
    let (mut scene, parent, child) = group_with_own_shape_and_child();
    let cube_box = shape_box(&scene, 0);
    place(
        &mut scene,
        parent,
        [7.0, 7.0, 7.0],
        [0.0, 0.0, 33.0],
        [5.0; 3],
    );
    // Child at x = 100, scaled (2, 1, 1) then turned 90 degrees about Z, so its x extent is
    // exchanged for its y extent: the box grows along x by the child's translation only.
    place(
        &mut scene,
        child,
        [100.0, 0.0, 0.0],
        [0.0, 0.0, 90.0],
        [2.0, 1.0, 1.0],
    );
    let h = cube_box.1;
    let child_box = (
        DVec3::new(100.0 - h.y, -2.0 * h.x, -h.z),
        DVec3::new(100.0 + h.y, 2.0 * h.x, h.z),
    );
    assert_box(
        "parent",
        scene.hierarchy_bounds_local(parent).unwrap(),
        union(cube_box, child_box),
    );
    assert_box(
        "child alone",
        scene.hierarchy_bounds_local(child).unwrap(),
        cube_box,
    );
}

#[test]
fn only_the_matrices_below_the_node_are_applied_in_a_deep_chain() {
    let (mut scene, root, mid, leaf) = chain_of_three();
    let mut leaf_shape = scene.shapes[0].clone();
    leaf_shape.id = NativeId(scene.next_native_id);
    scene.next_native_id += 1;
    scene.dag.push(DagEdge {
        parent: Some(scene.node(leaf).unwrap().transform_id),
        child: leaf_shape.id,
    });
    scene.shapes.push(leaf_shape);
    scene.validate().unwrap();
    place(&mut scene, root, [1000.0; 3], [0.0; 3], [9.0; 3]);
    place(&mut scene, mid, [10.0, 0.0, 0.0], [0.0; 3], [1.0; 3]);
    place(&mut scene, leaf, [0.0, 20.0, 0.0], [0.0; 3], [1.0; 3]);
    let own = shape_box(&scene, 0);
    assert_box(
        "root",
        scene.hierarchy_bounds_local(root).unwrap(),
        moved(own, DVec3::new(10.0, 20.0, 0.0)),
    );
    assert_box(
        "mid",
        scene.hierarchy_bounds_local(mid).unwrap(),
        moved(own, DVec3::new(0.0, 20.0, 0.0)),
    );
}

#[test]
fn proved_invisible_geometry_is_left_out() {
    let (mut scene, parent, child) = group_with_own_shape_and_child();
    let own = shape_box(&scene, 0);
    let at_child = moved(own, DVec3::new(100.0, 0.0, 0.0));
    // The child's shape is hidden: only the parent's own shape counts.
    scene.shapes[1].view.local_visibility = Some(false);
    assert_box(
        "hidden child shape",
        scene.hierarchy_bounds_local(parent).unwrap(),
        own,
    );
    // The child's transform is hidden instead.
    scene.shapes[1].view.local_visibility = Some(true);
    scene.nodes[1].view.local_visibility = Some(false);
    assert_box(
        "hidden child node",
        scene.hierarchy_bounds_local(parent).unwrap(),
        own,
    );
    // The parent's own shape is hidden: the child alone remains.
    scene.nodes[1].view.local_visibility = Some(true);
    hide_first_shape(&mut scene);
    assert_box(
        "hidden own shape",
        scene.hierarchy_bounds_local(parent).unwrap(),
        at_child,
    );
    // Everything hidden: no geometry.
    scene.shapes[1].view.local_visibility = Some(false);
    assert_eq!(scene.hierarchy_bounds_local(parent), Ok(None));
    // The node's own visibility and its ancestors' are not consulted.
    scene.shapes[1].view.local_visibility = Some(true);
    scene.shapes[0].view.local_visibility = Some(true);
    scene.nodes[0].view.local_visibility = Some(false);
    assert_box(
        "hidden node",
        scene.hierarchy_bounds_local(child).unwrap(),
        own,
    );
    assert!(scene.hierarchy_bounds_local(parent).unwrap().is_some());
}

#[test]
fn unproved_visibility_is_a_typed_refusal() {
    let (mut scene, parent, _) = group_with_own_shape_and_child();
    let unproved = |scene: &NativeScene, reason| {
        assert_eq!(
            scene.hierarchy_bounds_local(parent),
            Err(NativeError::UnsupportedView(reason))
        );
    };
    scene.shapes[1].view.local_visibility = None;
    unproved(&scene, "unproved local visibility");
    scene.shapes[1].view.local_visibility = Some(true);
    scene.nodes[1].view.local_visibility = None;
    unproved(&scene, "unproved local visibility");
    scene.nodes[1].view.local_visibility = Some(true);
    scene.shapes[1].view.geometry = None;
    unproved(&scene, "unproved geometry evaluation");
    scene.shapes[1].view.geometry = Some(StaticGeometry);
    scene.nodes[1].unsupported_transform = vec!["unknown effect".into()];
    unproved(&scene, "unproved transform effects");
    // An unproved child only matters when geometry sits below it.
    let gone = scene.shapes.pop().unwrap().id;
    scene.dag.retain(|e| e.child != gone);
    scene.validate().unwrap();
    assert!(scene.hierarchy_bounds_local(parent).unwrap().is_some());
}

#[test]
fn a_hierarchy_without_geometry_has_no_bounds() {
    let (scene, cube_id, empty) = cube_and_empty_transform();
    assert_eq!(scene.hierarchy_bounds_local(empty), Ok(None));
    assert!(scene.hierarchy_bounds_local(cube_id).unwrap().is_some());
    let (mut scene, parent, child) = group_with_own_shape_and_child();
    scene.shapes.clear();
    let transforms: Vec<_> = scene.nodes.iter().map(|n| n.transform_id).collect();
    scene.dag.retain(|e| transforms.contains(&e.child));
    scene.validate().unwrap();
    assert_eq!(scene.hierarchy_bounds_local(parent), Ok(None));
    assert_eq!(scene.hierarchy_bounds_local(child), Ok(None));
}

const GROUPED_CURVE: &str = r#"//Maya ASCII 2016 scene
requires maya "2016";
currentUnit -l centimeter -a degree -t film;
createNode transform -n "grp";
createNode transform -n "curve" -p "grp";
setAttr ".t" -type "double3" 12 0 0;
createNode nurbsCurve -n "curveShape" -p "curve";
setAttr ".cc" -type "nurbsCurve" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;
"#;

#[test]
fn curve_shapes_are_boxed_by_their_control_points() {
    let scene = ascii::decode(Arc::from(GROUPED_CURVE.as_bytes())).unwrap();
    let [group, curve] = [0, 1].map(|i| scene.nodes()[i].id());
    let cvs = (DVec3::new(0.0, -3.0, 0.0), DVec3::new(3.0, 3.0, 0.0));
    assert_box(
        "curve node",
        scene.hierarchy_bounds_local(curve).unwrap(),
        cvs,
    );
    assert_box(
        "group",
        scene.hierarchy_bounds_local(group).unwrap(),
        moved(cvs, DVec3::new(12.0, 0.0, 0.0)),
    );
    let mut hidden = scene.clone();
    hidden.curves[0].view.local_visibility = Some(false);
    assert_eq!(hidden.hierarchy_bounds_local(group), Ok(None));
    hidden.curves[0].view.local_visibility = None;
    assert_eq!(
        hidden.hierarchy_bounds_local(group),
        Err(NativeError::UnsupportedView("unproved local visibility"))
    );
}

#[test]
fn a_product_of_scales_that_overflows_is_refused() {
    let (mut scene, root, mid, leaf) = chain_of_three();
    place(&mut scene, mid, [0.0; 3], [0.0; 3], [1e200; 3]);
    place(&mut scene, leaf, [0.0; 3], [0.0; 3], [1e200; 3]);
    assert_eq!(
        scene.hierarchy_bounds_local(root),
        Err(NativeError::NonFinite)
    );
}

#[test]
fn unknown_nodes_are_refused() {
    let (scene, _) = cube();
    assert_eq!(
        scene.hierarchy_bounds_local(NodeId::new(999)),
        Err(SceneError::MissingNode(NodeId::new(999)).into())
    );
}
