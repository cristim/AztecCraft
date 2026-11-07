use super::*;
use crate::{
    native_scene::{Points, test_fixtures::cube},
    obj::import::parse,
    scene::Primitive,
};

fn parsed(text: &str) -> ParsedObj {
    parse(text.as_bytes()).unwrap()
}

fn import(scene: &mut NativeScene, text: &str) -> Vec<NodeId> {
    let plan = scene.plan_obj_import(&parsed(text)).unwrap();
    scene.import_polygon_objects(&plan).unwrap()
}

fn names(scene: &NativeScene, ids: &[NodeId]) -> Vec<(String, String)> {
    ids.iter()
        .map(|id| {
            let node = scene.node(*id).unwrap();
            let shape = authored::shape_of(scene, node).unwrap();
            (node.name().to_owned(), shape.name().to_owned())
        })
        .collect()
}

const TWO_OBJECTS: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 5 5 5\nv 2 0 0\nv 2 1 0\n\
                           o quad\nf 1 2 3 4\no tri\nf 2 6 7\nf 2 7 3\n";

#[test]
fn objects_become_flat_identity_roots_with_remapped_points() {
    let (mut scene, existing) = cube();
    let ids = import(&mut scene, TWO_OBJECTS);
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&existing));
    assert_eq!(
        names(&scene, &ids),
        [
            ("quad".into(), "quadShape".into()),
            ("tri".into(), "triShape".into())
        ]
    );
    scene.validate().unwrap();
    authored::check_scene(&scene).unwrap();
    for id in &ids {
        assert_eq!(
            scene.node(*id).unwrap().transform(),
            NativeTransform::default()
        );
    }
    let mesh = |id: NodeId| {
        authored::shape_of(&scene, scene.node(id).unwrap())
            .unwrap()
            .mesh()
    };
    let quad = mesh(ids[0]);
    assert_eq!(quad.faces().len(), 1);
    assert_eq!(quad.faces()[0].outer.len(), 4);
    assert_eq!(quad.point_count(), 4);
    let tri = mesh(ids[1]);
    // Vertices 2, 3, 6, 7 in ascending OBJ order; vertex 5 is unused and dropped.
    assert_eq!(
        tri.points(),
        &Points::Float3(vec![
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0]
        ])
    );
    assert_eq!(tri.faces().len(), 2);
    // The two triangles share the edge 2-7, so 5 edges rather than 6.
    assert_eq!(tri.edges().len(), 5);
}

#[test]
fn imported_meshes_carry_the_same_edge_flags_and_precision_as_a_primitive() {
    let (mut scene, id) = cube();
    let primitive = authored::shape_of(&scene, scene.node(id).unwrap())
        .unwrap()
        .mesh()
        .clone();
    let ids = import(&mut scene, TWO_OBJECTS);
    let imported = authored::shape_of(&scene, scene.node(ids[0]).unwrap())
        .unwrap()
        .mesh();
    assert!(matches!(imported.points(), Points::Float3(_)));
    assert!(matches!(primitive.points(), Points::Float3(_)));
    assert!(imported.edges().iter().all(|e| e.smooth() == Some(false)));
    assert!(primitive.edges().iter().all(|e| e.smooth() == Some(false)));
}

#[test]
fn names_are_sanitised_and_made_unique_against_nodes_shapes_and_reserved_names() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "pCube1").unwrap();
    scene.create(Primitive::Cube, "x").unwrap();
    let long = "a".repeat(256);
    let text = format!(
        "v 0 0 0\nv 1 0 0\nv 0 1 0\no pCube1\nf 1 2 3\no persp\nf 1 2 3\no 1abc\nf 1 2 3\no\nf 1 2 3\n\
         o \u{e9}t\u{e9}\nf 1 2 3\no xShape\nf 1 2 3\no pCube1\nf 1 2 3\no {long}\nf 1 2 3\no a-b\nf 1 2 3\n"
    );
    let ids = import(&mut scene, &text);
    let got: Vec<String> = names(&scene, &ids)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        got,
        [
            "pCube2",
            "persp1",
            "_1abc",
            "_",
            "_t_",
            "xShape1",
            "pCube3",
            long.as_str(),
            "a_b"
        ]
        .map(|name| name.replace("_t_", "__t__"))
    );
    assert_eq!(names(&scene, &ids)[0].1, "pCubeShape2");
    authored::check_scene(&scene).unwrap();
}

#[test]
fn a_second_import_of_the_same_names_gets_fresh_numbers() {
    let mut scene = NativeScene::default();
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\no m\nf 1 2 3\n";
    let first = import(&mut scene, text);
    let second = import(&mut scene, text);
    assert_eq!(names(&scene, &first)[0].0, "m");
    assert_eq!(names(&scene, &second)[0].0, "m1");
    authored::check_scene(&scene).unwrap();
}

#[test]
fn a_stale_plan_fails_without_changing_the_scene() {
    let mut scene = NativeScene::default();
    let text = "v 0 0 0\nv 1 0 0\nv 0 1 0\no a\nf 1 2 3\no b\nf 1 2 3\n";
    let plan = scene.plan_obj_import(&parsed(text)).unwrap();
    scene.create(Primitive::Cube, "b").unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.import_polygon_objects(&plan),
        Err(NativeError::NameCollision)
    );
    assert_eq!(scene, before);
}

#[test]
fn plan_counts_report_the_objects_vertices_and_faces() {
    let plan = NativeScene::default()
        .plan_obj_import(&parsed(TWO_OBJECTS))
        .unwrap();
    assert_eq!(
        (plan.object_count(), plan.vertex_count(), plan.face_count()),
        (2, 8, 3)
    );
}

#[test]
fn a_free_name_whose_shape_name_is_taken_gets_a_number() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "mShape").unwrap();
    let ids = import(&mut scene, "v 0 0 0\nv 1 0 0\nv 0 1 0\no m\nf 1 2 3\n");
    assert_eq!(names(&scene, &ids), [("m1".into(), "mShape1".into())]);
}

#[test]
fn imported_shape_names_match_the_ones_create_makes() {
    let mut created = NativeScene::default();
    let options = crate::primitive_options::PrimitiveOptions::default_for(Primitive::Cube);
    let id = created
        .create_default_named(Primitive::Cube, &options)
        .unwrap();
    let expected = names(&created, &[id]);
    let mut scene = NativeScene::default();
    let ids = import(&mut scene, "v 0 0 0\nv 1 0 0\nv 0 1 0\no pCube1\nf 1 2 3\n");
    assert_eq!(names(&scene, &ids), expected);
    assert_eq!(
        expected,
        [("pCube1".to_string(), "pCubeShape1".to_string())]
    );
}
