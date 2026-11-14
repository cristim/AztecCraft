//! One row per node condition, one column per tool and space: the exact refusal or an allowed edit.
use super::*;
use glam::{DMat3, DVec3};

type Expect<T> = Option<T>;

struct Row {
    name: &'static str,
    node: TransformExtras,
    parent: TransformExtras,
    move_world: Expect<WorldMoveUnsupported>,
    rotate_object: Expect<RotateUnsupported>,
    rotate_world: Expect<RotateUnsupported>,
    scale_object: Expect<ScaleUnsupported>,
    scale_world: Expect<ScaleUnsupported>,
}

fn extras(f: impl FnOnce(&mut TransformExtras)) -> TransformExtras {
    let mut value = TransformExtras::default();
    f(&mut value);
    value
}

fn row(name: &'static str) -> Row {
    Row {
        name,
        node: TransformExtras::default(),
        parent: TransformExtras::default(),
        move_world: None,
        rotate_object: None,
        rotate_world: None,
        scale_object: None,
        scale_world: None,
    }
}

fn rows() -> Vec<Row> {
    use RotateUnsupported::{RotateAxis as Ra, RotateOrder as Ro};
    let yzx = extras(|e| e.rotate_order = Some(1));
    let axis = extras(|e| e.rotate_axis = Some([17.0, -33.0, 52.0]));
    let both = extras(|e| {
        e.rotate_order = Some(4);
        e.rotate_axis = Some([0.0, 0.0, 90.0]);
    });
    let shear = extras(|e| e.shear = Some([0.5, 0.0, -0.25]));
    let pivots = extras(|e| {
        e.rotate_pivot_translate = Some([1.0, 2.0, 3.0]);
        e.scale_pivot_translate = Some([-1.0, 0.5, 2.0]);
    });
    vec![
        row("no extras"),
        Row {
            node: extras(|e| e.rotate_order = Some(0)),
            ..row("explicit xyz order")
        },
        Row {
            node: yzx.clone(),
            rotate_object: Some(Ro),
            rotate_world: Some(Ro),
            ..row("ro != xyz")
        },
        Row {
            node: axis.clone(),
            rotate_object: Some(Ra),
            rotate_world: Some(Ra),
            ..row("ra != 0")
        },
        Row {
            node: both,
            rotate_object: Some(Ro),
            rotate_world: Some(Ro),
            ..row("ro != xyz and ra != 0")
        },
        Row {
            node: shear.clone(),
            scale_world: Some(ScaleUnsupported::Shear),
            ..row("sh != 0")
        },
        Row {
            node: pivots,
            ..row("rpt and spt")
        },
        Row {
            parent: yzx,
            ..row("parent ro != xyz")
        },
        Row {
            parent: axis,
            ..row("parent ra != 0")
        },
        Row {
            parent: shear,
            rotate_world: Some(RotateUnsupported::ParentTransform),
            scale_world: Some(ScaleUnsupported::ParentTransform),
            ..row("parent sh != 0")
        },
    ]
}

fn build(row: &Row) -> (NativeScene, TransformTarget) {
    let (mut scene, root, node, _) = test_fixtures::grouped_shape();
    // A turned parent and an offset child keep every frame well conditioned and non-trivial.
    let parent = NativeTransform {
        rotation: [0.0, 0.0, 30.0],
        scale: [2.0, 2.0, 2.0],
        ..NativeTransform::default()
    };
    scene.set_transform(root, parent).unwrap();
    test_fixtures::set_extras(&mut scene, root, row.parent.clone());
    test_fixtures::set_extras(&mut scene, node, row.node.clone());
    let target = scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|path| path.node == node)
        .unwrap();
    (scene, target)
}

#[test]
fn edit_policy_table_names_the_exact_refusal_or_allows_every_cell() {
    for row in rows() {
        let (scene, target) = build(&row);
        let before = scene.clone();
        let name = row.name;
        assert_eq!(
            scene.world_move_frame(&target).err(),
            row.move_world.map(NativeError::UnsupportedMove),
            "{name}: Move world"
        );
        for (space, expected, label) in [
            (RotateSpace::Object, row.rotate_object, "Rotate object"),
            (RotateSpace::World, row.rotate_world, "Rotate world"),
            // The Gimbal channel edit never reads the parent map, so it refuses what Object does.
            (RotateSpace::Gimbal, row.rotate_object, "Rotate gimbal"),
        ] {
            assert_eq!(
                scene.begin_rotation(&target, RotateAxis::Z, space).err(),
                expected.map(NativeError::UnsupportedRotate),
                "{name}: {label}"
            );
        }
        // The ring axes also need a rotation x uniform scale parent, like World.
        assert_eq!(
            scene.gimbal_axes(&target).err(),
            row.rotate_world.map(NativeError::UnsupportedRotate),
            "{name}: gimbal axes"
        );
        for (space, expected, label) in [
            (ScaleSpace::Object, row.scale_object, "Scale object"),
            (ScaleSpace::World, row.scale_world, "Scale world"),
        ] {
            assert_eq!(
                scene
                    .begin_scale_frame(&target, ScaleHandle::Uniform, space)
                    .err(),
                expected.map(NativeError::UnsupportedScale),
                "{name}: {label}"
            );
        }
        assert_eq!(scene, before, "{name}: a refusal must not mutate");
    }
}

// Independent world linear map: P * (RA then R(ro), from the row-vector reference) * SH * S.
fn world_linear(r: [f64; 3], scale: [f64; 3], extras: &TransformExtras) -> DMat3 {
    let parent = DMat3::from_rotation_z(90f64.to_radians()) * 2.0;
    let rad = |v: [f64; 3]| v.map(f64::to_radians);
    let axes = crate::native_scene::transform_extras::tests::row_vector_axes(
        extras.rotate_order.unwrap_or(0),
        rad(r),
        rad(extras.rotate_axis.unwrap_or_default()),
    );
    let rotation = DMat3::from_cols(
        DVec3::from_array(axes[0]),
        DVec3::from_array(axes[1]),
        DVec3::from_array(axes[2]),
    );
    let [xy, xz, yz] = extras.shear.unwrap_or_default();
    let shear = DMat3::from_cols(DVec3::X, DVec3::new(xy, 1.0, 0.0), DVec3::new(xz, yz, 1.0));
    parent * rotation * shear * DMat3::from_diagonal(DVec3::from_array(scale))
}

fn assert_close(actual: DMat3, expected: DMat3, what: &str) {
    let drift = (actual - expected).to_cols_array().map(f64::abs);
    let drift = drift.into_iter().fold(0.0, f64::max);
    assert!(drift < 1e-12, "{what}: drift {drift}");
}

#[test]
fn allowed_world_edits_apply_the_world_axis_operator_to_the_world_linear_map() {
    let scale = [2.0, 3.0, 4.0];
    for row in rows()
        .into_iter()
        .filter(|row| row.parent == TransformExtras::default())
    {
        let name = row.name;
        let build_at = |r: [f64; 3]| {
            let (mut scene, root, node, _) = test_fixtures::grouped_shape();
            scene
                .set_transform(
                    root,
                    NativeTransform {
                        rotation: [0.0, 0.0, 90.0],
                        scale: [2.0; 3],
                        ..NativeTransform::default()
                    },
                )
                .unwrap();
            scene
                .set_transform(
                    node,
                    NativeTransform {
                        rotation: r,
                        scale,
                        ..NativeTransform::default()
                    },
                )
                .unwrap();
            test_fixtures::set_extras(&mut scene, node, row.node.clone());
            let target = scene
                .transform_paths()
                .unwrap()
                .into_iter()
                .find(|path| path.node == node)
                .unwrap();
            (scene, target)
        };
        if row.rotate_world.is_none() {
            let r = [30.0, 20.0, 70.0];
            let (scene, target) = build_at(r);
            for (axis, vector) in [
                (RotateAxis::X, DVec3::X),
                (RotateAxis::Y, DVec3::Y),
                (RotateAxis::Z, DVec3::Z),
            ] {
                let (frame, _) = scene
                    .begin_rotation(&target, axis, RotateSpace::World)
                    .unwrap();
                let new_r = frame.rotation(axis, 30.0).unwrap();
                let operator = DMat3::from_axis_angle(vector, 30f64.to_radians());
                assert_close(
                    world_linear(new_r, scale, &row.node),
                    operator * world_linear(r, scale, &row.node),
                    &format!("{name}: Rotate world {axis:?}"),
                );
            }
        }
        if row.scale_world.is_none() && row.node.rotate_axis.is_none() {
            let r = [90.0, 0.0, 90.0];
            let (scene, target) = build_at(r);
            for (handle, vector) in [
                (ScaleHandle::X, DVec3::X),
                (ScaleHandle::Y, DVec3::Y),
                (ScaleHandle::Z, DVec3::Z),
            ] {
                let (frame, _) = scene
                    .begin_scale_frame(&target, handle, ScaleSpace::World)
                    .unwrap();
                let new_scale = frame.scale(handle, 1.7).unwrap();
                let operator = DMat3::IDENTITY
                    + DMat3::from_cols(vector * vector.x, vector * vector.y, vector * vector.z)
                        * 0.7;
                assert_close(
                    world_linear(r, new_scale, &row.node),
                    operator * world_linear(r, scale, &row.node),
                    &format!("{name}: Scale world {handle:?}"),
                );
            }
        }
    }
    // Quarter-turn rotate axes keep the world axes aligned, so the edit is a single channel.
    for (name, node) in [
        ("ra z90", extras(|e| e.rotate_axis = Some([0.0, 0.0, 90.0]))),
        (
            "ro zyx and ra x90 y90",
            extras(|e| {
                e.rotate_order = Some(5);
                e.rotate_axis = Some([90.0, 90.0, 0.0]);
            }),
        ),
    ] {
        let row = Row { node, ..row(name) };
        let (mut scene, root, id, _) = test_fixtures::grouped_shape();
        scene
            .set_transform(
                root,
                NativeTransform {
                    rotation: [0.0, 0.0, 90.0],
                    scale: [2.0; 3],
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        let r = [90.0, 0.0, 90.0];
        scene
            .set_transform(
                id,
                NativeTransform {
                    rotation: r,
                    scale,
                    ..NativeTransform::default()
                },
            )
            .unwrap();
        test_fixtures::set_extras(&mut scene, id, row.node.clone());
        let target = scene
            .transform_paths()
            .unwrap()
            .into_iter()
            .find(|path| path.node == id)
            .unwrap();
        for (handle, vector) in [
            (ScaleHandle::X, DVec3::X),
            (ScaleHandle::Y, DVec3::Y),
            (ScaleHandle::Z, DVec3::Z),
        ] {
            let (frame, _) = scene
                .begin_scale_frame(&target, handle, ScaleSpace::World)
                .unwrap();
            let new_scale = frame.scale(handle, 1.7).unwrap();
            let operator = DMat3::IDENTITY
                + DMat3::from_cols(vector * vector.x, vector * vector.y, vector * vector.z) * 0.7;
            assert_close(
                world_linear(r, new_scale, &row.node),
                operator * world_linear(r, scale, &row.node),
                &format!("{name}: Scale world {handle:?}"),
            );
        }
    }
}
