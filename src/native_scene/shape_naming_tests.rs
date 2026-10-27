//! Maya shape naming on creation and rename (#175). Unless a case says otherwise, expected names
//! were observed in Maya 2025.1 (`polyCube`, `polyPlane`, `polySphere`, `polyCylinder`,
//! `polyCone`, `rename` and `instance`); cases marked PROVISIONAL follow the inferred rule.
//! Maya 2026 is unverified (#5).

use super::test_fixtures::shared_shape;
use super::*;

fn shape_of(scene: &NativeScene, id: NodeId) -> &str {
    let transform = scene.node(id).unwrap().transform_id;
    let child = scene
        .dag
        .iter()
        .find(|edge| edge.parent == Some(transform))
        .unwrap()
        .child;
    scene.shapes.iter().find(|s| s.id == child).unwrap().name()
}

fn default_named(scene: &mut NativeScene, primitive: Primitive) -> NodeId {
    scene
        .create_default_named(primitive, &PrimitiveOptions::default_for(primitive))
        .unwrap()
}

#[test]
fn default_names_put_the_number_after_shape_for_every_primitive() {
    let mut scene = NativeScene::default();
    for (primitive, transform, shape) in [
        (Primitive::Cube, "pCube1", "pCubeShape1"),
        (Primitive::Cube, "pCube2", "pCubeShape2"),
        (Primitive::Plane, "pPlane1", "pPlaneShape1"),
        (Primitive::Sphere, "pSphere1", "pSphereShape1"),
        (Primitive::Cylinder, "pCylinder1", "pCylinderShape1"),
        (Primitive::Cone, "pCone1", "pConeShape1"),
    ] {
        let id = default_named(&mut scene, primitive);
        assert_eq!(scene.node(id).unwrap().name(), transform);
        assert_eq!(shape_of(&scene, id), shape);
    }
    scene.validate().unwrap();
}

#[test]
fn explicit_names_keep_the_shape_suffix_even_when_they_look_like_defaults() {
    let mut scene = NativeScene::default();
    for name in ["foo", "foo3", "pCube7"] {
        let id = scene.create(Primitive::Cube, name).unwrap();
        assert_eq!(shape_of(&scene, id), format!("{name}Shape"));
    }
}

#[test]
fn transform_and_shape_numbers_are_chosen_separately() {
    // Maya gave pCube8 with pCubeShape1 when pCube5 and pCube7 existed but no pCubeShape<N>.
    let mut scene = NativeScene::default();
    let first = default_named(&mut scene, Primitive::Cube);
    scene.rename(first, "box").unwrap();
    scene.create(Primitive::Cube, "pCube1").unwrap();
    let next = default_named(&mut scene, Primitive::Cube);
    assert_eq!(scene.node(next).unwrap().name(), "pCube2");
    assert_eq!(shape_of(&scene, next), "pCubeShape1");
}

#[test]
fn derived_shape_names_follow_maya_rename() {
    for (shape, old, new, expected) in [
        ("pCubeShape1", "pCube1", "box", Some("boxShape")),
        ("boxShape", "box", "crate7", Some("crate7Shape")),
        ("pCubeShape1", "pCube1", "bar1", Some("barShape1")),
        ("pCubeShape3", "pCube3", "pCube99", Some("pCubeShape99")),
        ("pCubeShape1", "pCube8", "x_9b", Some("x_9bShape")),
        ("x_9bShape", "x_9b", "q4", Some("q4Shape")),
        ("foo3Shape", "foo3", "bar7", Some("bar7Shape")),
        ("pCubeShape1", "pCube8", "pSphere2", Some("pSphereShape2")),
        ("customMesh", "pCube1", "bar", None),
        ("inst_srcShape", "pCube2", "inst_copy", None),
        // PROVISIONAL: not observed, derived from the rule.
        ("pCubeShape", "pCube1", "bar", Some("barShape")),
        ("pCubeShapeX", "pCube1", "bar", None),
        ("pCubeShape1", "pCylinder1", "bar", None),
    ] {
        assert_eq!(
            derived_shape_name(shape, old, new).as_deref(),
            expected,
            "{shape} under {old} -> {new}"
        );
    }
}

#[test]
fn rename_and_edit_node_rename_the_derived_shape() {
    let mut scene = NativeScene::default();
    let id = default_named(&mut scene, Primitive::Cube);
    scene.rename(id, "box").unwrap();
    assert_eq!(shape_of(&scene, id), "boxShape");
    scene.edit_node(id, Some("crate7"), [None; 9]).unwrap();
    assert_eq!(shape_of(&scene, id), "crate7Shape");
    scene.rename(id, "crate7").unwrap();
    assert_eq!(shape_of(&scene, id), "crate7Shape");
    scene.validate().unwrap();
}

#[test]
fn a_custom_shape_name_and_a_refused_rename_leave_the_shape_alone() {
    let mut scene = NativeScene::default();
    let id = default_named(&mut scene, Primitive::Cube);
    let other = default_named(&mut scene, Primitive::Cube);
    assert_eq!(scene.rename(id, "pCube2"), Err(NativeError::NameCollision));
    assert_eq!(scene.rename(id, "1bad"), Err(NativeError::InvalidName));
    assert_eq!(shape_of(&scene, id), "pCubeShape1");
    let index = scene
        .shapes
        .iter()
        .position(|s| s.name == "pCubeShape2")
        .unwrap();
    scene.shapes[index].name = "customMesh".into();
    scene.rename(other, "bar").unwrap();
    assert_eq!(shape_of(&scene, other), "customMesh");
}

#[test]
fn names_held_by_any_transform_or_shape_are_never_reused() {
    // Authored saves refuse duplicate names; Maya would allow these under different parents.
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "pCubeShape1").unwrap();
    let id = default_named(&mut scene, Primitive::Cube);
    assert_eq!(scene.node(id).unwrap().name(), "pCube1");
    assert_eq!(shape_of(&scene, id), "pCubeShape2");

    let mut scene = NativeScene::default();
    let first = scene.create(Primitive::Cube, "pCube1").unwrap();
    let second = default_named(&mut scene, Primitive::Cube);
    let third = default_named(&mut scene, Primitive::Cube);
    assert_eq!(shape_of(&scene, second), "pCubeShape1");
    assert_eq!(shape_of(&scene, third), "pCubeShape2");
    scene.rename(first, "a").unwrap();
    scene.rename(third, "pCube1").unwrap();
    assert_eq!(shape_of(&scene, third), "pCubeShape2");
    let mut names: Vec<_> = authored::names(&scene).map(|(_, name)| name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), authored::names(&scene).count());
}

#[test]
fn a_shared_shape_follows_only_the_transform_it_was_named_after() {
    // PROVISIONAL: the rule is by name, so renaming a copy whose old name the shared shape
    // derives from would rename it too; Maya was only observed renaming the original first.
    // Editable scenes cannot create instances yet.
    let (mut scene, first, second, _) = shared_shape();
    assert_eq!(shape_of(&scene, first), "pCube1Shape");
    scene.rename(second, "copy").unwrap();
    assert_eq!(shape_of(&scene, second), "pCube1Shape");
    scene.rename(first, "source").unwrap();
    assert_eq!(shape_of(&scene, first), "sourceShape");
    assert_eq!(shape_of(&scene, second), "sourceShape");
}
