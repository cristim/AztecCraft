use super::*;
use crate::native_scene::test_fixtures::cube;
use crate::native_scene::{Channel, RotateAxis, RotateSpace, ScaleHandle, ScaleSpace};
use crate::scene::Primitive;
use glam::{DMat3, DMat4};
use std::sync::Arc;

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

fn request(parent: Option<NodeId>, position: [f64; 3]) -> JointRequest {
    JointRequest {
        parent,
        position,
        relative: false,
        radius: DEFAULT_JOINT_RADIUS,
        name: None,
    }
}

fn joint(scene: &mut NativeScene, parent: Option<NodeId>, position: [f64; 3]) -> NodeId {
    scene.create_joint(&request(parent, position)).unwrap().node
}

fn target(scene: &NativeScene, node: NodeId) -> TransformTarget {
    scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.node == node)
        .unwrap()
}

fn world(scene: &NativeScene, node: NodeId) -> DMat4 {
    let ancestors = scene.ancestor_world(&target(scene, node)).unwrap().unwrap();
    ancestors * scene.internal_transform(node).unwrap().matrix().unwrap()
}

fn set(scene: &mut NativeScene, node: NodeId, transform: NativeTransform) {
    scene.set_transform(node, transform).unwrap();
}

fn assert_near(actual: DVec3, expected: DVec3) {
    assert!(
        (actual - expected).abs().max_element() < 1e-9,
        "{actual:?} != {expected:?}"
    );
}

fn chain(count: usize) -> (NativeScene, Vec<NodeId>) {
    let mut scene = NativeScene::default();
    let mut nodes = Vec::new();
    for index in 0..count {
        let node = joint(&mut scene, nodes.last().copied(), [index as f64, 0.0, 0.0]);
        nodes.push(node);
    }
    (scene, nodes)
}

fn transform_id(scene: &NativeScene, node: NodeId) -> NativeId {
    scene.node(node).unwrap().transform_id
}

#[test]
fn a_root_joint_is_a_bare_root_transform_with_the_command_defaults() {
    let mut scene = NativeScene::default();
    let created = scene.create_joint(&request(None, [1.0, 2.0, 3.0])).unwrap();
    let node = scene.node(created.node).unwrap();
    assert_eq!(node.name(), "joint1");
    assert_eq!(created.path, vec![node.transform_id]);
    assert_eq!(node.transform().translation, [1.0, 2.0, 3.0]);
    assert_eq!(node.transform().rotation, [0.0; 3]);
    assert_eq!(node.transform().scale, [1.0; 3]);
    assert_eq!(
        scene.joint(node.transform_id).map(Joint::radius),
        Some(DEFAULT_JOINT_RADIUS)
    );
    assert!(scene.shapes().is_empty());
    assert_eq!(scene.dag_edges().len(), 1);
    assert_eq!(
        scene
            .internal_transform(created.node)
            .unwrap()
            .inverse_parent_scale,
        None,
        "a root joint has no parent scale to compensate"
    );
    scene.validate().unwrap();
}

#[test]
fn a_child_position_is_world_space_by_default_and_parent_space_when_relative() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [1.0, 2.0, 3.0]);
    let absolute = joint(&mut scene, Some(root), [4.0, 2.0, 3.0]);
    assert_eq!(
        scene.node(absolute).unwrap().transform().translation,
        [3.0, 0.0, 0.0]
    );
    let relative = scene
        .create_joint(&JointRequest {
            relative: true,
            ..request(Some(root), [4.0, 2.0, 3.0])
        })
        .unwrap();
    assert_eq!(
        scene.node(relative.node).unwrap().transform().translation,
        [4.0, 2.0, 3.0]
    );
    assert_eq!(relative.path.len(), 2);
    assert_eq!(relative.path[0], transform_id(&scene, root));
}

#[test]
fn relative_without_a_parent_is_the_same_as_absolute() {
    let mut scene = NativeScene::default();
    let created = scene
        .create_joint(&JointRequest {
            relative: true,
            ..request(None, [5.0, 0.0, 0.0])
        })
        .unwrap();
    assert_eq!(
        scene.node(created.node).unwrap().transform().translation,
        [5.0, 0.0, 0.0]
    );
}

#[test]
fn an_absolute_position_lands_on_the_world_point_under_a_rotated_scaled_chain() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [1.0, 0.0, 0.0]);
    let middle = joint(&mut scene, Some(root), [2.0, 1.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            translation: [1.0, 0.0, 0.0],
            rotation: [10.0, 20.0, 30.0],
            scale: [2.0, 3.0, 0.5],
        },
    );
    set(
        &mut scene,
        middle,
        NativeTransform {
            translation: [1.0, 1.0, 0.0],
            rotation: [0.0, 40.0, 0.0],
            scale: [1.5, 1.0, 2.0],
        },
    );
    let wanted = DVec3::new(-3.0, 4.0, 7.5);
    let child = joint(&mut scene, Some(middle), wanted.to_array());
    assert_near(world(&scene, child).transform_point3(DVec3::ZERO), wanted);
    assert_eq!(scene.node(child).unwrap().transform().rotation, [0.0; 3]);
}

#[test]
fn default_names_take_the_lowest_free_number_across_transforms_and_shapes() {
    let (mut scene, cube_node) = cube();
    scene.rename(cube_node, "joint1").unwrap();
    let second = joint(&mut scene, None, [0.0; 3]);
    assert_eq!(scene.node(second).unwrap().name(), "joint2");
    let mut shape_taken = NativeScene::default();
    let node = shape_taken.create(Primitive::Cube, "mesh").unwrap();
    shape_taken.rename(node, "joint1").unwrap();
    shape_taken.shapes[0].name = "joint2".into();
    let third = joint(&mut shape_taken, None, [0.0; 3]);
    assert_eq!(shape_taken.node(third).unwrap().name(), "joint3");
    let (mut gaps, nodes) = chain(3);
    gaps.remove(nodes[2]).unwrap();
    let again = joint(&mut gaps, Some(nodes[1]), [0.0; 3]);
    assert_eq!(gaps.node(again).unwrap().name(), "joint3");
}

#[test]
fn refusals_name_the_cause_and_leave_the_scene_unchanged() {
    let (mut scene, mesh) = cube();
    joint(&mut scene, None, [0.0; 3]);
    let zero = joint(&mut scene, None, [0.0; 3]);
    set(
        &mut scene,
        zero,
        NativeTransform {
            scale: [0.0, 1.0, 1.0],
            ..NativeTransform::default()
        },
    );
    let before = scene.clone();
    let unknown = NodeId::new(9999);
    for (case, expected) in [
        (request(None, [f64::NAN, 0.0, 0.0]), NativeError::NonFinite),
        (
            JointRequest {
                radius: f64::INFINITY,
                ..request(None, [0.0; 3])
            },
            NativeError::NonFinite,
        ),
        (
            JointRequest {
                radius: 0.0,
                ..request(None, [0.0; 3])
            },
            NativeError::UnsupportedJoint(JointUnsupported::Radius),
        ),
        (
            JointRequest {
                radius: -1.0,
                ..request(None, [0.0; 3])
            },
            NativeError::UnsupportedJoint(JointUnsupported::Radius),
        ),
        (
            JointRequest {
                name: Some("1bad".into()),
                ..request(None, [0.0; 3])
            },
            NativeError::InvalidName,
        ),
        (
            JointRequest {
                name: Some("joint1".into()),
                ..request(None, [0.0; 3])
            },
            NativeError::NameCollision,
        ),
        (
            JointRequest {
                name: Some("pCube1Shape".into()),
                ..request(None, [0.0; 3])
            },
            NativeError::NameCollision,
        ),
        (
            request(Some(unknown), [0.0; 3]),
            NativeError::Legacy(SceneError::MissingNode(unknown)),
        ),
        (
            request(Some(mesh), [0.0; 3]),
            NativeError::UnsupportedJoint(JointUnsupported::ParentNotJoint),
        ),
        (
            request(Some(zero), [1.0, 0.0, 0.0]),
            NativeError::UnsupportedJoint(JointUnsupported::SingularParent),
        ),
    ] {
        assert_eq!(scene.create_joint(&case), Err(expected), "{case:?}");
        assert_eq!(scene, before);
    }
    // A relative position needs no inverse, so the singular parent still takes a child.
    scene
        .create_joint(&JointRequest {
            relative: true,
            ..request(Some(zero), [1.0, 0.0, 0.0])
        })
        .unwrap();
}

#[test]
fn a_sourced_scene_refuses_joints() {
    let mut binary = super::super::binary::decode(Arc::from(binary_fixture::specimen())).unwrap();
    assert_eq!(
        binary.create_joint(&request(None, [0.0; 3])),
        Err(NativeError::UnsupportedEdit)
    );
    let mut ascii = super::super::ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    assert_eq!(
        ascii.create_joint(&request(None, [0.0; 3])),
        Err(NativeError::ReadOnlyFormat)
    );
}

#[test]
fn the_depth_limit_refuses_the_joint_past_it_and_keeps_ids_free() {
    let (mut scene, nodes) = chain(crate::native_scene::MAX_TRANSFORM_DEPTH);
    let before = scene.clone();
    let (next_id, next_native_id) = (scene.next_id, scene.next_native_id);
    assert_eq!(
        scene.create_joint(&request(nodes.last().copied(), [0.0; 3])),
        Err(NativeError::ResourceLimit)
    );
    assert_eq!(scene, before);
    assert_eq!(
        (scene.next_id, scene.next_native_id),
        (next_id, next_native_id)
    );
    scene.validate().unwrap();
    scene.transform_paths().unwrap();
}

#[test]
fn a_child_rotation_and_scale_sit_under_the_inverse_of_the_parent_scale() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            scale: [2.0, 4.0, 0.5],
            ..NativeTransform::default()
        },
    );
    set(
        &mut scene,
        child,
        NativeTransform {
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 90.0],
            scale: [3.0, 1.0, 1.0],
        },
    );
    let internal = scene.internal_transform(child).unwrap();
    assert_eq!(internal.inverse_parent_scale, Some([0.5, 0.25, 2.0]));
    let expected = DMat4::from_scale(DVec3::new(2.0, 4.0, 0.5))
        * DMat4::from_translation(DVec3::new(1.0, 2.0, 3.0))
        * DMat4::from_scale(DVec3::new(0.5, 0.25, 2.0))
        * DMat4::from_rotation_z(90_f64.to_radians())
        * DMat4::from_scale(DVec3::new(3.0, 1.0, 1.0));
    assert!(world(&scene, child).abs_diff_eq(expected, 1e-12));
    // The translation is under the parent's scale; the segment below it is not.
    assert_near(
        world(&scene, child).transform_point3(DVec3::ZERO),
        DVec3::new(2.0, 8.0, 1.5),
    );
    let linear = DMat3::from_mat4(world(&scene, child));
    assert_near(linear.x_axis, DVec3::new(0.0, 3.0, 0.0));
    assert_near(linear.y_axis, DVec3::new(-1.0, 0.0, 0.0));
    assert_near(linear.z_axis, DVec3::new(0.0, 0.0, 1.0));
}

#[test]
fn a_parent_scale_without_a_finite_inverse_uses_one() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            scale: [0.0, 1e-320, 2.0],
            ..NativeTransform::default()
        },
    );
    let internal = scene.internal_transform(child).unwrap();
    assert_eq!(internal.inverse_parent_scale, Some([1.0, 1.0, 0.5]));
    assert!(internal.matrix().is_some());
    assert!(world(&scene, child).is_finite());
    crate::view_scene::ViewScene::from_native(&scene, 1 << 24).unwrap();
}

#[test]
fn other_nodes_never_carry_an_inverse_parent_scale() {
    let (mut scene, _, node, _) = crate::native_scene::test_fixtures::grouped_shape();
    assert_eq!(
        scene.internal_transform(node).unwrap().inverse_parent_scale,
        None
    );
    joint(&mut scene, None, [0.0; 3]);
    assert_eq!(
        scene.internal_transform(node).unwrap().inverse_parent_scale,
        None
    );
}

#[test]
fn move_keeps_the_translation_above_the_inverse_scale() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            scale: [2.0, 2.0, 2.0],
            ..NativeTransform::default()
        },
    );
    let frame = scene.manipulator_frame(&target(&scene, child)).unwrap();
    assert_near(frame.origin_cm, DVec3::new(2.0, 0.0, 0.0));
    let move_frame = scene.world_move_frame(&target(&scene, child)).unwrap();
    assert_near(
        move_frame.origin_cm(),
        world(&scene, child).transform_point3(DVec3::ZERO),
    );
}

#[test]
fn handle_axes_see_the_inverse_parent_scale() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            rotation: [0.0, 0.0, 90.0],
            scale: [2.0, 4.0, 0.5],
            ..NativeTransform::default()
        },
    );
    let frame = scene.manipulator_frame(&target(&scene, child)).unwrap();
    // The parent's rotation with its scale cancelled, so each axis has unit length.
    assert_near(frame.object_axes[0], DVec3::new(0.0, 1.0, 0.0));
    assert_near(frame.object_axes[1], DVec3::new(-1.0, 0.0, 0.0));
    assert_near(frame.object_axes[2], DVec3::new(0.0, 0.0, 1.0));
}

/// The node's own rotation in the world, with the node's scale taken out.
fn world_rotation(scene: &NativeScene, node: NodeId) -> DMat3 {
    let scale = DVec3::from_array(scene.internal_transform(node).unwrap().scale);
    DMat3::from_mat4(world(scene, node)) * DMat3::from_diagonal(scale.recip())
}

#[test]
fn world_rotate_under_two_negative_parent_scales_turns_about_the_world_axis() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            scale: [-2.0, -2.0, 2.0],
            ..NativeTransform::default()
        },
    );
    let before = world_rotation(&scene, child);
    let selected = target(&scene, child);
    let (frame, _) = scene
        .begin_rotation(&selected, RotateAxis::X, RotateSpace::World)
        .unwrap();
    let degrees = frame.rotation_declared(RotateAxis::X, 30.0, None).unwrap();
    set(
        &mut scene,
        child,
        NativeTransform {
            translation: [1.0, 0.0, 0.0],
            rotation: degrees,
            ..NativeTransform::default()
        },
    );
    let expected = DMat3::from_rotation_x(30_f64.to_radians()) * before;
    let drift = (world_rotation(&scene, child) - expected)
        .to_cols_array()
        .map(f64::abs)
        .into_iter()
        .fold(0.0, f64::max);
    assert!(drift < 1e-9, "drift {drift}");
}

#[test]
fn world_scale_under_two_negative_parent_scales_maps_the_world_axis_through_is() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            rotation: [45.0, 0.0, 0.0],
            scale: [-2.0, -2.0, 2.0],
            ..NativeTransform::default()
        },
    );
    set(
        &mut scene,
        child,
        NativeTransform {
            translation: [1.0, 0.0, 0.0],
            rotation: [45.0, 0.0, 0.0],
            ..NativeTransform::default()
        },
    );
    // The chain above the child's scale is a quarter turn about X once IS is applied, so world
    // Y is the child's Z. Without IS it reads as a half turn about Z and would pick channel Y.
    let (frame, _) = scene
        .begin_scale_frame(&target(&scene, child), ScaleHandle::Y, ScaleSpace::World)
        .unwrap();
    assert_eq!(frame.scale(ScaleHandle::Y, 3.0), Ok([1.0, 1.0, 3.0]));
}

#[test]
fn a_non_uniform_parent_stays_refused_by_world_rotate_and_scale() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let child = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    set(
        &mut scene,
        root,
        NativeTransform {
            scale: [2.0, 1.0, 1.0],
            ..NativeTransform::default()
        },
    );
    let selected = target(&scene, child);
    assert!(
        scene
            .rotation_refusal(&selected, RotateAxis::X, RotateSpace::World)
            .is_some()
    );
    assert!(
        scene
            .scale_refusal(&selected, ScaleHandle::X, ScaleSpace::World)
            .is_some()
    );
    assert_eq!(
        scene.rotation_refusal(&selected, RotateAxis::X, RotateSpace::Object),
        None
    );
}

#[test]
fn deleting_a_joint_removes_everything_below_it_and_prunes_the_table() {
    let (mut scene, nodes) = chain(4);
    let kept = transform_id(&scene, nodes[0]);
    let removed = transform_id(&scene, nodes[2]);
    scene.remove(nodes[1]).unwrap();
    assert_eq!(scene.nodes().len(), 1);
    assert_eq!(scene.dag_edges().len(), 1);
    assert!(scene.joint(kept).is_some());
    assert!(scene.joint(removed).is_none());
    assert!(!scene.session_joints.joints.is_empty());
    scene.validate().unwrap();
    scene.remove(nodes[0]).unwrap();
    assert!(scene.nodes().is_empty() && scene.dag_edges().is_empty());
    assert!(!scene.has_joints());
}

#[test]
fn deleting_a_branch_leaves_its_sibling() {
    let mut scene = NativeScene::default();
    let root = joint(&mut scene, None, [0.0; 3]);
    let left = joint(&mut scene, Some(root), [1.0, 0.0, 0.0]);
    let right = joint(&mut scene, Some(root), [-1.0, 0.0, 0.0]);
    scene.remove(left).unwrap();
    assert_eq!(scene.nodes().len(), 2);
    scene.node(right).unwrap();
    scene.validate().unwrap();
}

#[test]
fn remove_many_skips_ids_removed_with_an_ancestor_in_either_order() {
    let (scene, nodes) = chain(3);
    for ids in [
        vec![nodes[0], nodes[1], nodes[2]],
        vec![nodes[2], nodes[1], nodes[0]],
        vec![nodes[1], nodes[0], nodes[2]],
    ] {
        let mut scene = scene.clone();
        scene.remove_many(&ids).unwrap();
        assert!(scene.nodes().is_empty(), "{ids:?}");
        scene.validate().unwrap();
    }
}

#[test]
fn remove_many_still_refuses_an_unknown_id_after_a_chain() {
    let (mut scene, nodes) = chain(3);
    let unknown = NodeId::new(9999);
    assert_eq!(
        scene.remove_many(&[nodes[0], unknown]),
        Err(NativeError::Legacy(SceneError::MissingNode(unknown)))
    );
}

#[test]
fn delete_rules_for_mesh_hierarchies_are_untouched() {
    let (mut scene, root, child, _) = crate::native_scene::test_fixtures::grouped_shape();
    assert_eq!(
        scene.remove(root),
        Err(NativeError::UnsupportedDelete(
            crate::native_scene::DeleteUnsupported::HasChildren
        ))
    );
    assert_eq!(
        scene.remove(child),
        Err(NativeError::UnsupportedDelete(
            crate::native_scene::DeleteUnsupported::IsChild
        ))
    );
}

#[test]
fn a_radius_past_the_safety_bound_is_refused_before_mutation() {
    let mut scene = NativeScene::default();
    let before = scene.clone();
    // f32-finite values that still overflowed the renderer (3.3e38) and framing (1e37).
    for radius in [3.3e38, 1e37, 1.000001e6] {
        assert_eq!(
            scene.create_joint(&JointRequest {
                radius,
                ..request(None, [0.0; 3])
            }),
            Err(NativeError::UnsupportedJoint(JointUnsupported::Radius)),
            "{radius}"
        );
        assert_eq!(scene, before, "{radius}");
    }
    scene
        .create_joint(&JointRequest {
            radius: 1e6,
            ..request(None, [0.0; 3])
        })
        .expect("the bound itself is allowed");
}

#[test]
fn the_table_participates_in_equality_so_history_sees_a_radius_change() {
    let (scene, nodes) = chain(1);
    let mut changed = scene.clone();
    let id = transform_id(&scene, nodes[0]);
    changed
        .session_joints
        .joints
        .insert(id, Joint { radius: 2.0 });
    assert_ne!(scene, changed);
    let mut other = scene.clone();
    other.session_joints.joints.clear();
    assert_ne!(scene, other);
}

#[test]
fn validate_refuses_each_malformed_table() {
    let (scene, nodes) = chain(2);
    let child = transform_id(&scene, nodes[1]);
    let root = transform_id(&scene, nodes[0]);
    let refused = Err(NativeError::InvalidSessionJoints);

    for radius in [0.0, -1.0, f64::NAN, f64::INFINITY, 3.3e38] {
        let mut bad = scene.clone();
        bad.session_joints.joints.insert(child, Joint { radius });
        assert_eq!(bad.validate(), refused);
    }

    let mut unknown_key = scene.clone();
    unknown_key
        .session_joints
        .joints
        .insert(NativeId(999), Joint { radius: 1.0 });
    assert_eq!(unknown_key.validate(), refused);

    let (mut with_shape, _) = cube();
    let mesh = with_shape.nodes[0].transform_id;
    with_shape
        .session_joints
        .joints
        .insert(mesh, Joint { radius: 1.0 });
    assert_eq!(with_shape.validate(), refused, "shape child");

    let mut plain_parent = scene.clone();
    plain_parent.session_joints.joints.remove(&root);
    assert_eq!(plain_parent.validate(), refused, "non-joint parent");

    let mut plain_child = scene.clone();
    plain_child.session_joints.joints.remove(&child);
    assert_eq!(plain_child.validate(), refused, "non-joint child");

    let mut extras = scene.clone();
    extras.nodes[1].extras.rotate_order = Some(1);
    assert_eq!(extras.validate(), refused);

    let (mut sourced, _) = chain(1);
    let ascii = super::super::ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    sourced.source = ascii.source;
    assert_eq!(sourced.validate(), refused);
}

#[test]
fn a_joint_with_two_incoming_edges_is_refused() {
    let (mut scene, nodes) = chain(3);
    let (root, child) = (
        transform_id(&scene, nodes[0]),
        transform_id(&scene, nodes[2]),
    );
    scene.dag.push(DagEdge {
        parent: Some(root),
        child,
    });
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionJoints));
}

#[test]
fn the_legacy_view_refuses_joints() {
    let (scene, _) = chain(1);
    assert_eq!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView("joints in legacy mesh view"))
    );
}

#[test]
fn edits_reach_joints_through_the_ordinary_node_paths() {
    let (mut scene, nodes) = chain(2);
    scene.set_channel(nodes[1], Channel::RotateZ, 45.0).unwrap();
    scene.rename(nodes[1], "elbow").unwrap();
    assert_eq!(scene.node(nodes[1]).unwrap().name(), "elbow");
    scene.validate().unwrap();
}
