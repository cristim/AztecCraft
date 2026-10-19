use super::*;
use crate::{
    native_scene::{AngularUnit, LinearUnit, NativeId, NativeMesh, test_fixtures as fixtures},
    scene::Primitive,
};
use std::sync::Arc;
const PLANE_ARRAY: &[u8] = b"\tsetAttr -s 4 \".vt[0:3]\"\n\
\t-0.5 0 -0.5\n\
\t-0.5 0 0.5\n\
\t0.5 0 0.5\n\
\t0.5 0 -0.5\n\
;\n\
\tsetAttr -s 4 \".ed[0:3]\"\n\
\t0 1 0\n\
\t1 2 0\n\
\t2 3 0\n\
\t3 0 0\n\
;\n\
\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"\n\
\tf 4 0 1 2 3\n\
;\n";
const CUBE_FACES: &[u8] = b"\tsetAttr -s 6 \".fc[0:5]\" -type \"polyFaces\"\n\
\tf 4 0 1 2 3\n\
\tf 4 4 5 6 7\n\
\tf 4 8 -8 9 -1\n\
\tf 4 -3 10 -6 11\n\
\tf 4 -4 -12 -5 -9\n\
\tf 4 -10 -7 -11 -2\n\
;\n";
#[path = "../../../../tests/support/maya_ascii.rs"]
mod ascii_fixture;
#[path = "../../../../tests/support/maya_ascii_structure/roster.rs"]
mod roster;
#[path = "../../../../tests/support/maya_ascii_structure/topology.rs"]
mod topology;
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
fn prefix(scene: &NativeScene) -> Result<Vec<u8>, AsciiWriteRefusal> {
    let original = scene.clone();
    let result = emit_geometry_prefix(scene);
    assert_eq!(scene, &original);
    assert_eq!(
        (scene.next_id, scene.next_native_id),
        (original.next_id, original.next_native_id)
    );
    result
}

#[test]
fn emitted_multi_node_prefix_is_accepted_by_the_framing_and_roster_oracles() {
    let mut scene = scene(&["first", "second"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    let bytes = prefix(&scene).unwrap();
    let report = roster::inspect(&bytes).unwrap();
    assert_eq!(report.boundary, bytes.len());
    assert!(report.tail.is_empty());
    assert_eq!(report.shapes, ["firstShape", "secondShape"]);
    let declared: Vec<_> = report.declared.iter().map(String::as_str).collect();
    assert_eq!(declared, ["first", "firstShape", "second", "secondShape"]);
}

#[test]
fn plane_face_output_preserves_stored_order() {
    let mut scene = scene(&["plane"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    let bytes = prefix(&scene).unwrap();
    assert!(
        bytes.ends_with(b"\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"\n\tf 4 0 1 2 3\n;\n")
    );
}

#[test]
fn cube_faces_keep_literal_signed_edge_order() {
    let bytes = prefix(&scene(&["cube"])).unwrap();
    assert!(bytes.ends_with(CUBE_FACES));
}

#[test]
fn preflight_refusals_preserve_the_scene_and_canonical_causes() {
    use AsciiWriteRefusal as R;
    type Case = (fn(&mut NativeScene), R);
    let cases: [Case; 34] = [
        (
            |s| s.nodes[0].rotate_pivot[0].value = 1.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].rotate_pivot[0].value = -0.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].scale_pivot[0].value = 1.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].scale_pivot[0].value = -0.0,
            R::NonDefaultPivot,
        ),
        (
            |s| s.nodes[0].extras.rotate_order = Some(0),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].extras.rotate_axis = Some([0.0; 3]),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].extras.shear = Some([0.0; 3]),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].extras.rotate_pivot_translate = Some([0.0; 3]),
            R::TransformExtras,
        ),
        (
            |s| s.nodes[0].extras.scale_pivot_translate = Some([0.0; 3]),
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
        (
            |s| s.nodes[0].view.local_visibility = None,
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
        (
            |s| s.shapes[0].view.local_visibility = None,
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
        (|s| s.units.linear = LinearUnit::Inch, R::UnsupportedUnits),
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
            |s| s.dag[0].child = NativeId(0),
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
    ];
    for (mutate, expected) in cases {
        let mut scene = scene(&["cube", "neighbor"]);
        scene.validate().unwrap();
        prefix(&scene).unwrap();
        mutate(&mut scene);
        if !matches!(expected, R::InvalidScene(_)) {
            scene.validate().unwrap();
        }
        let error = prefix(&scene).unwrap_err();
        if let R::InvalidScene(cause) = &expected {
            assert_eq!(
                error.source().unwrap().downcast_ref::<NativeError>(),
                Some(cause)
            );
        } else {
            assert!(error.source().is_none());
        }
        assert_eq!(error, expected);
    }
    assert_eq!(
        prefix(&scene(&["cube", "cubeShape"])),
        Err(R::NameCollision)
    );
    for scene in [fixtures::shared_shape().0, fixtures::grouped_shape().0] {
        scene.validate().unwrap();
        assert_eq!(prefix(&scene), Err(R::NonAuthoredHierarchy));
    }
    let mut no_faces = scene(&["cube"]);
    no_faces.shapes[0].mesh = NativeMesh::polygon_fixture(vec![[0.0; 3]], &[]);
    no_faces.validate().unwrap();
    assert_eq!(prefix(&no_faces), Err(R::EmptyMesh));
}

#[test]
fn sourced_formats_and_source_free_curves_refuse_without_touching_retention() {
    let ascii = super::super::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let binary =
        crate::native_scene::binary::decode(Arc::from(binary_fixture::specimen())).unwrap();
    for scene in [ascii.clone(), binary] {
        let source = scene.source.as_ref().unwrap().bytes.clone();
        assert_eq!(prefix(&scene), Err(AsciiWriteRefusal::SourcedScene));
        assert!(Arc::ptr_eq(&source, &scene.source.as_ref().unwrap().bytes));
    }
    let mut curve = ascii;
    curve.source = None;
    assert_eq!(prefix(&curve), Err(AsciiWriteRefusal::Curves));
}

#[test]
fn transform_prefix_keeps_uuid_values_and_relative_statements_with_their_owner() {
    let mut scene = scene(&["second", "first"]);
    for shape in &mut scene.shapes {
        shape.mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    }
    scene.nodes[0].transform_uuid = Some(uuid::Uuid::from_u128(0x6c1b2d4e0a3f4b5c8d7e9f0a1b2c3d4e));
    scene.nodes[1].transform_uuid = Some(uuid::Uuid::from_u128(0x11111111222233334444555555555555));
    scene.shapes[0].name = "storedSecond".into();
    scene.shapes[1].name = "storedFirst".into();
    scene.shapes[0].uuid = Some(uuid::Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa));
    scene.shapes[1].uuid = Some(uuid::Uuid::from_u128(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb));
    scene.shapes.swap(0, 1);
    scene.nodes[0].channels[0].value = -0.0;
    scene.nodes[0].channels[3].value = 90.0;
    scene.nodes[0].channels[6].value = -2.0;
    let values = [
        2.54,
        1e-9,
        1e15,
        0.1,
        1.0 / 3.0,
        std::f64::consts::PI,
        1.5,
        -2.25,
        3.0,
    ];
    for (channel, value) in scene.nodes[1].channels.iter_mut().zip(values) {
        channel.value = value;
    }
    let golden = [
        b"//Maya ASCII 2026 scene\n\
requires maya \"2026\";\n\
currentUnit -l centimeter -a degree -t film;\n\
fileInfo \"AztecCraft\" \"1\";\n\
createNode transform -n \"second\";\n\
\trename -uid \"6c1b2d4e-0a3f-4b5c-8d7e-9f0a1b2c3d4e\";\n\
\tsetAttr \".t\" -type \"double3\" -0 0 0;\n\
\tsetAttr \".r\" -type \"double3\" 90 0 0;\n\
\tsetAttr \".s\" -type \"double3\" -2 1 1;\n\
createNode mesh -n \"storedSecond\" -p \"second\";\n\
\trename -uid \"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa\";\n"
            .as_slice(),
        PLANE_ARRAY,
        b"createNode transform -n \"first\";\n\
\trename -uid \"11111111-2222-3333-4444-555555555555\";\n\
\tsetAttr \".t\" -type \"double3\" 2.54 0.000000001 1000000000000000;\n\
\tsetAttr \".r\" -type \"double3\" 0.1 0.3333333333333333 3.141592653589793;\n\
\tsetAttr \".s\" -type \"double3\" 1.5 -2.25 3;\n\
createNode mesh -n \"storedFirst\" -p \"first\";\n\
\trename -uid \"bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb\";\n",
        PLANE_ARRAY,
    ]
    .concat();
    let actual = prefix(&scene).unwrap();
    assert_eq!(actual, golden);
    assert_eq!(actual.len(), prefix_bytes(&scene).unwrap());
    let text = std::str::from_utf8(&actual).unwrap();
    let uuids: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("\trename -uid \""))
        .map(|tail| uuid::Uuid::parse_str(tail.strip_suffix("\";").unwrap()).unwrap())
        .collect();
    assert_eq!(
        uuids,
        [
            scene.nodes[0].transform_uuid(),
            scene.shapes[1].uuid(),
            scene.nodes[1].transform_uuid(),
            scene.shapes[0].uuid()
        ]
        .map(Option::unwrap)
    );
    assert_eq!(
        scene
            .internal_transform(scene.nodes[0].id())
            .unwrap()
            .rotation_radians[0]
            .to_bits(),
        std::f64::consts::FRAC_PI_2.to_bits()
    );
    scene.nodes[1].transform_uuid = None;
    assert_eq!(prefix(&scene).unwrap().len(), golden.len() - 53);
    scene.shapes[0].uuid = None;
    let omitted = prefix(&scene).unwrap();
    assert_eq!(
        std::str::from_utf8(&omitted)
            .unwrap()
            .matches("rename -uid")
            .count(),
        2
    );
    assert_eq!(parsed_prefix(&omitted).channels.len(), 6);
    assert_eq!(prefix(&NativeScene::default()).unwrap(), HEADER.as_bytes());
    let mut legacy = crate::scene::Scene::default();
    legacy.create(Primitive::Cube, "legacy").unwrap();
    let mut scenes = vec![NativeScene::from_legacy(legacy).unwrap()];
    for primitive in [
        Primitive::Cube,
        Primitive::Plane,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ] {
        let mut scene = NativeScene::default();
        scene.create(primitive, "primitive").unwrap();
        scenes.push(scene);
    }
    let mut extruded = NativeScene::default();
    extruded.create(Primitive::Cube, "extruded").unwrap();
    extruded
        .extrude_face(extruded.shapes[0].id, 1, 0.5)
        .unwrap();
    scenes.push(extruded);
    for mut scene in scenes {
        scene.create(Primitive::Cube, "neighbor").unwrap();
        scene.shapes.swap(0, 1);
        scene.validate().unwrap();
        let bytes = prefix(&scene).unwrap();
        assert_eq!(bytes.len(), prefix_bytes(&scene).unwrap());
        let blocks = parsed_prefix(&bytes).blocks;
        assert_eq!(blocks.len(), 6);
        for (index, node) in scene.nodes.iter().enumerate() {
            let shape = child(&scene, node).unwrap();
            let face_block = &blocks[index * 3 + 2];
            assert_eq!((face_block.0, face_block.1), (shape.name(), ".fc"));
            assert_eq!(face_block.2.len(), shape.mesh().faces().len());
            for (row, face) in face_block.2.iter().zip(shape.mesh().faces()) {
                assert_eq!(row[0], "f");
                assert_eq!(row[1].parse::<usize>().unwrap(), face.outer().len());
                let actual = row[2..]
                    .iter()
                    .map(|word| word.parse::<i64>().unwrap())
                    .collect::<Vec<_>>();
                let stored = face
                    .outer()
                    .iter()
                    .map(|edge| {
                        let id = i64::try_from(edge.edge).unwrap();
                        if edge.reversed { -id - 1 } else { id }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, stored);
            }
        }
        let mesh = child(&scene, &scene.nodes[0]).unwrap().mesh();
        let Points::Float3(points) = mesh.points() else {
            panic!()
        };
        assert_eq!(
            (blocks[0].2.len(), blocks[1].2.len(), blocks[2].2.len()),
            (points.len(), mesh.edges().len(), mesh.faces().len())
        );
        for (row, point) in blocks[0].2.iter().zip(points) {
            let row: [&str; 3] = row.clone().try_into().unwrap();
            assert_eq!(
                row.map(|v| v.parse::<f32>().unwrap().to_bits()),
                point.map(f32::to_bits)
            );
        }
        for (row, edge) in blocks[1].2.iter().zip(mesh.edges()) {
            let row: [&str; 3] = row.clone().try_into().unwrap();
            let [a, b] = edge.endpoints();
            assert_eq!(row.map(|v| v.parse::<usize>().unwrap()), [a, b, 0]);
        }
    }
}

#[test]
fn emitted_representative_owners_have_independent_local_topology() {
    let mut scenes = Vec::new();
    let mut legacy = crate::scene::Scene::default();
    legacy.create(Primitive::Cube, "legacy").unwrap();
    scenes.push((
        NativeScene::from_legacy(legacy).unwrap(),
        "legacyShape",
        (8, 12, 6),
        true,
        Some(1.0),
    ));
    for (primitive, counts, closed, volume) in [
        (Primitive::Cube, (8, 12, 6), true, Some(1.0)),
        (Primitive::Plane, (4, 4, 1), false, Some(0.0)),
        (Primitive::Sphere, (382, 780, 400), true, None),
        (Primitive::Cylinder, (40, 60, 22), true, None),
        (Primitive::Cone, (21, 40, 21), true, None),
    ] {
        let mut scene = NativeScene::default();
        scene.create(primitive, "primitive").unwrap();
        scenes.push((scene, "primitiveShape", counts, closed, volume));
    }
    let mut extruded = NativeScene::default();
    extruded.create(Primitive::Cube, "extruded").unwrap();
    extruded
        .extrude_face(extruded.shapes[0].id, 1, 0.5)
        .unwrap();
    scenes.push((extruded, "extrudedShape", (12, 20, 10), true, Some(1.5)));

    for (mut scene, owner, counts, closed, volume) in scenes {
        scene.create(Primitive::Cube, "neighbor").unwrap();
        scene.shapes.swap(0, 1);
        scene.validate().unwrap();
        let bytes = prefix(&scene).unwrap();
        let blocks = parsed_prefix(&bytes).blocks;
        assert_eq!(blocks.len(), 6);
        for (owner_blocks, (expected_owner, expected_counts, is_closed, exact_volume)) in
            blocks.as_chunks::<3>().0.iter().zip([
                (owner, counts, closed, volume),
                ("neighborShape", (8, 12, 6), true, Some(1.0)),
            ])
        {
            assert!(owner_blocks.iter().all(|block| block.0 == expected_owner));
            let mesh = output_mesh(owner_blocks);
            assert_eq!(
                (mesh.points.len(), mesh.edges.len(), mesh.faces.len()),
                expected_counts
            );
            mesh.check(is_closed).unwrap();
            let actual_volume = mesh.signed_volume().unwrap();
            if is_closed {
                assert!(actual_volume > 0.0, "{expected_owner}: {actual_volume}");
            } else {
                assert!(mesh.check(true).is_err());
                assert_eq!(actual_volume, 0.0);
            }
            if let Some(expected_volume) = exact_volume {
                assert!((actual_volume - expected_volume).abs() < 1e-6);
            }
            if expected_owner == "legacyShape" {
                let mut reversed = output_mesh(owner_blocks);
                for face in &mut reversed.faces {
                    face.reverse();
                    for token in face {
                        *token = !*token;
                    }
                }
                reversed.check(true).unwrap();
                assert!(reversed.signed_volume().unwrap() < 0.0);
                let mut broken = output_mesh(owner_blocks);
                broken.faces[0].swap(1, 2);
                assert!(broken.loops().is_err());
            }
        }
    }
}

#[test]
fn child_work_counts_are_checked_before_traversal() {
    use crate::maya_ascii::ErrorKind;
    assert_eq!(mesh_declaration_work(0, 0, 0).unwrap(), 0);
    assert_eq!(mesh_declaration_work(100, 100, 200).unwrap(), 76_400);
    for (nodes, shapes, dag) in [(usize::MAX, 0, 0), (1, usize::MAX, 1), (1, 0, usize::MAX)] {
        assert_eq!(
            mesh_declaration_work(nodes, shapes, dag)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
}

struct ParsedPrefix<'a> {
    channels: Vec<(&'a str, &'a str, [f64; 3])>,
    blocks: Vec<(&'a str, &'a str, Vec<Vec<&'a str>>)>,
}

fn parsed_prefix(bytes: &[u8]) -> ParsedPrefix<'_> {
    assert!(bytes.ends_with(b"\n"));
    let body = bytes.strip_prefix(HEADER.as_bytes()).unwrap();
    let mut lines = std::str::from_utf8(body).unwrap().split_terminator('\n');
    let empty_scene = NativeScene::default();
    let mut transform = None;
    let mut owner = None;
    let mut transform_uuid = false;
    let mut shape_uuid = false;
    let mut next_channel = 0;
    let mut next_array = 0;
    let mut point_count = None;
    let mut edge_count = None;
    let mut parsed = ParsedPrefix {
        channels: Vec::new(),
        blocks: Vec::new(),
    };
    while let Some(line) = lines.next() {
        if let Some(name) = line.strip_prefix("createNode transform -n \"") {
            assert!(transform.is_none() || next_array == 3);
            let name = name.strip_suffix("\";").unwrap();
            assert!(empty_scene.validate_new_name(None, name).is_ok());
            transform = Some(name);
            owner = None;
            transform_uuid = false;
            next_channel = 0;
            next_array = 0;
            continue;
        }
        if let Some(declaration) = line.strip_prefix("createNode mesh -n \"") {
            assert!(transform.is_some() && owner.is_none());
            let (name, parent) = declaration.split_once("\" -p \"").unwrap();
            assert!(empty_scene.validate_new_name(None, name).is_ok());
            assert_eq!(parent.strip_suffix("\";").unwrap(), transform.unwrap());
            owner = Some(name);
            shape_uuid = false;
            point_count = None;
            edge_count = None;
            continue;
        }
        if let Some(value) = line.strip_prefix("\trename -uid \"") {
            let value = value.strip_suffix("\";").unwrap();
            let uuid = uuid::Uuid::parse_str(value).unwrap();
            assert_eq!(uuid.hyphenated().to_string(), value);
            if owner.is_some() {
                assert!(next_array == 0 && !shape_uuid);
                shape_uuid = true;
            } else {
                assert!(transform.is_some() && next_channel == 0 && !transform_uuid);
                transform_uuid = true;
            }
            continue;
        }
        if let Some(row) = line.strip_prefix("\tsetAttr \"") {
            assert!(transform.is_some() && owner.is_none());
            let (attribute, values) = row.split_once("\" -type \"double3\" ").unwrap();
            let index = [".t", ".r", ".s"]
                .iter()
                .position(|known| *known == attribute)
                .unwrap();
            assert!(index >= next_channel);
            next_channel = index + 1;
            let values = values
                .strip_suffix(';')
                .unwrap()
                .split(' ')
                .map(|word| word.parse::<f64>().unwrap())
                .collect::<Vec<_>>();
            let values: [f64; 3] = values.try_into().unwrap();
            assert!(values.iter().all(|value| value.is_finite()));
            parsed
                .channels
                .push((transform.unwrap(), attribute, values));
            continue;
        }
        let header = line.strip_prefix("\tsetAttr -s ").unwrap();
        assert!(owner.is_some() && next_array < 3);
        let (count, range) = header.split_once(" \"").unwrap();
        let count = count.parse::<usize>().unwrap();
        let (attribute, last) = range.split_once("[0:").unwrap();
        assert_eq!(attribute, [".vt", ".ed", ".fc"][next_array]);
        let suffix = if attribute == ".fc" {
            "]\" -type \"polyFaces\""
        } else {
            "]\""
        };
        let last = last.strip_suffix(suffix).unwrap().parse::<usize>().unwrap();
        assert!(count > 0 && count <= i32::MAX as usize);
        assert_eq!(last.checked_add(1), Some(count));
        let mut rows = Vec::new();
        for _ in 0..count {
            let row = lines.next().unwrap().strip_prefix('\t').unwrap();
            let values = row.split(' ').collect::<Vec<_>>();
            if attribute == ".fc" {
                assert_eq!(values.first(), Some(&"f"));
                let corners = values[1].parse::<i32>().unwrap();
                assert!(corners >= 3);
                let corners = usize::try_from(corners).unwrap();
                assert_eq!(values.len(), corners + 2);
                for token in &values[2..] {
                    let token = token.parse::<i64>().unwrap();
                    let index = if token < 0 { !token } else { token };
                    assert!(usize::try_from(index).unwrap() < edge_count.unwrap());
                }
            } else {
                assert_eq!(values.len(), 3);
                for (index, value) in values.iter().enumerate() {
                    if attribute == ".vt" {
                        assert!(value.parse::<f32>().unwrap().is_finite());
                    } else {
                        let value = value.parse::<i32>().unwrap();
                        assert!(value >= 0);
                        if index == 2 {
                            assert_eq!(value, 0);
                        } else {
                            assert!((value as usize) < point_count.unwrap());
                        }
                    }
                }
            }
            rows.push(values);
        }
        assert_eq!(lines.next(), Some(";"));
        if attribute == ".vt" {
            point_count = Some(count);
        }
        if attribute == ".ed" {
            edge_count = Some(count);
        }
        parsed.blocks.push((owner.unwrap(), attribute, rows));
        next_array += 1;
    }
    assert!(transform.is_none() || next_array == 3);
    parsed
}

fn output_mesh(blocks: &[(&str, &str, Vec<Vec<&str>>)]) -> topology::Mesh {
    assert_eq!(blocks.len(), 3);
    assert_eq!(
        (blocks[0].1, blocks[1].1, blocks[2].1),
        (".vt", ".ed", ".fc")
    );
    let points = blocks[0]
        .2
        .iter()
        .map(|row| {
            let [x, y, z] = row.as_slice() else { panic!() };
            [x.parse().unwrap(), y.parse().unwrap(), z.parse().unwrap()]
        })
        .collect();
    let edges = blocks[1]
        .2
        .iter()
        .map(|row| {
            let [start, end, _flag] = row.as_slice() else {
                panic!()
            };
            [start.parse().unwrap(), end.parse().unwrap()]
        })
        .collect();
    let faces = blocks[2]
        .2
        .iter()
        .map(|row| {
            row[2..]
                .iter()
                .map(|token| token.parse().unwrap())
                .collect()
        })
        .collect();
    topology::Mesh {
        points,
        edges,
        faces,
    }
}

#[test]
fn strict_prefix_rejects_crlf_in_emitted_body() {
    let mut scene = scene(&["plane"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    let bytes = prefix(&scene).unwrap();
    parsed_prefix(&bytes);
    for line in [
        "createNode transform -n \"plane\";\n",
        "\t-0.5 0 -0.5\n",
        ";\n\tsetAttr -s 4 \".ed",
    ] {
        rejects_replacement(&bytes, line, &line.replacen('\n', "\r\n", 1));
    }
}

#[test]
fn prefix_extractor_rejects_unknown_trailing_statement() {
    let mut bytes = prefix(&scene(&["cube"])).unwrap();
    bytes.extend_from_slice(b"unknownStatement;\n");
    assert!(std::panic::catch_unwind(|| parsed_prefix(&bytes)).is_err());
}

fn rejects_replacement(bytes: &[u8], old: &str, new: &str) {
    let source = std::str::from_utf8(bytes).unwrap();
    assert!(source.contains(old), "missing mutation anchor {old}");
    let changed = source.replacen(old, new, 1);
    assert!(
        std::panic::catch_unwind(|| parsed_prefix(changed.as_bytes())).is_err(),
        "accepted mutation of {old} to {new}"
    );
}

#[test]
fn strict_prefix_rejects_malformed_statements_and_arrays() {
    let mut scene = scene(&["first"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    scene.nodes[0].transform_uuid = Some(uuid::Uuid::nil());
    scene.shapes[0].uuid = Some(uuid::Uuid::from_u128(1));
    scene.nodes[0].channels[0].value = 0.1;
    scene.nodes[0].channels[3].value = 0.2;
    scene.nodes[0].channels[6].value = 2.0;
    let bytes = prefix(&scene).unwrap();
    parsed_prefix(&bytes);
    let transform_uuid = "\trename -uid \"00000000-0000-0000-0000-000000000000\";\n";
    let shape_uuid = "\trename -uid \"00000000-0000-0000-0000-000000000001\";\n";
    let translate = "\tsetAttr \".t\" -type \"double3\" 0.1 0 0;\n";
    let rotate = "\tsetAttr \".r\" -type \"double3\" 0.2 0 0;\n";
    let scale = "\tsetAttr \".s\" -type \"double3\" 2 1 1;\n";
    for (old, new) in [
        ("//Maya ASCII 2026 scene", "//Maya ASCII 2025 scene"),
        ("requires maya \"2026\";", "requires maya \"2025\";"),
        (
            "createNode transform -n \"first\";",
            "createNode transform -n \"first\"",
        ),
        (
            "createNode mesh -n \"firstShape\" -p \"first\";",
            "createNode mesh -n \"firstShape\" -p \"other\";",
        ),
        (
            "createNode mesh -n \"firstShape\" -p \"first\";",
            "createNode mesh -n \"firstShape\" -p \"first\";\ncreateNode mesh -n \"extra\" -p \"first\";",
        ),
        (shape_uuid, "\trename -uid \"not-a-uuid\";\n"),
        ("\tsetAttr -s 4 \".vt[0:3]\"", "\tsetAttr -s 3 \".vt[0:3]\""),
        ("\tsetAttr -s 4 \".vt[0:3]\"", "\tsetAttr -s 4 \".vt[0:2]\""),
        ("\tsetAttr -s 4 \".vt[0:3]\"", "\tsetAttr -s 4 \".vt[1:4]\""),
        (
            "\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"",
            "\tsetAttr -s 1 \".fc[0:0]\" -type \"wrong\"",
        ),
        ("\t-0.5 0 -0.5\n", "\t-0.5 0\n"),
        ("\t-0.5 0 -0.5\n", "\tNaN 0 -0.5\n"),
        ("\t0 1 0\n", "\t0 1 1\n"),
        ("\t0 1 0\n", "\t0 4 0\n"),
        ("\t0 1 0\n", "\t-1 1 0\n"),
        ("\tf 4 0 1 2 3\n", "\tf 5 0 1 2 3\n"),
        ("\tf 4 0 1 2 3\n", "\tf 4 0 1 2 4\n"),
        ("\tf 4 0 1 2 3\n", "\tf 4 0 1 2 -5\n"),
        ("\tf 4 0 1 2 3\n", "\tf 2 0 1\n"),
        ("\tf 4 0 1 2 3\n", "\tf 0\n"),
        (";\n\tsetAttr -s 4 \".ed", "!\n\tsetAttr -s 4 \".ed"),
        (";\n\tsetAttr -s 1 \".fc", "\tsetAttr -s 1 \".fc"),
    ] {
        rejects_replacement(&bytes, old, new);
    }
    rejects_replacement(&bytes, transform_uuid, &transform_uuid.repeat(2));
    rejects_replacement(&bytes, shape_uuid, &shape_uuid.repeat(2));
    rejects_replacement(&bytes, translate, &translate.repeat(2));
    rejects_replacement(
        &bytes,
        &format!("{translate}{rotate}"),
        &format!("{rotate}{translate}"),
    );
    rejects_replacement(
        &bytes,
        &format!("{rotate}{scale}"),
        &format!("{scale}{rotate}"),
    );
    rejects_replacement(
        &bytes,
        &format!("{transform_uuid}{translate}"),
        &format!("{translate}{transform_uuid}"),
    );
    let moved_shape_uuid = format!(
        "{}{shape_uuid}",
        std::str::from_utf8(&bytes)
            .unwrap()
            .replacen(shape_uuid, "", 1)
    );
    assert!(std::panic::catch_unwind(|| parsed_prefix(moved_shape_uuid.as_bytes())).is_err());
    rejects_replacement(
        &bytes,
        translate,
        "\tsetAttr \".t\" -type \"double3\" NaN 0 0;\n",
    );
    rejects_replacement(&bytes, "\tsetAttr -s 4 \".vt[0:3]\"\n", "");
    rejects_replacement(&bytes, "\tsetAttr -s 4 \".ed[0:3]\"\n", "");
    rejects_replacement(
        &bytes,
        "\tsetAttr -s 1 \".fc[0:0]\" -type \"polyFaces\"\n",
        "",
    );
    let source = std::str::from_utf8(&bytes).unwrap();
    let vt = source.find("\tsetAttr -s 4 \".vt").unwrap();
    let ed = source.find("\tsetAttr -s 4 \".ed").unwrap();
    let fc = source.find("\tsetAttr -s 1 \".fc").unwrap();
    for changed in [
        format!("{}{}", &source[..vt], &source[ed..]),
        format!("{}{}{}", &source[..ed], &source[vt..ed], &source[ed..]),
        format!("{}{}", &source[..vt], source[vt..ed].repeat(3)),
        format!("{}{}", &source[..ed], &source[fc..]),
        source[..fc].to_owned(),
        source[..source.len() - 1].to_owned(),
        format!("{source}\n"),
    ] {
        assert!(std::panic::catch_unwind(|| parsed_prefix(changed.as_bytes())).is_err());
    }
}

#[test]
fn strict_prefix_requires_each_transform_to_close_before_transition_or_eof() {
    let mut scene = scene(&["first", "second"]);
    for shape in &mut scene.shapes {
        shape.mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    }
    let bytes = prefix(&scene).unwrap();
    let source = std::str::from_utf8(&bytes).unwrap();
    let mesh = source.find("createNode mesh -n \"firstShape\"").unwrap();
    let second = source.find("createNode transform -n \"second\"").unwrap();
    for changed in [
        source[..mesh].to_owned(),
        format!("{}{}", &source[..mesh], &source[second..]),
    ] {
        assert!(std::panic::catch_unwind(|| parsed_prefix(changed.as_bytes())).is_err());
    }
    assert!(parsed_prefix(HEADER.as_bytes()).blocks.is_empty());
}

#[test]
fn strict_prefix_rejects_invalid_declaration_names() {
    let bytes = prefix(&scene(&["cube"])).unwrap();
    let source = std::str::from_utf8(&bytes).unwrap();
    for name in ["bad name", "3bad", "bad:namespace", "bad\"quote"] {
        let transform = source
            .replacen(
                "createNode transform -n \"cube\";",
                &format!("createNode transform -n \"{name}\";"),
                1,
            )
            .replacen("-p \"cube\";", &format!("-p \"{name}\";"), 1);
        let shape = source.replacen(
            "createNode mesh -n \"cubeShape\"",
            &format!("createNode mesh -n \"{name}\""),
            1,
        );
        for changed in [transform, shape] {
            assert!(std::panic::catch_unwind(|| parsed_prefix(changed.as_bytes())).is_err());
        }
    }
}

#[test]
fn strict_prefix_accepts_admitted_unused_duplicate_and_self_loop_edges() {
    let mut scene = scene(&["odd"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    fixtures::add_loose_edge_unvalidated(&mut scene, [0, 1]);
    fixtures::add_loose_edge_unvalidated(&mut scene, [0, 0]);
    scene.validate().unwrap();
    let bytes = prefix(&scene).unwrap();
    let blocks = parsed_prefix(&bytes).blocks;
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[1].2.len(), 6);
    assert_eq!(blocks[1].2[4], ["0", "1", "0"]);
    assert_eq!(blocks[1].2[5], ["0", "0", "0"]);
    assert_eq!(blocks[2].2[0], ["f", "4", "0", "1", "2", "3"]);
    let doubled = std::str::from_utf8(&bytes).unwrap().replacen(
        "\tf 4 0 1 2 3\n",
        "\tf 8 0 1 2 3 0 1 2 3\n",
        1,
    );
    assert_eq!(
        parsed_prefix(doubled.as_bytes()).blocks[2].2[0],
        ["f", "8", "0", "1", "2", "3", "0", "1", "2", "3"]
    );
}

#[test]
fn raw_float_arrays_keep_owner_bits_unused_points_and_duplicate_edges() {
    use glam::Vec3;
    let values = [
        0.0,
        0.1,
        f32::MAX,
        f32::from_bits(f32::MAX.to_bits() - 1),
        f32::MIN_POSITIVE,
        f32::from_bits(f32::MIN_POSITIVE.to_bits() - 1),
        f32::from_bits(f32::MIN_POSITIVE.to_bits() + 1),
        f32::from_bits(1),
        f32::from_bits(2),
    ];
    for value in values.into_iter().flat_map(|value| [value, -value]) {
        for slot in 0..3 {
            let mut points = vec![
                Vec3::new(0.1, 0.0, 0.0),
                Vec3::X,
                Vec3::new(1.0, 0.0, 1.0),
                Vec3::Z,
                Vec3::ZERO,
            ];
            points[4][slot] = value;
            let mesh = crate::mesh::Mesh::from_view(points.clone(), vec![[0, 1, 2, 3]]).unwrap();
            let mut scene = scene(&["quad", "neighbor"]);
            scene.shapes[0].mesh = NativeMesh::from_legacy(&mesh);
            fixtures::add_loose_edge_unvalidated(&mut scene, [0, 1]);
            scene.shapes.swap(0, 1);
            scene.validate().unwrap();
            let bytes = prefix(&scene).unwrap();
            assert_eq!(bytes.len(), prefix_bytes(&scene).unwrap());
            let blocks = parsed_prefix(&bytes).blocks;
            assert_eq!(
                (blocks[0].0, blocks[0].1, blocks[0].2[0].as_slice()),
                ("quadShape", ".vt", ["0.1", "0", "0"].as_slice())
            );
            assert_eq!(
                (blocks[3].0, blocks[3].2.len(), blocks[4].2.len()),
                ("neighborShape", 8, 12)
            );
            assert_eq!(
                blocks[0]
                    .2
                    .iter()
                    .map(|row| row
                        .iter()
                        .map(|v| v.parse::<f32>().unwrap().to_bits())
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                points
                    .iter()
                    .map(|point| point.to_array().map(f32::to_bits).to_vec())
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                blocks[1].2,
                vec![
                    vec!["0", "1", "0"],
                    vec!["1", "2", "0"],
                    vec!["2", "3", "0"],
                    vec!["3", "0", "0"],
                    vec!["0", "1", "0"]
                ]
            );
        }
    }
}

#[test]
fn actual_mesh_producer_propagates_both_consumers_at_each_chunk_boundary() {
    use AsciiWriteRefusal as R;
    let mut scene = scene(&["plane"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    scene.validate().unwrap();
    prefix(&scene).unwrap();
    let mesh = scene.shapes[0].mesh();
    assert_eq!(PLANE_ARRAY.len(), 188);
    let mut chunks = Vec::new();
    mesh_arrays(mesh, |bytes| {
        chunks.push(bytes.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(chunks.concat(), PLANE_ARRAY);
    let mut limits = vec![0, PLANE_ARRAY.len()];
    let mut boundary = 0;
    for chunk in &chunks {
        boundary += chunk.len();
        limits.extend([boundary - 1, boundary]);
    }
    limits.sort_unstable();
    limits.dedup();
    for available in limits {
        let mut accepted = 0;
        let mut visits = 0;
        for chunk in &chunks {
            visits += 1;
            if accepted + chunk.len() > available {
                break;
            }
            accepted += chunk.len();
        }
        let expected = if available == PLANE_ARRAY.len() {
            Ok(())
        } else {
            Err(R::TooLarge)
        };
        let mut length = MAX_BYTES - available;
        let mut calls = 0;
        let result = mesh_arrays(mesh, |bytes| {
            calls += 1;
            length = add_prefix_bytes(length, bytes.len())?;
            Ok(())
        });
        assert_eq!(result, expected);
        assert_eq!(
            (length - (MAX_BYTES - available), calls),
            (accepted, visits)
        );
        let mut out = Vec::with_capacity(PLANE_ARRAY.len() + 1);
        let capacity = out.capacity();
        calls = 0;
        let result = mesh_arrays(mesh, |bytes| {
            calls += 1;
            append(&mut out, available, bytes)
        });
        assert_eq!(result, expected);
        assert_eq!(out, PLANE_ARRAY[..accepted]);
        assert_eq!((calls, out.capacity()), (visits, capacity));
    }
}

#[test]
fn array_headers_and_edge_rows_enforce_the_signed_wire_domain() {
    use AsciiWriteRefusal as R;
    let mut line = ChannelLine {
        bytes: [0; CHANNEL_BYTES],
        used: 0,
    };
    line.array_header(".vt", i32::MAX as usize).unwrap();
    assert_eq!(
        &line.bytes[..line.used],
        b"\tsetAttr -s 2147483647 \".vt[0:2147483646]\"\n"
    );
    assert_eq!(line.array_header(".ed", 0), Err(R::EmptyMesh));
    line.edge_row([0, i32::MAX as usize]).unwrap();
    assert_eq!(&line.bytes[..line.used], b"\t0 2147483647 0\n");
    for value in [i32::MAX as usize + 1, usize::MAX] {
        assert_eq!(line.array_header(".ed", value), Err(R::IndexOutOfRange));
        assert_eq!(line.edge_row([value, 0]), Err(R::IndexOutOfRange));
        assert_eq!(line.edge_row([0, value]), Err(R::IndexOutOfRange));
        assert_eq!(line.face_header(value), Err(R::IndexOutOfRange));
    }
    assert_eq!(line.face_header(0), Err(R::EmptyMesh));
    line.face_header(i32::MAX as usize).unwrap();
    assert_eq!(
        &line.bytes[..line.used],
        b"\tsetAttr -s 2147483647 \".fc[0:2147483646]\" -type \"polyFaces\"\n"
    );
    for (id, reversed, expected) in [
        (0, false, 0),
        (0, true, -1),
        (i32::MAX as usize - 1, true, -i32::MAX),
        (i32::MAX as usize, false, i32::MAX),
        (i32::MAX as usize, true, i32::MIN),
    ] {
        assert_eq!(
            signed_face_edge(DirectedEdge { edge: id, reversed }),
            Ok(expected)
        );
    }
    assert_eq!(
        signed_face_edge(DirectedEdge {
            edge: usize::MAX,
            reversed: false
        }),
        Err(R::IndexOutOfRange)
    );
    assert_eq!(
        signed_face_edge(DirectedEdge {
            edge: usize::MAX,
            reversed: true
        }),
        Err(R::IndexOutOfRange)
    );
    assert!(mesh_array_work(usize::MAX, usize::MAX).is_err());
    assert!(mesh_face_work(usize::MAX, usize::MAX).is_err());
}

#[test]
fn every_slot_round_trips_defaults_signed_zero_and_finite_extremes() {
    let values = [
        0.0,
        -0.0,
        1.0,
        0.1,
        1.0 / 3.0,
        std::f64::consts::PI,
        f64::from_bits(1.0f64.to_bits() + 1),
        1e-9,
        1e15,
        -2.0,
        f64::MAX,
        -f64::MAX,
        f64::from_bits(f64::MAX.to_bits() - 1),
        -f64::from_bits(f64::MAX.to_bits() - 1),
        f64::MIN_POSITIVE,
        -f64::MIN_POSITIVE,
        f64::from_bits(f64::MIN_POSITIVE.to_bits() + 1),
        -f64::from_bits(f64::MIN_POSITIVE.to_bits() + 1),
        f64::from_bits(1),
        -f64::from_bits(1),
        f64::from_bits(2),
        -f64::from_bits(2),
    ];
    for slot in 0..9 {
        for value in values {
            let mut scene = scene(&["value"]);
            scene.nodes[0].transform_uuid = None;
            scene.nodes[0].channels[slot].value = value;
            let mut expected = if slot < 6 { [0.0f64; 3] } else { [1.0f64; 3] };
            let changed = value.to_bits() != expected[slot % 3].to_bits();
            expected[slot % 3] = value;
            let bytes = prefix(&scene).unwrap();
            let actual = parsed_prefix(&bytes).channels;
            assert_eq!(actual.len(), usize::from(changed));
            if changed {
                assert_eq!(
                    (actual[0].0, actual[0].1),
                    ("value", [".t", ".r", ".s"][slot / 3])
                );
                assert_eq!(actual[0].2.map(f64::to_bits), expected.map(f64::to_bits));
            }
        }
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut scene = scene(&["invalid"]);
        scene.nodes[0].channels[0].value = value;
        assert_eq!(
            prefix(&scene),
            Err(AsciiWriteRefusal::InvalidScene(NativeError::NonFinite))
        );
    }
}

#[test]
fn uuid_and_channel_bytes_are_in_the_admitted_size() {
    let mut scene = scene(&["plain"]);
    scene.shapes[0].mesh = NativeMesh::primitive(Primitive::Plane).unwrap();
    scene.nodes[0].transform_uuid = None;
    scene.shapes[0].uuid = None;
    assert_eq!(prefix_bytes(&scene).unwrap(), 383);
    assert_eq!(prefix(&scene).unwrap().len(), 383);
    scene.nodes[0].transform_uuid = Some(uuid::Uuid::nil());
    let bytes = prefix(&scene).unwrap();
    assert_eq!(bytes.len(), 436);
    assert_eq!(bytes.len(), prefix_bytes(&scene).unwrap());
    assert!(parsed_prefix(&bytes).channels.is_empty());
    for contribution in [53, 37, MESH_SYNTAX_BYTES] {
        assert_eq!(
            add_prefix_bytes(MAX_BYTES - contribution, contribution),
            Ok(MAX_BYTES)
        );
        assert_eq!(
            add_prefix_bytes(MAX_BYTES - contribution + 1, contribution),
            Err(AsciiWriteRefusal::TooLarge)
        );
    }
    assert_eq!(
        add_prefix_bytes(usize::MAX, 1),
        Err(AsciiWriteRefusal::TooLarge)
    );
}

#[test]
fn formatting_workspace_and_admitted_output_refuse_one_extra_byte() {
    let mut line = ChannelLine {
        bytes: [0; CHANNEL_BYTES],
        used: 0,
    };
    line.format(format_args!("{:width$}", "", width = CHANNEL_BYTES))
        .unwrap();
    assert_eq!(line.used, CHANNEL_BYTES);
    let before = line.bytes;
    assert_eq!(
        line.format(format_args!("x")),
        Err(AsciiWriteRefusal::ResourceLimit)
    );
    assert_eq!((line.used, line.bytes), (CHANNEL_BYTES, before));
    assert!(fmt::Write::write_str(&mut line, "x").is_err());
    assert_eq!((line.used, line.bytes), (CHANNEL_BYTES, before));
    let mut out = Vec::with_capacity(32);
    append(&mut out, 2, b"ab").unwrap();
    let capacity = out.capacity();
    assert_eq!(append(&mut out, 2, b"c"), Err(AsciiWriteRefusal::TooLarge));
    assert_eq!(out, b"ab");
    assert_eq!(out.capacity(), capacity);
}

#[test]
fn added_formatting_work_refuses_a_previously_admissible_small_scene() {
    let mut scene = NativeScene::default();
    for index in 0..100 {
        scene
            .create(Primitive::Cube, &format!("{}{index}", "a".repeat(1225)))
            .unwrap();
    }
    let (n, s, d) = (scene.nodes.len(), scene.shapes.len(), scene.dag.len());
    let b: usize = names(&scene).map(|(_, name)| name.len()).sum();
    assert_eq!((n, s, d, b), (100, 100, 200, 245880));
    let mesh = scene.shapes[0].mesh();
    let corners: usize = mesh.faces().iter().map(|face| face.outer().len()).sum();
    let holes: usize = mesh.faces().iter().map(|face| face.holes().len()).sum();
    assert_eq!(
        (
            mesh.point_count(),
            mesh.edges().len(),
            mesh.faces().len(),
            corners,
            holes
        ),
        (8, 12, 6, 24, 0)
    );
    let linear = n
        + s
        + d
        + n * (8 + 12 + 6 + 24)
        + scene.session_materials.assignments.len()
        + scene.session_materials.materials.len()
        + scene.session_materials.groups.len()
        + 1;
    let prior = 2 * (n + s) * d + n * (d + s) + (n + s) * b + 32 * linear;
    assert_eq!(prior, 49_458_896);
    assert_eq!((CHANNEL_BYTES, FORMATTING_WORK * n), (1063, 645000));
    assert_eq!(prior + FORMATTING_WORK * n, 50_103_896);
    scene.validate().unwrap();
    assert_eq!(prefix(&scene), Err(AsciiWriteRefusal::ResourceLimit));
    for (index, (node, shape)) in scene.nodes.iter_mut().zip(&mut scene.shapes).enumerate() {
        node.name = format!("{}{index}", "a".repeat(1209));
        shape.name = format!("{}Shape", node.name);
    }
    let shorter_names: usize = names(&scene).map(|(_, name)| name.len()).sum();
    assert_eq!(shorter_names, 242680);
    let parent = prior - (n + s) * (b - shorter_names) + FORMATTING_WORK * n;
    let lookup = 2 * n * (d + s);
    let declarations = 2 * (n * (MESH_SYNTAX_BYTES + UUID_STATEMENT_BYTES) + shorter_names);
    assert_eq!((MESH_SYNTAX_BYTES, UUID_STATEMENT_BYTES), (29, 53));
    assert_eq!(
        (parent, lookup, declarations),
        (49_463_896, 60_000, 501_760)
    );
    assert_eq!(parent + lookup + declarations, 50_025_656);
    assert!(parent + lookup < 50_000_000 && parent + declarations < 50_000_000);
    scene.validate().unwrap();
    assert_eq!(prefix(&scene), Err(AsciiWriteRefusal::ResourceLimit));
    for (index, (node, shape)) in scene.nodes.iter_mut().zip(&mut scene.shapes).enumerate() {
        node.name = format!("{}{index}", "a".repeat(1200));
        shape.name = format!("{}Shape", node.name);
    }
    let array_names: usize = names(&scene).map(|(_, name)| name.len()).sum();
    assert_eq!(array_names, 240880);
    let prior_arrays = parent + lookup + declarations - (n + s + 2) * (shorter_names - array_names);
    let added = n * mesh_array_work(8, 12).unwrap();
    assert_eq!(
        (POINT_ROW_BYTES, EDGE_ROW_BYTES, ARRAY_HEADER_BYTES),
        (172, 25, 43)
    );
    assert_eq!((prior_arrays, added), (49_662_056, 565_800));
    assert_eq!(prior_arrays + added, 50_227_856);
    scene.validate().unwrap();
    assert_eq!(prefix(&scene), Err(AsciiWriteRefusal::ResourceLimit));
    for (index, (node, shape)) in scene.nodes.iter_mut().zip(&mut scene.shapes).enumerate() {
        node.name = format!("{}{index}", "a".repeat(1194));
        shape.name = format!("{}Shape", node.name);
    }
    let face_names: usize = names(&scene).map(|(_, name)| name.len()).sum();
    let parent = prior_arrays - (n + s + 2) * (array_names - face_names) + added;
    let face_charge = n * mesh_face_work(6, corners).unwrap();
    assert_eq!(
        (face_names, parent, face_charge),
        (239680, 49_985_456, 88_200)
    );
    assert!(parent < 50_000_000 && parent + face_charge > 50_000_000);
    assert_eq!(
        (FACE_HEADER_BYTES, FACE_ROW_BYTES, FACE_TOKEN_BYTES),
        (61, 15, 12)
    );
    assert_eq!(mesh_face_work(0, 0).unwrap(), 126);
    assert_eq!(mesh_face_work(6, 0).unwrap(), 306);
    assert_eq!(mesh_face_work(0, 24).unwrap(), 702);
    scene.validate().unwrap();
    crate::native_scene::FULL_VALIDATIONS.with(|count| count.set(0));
    assert!(matches!(
        prefix(&scene),
        Err(AsciiWriteRefusal::ResourceLimit)
    ));
    crate::native_scene::FULL_VALIDATIONS.with(|count| assert_eq!(count.get(), 0));
}

#[test]
fn hole_container_and_checked_resource_boundaries_precede_canonical_work() {
    use crate::maya_ascii::ErrorKind;
    let maximum_holes = 50_000_000 / 32;
    charge_face_counts(&mut Work::new(), 0, maximum_holes).unwrap();
    for (outer, holes) in [(0, maximum_holes + 1), (0, usize::MAX), (usize::MAX, 1)] {
        assert_eq!(
            charge_face_counts(&mut Work::new(), outer, holes)
                .unwrap_err()
                .kind(),
            ErrorKind::ResourceLimit
        );
    }
    let mut invalid_hole = scene(&["cube"]);
    invalid_hole.shapes[0].mesh.add_hole_fixture(&[]);
    assert_eq!(
        prefix(&invalid_hole),
        Err(AsciiWriteRefusal::InvalidScene(
            NativeError::InvalidTopology
        ))
    );
    let mut over_work = NativeScene::default();
    for index in 0..100 {
        over_work
            .create(Primitive::Cube, &format!("{}{index}", "a".repeat(2500)))
            .unwrap();
    }
    over_work.validate().unwrap();
    crate::native_scene::FULL_VALIDATIONS.with(|count| count.set(0));
    assert_eq!(prefix(&over_work), Err(AsciiWriteRefusal::ResourceLimit));
    crate::native_scene::FULL_VALIDATIONS.with(|count| assert_eq!(count.get(), 0));
}

#[test]
fn declaration_size_checks_exact_cap_and_overflow_without_an_output_buffer() {
    let fixed = declaration_bytes(1, 0).unwrap();
    assert_eq!(declaration_bytes(1, MAX_BYTES - fixed), Ok(MAX_BYTES));
    for counts in [(1, MAX_BYTES - fixed + 1), (usize::MAX, 0), (0, usize::MAX)] {
        assert_eq!(
            declaration_bytes(counts.0, counts.1),
            Err(AsciiWriteRefusal::TooLarge)
        );
    }
}
