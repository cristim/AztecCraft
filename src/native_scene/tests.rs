use super::test_fixtures::{chain_of_three, cube, grouped_shape, shared_shape};
use super::*;

#[test]
fn an_invalid_instance_path_error_names_no_target_kind() {
    assert_eq!(
        NativeError::InvalidInstancePath.to_string(),
        "target does not match a valid full instance path"
    );
}

#[test]
fn a_refused_multi_delete_names_the_refusing_node() {
    let (mut scene, _) = cube();
    let first = scene.create(Primitive::Cube, "first").unwrap();
    let bad = scene.create(Primitive::Cube, "bad").unwrap();
    let last = scene.create(Primitive::Cube, "last").unwrap();
    test_fixtures::restrict_transform(&mut scene, bad);
    let before = scene.clone();

    let error = scene.remove_many(&[first, bad, last]).unwrap_err();

    assert_eq!(
        error,
        NativeError::DeleteMemberRefused {
            node: "bad".into(),
            reason: Box::new(NativeError::UnsupportedEdit),
        }
    );
    assert_eq!(
        error.to_string(),
        "bad: native ownership or edit effects are unsupported"
    );
    assert_eq!(scene, before);
}

#[test]
fn a_refusal_name_is_bounded_on_a_character_boundary_without_control_characters() {
    let plain = "a".repeat(MAX_REFUSAL_NAME_BYTES);
    assert_eq!(refusal_name(&plain), plain);
    assert_eq!(refusal_name(&format!("{plain}a")), plain);
    let wide = format!("{}é", "a".repeat(MAX_REFUSAL_NAME_BYTES - 1));
    assert_eq!(refusal_name(&wide), "a".repeat(MAX_REFUSAL_NAME_BYTES - 1));
    assert_eq!(refusal_name("a\nb\u{7}c"), "abc");
    assert_eq!(refusal_name(""), "");
}

#[test]
fn a_refused_delete_of_one_distinct_node_keeps_the_plain_error() {
    let (mut scene, bad) = cube();
    test_fixtures::restrict_transform(&mut scene, bad);
    assert_eq!(
        scene.remove_many(&[bad, bad]),
        Err(NativeError::UnsupportedEdit)
    );
}

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

/// The pre-index algorithm, kept verbatim as the equality oracle for `transform_paths`.
fn transform_paths_oracle(scene: &NativeScene) -> Result<Vec<TransformTarget>, NativeError> {
    let mut paths = Vec::<TransformTarget>::new();
    for root in scene.dag.iter().filter(|edge| edge.parent.is_none()) {
        let node = scene
            .nodes
            .iter()
            .find(|node| node.transform_id == root.child)
            .ok_or(NativeError::InvalidInstancePath)?;
        if paths.len() == MAX_TRANSFORM_PATHS {
            return Err(NativeError::ResourceLimit);
        }
        paths
            .try_reserve(1)
            .map_err(|_| NativeError::ResourceLimit)?;
        let mut path = Vec::new();
        path.try_reserve_exact(1)
            .map_err(|_| NativeError::ResourceLimit)?;
        path.push(root.child);
        paths.push(TransformTarget {
            node: node.id,
            path,
        });
    }
    let mut identities = paths.len();
    let mut cursor = 0;
    while cursor < paths.len() {
        let parent = *paths[cursor]
            .path
            .last()
            .ok_or(NativeError::InvalidInstancePath)?;
        for edge in scene.dag.iter().filter(|edge| edge.parent == Some(parent)) {
            let Some(node) = scene
                .nodes
                .iter()
                .find(|node| node.transform_id == edge.child)
            else {
                continue;
            };
            let depth = paths[cursor]
                .path
                .len()
                .checked_add(1)
                .ok_or(NativeError::ResourceLimit)?;
            identities = identities
                .checked_add(depth)
                .ok_or(NativeError::ResourceLimit)?;
            if depth > MAX_TRANSFORM_DEPTH
                || paths.len() == MAX_TRANSFORM_PATHS
                || identities > MAX_PATH_IDENTITIES
            {
                return Err(NativeError::ResourceLimit);
            }
            let mut path = Vec::new();
            path.try_reserve_exact(depth)
                .map_err(|_| NativeError::ResourceLimit)?;
            path.extend_from_slice(&paths[cursor].path);
            path.push(edge.child);
            paths
                .try_reserve(1)
                .map_err(|_| NativeError::ResourceLimit)?;
            paths.push(TransformTarget {
                node: node.id,
                path,
            });
        }
        cursor += 1;
    }
    paths.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(paths)
}

fn flat_cubes(count: usize) -> NativeScene {
    let mut legacy = Scene::default();
    for index in 0..count {
        legacy
            .create(Primitive::Cube, &format!("cube_{index:04}"))
            .unwrap();
    }
    NativeScene::from_legacy(legacy).unwrap()
}

fn transform_ids(scene: &NativeScene) -> Vec<NativeId> {
    scene.nodes.iter().map(|node| node.transform_id).collect()
}

fn instance(scene: &mut NativeScene, parent: NativeId, child: NativeId, keep_root: bool) {
    if !keep_root {
        scene
            .dag
            .retain(|edge| !(edge.parent.is_none() && edge.child == child));
    }
    scene.dag.push(DagEdge {
        parent: Some(parent),
        child,
    });
}

fn assert_matches_oracle(scene: &NativeScene) -> Vec<TransformTarget> {
    let paths = scene.transform_paths().unwrap();
    assert_eq!(paths, transform_paths_oracle(scene).unwrap());
    paths
}

#[test]
fn transform_paths_equal_the_oracle_for_an_empty_scene() {
    let scene = NativeScene::from_legacy(Scene::default()).unwrap();
    assert_eq!(assert_matches_oracle(&scene), vec![]);
}

#[test]
fn transform_paths_equal_the_oracle_for_a_flat_scene() {
    let scene = flat_cubes(1500);
    assert_eq!(assert_matches_oracle(&scene).len(), 1500);
}

#[test]
fn transform_paths_equal_the_oracle_for_nested_instancing_in_any_dag_order() {
    let mut scene = flat_cubes(300);
    let t = transform_ids(&scene);
    for i in 1..=10 {
        instance(&mut scene, t[0], t[i], false);
    }
    for i in 11..=60 {
        instance(&mut scene, t[1 + (i - 11) % 10], t[i], false);
    }
    for i in 61..=100 {
        instance(&mut scene, t[11 + (i - 61) % 50], t[i], false);
    }
    for i in 101..=200 {
        instance(&mut scene, t[1 + i % 10], t[i], false);
        instance(&mut scene, t[11 + i % 50], t[i], true);
    }
    // A group that has its own children is itself instanced under a second parent.
    instance(&mut scene, t[3], t[2], true);
    scene.validate().unwrap();
    let paths = assert_matches_oracle(&scene);
    let count = |index: usize| {
        paths
            .iter()
            .filter(|target| target.node == scene.nodes[index].id)
            .count()
    };
    assert!(count(2) > 1);
    assert!(count(12) > 1, "a child of the instanced group");
    for rotation in [1, 97] {
        scene.dag.rotate_left(rotation);
        assert_eq!(assert_matches_oracle(&scene), paths);
    }
}

#[test]
fn transform_paths_accept_exactly_the_path_cap_and_refuse_one_more_root() {
    let at_cap = flat_cubes(MAX_TRANSFORM_PATHS);
    at_cap.validate().unwrap();
    assert_eq!(assert_matches_oracle(&at_cap).len(), MAX_TRANSFORM_PATHS);
    let over = flat_cubes(MAX_TRANSFORM_PATHS + 1);
    over.validate().unwrap();
    assert!(matches!(
        over.transform_paths(),
        Err(NativeError::ResourceLimit)
    ));
    assert_eq!(
        transform_paths_oracle(&over),
        Err(NativeError::ResourceLimit)
    );
}

#[test]
fn transform_paths_refuse_an_instance_edge_past_the_path_cap() {
    let mut scene = flat_cubes(MAX_TRANSFORM_PATHS);
    let t = transform_ids(&scene);
    instance(&mut scene, t[0], t[1], true);
    scene.validate().unwrap();
    assert!(matches!(
        scene.transform_paths(),
        Err(NativeError::ResourceLimit)
    ));
    assert_eq!(
        transform_paths_oracle(&scene),
        Err(NativeError::ResourceLimit)
    );
}

#[test]
fn transform_paths_refuse_excess_path_identities_below_the_other_caps() {
    let mut scene = flat_cubes(MAX_TRANSFORM_DEPTH + 1);
    let t = transform_ids(&scene);
    instance(&mut scene, t[0], t[2], false);
    for group in 3..t.len() {
        instance(&mut scene, t[group - 1], t[group], false);
    }
    scene.validate().unwrap();
    // One root over the chain: the deepest path is exactly at the depth cap and the path count is
    // far under its cap, so neither can be what fails below.
    assert_eq!(scene.transform_paths().unwrap().len(), t.len());
    // The second root shares the chain: 2 roots, then each chain path adds its depth, twice.
    instance(&mut scene, t[1], t[2], true);
    scene.validate().unwrap();
    let identities = 2 + 2 * (2..t.len()).sum::<usize>();
    assert!(identities > MAX_PATH_IDENTITIES);
    assert!(2 * t.len() - 2 < MAX_TRANSFORM_PATHS);
    assert!(matches!(
        scene.transform_paths(),
        Err(NativeError::ResourceLimit)
    ));
    assert_eq!(
        transform_paths_oracle(&scene),
        Err(NativeError::ResourceLimit)
    );
}

fn retain(scene: &mut NativeScene, bytes: &[u8], records: Vec<Range<usize>>) {
    scene.source = Some(SourceRetention {
        has_removals: false,
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
        has_removals: false,
        sites: Vec::new(),
        bytes: vec![0; 4].into(),
        format: SourceFormat::Ascii,
        records: vec![0..3, 2..4],
        leaves: Vec::new(),
        appearance: None,
    });
    assert_eq!(scene.validate(), Err(NativeError::InvalidRetention));
    scene.source = Some(SourceRetention {
        has_removals: false,
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
fn remove_validates_the_scene_before_any_mutation() {
    let (mut scene, id) = cube();
    scene.nodes[0].rotate_pivot[0].value = f64::NAN;
    let before = scene.clone();
    assert_eq!(scene.remove(id), Err(NativeError::NonFinite));
    assert_eq!(scene, before);
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
fn removal_preserves_shared_shapes_and_refuses_source_rewrites() {
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
    scene.remove(child).unwrap();
    scene.validate().unwrap();
    assert_eq!((scene.nodes.len(), scene.shapes.len()), (1, 0));
    scene.remove(root).unwrap();
    assert!(scene.nodes.is_empty() && scene.shapes.is_empty() && scene.dag.is_empty());
    scene.validate().unwrap();
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
fn a_middle_node_goes_with_its_descendants_and_leaves_its_parent() {
    let (mut scene, root, mid, _) = chain_of_three();
    scene.remove(mid).unwrap();
    scene.validate().unwrap();
    assert_eq!(scene.nodes.len(), 1);
    assert!(scene.node(root).is_ok() && scene.shapes.is_empty());
    assert_eq!(scene.dag.len(), 1);
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
        has_removals: false,
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

#[test]
fn a_restricted_descendant_refuses_the_subtree_delete_but_a_restricted_ancestor_does_not() {
    for restricted in [0, 1] {
        let (mut scene, root, child, _) = grouped_shape();
        let at = scene
            .nodes
            .iter()
            .position(|node| node.id == [root, child][restricted])
            .unwrap();
        scene.nodes[at]
            .unsupported_transform
            .push("unproved effect".into());
        let before = scene.clone();
        assert_eq!(scene.remove(root), Err(NativeError::UnsupportedEdit));
        assert_eq!(scene, before);
        let child_removed = scene.remove(child);
        if restricted == 0 {
            // Only the subtree is checked: a restricted ancestor does not matter to a child delete.
            child_removed.unwrap();
            scene.validate().unwrap();
            assert_eq!(scene.nodes.len(), 1);
        } else {
            assert_eq!(child_removed, Err(NativeError::UnsupportedEdit));
            assert_eq!(scene, before);
        }
    }
}

#[test]
fn read_only_format_wins_over_the_hierarchy_refusal_on_an_ascii_scene() {
    let (mut scene, root, child, _) = grouped_shape();
    retain(&mut scene, b"original", std::iter::once(0..8).collect());
    let before = scene.clone();
    for id in [root, child] {
        assert_eq!(scene.remove(id), Err(NativeError::ReadOnlyFormat));
    }
    assert_eq!(scene, before);
}

#[test]
fn missing_node_on_a_binary_source_scene_is_missing_node_not_unsupported_edit() {
    let binary = |scene: &mut NativeScene| {
        scene.source = Some(SourceRetention {
            has_removals: false,
            sites: Vec::new(),
            bytes: std::sync::Arc::from(&b"original"[..]),
            format: SourceFormat::Binary,
            records: std::iter::once(0..8).collect(),
            leaves: Vec::new(),
            appearance: None,
        });
        scene.validate().unwrap();
    };
    let (mut scene, gone) = cube();
    scene.remove(gone).unwrap();
    binary(&mut scene);
    let before = scene.clone();
    assert_eq!(
        scene.remove(gone),
        Err(SceneError::MissingNode(gone).into())
    );
    assert_eq!(scene, before);
    let (mut scene, present) = cube();
    binary(&mut scene);
    assert_eq!(
        scene.remove(present),
        Err(NativeError::UnsupportedDelete(DeleteUnsupported::Source(
            crate::native_scene::binary::RemovalRefusal::Unreadable
        )))
    );
}

#[test]
fn descendant_targets_are_the_instances_strictly_below_the_roots_in_path_order() {
    let (scene, root, mid, leaf) = chain_of_three();
    let paths = scene.transform_paths().unwrap();
    let of = |node| paths.iter().find(|t| t.node == node).unwrap().clone();
    let (root_t, mid_t, leaf_t) = (of(root), of(mid), of(leaf));
    assert_eq!(
        scene
            .descendant_targets(std::slice::from_ref(&root_t))
            .unwrap(),
        [mid_t.clone(), leaf_t.clone()]
    );
    assert_eq!(
        scene
            .descendant_targets(std::slice::from_ref(&mid_t))
            .unwrap(),
        [of(leaf)]
    );
    assert!(
        scene
            .descendant_targets(std::slice::from_ref(&leaf_t))
            .unwrap()
            .is_empty()
    );
    assert!(scene.descendant_targets(&[]).unwrap().is_empty());
    assert_eq!(
        scene.descendant_targets(&[mid_t, root_t]).unwrap(),
        [of(mid), leaf_t]
    );
}

#[test]
fn descendant_targets_follow_each_instance_path_of_a_reparented_node() {
    let (mut scene, root, child, _) = test_fixtures::hidden_group();
    let root_id = scene.node(root).unwrap().transform_id;
    let child_id = scene.node(child).unwrap().transform_id;
    scene.dag.push(DagEdge {
        parent: None,
        child: child_id,
    });
    scene.validate().unwrap();
    let root_target = TransformTarget {
        node: root,
        path: vec![root_id],
    };
    let below = scene.descendant_targets(&[root_target]).unwrap();
    assert_eq!(
        below,
        [TransformTarget {
            node: child,
            path: vec![root_id, child_id],
        }]
    );
    let direct = TransformTarget {
        node: child,
        path: vec![child_id],
    };
    assert!(scene.descendant_targets(&[direct]).unwrap().is_empty());
}

fn validations_during(action: impl FnOnce()) -> usize {
    FULL_VALIDATIONS.with(|count| count.set(0));
    action();
    FULL_VALIDATIONS.with(|count| count.get())
}

fn three_roots() -> (NativeScene, [NodeId; 3]) {
    let (mut scene, first) = cube();
    let second = scene.create(Primitive::Cube, "b").unwrap();
    let third = scene.create(Primitive::Cube, "c").unwrap();
    (scene, [first, second, third])
}

/// Baseline: N single removes validate N times; remove_many validates once.
#[test]
fn remove_many_validates_once_where_single_removes_validate_per_node() {
    let (mut scene, ids) = three_roots();
    let mut singly = scene.clone();
    assert_eq!(
        validations_during(|| ids.iter().for_each(|id| singly.remove(*id).unwrap())),
        3
    );
    assert_eq!(validations_during(|| scene.remove_many(&ids).unwrap()), 1);
    assert_eq!(scene, singly);
    assert!(scene.nodes.is_empty() && scene.shapes.is_empty() && scene.dag.is_empty());
}

#[test]
fn remove_many_removes_a_shared_shape_only_with_its_last_parent() {
    let (scene, first, second, shape) = shared_shape();
    let mut one = scene.clone();
    one.remove_many(&[first]).unwrap();
    assert!(one.shape(shape).is_ok());
    let mut both = scene.clone();
    both.remove_many(&[first, second]).unwrap();
    assert!(both.shapes.is_empty() && both.dag.is_empty() && both.nodes.is_empty());
    both.validate().unwrap();
    let mut reversed = scene;
    reversed.remove_many(&[second, first]).unwrap();
    assert_eq!(reversed, both);
}

#[test]
fn remove_many_deduplicates_repeated_ids_and_accepts_an_empty_list() {
    let (mut scene, [a, b, c]) = three_roots();
    let before = scene.clone();
    scene.remove_many(&[]).unwrap();
    assert_eq!(scene, before);
    scene.remove_many(&[b, b, a, b]).unwrap();
    assert_eq!(scene.nodes.len(), 1);
    assert!(scene.node(c).is_ok());
}

#[test]
fn remove_many_takes_a_group_once_in_either_order_and_a_lone_child_alone() {
    let (scene, root, child, _) = grouped_shape();
    for order in [
        vec![root, child],
        vec![child, root],
        vec![child, root, child],
    ] {
        let mut both = scene.clone();
        both.remove_many(&order).unwrap();
        assert!(both.nodes.is_empty() && both.shapes.is_empty() && both.dag.is_empty());
        both.validate().unwrap();
    }
    let mut lone = scene.clone();
    lone.remove_many(&[child]).unwrap();
    assert_eq!(lone.nodes.len(), 1);
    let (mut scene, id) = cube();
    retain(&mut scene, b"original", std::iter::once(0..8).collect());
    assert_eq!(scene.remove_many(&[id]), Err(NativeError::ReadOnlyFormat));
    let (mut scene, ids) = three_roots();
    let missing = NodeId::from_decimal("999").unwrap();
    assert_eq!(
        scene.remove_many(&[ids[0], missing]),
        Err(NativeError::Legacy(SceneError::MissingNode(missing)))
    );
}

/// The scanning form `geometry_view_admission_indexed` replaced (#341), kept verbatim so the
/// indexed form can be proved identical rather than merely plausible.
fn admission_by_scanning(
    scene: &NativeScene,
    path: &[NativeId],
) -> Result<GeometryViewAdmission, NativeError> {
    fn transform_prefix(scene: &NativeScene, path: &[NativeId]) -> Result<(), NativeError> {
        if path.is_empty()
            || !scene
                .dag
                .iter()
                .any(|edge| edge.parent.is_none() && edge.child == path[0])
            || path.iter().enumerate().any(|(index, id)| {
                path[..index].contains(id)
                    || !scene.nodes.iter().any(|node| node.transform_id == *id)
            })
            || path.windows(2).any(|pair| {
                !scene
                    .dag
                    .iter()
                    .any(|edge| edge.parent == Some(pair[0]) && edge.child == pair[1])
            })
        {
            return Err(NativeError::InvalidInstancePath);
        }
        Ok(())
    }
    if path.len() < 2 {
        return Err(NativeError::InvalidInstancePath);
    }
    transform_prefix(scene, &path[..path.len() - 1])?;
    if path[..path.len() - 1].contains(&path[path.len() - 1])
        || !scene.dag.contains(&DagEdge {
            parent: Some(path[path.len() - 2]),
            child: path[path.len() - 1],
        })
    {
        return Err(NativeError::InvalidInstancePath);
    }
    let id = path[path.len() - 1];
    let facts = scene
        .shapes
        .iter()
        .find(|shape| shape.id == id)
        .map(|shape| &shape.view)
        .or_else(|| {
            scene
                .curves
                .iter()
                .find(|curve| curve.id == id)
                .map(|curve| &curve.view)
        })
        .ok_or(NativeError::InvalidInstancePath)?;
    let mut visible = facts.visible()?;
    for id in &path[..path.len() - 1] {
        let node = scene
            .nodes
            .iter()
            .find(|node| node.transform_id == *id)
            .ok_or(NativeError::InvalidInstancePath)?;
        if !node.unsupported_transform.is_empty() {
            return Err(NativeError::UnsupportedView("unproved transform effects"));
        }
        visible &= node.view.visible()?;
    }
    Ok(GeometryViewAdmission {
        visible,
        appearance_incomplete: scene.appearance_incomplete(),
    })
}

#[test]
fn the_admission_index_matches_the_scanning_form_on_valid_and_malformed_paths() {
    let unknown = NativeId(u64::MAX);
    // A node carrying unsupported transform effects, so the `UnsupportedView` arm is exercised;
    // no shared fixture sets one. The deeper node is chosen so the ancestor walk must reach past
    // the first element to find it.
    let unsupported_leaf = {
        let (mut scene, _, _, _) = chain_of_three();
        let last = scene.nodes.len() - 1;
        scene.nodes[last].unsupported_transform = vec!["shear".to_string()];
        scene
    };
    let unsupported_root = {
        let (mut scene, _, _, _) = chain_of_three();
        scene.nodes[0].unsupported_transform = vec!["shear".to_string()];
        scene
    };
    // Visibility hidden at the deepest ancestor rather than the root, so a walk that stops early
    // reports the wrong answer instead of the same one.
    let hidden_leaf = {
        let (mut scene, _, _, _) = chain_of_three();
        let last = scene.nodes.len() - 1;
        scene.nodes[last].view.local_visibility = Some(false);
        scene
    };
    for (label, scene) in [
        ("grouped", grouped_shape().0),
        ("hidden group", test_fixtures::hidden_group().0),
        ("hidden shape", test_fixtures::hidden_shape().0),
        ("unproved group", test_fixtures::unproved_group().0),
        ("shared shape", shared_shape().0),
        ("chain of three", chain_of_three().0),
        ("unsupported leaf", unsupported_leaf),
        ("unsupported root", unsupported_root),
        ("hidden leaf", hidden_leaf),
    ] {
        let index = ViewAdmissionIndex::new(&scene).unwrap();
        let mut probes: Vec<Vec<NativeId>> = Vec::new();
        // Every real instance path in the scene.
        for target in scene.transform_paths().unwrap() {
            let prefix = target.path.clone();
            probes.push(prefix.clone());
            for shape in &scene.shapes {
                let mut path = prefix.clone();
                path.push(shape.id);
                probes.push(path);
            }
            for curve in &scene.curves {
                let mut path = prefix.clone();
                path.push(curve.id);
                probes.push(path);
            }
        }
        let roots: Vec<NativeId> = scene.nodes.iter().map(|node| node.transform_id).collect();
        // Malformed paths: empty, too short, unknown ids, repeated ids, non-adjacent pairs.
        probes.push(Vec::new());
        probes.push(vec![roots[0]]);
        probes.push(vec![unknown, unknown]);
        probes.push(vec![roots[0], unknown]);
        probes.push(vec![unknown, roots[0]]);
        probes.push(vec![roots[0], roots[0]]);
        if let Some(shape) = scene.shapes.first() {
            probes.push(vec![shape.id, shape.id]);
            probes.push(vec![roots[0], shape.id, shape.id]);
            probes.push(vec![shape.id, roots[0]]);
        }
        for a in &roots {
            for b in &roots {
                probes.push(vec![*a, *b]);
                if let Some(shape) = scene.shapes.first() {
                    probes.push(vec![*a, *b, shape.id]);
                }
            }
        }
        // Each malformed shape below is rejected by exactly one check, so a dropped check shows up
        // as a difference rather than being masked by an earlier one.
        for target in scene.transform_paths().unwrap() {
            let prefix = &target.path;
            // Repeated transform id with every element still a real, adjacent transform: only the
            // cycle check rejects it. A self-loop on the last element keeps adjacency plausible.
            for index in 0..prefix.len() {
                let mut looped = prefix.clone();
                looped.insert(index, prefix[index]);
                probes.push(looped.clone());
                for shape in &scene.shapes {
                    let mut path = looped.clone();
                    path.push(shape.id);
                    probes.push(path);
                }
            }
            // A one-element path and a bare prefix: only the length check rejects the former.
            probes.push(prefix.clone());
            if let Some(first) = prefix.first() {
                probes.push(vec![*first]);
            }
        }
        // A shape id used where a transform belongs: only the unknown-transform check rejects it.
        for shape in &scene.shapes {
            probes.push(vec![shape.id, shape.id]);
            probes.push(vec![shape.id]);
            for root in &roots {
                probes.push(vec![shape.id, *root]);
                probes.push(vec![*root, shape.id, shape.id]);
            }
        }
        for path in probes {
            assert_eq!(
                scene.geometry_view_admission_indexed(&index, &path),
                admission_by_scanning(&scene, &path),
                "{label}: path {path:?}"
            );
        }
    }
}

/// Group `g` with children `a` (owning shape `s`) and `b` (also showing `s`).
fn group_sharing_a_shape() -> (NativeScene, NodeId, NativeId) {
    let (mut scene, g, _, s) = grouped_shape();
    let b = scene.create(Primitive::Cube, "b").unwrap();
    let discarded = scene.shapes.pop().unwrap().id;
    let (g_id, b_id) = (
        scene.node(g).unwrap().transform_id,
        scene.node(b).unwrap().transform_id,
    );
    scene
        .dag
        .retain(|edge| edge.child != discarded && !(edge.child == b_id && edge.parent.is_none()));
    scene.dag.push(DagEdge {
        parent: Some(g_id),
        child: b_id,
    });
    scene.dag.push(DagEdge {
        parent: Some(b_id),
        child: s,
    });
    scene.validate().unwrap();
    (scene, g, s)
}

#[test]
fn a_deep_chain_goes_with_its_root() {
    let (mut scene, root, _, _) = chain_of_three();
    scene.remove(root).unwrap();
    assert!(scene.nodes.is_empty() && scene.shapes.is_empty() && scene.dag.is_empty());
    scene.validate().unwrap();
}

#[test]
fn a_shape_shared_by_two_removed_transforms_goes_and_the_scene_stays_valid() {
    let (scene, g, s) = group_sharing_a_shape();
    let mut removed = scene.clone();
    removed.remove(g).unwrap();
    removed.validate().unwrap();
    assert!(removed.shape(s).is_err());
    assert!(removed.nodes.is_empty() && removed.dag.is_empty());
}

#[test]
fn a_shape_shared_with_a_survivor_stays_and_loses_only_the_removed_edges() {
    let (mut scene, g, s) = group_sharing_a_shape();
    let outsider = scene.create(Primitive::Cube, "outsider").unwrap();
    let outsider_id = scene.node(outsider).unwrap().transform_id;
    let discarded = scene.shapes.pop().unwrap().id;
    scene.dag.retain(|edge| edge.child != discarded);
    scene.dag.push(DagEdge {
        parent: Some(outsider_id),
        child: s,
    });
    scene.validate().unwrap();
    scene.remove(g).unwrap();
    scene.validate().unwrap();
    assert!(scene.shape(s).is_ok());
    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(
        scene.dag.iter().filter(|edge| edge.child == s).count(),
        1,
        "{:?}",
        scene.dag
    );
}

#[test]
fn a_descendant_transform_with_an_outside_edge_refuses_the_delete() {
    for root_edge in [false, true] {
        let (mut scene, root, child, _) = grouped_shape();
        let child_id = scene.node(child).unwrap().transform_id;
        let parent = if root_edge {
            None
        } else {
            let other = scene.create(Primitive::Cube, "other").unwrap();
            Some(scene.node(other).unwrap().transform_id)
        };
        scene.dag.push(DagEdge {
            parent,
            child: child_id,
        });
        scene.validate().unwrap();
        let before = scene.clone();
        assert_eq!(scene.remove(root), Err(NativeError::UnsupportedEdit));
        assert_eq!(scene, before);
        // Deleting the instanced transform itself would remove it from both parents.
        assert_eq!(scene.remove(child), Err(NativeError::UnsupportedEdit));
        assert_eq!(scene, before);
    }
}

#[test]
fn remove_many_with_a_refused_member_among_deletable_ones_changes_nothing() {
    let (mut scene, _, child, _) = grouped_shape();
    let other = scene.create(Primitive::Cube, "other").unwrap();
    test_fixtures::restrict_transform(&mut scene, other);
    let before = scene.clone();
    assert_eq!(
        scene.remove_many(&[child, other]),
        Err(NativeError::DeleteMemberRefused {
            node: "other".into(),
            reason: Box::new(NativeError::UnsupportedEdit),
        })
    );
    assert_eq!(scene, before);
}

#[test]
fn a_restricted_shape_in_the_subtree_refuses_the_delete() {
    let (mut scene, root, _, _) = grouped_shape();
    scene.shapes[0]
        .restrictions
        .push("unproved shape effect".into());
    let before = scene.clone();
    assert_eq!(scene.remove(root), Err(NativeError::UnsupportedEdit));
    assert_eq!(scene, before);
}

/// `root` with three cube children `a`, `b`, `c` in that order.
fn root_with_three_children() -> (NativeScene, NodeId, [NodeId; 3]) {
    let (mut scene, root) = cube();
    let root_id = scene.node(root).unwrap().transform_id;
    let kids = ["a", "b", "c"].map(|name| scene.create(Primitive::Cube, name).unwrap());
    for kid in kids {
        let id = scene.node(kid).unwrap().transform_id;
        scene
            .dag
            .iter_mut()
            .find(|edge| edge.child == id)
            .unwrap()
            .parent = Some(root_id);
    }
    scene.validate().unwrap();
    (scene, root, kids)
}

#[test]
fn deleting_a_middle_child_keeps_the_parent_its_other_children_and_their_order() {
    let (mut scene, root, [a, b, c]) = root_with_three_children();
    scene.remove(b).unwrap();
    scene.validate().unwrap();
    let order: Vec<_> = scene
        .transform_paths()
        .unwrap()
        .iter()
        .map(|t| t.node)
        .collect();
    assert_eq!(order, [root, a, c]);
    scene.set_channel(root, Channel::TranslateX, 5.0).unwrap();
    scene.remove(a).unwrap();
    assert_eq!(scene.nodes.len(), 2);
}

#[test]
fn siblings_are_deleted_together_and_the_parent_survives() {
    let (scene, root, [a, b, c]) = root_with_three_children();
    for ids in [vec![a, c], vec![c, a, b]] {
        let mut both = scene.clone();
        both.remove_many(&ids).unwrap();
        both.validate().unwrap();
        assert_eq!(both.nodes.len(), 4 - ids.len());
        assert!(both.node(root).is_ok());
    }
}
