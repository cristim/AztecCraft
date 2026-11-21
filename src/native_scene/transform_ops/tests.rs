use super::*;
use crate::native_scene::group::tests::{bits, cube, set};
use crate::native_scene::{ALL_CHANNELS, ReparentKeep, test_fixtures};
use glam::DVec3;

const TOLERANCE: f64 = 1e-4;

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

/// Every mesh point of every shape in world space, by shape order, from the local matrices.
fn world_points(scene: &NativeScene) -> Vec<Vec<DVec3>> {
    scene
        .shapes
        .iter()
        .map(|shape| {
            let parent = scene
                .dag
                .iter()
                .find(|e| e.child == shape.id)
                .unwrap()
                .parent;
            let world = scene.path_world(&chain(scene, parent.unwrap())).unwrap();
            (0..shape.mesh.point_count())
                .map(|i| {
                    world.transform_point3(DVec3::from_array(
                        shape.mesh.effective_point_cm(i).unwrap(),
                    ))
                })
                .collect()
        })
        .collect()
}

/// The unit world normal of every face (Newell), which a lost rotation of the points changes.
fn world_normals(scene: &NativeScene) -> Vec<Vec<DVec3>> {
    let points = world_points(scene);
    scene
        .shapes
        .iter()
        .zip(&points)
        .map(|(shape, points)| {
            shape
                .mesh
                .faces()
                .iter()
                .map(|face| {
                    let ring: Vec<DVec3> = face
                        .outer()
                        .iter()
                        .map(|d| {
                            let [a, b] = shape.mesh.edges()[d.edge].endpoints();
                            points[if d.reversed { b } else { a }]
                        })
                        .collect();
                    let mut normal = DVec3::ZERO;
                    for (i, p) in ring.iter().enumerate() {
                        let q = ring[(i + 1) % ring.len()];
                        normal += DVec3::new(
                            (p.y - q.y) * (p.z + q.z),
                            (p.z - q.z) * (p.x + q.x),
                            (p.x - q.x) * (p.y + q.y),
                        );
                    }
                    normal.normalize()
                })
                .collect()
        })
        .collect()
}

fn same(a: &[Vec<DVec3>], b: &[Vec<DVec3>]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().flatten().zip(b.iter().flatten()) {
        assert!((*a - *b).abs().max_element() < TOLERANCE, "{a:?} != {b:?}");
    }
}

fn identity_bits() -> [u64; 9] {
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0].map(f64::to_bits)
}

/// A rotated, non-uniformly scaled parent cube with two rotated, scaled child cubes, the second
/// one nested a level deeper.
fn family() -> (NativeScene, NodeId, NodeId, NodeId) {
    let mut scene = NativeScene::default();
    let parent = cube(&mut scene, "parentCube");
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    set(
        &mut scene,
        parent,
        [4.0, -2.0, 1.0],
        [10.0, 20.0, 90.0],
        [2.0, 0.5, 3.0],
    );
    set(
        &mut scene,
        a,
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 30.0],
        [1.0, 3.0, 1.0],
    );
    set(
        &mut scene,
        b,
        [0.0, 2.0, 0.0],
        [15.0, 0.0, 0.0],
        [2.0, 2.0, 2.0],
    );
    scene
        .reparent(a, Some(parent), ReparentKeep::Local)
        .unwrap();
    scene.reparent(b, Some(a), ReparentKeep::Local).unwrap();
    (scene, parent, a, b)
}

#[test]
fn freezing_a_cube_bakes_translate_rotate_and_scale_into_its_points() {
    let (mut scene, id) = test_fixtures::cube();
    set(
        &mut scene,
        id,
        [1.0, 2.0, 3.0],
        [20.0, 35.0, 50.0],
        [2.0, 3.0, 0.5],
    );
    let (points, normals) = (world_points(&scene), world_normals(&scene));
    scene
        .freeze_transformations(&[id], TransformChannels::ALL)
        .unwrap();
    assert_eq!(bits(&scene, id), identity_bits());
    same(&points, &world_points(&scene));
    same(&normals, &world_normals(&scene));
    let local = scene.shapes[0].mesh.effective_point_cm(0).unwrap();
    assert!(
        local.iter().any(|v| v.abs() > 1.0),
        "the points carry the transform"
    );
}

#[test]
fn freezing_a_parent_freezes_its_children_and_keeps_every_world_point() {
    let (mut scene, parent, a, b) = family();
    let (points, normals) = (world_points(&scene), world_normals(&scene));
    scene
        .freeze_transformations(&[parent], TransformChannels::ALL)
        .unwrap();
    for id in [parent, a, b] {
        assert_eq!(bits(&scene, id), identity_bits());
    }
    same(&points, &world_points(&scene));
    same(&normals, &world_normals(&scene));
}

#[test]
fn a_listed_descendant_is_carried_by_its_listed_ancestor() {
    let (mut scene, parent, a, _) = family();
    let points = world_points(&scene);
    scene
        .freeze_transformations(&[a, parent], TransformChannels::ALL)
        .unwrap();
    same(&points, &world_points(&scene));
}

#[test]
fn freezing_below_a_parent_keeps_the_parent_and_the_world() {
    let (mut scene, parent, a, b) = family();
    let (points, parent_bits) = (world_points(&scene), bits(&scene, parent));
    scene
        .freeze_transformations(&[a], TransformChannels::ALL)
        .unwrap();
    assert_eq!(bits(&scene, parent), parent_bits);
    assert_eq!(bits(&scene, a), identity_bits());
    assert_eq!(bits(&scene, b), identity_bits());
    same(&points, &world_points(&scene));
}

#[test]
fn a_partial_freeze_resets_only_the_chosen_channels_and_keeps_the_world() {
    for channels in [
        TransformChannels {
            translate: true,
            rotate: false,
            scale: false,
        },
        TransformChannels {
            translate: false,
            rotate: true,
            scale: false,
        },
        TransformChannels {
            translate: false,
            rotate: false,
            scale: true,
        },
        TransformChannels {
            translate: true,
            rotate: true,
            scale: false,
        },
    ] {
        let (mut scene, id) = test_fixtures::cube();
        set(
            &mut scene,
            id,
            [1.0, 2.0, 3.0],
            [20.0, 35.0, 50.0],
            [2.0, 3.0, 0.5],
        );
        let (points, normals) = (world_points(&scene), world_normals(&scene));
        scene.freeze_transformations(&[id], channels).unwrap();
        let t = scene.node(id).unwrap().transform();
        let zero = |v: [f64; 3], default: f64| v.iter().all(|c| (c - default).abs() < 1e-9);
        assert_eq!(zero(t.translation, 0.0), channels.translate, "{channels:?}");
        assert_eq!(zero(t.rotation, 0.0), channels.rotate, "{channels:?}");
        assert_eq!(zero(t.scale, 1.0), channels.scale, "{channels:?}");
        same(&points, &world_points(&scene));
        same(&normals, &world_normals(&scene));
    }
}

#[test]
fn reset_writes_the_defaults_of_the_chosen_channels_and_leaves_children_to_follow() {
    let (mut scene, parent, a, _) = family();
    let child = bits(&scene, a);
    scene
        .reset_transformations(&[parent], TransformChannels::ALL)
        .unwrap();
    assert_eq!(bits(&scene, parent), identity_bits());
    assert_eq!(bits(&scene, a), child);
    let (mut scene, parent, _, _) = family();
    let only_scale = TransformChannels {
        translate: false,
        rotate: false,
        scale: true,
    };
    scene.reset_transformations(&[parent], only_scale).unwrap();
    let t = scene.node(parent).unwrap().transform();
    assert_eq!(
        (t.translation, t.rotation, t.scale),
        ([4.0, -2.0, 1.0], [10.0, 20.0, 90.0], [1.0; 3])
    );
}

#[test]
fn refusals_leave_the_scene_untouched() {
    let all = TransformChannels::ALL;
    let none = TransformChannels {
        translate: false,
        rotate: false,
        scale: false,
    };
    let (scene, parent, a, _) = family();
    let mut cases: Vec<(
        NativeScene,
        Vec<NodeId>,
        TransformChannels,
        TransformRefusal,
    )> = vec![
        (
            scene.clone(),
            vec![parent],
            none,
            TransformRefusal::EmptySet,
        ),
        (scene.clone(), vec![], all, TransformRefusal::EmptySet),
    ];
    let mut keyed = scene.clone();
    keyed.set_keys(&[a], 1, ALL_CHANNELS, 1).unwrap();
    cases.push((keyed, vec![parent], all, TransformRefusal::Keyed));
    let (pivoted, pivoted_id) = test_fixtures::pivoted_shape();
    cases.push((pivoted, vec![pivoted_id], all, TransformRefusal::NotPlain));
    let mut mirrored = scene.clone();
    set(
        &mut mirrored,
        a,
        [1.0, 0.0, 0.0],
        [0.0; 3],
        [-1.0, 1.0, 1.0],
    );
    cases.push((mirrored, vec![parent], all, TransformRefusal::Reflection));
    let mut flat = scene.clone();
    set(&mut flat, a, [1.0, 0.0, 0.0], [0.0; 3], [0.0, 1.0, 1.0]);
    cases.push((flat, vec![parent], all, TransformRefusal::Degenerate));
    for (mut scene, ids, channels, refusal) in cases {
        let before = scene.clone();
        assert_eq!(
            scene.freeze_transformations(&ids, channels),
            Err(NativeError::InvalidTransformEdit(refusal))
        );
        assert_eq!(scene, before, "{refusal:?}");
    }
    let mut keyed = scene.clone();
    keyed.set_keys(&[parent], 1, ALL_CHANNELS, 1).unwrap();
    let before = keyed.clone();
    assert_eq!(
        keyed.reset_transformations(&[parent], all),
        Err(NativeError::InvalidTransformEdit(TransformRefusal::Keyed))
    );
    assert_eq!(keyed, before);
    let mut duplicate = scene.clone();
    assert!(
        duplicate
            .freeze_transformations(&[parent, parent], all)
            .is_err()
    );
    assert!(
        duplicate
            .reset_transformations(&[parent, parent], all)
            .is_err()
    );
    assert_eq!(duplicate, scene);
}

mod coverage;
