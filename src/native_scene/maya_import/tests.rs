use super::*;
use crate::{
    native_scene::{
        NativeScene, NativeTransform,
        ascii::{AsciiDecoded, decode_document, emit_geometry_prefix},
        binary,
    },
    scene::Primitive,
};
use std::sync::Arc;

mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

fn authored(names: &[&str]) -> NativeScene {
    let mut scene = NativeScene::default();
    for name in names {
        scene.create(Primitive::Cube, name).unwrap();
    }
    scene
}

/// The scene a `.ma` file holding `scene` decodes to: the real writer and reader.
fn through_ascii(scene: &NativeScene) -> NativeScene {
    let bytes = emit_geometry_prefix(scene).unwrap();
    match decode_document(Arc::from(bytes), true).unwrap() {
        AsciiDecoded::Authored(projection) => projection.into_scene(),
        AsciiDecoded::Retained(scene) => scene,
    }
}

fn names(scene: &NativeScene) -> Vec<String> {
    scene.nodes().iter().map(|n| n.name().to_owned()).collect()
}

fn refusal(error: NativeError) -> MayaImportRefusal {
    match error {
        NativeError::MayaImport(reason) => reason,
        other => panic!("expected a Maya import refusal, got {other:?}"),
    }
}

#[test]
fn imports_ascii_meshes_with_values_intact_and_fresh_identity() {
    let mut source = authored(&["wall"]);
    let id = source.nodes()[0].id();
    let moved = NativeTransform {
        translation: [1.5, -2.0, 3.25],
        rotation: [10.0, 20.0, 30.0],
        scale: [2.0, 1.0, 0.5],
    };
    source.set_transform(id, moved).unwrap();
    let source = through_ascii(&source);
    let mut target = authored(&["floor"]);
    let plan = target
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap();
    assert_eq!(
        (plan.object_count(), plan.vertex_count(), plan.face_count()),
        (1, 8, 6)
    );
    let ids = target.import_maya_objects(&plan).unwrap();
    assert_eq!(names(&target), ["floor", "wall"]);
    let imported = target.node(ids[0]).unwrap();
    assert_eq!(imported.transform(), moved);
    assert_ne!(
        imported.transform_uuid(),
        source.nodes()[0].transform_uuid()
    );
    target.validate().unwrap();
    assert_eq!(source.nodes()[0].name(), "wall", "the source is untouched");
}

#[test]
fn clashes_get_the_lowest_free_numeric_suffix_deterministically() {
    let source = through_ascii(&authored(&["box", "box1x"]));
    let mut target = authored(&["box", "box1", "box2"]);
    let options = MayaImportOptions::default();
    let plan = target.plan_maya_import(&source, &options).unwrap();
    assert_eq!(plan.names().collect::<Vec<_>>(), ["box3", "box1x"]);
    assert_eq!(plan, target.plan_maya_import(&source, &options).unwrap());
    target.import_maya_objects(&plan).unwrap();
    // Importing the same file again resolves against the grown scene, never reusing a name.
    let again = target.plan_maya_import(&source, &options).unwrap();
    assert_eq!(again.names().collect::<Vec<_>>(), ["box4", "box1x1"]);
    target.import_maya_objects(&again).unwrap();
    let all = names(&target);
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len());
}

#[test]
fn shape_names_are_kept_and_never_collide_with_any_node() {
    let source = through_ascii(&authored(&["cube"]));
    let mut target = authored(&["cube"]);
    let plan = target
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap();
    target.import_maya_objects(&plan).unwrap();
    let all: Vec<String> = authored::names(&target)
        .map(|(_, n)| n.to_owned())
        .collect();
    let unique: std::collections::HashSet<_> = all.iter().collect();
    assert_eq!(all.len(), 4);
    assert_eq!(unique.len(), 4, "{all:?}");
}

#[test]
fn prefix_namespaces_transforms_and_shapes() {
    let source = through_ascii(&authored(&["wall"]));
    let mut target = authored(&[]);
    let options = MayaImportOptions {
        prefix: Some("ns".into()),
    };
    let plan = target.plan_maya_import(&source, &options).unwrap();
    target.import_maya_objects(&plan).unwrap();
    assert_eq!(names(&target), ["ns_wall"]);
    assert!(authored::names(&target).any(|(_, n)| n == "ns_wallShape"));
}

#[test]
fn invalid_prefixes_are_refused_before_anything_changes() {
    let source = through_ascii(&authored(&["wall"]));
    let target = authored(&[]);
    for prefix in [
        "",
        "1a",
        "a:b",
        "a b",
        "a-b",
        &"x".repeat(MAX_PREFIX_BYTES + 1),
    ] {
        let options = MayaImportOptions {
            prefix: Some(prefix.into()),
        };
        let error = target.plan_maya_import(&source, &options).unwrap_err();
        assert_eq!(
            refusal(error),
            MayaImportRefusal::InvalidPrefix,
            "{prefix:?}"
        );
    }
    let ok = MayaImportOptions {
        prefix: Some("x".repeat(MAX_PREFIX_BYTES)),
    };
    target.plan_maya_import(&source, &ok).unwrap();
}

#[test]
fn reserved_default_names_are_resolved_like_clashes() {
    let mut source = through_ascii(&authored(&["wall"]));
    source.nodes[0].name = "lambert1".into();
    let target = authored(&[]);
    let plan = target
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap();
    assert_eq!(plan.names().collect::<Vec<_>>(), ["lambert2"]);
}

#[test]
fn an_empty_source_scene_is_refused() {
    let target = authored(&[]);
    let error = target
        .plan_maya_import(&NativeScene::default(), &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(refusal(error), MayaImportRefusal::EmptyScene);
}

#[test]
fn a_target_that_cannot_take_structural_edits_refuses_first() {
    let source = through_ascii(&authored(&["wall"]));
    let sourced = binary::decode(binary_fixture::specimen().into()).unwrap();
    let error = sourced
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(error, NativeError::UnsupportedEdit);
}

#[test]
fn a_binary_source_keeps_its_raw_records_so_it_is_refused() {
    let source = binary::decode(binary_fixture::specimen().into()).unwrap();
    assert!(source.units() == &Units::default());
    let error = authored(&[])
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(refusal(error), MayaImportRefusal::UnrecognizedContent);
}

#[test]
fn each_unsupported_source_feature_has_its_own_typed_refusal() {
    use MayaImportRefusal as R;
    type Case = (fn(&mut NativeScene), R);
    let cases: [Case; 6] = [
        (
            |s| s.nodes[0].rotate_pivot[0].value = 1.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].restrictions.push("locked".into()),
            R::TransformRestriction,
        ),
        (
            |s| s.nodes[0].view.local_visibility = Some(false),
            R::Visibility,
        ),
        (
            |s| s.shapes[0].restrictions.push("locked".into()),
            R::ShapeRestriction,
        ),
        (
            |s| s.units.linear = crate::native_scene::LinearUnit::Meter,
            R::UnsupportedUnits,
        ),
        (|s| s.shapes.clear(), R::InvalidScene),
    ];
    for (index, (mutate, expected)) in cases.into_iter().enumerate() {
        let mut source = through_ascii(&authored(&["wall"]));
        mutate(&mut source);
        let error = authored(&[])
            .plan_maya_import(&source, &MayaImportOptions::default())
            .unwrap_err();
        assert_eq!(refusal(error), expected, "case {index}");
    }
}

#[test]
fn a_failed_import_leaves_the_scene_as_it_was() {
    let source = through_ascii(&authored(&["a", "b"]));
    let target = authored(&["keep"]);
    let plan = target
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap();
    // A plan made on another scene collides here: the second object's name is taken.
    let mut other = authored(&["b"]);
    let before = other.clone();
    let error = other.import_maya_objects(&plan).unwrap_err();
    assert_eq!(error, NativeError::NameCollision);
    assert!(other == before);
    assert_eq!(names(&target), ["keep"]);
}

#[test]
fn a_nested_transform_is_refused_as_non_flat() {
    let mut source = through_ascii(&authored(&["a", "b"]));
    let parent = source.nodes[0].transform_id;
    let child = source.nodes[1].transform_id;
    let edge = source.dag.iter_mut().find(|e| e.child == child).unwrap();
    edge.parent = Some(parent);
    let error = authored(&[])
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(refusal(error), MayaImportRefusal::NonFlatHierarchy);
}

#[test]
fn defined_materials_and_oversized_sources_are_refused_not_dropped() {
    let mut source = through_ascii(&authored(&["a"]));
    source.session_materials.materials[0].color = [1.0, 0.0, 0.0];
    let error = authored(&[])
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(refusal(error), MayaImportRefusal::Materials);
    let mut big = through_ascii(&authored(&["a"]));
    let node = big.nodes[0].clone();
    big.nodes = vec![node; crate::native_scene::MAX_TRANSFORM_PATHS + 1];
    let error = authored(&[])
        .plan_maya_import(&big, &MayaImportOptions::default())
        .unwrap_err();
    assert_eq!(refusal(error), MayaImportRefusal::TooManyObjects);
}

#[test]
fn exactly_the_object_cap_is_accepted() {
    let mut source = through_ascii(&authored(&["a"]));
    let node = source.nodes[0].clone();
    source.nodes = vec![node; crate::native_scene::MAX_TRANSFORM_PATHS];
    let error = authored(&[])
        .plan_maya_import(&source, &MayaImportOptions::default())
        .unwrap_err();
    assert_ne!(refusal_of(&error), Some(MayaImportRefusal::TooManyObjects));
}

fn refusal_of(error: &NativeError) -> Option<MayaImportRefusal> {
    match error {
        NativeError::MayaImport(reason) => Some(*reason),
        _ => None,
    }
}
