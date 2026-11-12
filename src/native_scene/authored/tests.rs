use super::*;
use crate::{
    native_scene::{
        AngularUnit, LinearUnit, NativeId, NativeMesh, ascii, binary, test_fixtures as fixtures,
    },
    scene::Primitive,
};
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

fn scene(names: &[&str]) -> NativeScene {
    let mut scene = NativeScene::default();
    for name in names {
        scene.create(Primitive::Cube, name).unwrap();
    }
    scene
}
fn check(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    check_content_kind(scene)?;
    check_scene(scene)
}

#[test]
fn authored_primitives_are_eligible() {
    check(&scene(&["cube", "neighbor"])).unwrap();
}

#[test]
fn every_neutral_refusal_is_reported_for_its_own_cause() {
    use AuthoredIneligible as R;
    type Case = (fn(&mut NativeScene), R);
    let cases: [Case; 25] = [
        (
            |s| s.nodes[0].rotate_pivot[0].value = 1.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].scale_pivot[0].value = -0.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].extras.shear = Some([0.0; 3]),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].restrictions.push("locked".into()),
            R::TransformRestriction,
        ),
        (
            |s| s.nodes[0].unsupported_transform.push("effect".into()),
            R::UnsupportedTransformEffect,
        ),
        (
            |s| s.nodes[0].view.local_visibility = Some(false),
            R::TransformVisibility,
        ),
        (|s| s.nodes[0].view.geometry = None, R::TransformVisibility),
        (
            |s| s.shapes[0].restrictions.push("locked".into()),
            R::ShapeRestriction,
        ),
        (
            |s| s.shapes[0].view.local_visibility = Some(false),
            R::ShapeVisibility,
        ),
        (|s| s.shapes[0].view.geometry = None, R::ShapeVisibility),
        (
            |s| s.shapes[0].mesh = NativeMesh::triangle_double_fixture(),
            R::Double3Points,
        ),
        (
            |s| s.shapes[0].mesh.add_hole_fixture(&[4, 5, 6]),
            R::FaceHoles,
        ),
        (|s| s.nodes[0].name = "3bad".into(), R::InvalidName),
        (|s| s.shapes[0].name = "bad name".into(), R::InvalidName),
        (|s| s.nodes[0].name = "persp".into(), R::ReservedName),
        (|s| s.nodes[0].name = "time1".into(), R::ReservedName),
        (
            |s| s.shapes[0].name = s.shapes[1].name.clone(),
            R::NameCollision,
        ),
        (|s| s.units.time = "pal".into(), R::UnsupportedUnits),
        (|s| s.units.linear = LinearUnit::Meter, R::UnsupportedUnits),
        (
            |s| s.units.angular = AngularUnit::Radian,
            R::UnsupportedUnits,
        ),
        (
            |s| s.nodes[0].channels[0].value = f64::NAN,
            R::InvalidScene(NativeError::NonFinite),
        ),
        (
            |s| s.nodes[0].transform_id = NativeId(0),
            R::InvalidScene(NativeError::InvalidIdentity),
        ),
        (
            |s| fixtures::add_loose_edge_unvalidated(s, [0, usize::MAX]),
            R::InvalidScene(NativeError::InvalidTopology),
        ),
        (
            |s| s.shapes[0].mesh = NativeMesh::polygon_fixture(vec![], &[]),
            R::EmptyMesh,
        ),
        (
            |s| s.shapes[0].mesh = NativeMesh::polygon_fixture(vec![[0.0; 3]], &[]),
            R::EmptyMesh,
        ),
    ];
    for (mutate, expected) in cases {
        let mut scene = scene(&["cube", "neighbor"]);
        check(&scene).unwrap();
        mutate(&mut scene);
        assert_eq!(check(&scene), Err(expected));
    }
}

#[test]
fn shared_and_grouped_shapes_are_not_the_flat_authored_shape() {
    for scene in [fixtures::shared_shape().0, fixtures::grouped_shape().0] {
        scene.validate().unwrap();
        assert_eq!(check(&scene), Err(AuthoredIneligible::NonAuthoredHierarchy));
    }
}

#[test]
fn retained_sources_and_source_free_curves_are_refused_first() {
    let ascii = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let binary = binary::decode(Arc::from(binary_fixture::specimen())).unwrap();
    for mut scene in [ascii.clone(), binary] {
        scene.units.linear = LinearUnit::Meter;
        assert_eq!(check(&scene), Err(AuthoredIneligible::SourcedScene));
    }
    let mut curve = ascii;
    curve.source = None;
    assert_eq!(check(&curve), Err(AuthoredIneligible::Curves));
}

#[test]
fn session_keys_refuse_every_authored_writer() {
    let mut scene = NativeScene::default();
    let id = scene.create(Primitive::Cube, "a").unwrap();
    scene
        .set_keys(&[id], 1, crate::native_scene::ALL_CHANNELS, 1)
        .unwrap();
    assert_eq!(check(&scene), Err(AuthoredIneligible::Keys));
    assert_eq!(
        ascii::emit_geometry_prefix(&scene).map(|_| ()),
        Err(ascii::AsciiWriteRefusal::Keys)
    );
}

#[test]
fn every_default_scene_name_is_reserved_for_any_authored_file() {
    const DEFAULT_NAMES: [&str; 25] = [
        "persp",
        "perspShape",
        "top",
        "topShape",
        "front",
        "frontShape",
        "side",
        "sideShape",
        "lambert1",
        "standardSurface1",
        "particleCloud1",
        "time1",
        "hardwareRenderingGlobals",
        "renderPartition",
        "defaultShaderList1",
        "postProcessList1",
        "ikSystem",
        "renderGlobalsList1",
        "defaultRenderingList1",
        "initialShadingGroup",
        "initialParticleSE",
        "defaultRenderGlobals",
        "defaultResolution",
        "hardwareRenderGlobals",
        "defaultLightSet",
    ];
    for name in DEFAULT_NAMES {
        let mut as_transform = scene(&["cube"]);
        as_transform.nodes[0].name = name.into();
        assert_eq!(
            check(&as_transform),
            Err(AuthoredIneligible::ReservedName),
            "{name}"
        );
        let mut as_shape = scene(&["cube"]);
        as_shape.shapes[0].name = name.into();
        assert_eq!(
            check(&as_shape),
            Err(AuthoredIneligible::ReservedName),
            "{name}"
        );
    }
}

#[test]
fn the_first_refusal_follows_the_documented_check_order() {
    use AuthoredIneligible as R;
    let mut scene = scene(&["cube", "neighbor"]);
    scene.units.linear = LinearUnit::Meter;
    scene.nodes[0].name = "persp".into();
    scene.nodes[1].rotate_pivot[0].value = 1.0;
    assert_eq!(check(&scene), Err(R::UnsupportedUnits));
    scene.units = Units::default();
    assert_eq!(check(&scene), Err(R::NonDefaultPivot));
    scene.nodes[1].rotate_pivot[0].value = 0.0;
    assert_eq!(check(&scene), Err(R::ReservedName));
}

fn transforms_only(names: &[&str]) -> NativeScene {
    let mut scene = scene(names);
    scene.shapes.clear();
    scene.dag.retain(|edge| edge.parent.is_none());
    scene
}
fn check_bare(scene: &NativeScene) -> Result<(), AuthoredIneligible> {
    check_content_kind(scene)?;
    check_transforms_only(scene)
}

#[test]
fn bare_transforms_and_the_empty_scene_are_eligible_without_shapes() {
    check_bare(&transforms_only(&["a", "b"])).unwrap();
    check_bare(&NativeScene::default()).unwrap();
}

#[test]
fn a_scene_with_a_mesh_shape_is_not_a_transforms_only_scene() {
    assert_eq!(
        check_bare(&scene(&["cube"])),
        Err(AuthoredIneligible::NonAuthoredHierarchy)
    );
}

#[test]
fn bare_transforms_keep_the_node_and_name_refusals() {
    use AuthoredIneligible as R;
    type Case = (fn(&mut NativeScene), R);
    let cases: [Case; 8] = [
        (
            |s| s.nodes[0].rotate_pivot[0].value = 1.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].extras.shear = Some([0.0; 3]),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].restrictions.push("locked".into()),
            R::TransformRestriction,
        ),
        (
            |s| s.nodes[0].view.local_visibility = Some(false),
            R::TransformVisibility,
        ),
        (|s| s.nodes[0].name = "persp".into(), R::ReservedName),
        (|s| s.nodes[0].name = "3bad".into(), R::InvalidName),
        (
            |s| s.nodes[0].name = s.nodes[1].name.clone(),
            R::NameCollision,
        ),
        (|s| s.units.linear = LinearUnit::Meter, R::UnsupportedUnits),
    ];
    for (index, (mutate, expected)) in cases.into_iter().enumerate() {
        let mut bare = transforms_only(&["a", "b"]);
        mutate(&mut bare);
        assert_eq!(check_bare(&bare), Err(expected), "case {index}");
    }
}

#[test]
fn a_parented_transform_is_not_flat() {
    let mut nested = transforms_only(&["a", "b"]);
    nested.dag[1].parent = Some(nested.nodes[0].transform_id);
    assert_eq!(
        check_bare(&nested),
        Err(AuthoredIneligible::NonAuthoredHierarchy)
    );
}
