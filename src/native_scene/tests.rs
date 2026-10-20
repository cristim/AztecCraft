use super::test_fixtures::{cube, grouped_shape, shared_shape};
use super::*;

#[test]
fn transform_path_discovery_includes_hidden_groups_and_every_transform_instance() {
    let (mut scene, root, child, _) = test_fixtures::hidden_group();
    let root_id = scene.node(root).unwrap().transform_id;
    let child_id = scene.node(child).unwrap().transform_id;
    scene.dag.push(DagEdge {
        parent: None,
        child: child_id,
    });
    scene.validate().unwrap();
    let before = scene.clone();
    let paths = scene.transform_paths().unwrap();
    assert_eq!(paths.len(), 3);
    assert!(
        paths
            .iter()
            .any(|target| target.node == root && target.path == [root_id])
    );
    for path in [vec![root_id, child_id], vec![child_id]] {
        assert!(
            paths
                .iter()
                .any(|target| target.node == child && target.path == path)
        );
        assert_eq!(
            scene.resolve_transform_path(child, &path).unwrap().id(),
            child
        );
    }
    assert!(scene.resolve_transform_path(root, &[child_id]).is_err());
    assert!(
        scene
            .resolve_transform_path(child, &[root_id, root_id, child_id])
            .is_err()
    );
    assert_eq!(scene, before);
}

#[test]
fn transform_path_discovery_refuses_excess_depth_without_a_partial_result() {
    let (mut scene, root) = test_fixtures::cube();
    let template = scene.nodes[0].clone();
    let mut parent = scene.node(root).unwrap().transform_id;
    for index in 0..MAX_TRANSFORM_DEPTH {
        let mut node = template.clone();
        node.id = NodeId::from_decimal(&(index as u64 + 2).to_string()).unwrap();
        node.transform_id = NativeId(index as u64 + 3);
        node.name = format!("group{index}");
        scene.dag.push(DagEdge {
            parent: Some(parent),
            child: node.transform_id,
        });
        parent = node.transform_id;
        scene.nodes.push(node);
    }
    scene.next_id = MAX_TRANSFORM_DEPTH as u64 + 2;
    scene.next_native_id = MAX_TRANSFORM_DEPTH as u64 + 3;
    scene.validate().unwrap();
    assert!(matches!(
        scene.transform_paths(),
        Err(NativeError::ResourceLimit)
    ));
}

fn retain(scene: &mut NativeScene, bytes: &[u8], records: Vec<Range<usize>>) {
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: bytes.into(),
        format: SourceFormat::Ascii,
        records,
        leaves: Vec::new(),
        appearance: None,
    });
    scene.validate().unwrap();
}

#[test]
fn retained_ascii_lexemes_are_read_only_and_survive_restore() {
    let (mut scene, id) = cube();
    retain(
        &mut scene,
        b"setAttr .tx +0.000;",
        std::iter::once(0..19).collect(),
    );
    scene.nodes[0].channels[0].origin = Some(ScalarOrigin::Ascii { literal: 12..18 });
    let before = scene.clone();
    for value in [0.0, -0.0, 2.5] {
        assert_eq!(
            scene.set_channel(id, Channel::TranslateX, value),
            Err(NativeError::ReadOnlyFormat)
        );
        assert_eq!(scene, before);
    }
    scene.nodes[0].channels[0].value = -0.0;
    assert_ne!(scene, before);
    scene.restore_authoring(before.clone());
    assert_eq!(scene, before);
    let mut deleted = before.clone();
    deleted.source = None;
    assert_ne!(deleted, before);
    deleted = before.clone();
    deleted.nodes[0].channels[0].origin = Some(ScalarOrigin::Ascii { literal: 13..18 });
    assert_ne!(deleted, before);
}

#[test]
fn source_free_identical_bits_are_noops_but_signed_zero_remains_meaningful() {
    let (mut scene, id) = cube();
    let before = scene.clone();
    scene.set_channel(id, Channel::TranslateX, 0.0).unwrap();
    scene.set_transform(id, NativeTransform::default()).unwrap();
    assert_eq!(scene, before);
    scene.set_channel(id, Channel::TranslateX, -0.0).unwrap();
    assert_ne!(scene, before);
    let changed = scene.clone();
    scene
        .set_transform(id, scene.node(id).unwrap().transform())
        .unwrap();
    assert_eq!(scene, changed);
    let mut legacy = Scene::default();
    let id = legacy.create(Primitive::Cube, "legacy name").unwrap();
    let mut promoted = NativeScene::from_legacy(legacy).unwrap();
    let before = promoted.clone();
    promoted
        .set_transform(id, promoted.node(id).unwrap().transform())
        .unwrap();
    assert_eq!(promoted, before);
}

#[test]
fn dependency_unknown_transform_and_dag_effects_refuse_before_mutation() {
    for restriction in ["connection", "lock", "history", "unmapped owner"] {
        let (mut scene, id) = cube();
        scene.nodes[0].restrictions.push(restriction.into());
        let before = scene.clone();
        assert_eq!(
            scene.rename(id, "renamed"),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(scene.remove(id), Err(NativeError::UnsupportedEdit));
        assert_eq!(
            scene.set_channel(id, Channel::TranslateY, 1.0),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(
            scene.extrude_face(scene.shapes[0].id, 1, 1.0),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(scene, before);
    }
    let (mut scene, id) = cube();
    scene.nodes[0].unsupported_transform = vec!["rotationOrder YZX".into(), "shear .5".into()];
    let before = scene.clone();
    assert!(scene.supported_view().is_err());
    assert_eq!(
        scene.set_channel(id, Channel::ScaleX, 2.0),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, before);
    scene.nodes[0].unsupported_transform.clear();
    let other = scene.create(Primitive::Plane, "other").unwrap();
    scene.dag.push(DagEdge {
        parent: Some(scene.node(other).unwrap().transform_id),
        child: scene.shapes[0].id,
    });
    scene.validate().unwrap();
    assert!(scene.supported_view().is_err());
    let shared = scene.shapes[0].id;
    scene.remove(id).unwrap();
    assert!(scene.shape(shared).is_ok());
    let (mut scene, parent) = cube();
    let child = scene.create(Primitive::Plane, "child").unwrap();
    scene.dag.push(DagEdge {
        parent: Some(scene.node(parent).unwrap().transform_id),
        child: scene.node(child).unwrap().transform_id,
    });
    scene.validate().unwrap();
    let before = scene.clone();
    scene.set_channel(parent, Channel::TranslateX, 1.0).unwrap();
    assert_eq!(scene.dag, before.dag);
    assert_eq!(scene.shapes, before.shapes);
}

#[test]
fn angular_degrees_factor_follows_the_declared_unit_and_refuses_others() {
    let mut units = Units::default();
    assert_eq!(units.angular_degrees_factor().unwrap(), 1.0);
    units.angular = AngularUnit::Radian;
    assert_eq!(
        units.angular_degrees_factor().unwrap(),
        180.0 / std::f64::consts::PI
    );
    units.angular = AngularUnit::Unsupported("grad".into());
    assert!(units.angular_degrees_factor().is_err());
}

#[test]
fn unit_conversion_only_changes_derived_view() {
    let (mut scene, id) = cube();
    scene.units = Units {
        linear: LinearUnit::Meter,
        angular: AngularUnit::Radian,
        time: "pal".into(),
    };
    scene
        .set_transform(
            id,
            NativeTransform {
                translation: [1.0, 0.0, 0.0],
                rotation: [std::f64::consts::PI, 0.0, 0.0],
                ..NativeTransform::default()
            },
        )
        .unwrap();
    let before = scene.clone();
    let view = scene.supported_view().unwrap();
    assert_eq!(view.node(id).unwrap().transform().translation.x, 100.0);
    assert_eq!(view.node(id).unwrap().transform().rotation_degrees.x, 180.0);
    assert_eq!(view.node(id).unwrap().mesh().vertices()[0].x, -0.5);
    assert_eq!(scene, before);
    scene.units.linear = LinearUnit::Unsupported("native-token".into());
    scene.validate().unwrap();
    assert_eq!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView("linear unit"))
    );
    assert_eq!(
        scene.units.linear,
        LinearUnit::Unsupported("native-token".into())
    );
}

#[test]
fn retained_ranges_and_exhausted_allocators_are_checked() {
    let (mut scene, _) = cube();
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: vec![0; 4].into(),
        format: SourceFormat::Ascii,
        records: vec![0..3, 2..4],
        leaves: Vec::new(),
        appearance: None,
    });
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: vec![0; 4].into(),
        format: SourceFormat::Ascii,
        records: std::iter::once(0..5).collect(),
        leaves: Vec::new(),
        appearance: None,
    });
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    scene.source = None;
    scene.next_id = u64::MAX;
    scene.validate().unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.create(Primitive::Cube, "another"),
        Err(NativeError::Legacy(SceneError::IdsExhausted))
    );
    assert_eq!(scene, before);
    assert_eq!(scene.next_id, before.next_id);
    scene.next_id = 10;
    scene.next_native_id = u64::MAX - 1;
    let before = scene.clone();
    assert_eq!(
        scene.create(Primitive::Plane, "another"),
        Err(NativeError::Legacy(SceneError::IdsExhausted))
    );
    assert_eq!(scene, before);
    assert_eq!(scene.next_native_id, before.next_native_id);
}

#[test]
fn grouped_transform_and_pivots_preserve_native_precision_without_baking() {
    let (mut scene, root, child, shape) = grouped_shape();
    scene.nodes[1].rotate_pivot = [1.000000000001, -0.0, 2.0].map(Scalar::new);
    scene.nodes[1].scale_pivot = [-2.000000000001, 3.0, 4.0].map(Scalar::new);
    let before = scene.clone();
    scene
        .set_channel(root, Channel::TranslateY, 4.000000000001)
        .unwrap();
    scene
        .set_channel(child, Channel::RotateZ, 17.000000000001)
        .unwrap();
    assert_eq!(scene.nodes[1].rotate_pivot, before.nodes[1].rotate_pivot);
    assert_eq!(scene.nodes[1].scale_pivot, before.nodes[1].scale_pivot);
    assert_eq!(scene.shape(shape), before.shape(shape));
    assert_eq!(scene.dag, before.dag);
    assert_eq!(scene.source, before.source);
    assert!(matches!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView(_))
    ));
    scene.restore_authoring(before.clone());
    assert_eq!(scene, before);
    scene.nodes[1].rotate_pivot[1].value = 0.0;
    assert_ne!(scene, before);
    scene.nodes[1].scale_pivot[0].value = f64::INFINITY;
    assert_eq!(scene.validate(), Err(NativeError::NonFinite));
}

#[test]
fn shared_shape_extrusion_changes_one_owner_and_checks_the_other_instance() {
    let (mut scene, first, second, shape) = shared_shape();
    let links = scene.dag.clone();
    let shape_uuid = scene.shape(shape).unwrap().uuid();
    let points = scene.shape(shape).unwrap().mesh().points().clone();
    assert_eq!(scene.shapes.len(), 1);
    assert!(scene.supported_view().is_err());
    scene.extrude_face(shape, 1, 0.0).unwrap();
    assert_eq!(scene.shapes.len(), 1);
    assert_eq!(scene.dag, links);
    assert_eq!(scene.shape(shape).unwrap().uuid(), shape_uuid);
    assert_eq!(scene.shape(shape).unwrap().mesh().faces().len(), 10);
    let (Points::Float3(old), Points::Float3(new)) =
        (&points, scene.shape(shape).unwrap().mesh().points())
    else {
        panic!()
    };
    assert_eq!(Points::Float3(new[..old.len()].into()), points);
    let second_index = scene
        .nodes
        .iter()
        .position(|node| node.id == second)
        .unwrap();
    scene.nodes[second_index]
        .unsupported_transform
        .push("unproved ancestor effect".into());
    let target = FaceTarget {
        transform: first,
        shape,
        path: vec![scene.node(first).unwrap().transform_id, shape],
        face_index: 1,
    };
    scene.resolve_face_target(&target).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.extrude_face(shape, 1, 1.0),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, before);
}

#[test]
fn root_leaf_removal_preserves_shared_shapes_and_refuses_groups_or_source_rewrites() {
    let (mut scene, first, second, shape) = shared_shape();
    let preserved = scene.shape(shape).unwrap().clone();
    scene.remove(first).unwrap();
    assert_eq!(scene.shape(shape).unwrap(), &preserved);
    assert_eq!(scene.dag.len(), 2);
    scene.validate().unwrap();
    scene.remove(second).unwrap();
    assert!(scene.shapes.is_empty());
    assert!(scene.dag.is_empty());
    scene.validate().unwrap();
    let (mut scene, root, child, _) = grouped_shape();
    let before = scene.clone();
    assert_eq!(scene.remove(root), Err(NativeError::UnsupportedEdit));
    assert_eq!(scene.remove(child), Err(NativeError::UnsupportedEdit));
    assert_eq!(scene, before);
    let (mut scene, id) = cube();
    retain(&mut scene, b"original", std::iter::once(0..8).collect());
    let before = scene.clone();
    assert_eq!(scene.remove(id), Err(NativeError::ReadOnlyFormat));
    assert_eq!(
        scene.rename(id, "replacement"),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.create(Primitive::Cube, "new"),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.extrude_face(scene.shapes[0].id, 1, 1.0),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(scene, before);
    assert_eq!(scene.next_id, before.next_id);
    assert_eq!(scene.next_native_id, before.next_native_id);
}

#[test]
fn face_target_validates_full_native_identity_path_and_original_face_bounds() {
    let (scene, root, child, shape) = grouped_shape();
    let target = FaceTarget {
        transform: child,
        shape,
        path: vec![
            scene.node(root).unwrap().transform_id,
            scene.node(child).unwrap().transform_id,
            shape,
        ],
        face_index: 1,
    };
    assert_eq!(scene.resolve_face_target(&target).unwrap().id(), shape);
    let decimal = shape.to_decimal();
    assert_eq!(NativeId::from_decimal(&decimal).unwrap(), shape);
    for path in [
        vec![],
        vec![shape],
        vec![scene.node(child).unwrap().transform_id, shape],
        vec![scene.node(root).unwrap().transform_id, shape],
        vec![target.path[0], target.path[1], target.path[1], shape],
    ] {
        let bad = FaceTarget {
            path,
            ..target.clone()
        };
        assert_eq!(
            scene.resolve_face_target(&bad),
            Err(NativeError::InvalidInstancePath)
        );
    }
    let bad_transform = FaceTarget {
        transform: root,
        ..target.clone()
    };
    assert_eq!(
        scene.resolve_face_target(&bad_transform),
        Err(NativeError::InvalidInstancePath)
    );
    let bad_face = FaceTarget {
        face_index: 6,
        ..target
    };
    assert_eq!(
        scene.resolve_face_target(&bad_face),
        Err(SceneError::MissingFace(6).into())
    );
}

#[test]
fn vertex_and_edge_targets_validate_identity_path_and_original_bounds() {
    let (scene, root, child, shape) = grouped_shape();
    let path = vec![
        scene.node(root).unwrap().transform_id,
        scene.node(child).unwrap().transform_id,
        shape,
    ];
    let mesh = scene.shape(shape).unwrap().mesh();
    let (points, edges) = (mesh.point_count(), mesh.edges().len());
    let vertex = |path: Vec<NativeId>, transform, shape, vertex_index| VertexTarget {
        transform,
        shape,
        path,
        vertex_index,
    };
    let edge = |path: Vec<NativeId>, transform, shape, edge_index| EdgeTarget {
        transform,
        shape,
        path,
        edge_index,
    };
    for index in [0, points - 1] {
        let target = vertex(path.clone(), child, shape, index);
        assert_eq!(scene.resolve_vertex_target(&target).unwrap().id(), shape);
    }
    for index in [0, edges - 1] {
        let target = edge(path.clone(), child, shape, index);
        assert_eq!(scene.resolve_edge_target(&target).unwrap().id(), shape);
    }
    for index in [points, usize::MAX] {
        assert_eq!(
            scene.resolve_vertex_target(&vertex(path.clone(), child, shape, index)),
            Err(SceneError::MissingVertex(index).into())
        );
    }
    for index in [edges, usize::MAX] {
        assert_eq!(
            scene.resolve_edge_target(&edge(path.clone(), child, shape, index)),
            Err(SceneError::MissingEdge(index).into())
        );
    }
    let other = NativeId::from_decimal("999999").unwrap();
    let bad_paths = [
        vec![],
        vec![shape],
        path[1..].to_vec(),
        vec![path[0], shape],
        vec![path[0], path[1], path[1], shape],
        vec![path[0], path[1], shape, shape],
    ];
    for bad in bad_paths {
        assert_eq!(
            scene.resolve_vertex_target(&vertex(bad.clone(), child, shape, 0)),
            Err(NativeError::InvalidInstancePath)
        );
        assert_eq!(
            scene.resolve_edge_target(&edge(bad, child, shape, 0)),
            Err(NativeError::InvalidInstancePath)
        );
    }
    assert_eq!(
        scene.resolve_vertex_target(&vertex(path.clone(), root, shape, 0)),
        Err(NativeError::InvalidInstancePath)
    );
    assert_eq!(
        scene.resolve_edge_target(&edge(path.clone(), root, shape, 0)),
        Err(NativeError::InvalidInstancePath)
    );
    assert_eq!(
        scene.resolve_vertex_target(&vertex(path.clone(), child, other, 0)),
        Err(NativeError::MissingShape(other))
    );
    assert_eq!(
        scene.resolve_edge_target(&edge(path.clone(), child, other, 0)),
        Err(NativeError::MissingShape(other))
    );
    let (mut scene, gone) = cube();
    let shape = scene.shapes[0].id;
    let path = vec![scene.node(gone).unwrap().transform_id, shape];
    scene.remove(gone).unwrap();
    assert_eq!(
        scene.resolve_vertex_target(&vertex(path.clone(), gone, shape, 0)),
        Err(SceneError::MissingNode(gone).into())
    );
    assert_eq!(
        scene.resolve_edge_target(&edge(path, gone, shape, 0)),
        Err(SceneError::MissingNode(gone).into())
    );
}

#[test]
fn invalid_dag_cycles_duplicates_roles_and_dangling_ids_are_rejected() {
    let (original, root, child, shape) = grouped_shape();
    let root_native = original.node(root).unwrap().transform_id;
    let child_native = original.node(child).unwrap().transform_id;
    for edge in [
        DagEdge {
            parent: Some(child_native),
            child: root_native,
        },
        DagEdge {
            parent: None,
            child: root_native,
        },
        DagEdge {
            parent: None,
            child: shape,
        },
        DagEdge {
            parent: Some(shape),
            child: root_native,
        },
        DagEdge {
            parent: Some(NativeId(u64::MAX)),
            child: child_native,
        },
        DagEdge {
            parent: Some(root_native),
            child: NativeId(u64::MAX),
        },
    ] {
        let mut scene = original.clone();
        scene.dag.push(edge);
        assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
    }
    let mut scene = original;
    scene.dag.retain(|edge| edge.child != child_native);
    assert_eq!(scene.validate(), Err(NativeError::InvalidIdentity));
}

#[test]
fn unproved_ancestor_descendant_and_shape_effects_refuse_numeric_edits() {
    let (original, root, child, _) = grouped_shape();
    for (index, target) in [(0, child), (1, root)] {
        let mut scene = original.clone();
        scene.nodes[index]
            .unsupported_transform
            .push("unproved transform effect".into());
        let before = scene.clone();
        assert_eq!(
            scene.set_channel(target, Channel::TranslateX, 2.0),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(scene, before);
    }
    let mut scene = original;
    scene.shapes[0]
        .restrictions
        .push("unmapped geometry dependency".into());
    let before = scene.clone();
    assert_eq!(
        scene.set_transform(
            root,
            NativeTransform {
                scale: [2.0; 3],
                ..NativeTransform::default()
            }
        ),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, before);
}

#[test]
fn declared_extrusion_distance_converts_once_without_rescaling_stored_geometry() {
    let (mut scene, _) = cube();
    scene.units.linear = LinearUnit::Meter;
    let shape = scene.shapes[0].id;
    let old_points = scene.shapes[0].mesh.points().clone();
    scene.extrude_face(shape, 1, 1.0).unwrap();
    let Points::Float3(points) = scene.shapes[0].mesh.points() else {
        panic!()
    };
    assert_eq!(points[8][2], 100.5);
    assert_eq!(Points::Float3(points[..8].into()), old_points);
    scene.units.linear = LinearUnit::Unsupported("unproved".into());
    let before = scene.clone();
    assert_eq!(
        scene.extrude_face(shape, 1, 1.0),
        Err(NativeError::UnsupportedLinearUnit)
    );
    assert_eq!(scene, before);
}

#[test]
fn each_pivot_independently_refuses_the_restricted_view_and_validates_finite_bits() {
    for rotate in [true, false] {
        let (mut scene, _) = cube();
        scene.supported_view().unwrap();
        let pivot = if rotate {
            &mut scene.nodes[0].rotate_pivot
        } else {
            &mut scene.nodes[0].scale_pivot
        };
        pivot[0].value = 0.125;
        scene.validate().unwrap();
        assert_eq!(
            scene.supported_view(),
            Err(NativeError::UnsupportedView(
                "DAG, transform or dependencies"
            ))
        );
        let pivot = if rotate {
            &mut scene.nodes[0].rotate_pivot
        } else {
            &mut scene.nodes[0].scale_pivot
        };
        pivot[0].value = -0.0;
        let signed_zero = scene.clone();
        let pivot = if rotate {
            &mut scene.nodes[0].rotate_pivot
        } else {
            &mut scene.nodes[0].scale_pivot
        };
        pivot[0].value = 0.0;
        assert_ne!(scene, signed_zero);
        scene.supported_view().unwrap();
        for value in [f64::INFINITY, f64::NAN] {
            let pivot = if rotate {
                &mut scene.nodes[0].rotate_pivot
            } else {
                &mut scene.nodes[0].scale_pivot
            };
            pivot[2].value = value;
            assert_eq!(scene.validate(), Err(NativeError::NonFinite));
        }
    }
}

fn shape_path(scene: &NativeScene, transform: NodeId, shape: NativeId) -> Vec<NativeId> {
    vec![scene.node(transform).unwrap().transform_id(), shape]
}

#[test]
fn native_path_order_is_numeric_and_admission_preserves_alternate_instances() {
    assert!(NativeId::from_decimal("2").unwrap() < NativeId::from_decimal("10").unwrap());
    let (scene, first, second, shape) = shared_shape();
    let before = scene.clone();
    for transform in [first, second] {
        let path = shape_path(&scene, transform, shape);
        let admission = scene.geometry_view_admission(&path).unwrap();
        assert!(admission.visible());
        assert!(!admission.appearance_incomplete());
        let target = FaceTarget {
            transform,
            shape,
            path,
            face_index: 0,
        };
        assert_eq!(scene.resolve_face_target(&target).unwrap().id(), shape);
    }
    assert_eq!(scene, before);
}

#[test]
fn admission_checks_all_path_facts_even_when_hidden_and_ignores_other_instances() {
    use super::test_fixtures::{hidden_group, hidden_shape, unproved_group};
    let (mut scene, root, child, shape) = hidden_group();
    let path = vec![
        scene.node(root).unwrap().transform_id(),
        scene.node(child).unwrap().transform_id(),
        shape,
    ];
    assert!(!scene.geometry_view_admission(&path).unwrap().visible());
    scene.shapes[0].view.local_visibility = None;
    assert_eq!(
        scene.geometry_view_admission(&path),
        Err(NativeError::UnsupportedView("unproved local visibility"))
    );
    let (scene, id) = hidden_shape();
    assert!(
        !scene
            .geometry_view_admission(&shape_path(&scene, id, scene.shapes[0].id))
            .unwrap()
            .visible()
    );
    let (scene, root, child, shape) = unproved_group();
    let before = scene.clone();
    let path = vec![
        scene.node(root).unwrap().transform_id(),
        scene.node(child).unwrap().transform_id(),
        shape,
    ];
    assert_eq!(
        scene.geometry_view_admission(&path),
        Err(NativeError::UnsupportedView("unproved geometry evaluation"))
    );
    assert_eq!(scene, before);
    let (mut scene, first, second, shape) = shared_shape();
    scene.nodes[1].view.geometry = None;
    assert!(
        scene
            .geometry_view_admission(&shape_path(&scene, first, shape))
            .unwrap()
            .visible()
    );
    assert!(
        scene
            .geometry_view_admission(&shape_path(&scene, second, shape))
            .is_err()
    );
}

#[test]
fn unknown_visibility_and_geometry_refuse_without_using_edit_permission_or_source() {
    let (mut scene, id) = cube();
    let path = shape_path(&scene, id, scene.shapes[0].id);
    for shape in [false, true] {
        let mut unknown = scene.clone();
        let facts = if shape {
            &mut unknown.shapes[0].view
        } else {
            &mut unknown.nodes[0].view
        };
        facts.local_visibility = None;
        let before = unknown.clone();
        assert_eq!(
            unknown.geometry_view_admission(&path),
            Err(NativeError::UnsupportedView("unproved local visibility"))
        );
        assert_eq!(unknown, before);
        let facts = if shape {
            &mut unknown.shapes[0].view
        } else {
            &mut unknown.nodes[0].view
        };
        facts.local_visibility = Some(true);
        facts.geometry = None;
        assert_eq!(
            unknown.geometry_view_admission(&path),
            Err(NativeError::UnsupportedView("unproved geometry evaluation"))
        );
    }
    retain(
        &mut scene,
        b"appearance metadata",
        std::iter::once(0..19).collect(),
    );
    scene.shapes[0]
        .restrictions
        .push("unproved UV update".into());
    let admission = scene.geometry_view_admission(&path).unwrap();
    assert!(admission.visible());
    assert!(admission.appearance_incomplete());
    scene.nodes[0]
        .unsupported_transform
        .push("unproved shear".into());
    assert_eq!(
        scene.geometry_view_admission(&path),
        Err(NativeError::UnsupportedView("unproved transform effects"))
    );
}

#[test]
fn admission_and_face_resolution_share_exact_path_validation() {
    let (scene, root, child, shape) = grouped_shape();
    for path in [
        vec![],
        vec![shape],
        vec![scene.node(child).unwrap().transform_id(), shape],
        vec![scene.node(root).unwrap().transform_id(), shape],
    ] {
        assert_eq!(
            scene.geometry_view_admission(&path),
            Err(NativeError::InvalidInstancePath)
        );
        assert_eq!(
            scene.resolve_face_target(&FaceTarget {
                transform: child,
                shape,
                path,
                face_index: 0
            }),
            Err(NativeError::InvalidInstancePath)
        );
    }
    let (scene, id) = super::test_fixtures::pivoted_shape();
    assert!(
        scene
            .geometry_view_admission(&shape_path(&scene, id, scene.shapes[0].id))
            .unwrap()
            .visible()
    );
}

#[test]
fn instance_queries_do_not_repeat_whole_scene_validation() {
    let (scene, root, child, shape) = grouped_shape();
    let root_native = scene.node(root).unwrap().transform_id();
    let child_native = scene.node(child).unwrap().transform_id();
    let target = FaceTarget {
        transform: child,
        shape,
        path: vec![root_native, child_native, shape],
        face_index: 0,
    };
    let before = FULL_VALIDATIONS.with(|count| count.get());
    assert!(
        scene
            .geometry_view_admission(&target.path)
            .unwrap()
            .visible()
    );
    scene.resolve_face_target(&target).unwrap();
    for path in [
        vec![],
        vec![shape],
        vec![root_native, shape],
        vec![root_native, child_native],
        vec![root_native, child_native, child_native, shape],
    ] {
        assert_eq!(
            scene.geometry_view_admission(&path),
            Err(NativeError::InvalidInstancePath)
        );
        assert_eq!(
            scene.resolve_face_target(&FaceTarget {
                path,
                ..target.clone()
            }),
            Err(NativeError::InvalidInstancePath)
        );
    }
    assert_eq!(FULL_VALIDATIONS.with(|count| count.get()), before);
}

#[test]
fn path_roles_are_checked_without_revalidating_unrelated_scene_data() {
    let (mut scene, first) = cube();
    let second = scene.create(Primitive::Cube, "second").unwrap();
    let first_native = scene.node(first).unwrap().transform_id();
    let second_native = scene.node(second).unwrap().transform_id();
    let first_shape = scene.shapes[0].id;
    let second_shape = scene.shapes[1].id;
    scene.dag.push(DagEdge {
        parent: Some(first_shape),
        child: second_native,
    });
    let target = FaceTarget {
        transform: second,
        shape: second_shape,
        path: vec![first_native, first_shape, second_native, second_shape],
        face_index: 0,
    };
    let before = FULL_VALIDATIONS.with(|count| count.get());
    assert_eq!(
        scene.resolve_face_target(&target),
        Err(NativeError::InvalidInstancePath)
    );
    assert_eq!(
        scene.geometry_view_admission(&target.path),
        Err(NativeError::InvalidInstancePath)
    );
    assert_eq!(FULL_VALIDATIONS.with(|count| count.get()), before);
}
#[test]
fn binary_candidate_finishes_once_preserving_source_and_runtime_identity() {
    let (mut candidate, id) = test_fixtures::cube();
    candidate.nodes[0].transform_uuid = None;
    candidate.shapes[0].uuid = None;
    candidate.nodes[0].name = "native:arbitrary exact name".into();
    candidate.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: std::sync::Arc::from(&b"opaque original identity"[..]),
        format: SourceFormat::Binary,
        records: std::iter::once(0..24).collect(),
        leaves: Vec::new(),
        appearance: None,
    });
    let bytes = candidate.source.as_ref().unwrap().bytes.clone();
    candidate.next_id = u64::MAX;
    candidate.next_native_id = 0;
    FULL_VALIDATIONS.with(|v| v.set(0));
    let scene = NativeScene::from_binary_candidate(candidate.clone()).unwrap();
    FULL_VALIDATIONS.with(|v| assert_eq!(v.get(), 1));
    assert!(std::sync::Arc::ptr_eq(
        &bytes,
        &scene.source.as_ref().unwrap().bytes
    ));
    assert_eq!(
        scene.node(id).unwrap().name(),
        "native:arbitrary exact name"
    );
    assert_eq!(scene.next_id, id.value() + 1);
    assert_eq!(scene.next_native_id, scene.shapes[0].id.0 + 1);
    assert_eq!(scene, candidate);
    let empty = NativeScene {
        source: candidate.source.clone(),
        next_id: 0,
        next_native_id: u64::MAX,
        ..NativeScene::default()
    };
    let empty = NativeScene::from_binary_candidate(empty).unwrap();
    assert_eq!((empty.next_id, empty.next_native_id), (1, 1));
    for mut nested in [
        test_fixtures::grouped_shape().0,
        test_fixtures::shared_shape().0,
    ] {
        for node in &mut nested.nodes {
            node.transform_uuid = None;
        }
        for shape in &mut nested.shapes {
            shape.uuid = None;
        }
        nested.source = candidate.source.clone();
        let before = nested.clone();
        let finished = NativeScene::from_binary_candidate(nested).unwrap();
        assert_eq!(finished, before);
        assert_eq!(finished.shapes[0].mesh, before.shapes[0].mesh);
    }
    for change in 0..6 {
        let mut invalid = candidate.clone();
        match change {
            0 => invalid.source = None,
            1 => invalid.source.as_mut().unwrap().format = SourceFormat::Ascii,
            2 => invalid.nodes[0].transform_uuid = Some(Uuid::nil()),
            3 => invalid.dag.clear(),
            4 => invalid.nodes[0].transform_id = NativeId(u64::MAX),
            _ => invalid.nodes[0].id = NodeId::from_decimal("18446744073709551615").unwrap(),
        }
        assert!(NativeScene::from_binary_candidate(invalid).is_err());
    }
}

#[test]
fn local_visibility_reports_explicit_values_and_none_when_unproved() {
    use super::test_fixtures::{hidden_group, hidden_shape};
    let (scene, id) = hidden_shape();
    assert_eq!(scene.shapes()[0].local_visibility(), Some(false));
    assert_eq!(scene.node(id).unwrap().local_visibility(), Some(true));
    let (mut scene, root, child, _) = hidden_group();
    assert_eq!(scene.node(root).unwrap().local_visibility(), Some(false));
    assert_eq!(scene.node(child).unwrap().local_visibility(), Some(true));
    scene.nodes[0].view.local_visibility = None;
    scene.shapes[0].view.local_visibility = None;
    assert_eq!(scene.node(root).unwrap().local_visibility(), None);
    assert_eq!(scene.shapes()[0].local_visibility(), None);
}

#[test]
fn root_transform_resolves_the_top_of_any_valid_path_and_refuses_others() {
    let (scene, root) = test_fixtures::cube();
    let id = scene.node(root).unwrap().transform_id;
    assert_eq!(scene.root_transform(&[id]), Ok(root));
    assert_eq!(
        scene.root_transform(&[]),
        Err(NativeError::InvalidInstancePath)
    );
    assert_eq!(
        scene.root_transform(&[NativeId(u64::MAX)]),
        Err(NativeError::InvalidInstancePath)
    );
    let (scene, root, child, _) = grouped_shape();
    let root_id = scene.node(root).unwrap().transform_id;
    let child_id = scene.node(child).unwrap().transform_id;
    assert_eq!(scene.root_transform(&[root_id, child_id]), Ok(root));
    assert_eq!(
        scene.root_transform(&[child_id]),
        Err(NativeError::InvalidInstancePath)
    );
    let too_deep = vec![root_id; MAX_TRANSFORM_DEPTH + 1];
    assert_eq!(
        scene.root_transform(&too_deep),
        Err(NativeError::ResourceLimit)
    );
}
