use azteccraft::{
    native_scene::{Channel, NativeScene},
    obj::{ExportSummary, ObjError, export_native_new},
    scene::Primitive,
    view_scene::{ViewInstance, ViewScene},
};
use glam::DVec3;
use std::{fs, io};

const BUDGET: usize = 64 * 1024 * 1024;

struct Parsed {
    names: Vec<String>,
    vertices: Vec<DVec3>,
    faces: Vec<Vec<usize>>,
}

fn parse(text: &str) -> Parsed {
    let mut parsed = Parsed {
        names: Vec::new(),
        vertices: Vec::new(),
        faces: Vec::new(),
    };
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut fields = line.split_whitespace();
        match fields.next().unwrap() {
            "o" => parsed.names.push(fields.next().unwrap().to_string()),
            "v" => {
                let xyz: Vec<f64> = fields.map(|s| s.parse().unwrap()).collect();
                assert_eq!(xyz.len(), 3);
                parsed.vertices.push(DVec3::new(xyz[0], xyz[1], xyz[2]));
            }
            "f" => {
                let indices: Vec<usize> = fields.map(|s| s.parse().unwrap()).collect();
                assert!(indices.len() >= 3);
                assert!(
                    indices
                        .iter()
                        .all(|i| *i > 0 && *i <= parsed.vertices.len())
                );
                parsed.faces.push(indices);
            }
            other => panic!("unexpected OBJ record {other}"),
        }
    }
    parsed
}

fn scene_with(primitives: &[(Primitive, &str)]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (primitive, name) in primitives {
        scene.create(*primitive, name).unwrap();
    }
    scene
}

fn all(view: &ViewScene) -> Vec<&ViewInstance> {
    view.instances().iter().collect()
}

#[test]
fn exports_exact_f64_world_positions_beyond_f32_precision() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let id = scene.nodes()[0].id();
    scene
        .set_channel(id, Channel::TranslateX, 16_777_219.125)
        .unwrap();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cube.obj");
    let summary = export_native_new(&path, &scene, &all(&view), &|| true).unwrap();
    assert_eq!(
        summary,
        ExportSummary {
            objects: 1,
            vertices: 8,
            faces: 6
        }
    );
    let parsed = parse(&fs::read_to_string(&path).unwrap());
    assert_eq!(parsed.names, ["pCube1"]);
    assert_eq!(parsed.vertices.len(), 8);
    assert_eq!(parsed.faces.len(), 6);
    let xs: Vec<f64> = parsed.vertices.iter().map(|v| v.x).collect();
    // The unit cube spans +-0.5; f32 cannot represent either sum, f64 can.
    assert!(xs.contains(&16_777_218.625) && xs.contains(&16_777_219.625));
}

#[test]
fn mirrored_instance_reverses_face_order() {
    let mut plain = scene_with(&[(Primitive::Plane, "pPlane1")]);
    let mut mirrored = plain.clone();
    let id = mirrored.nodes()[0].id();
    mirrored.set_channel(id, Channel::ScaleX, -1.0).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let mut faces = Vec::new();
    for (index, scene) in [&mut plain, &mut mirrored].into_iter().enumerate() {
        let view = ViewScene::from_native(scene, BUDGET).unwrap();
        let path = directory.path().join(format!("{index}.obj"));
        export_native_new(&path, scene, &all(&view), &|| true).unwrap();
        faces.push(parse(&fs::read_to_string(&path).unwrap()).faces);
    }
    assert!(!faces[0].is_empty());
    for (plain, mirrored) in faces[0].iter().zip(&faces[1]) {
        let mut reversed = plain.clone();
        reversed.reverse();
        assert_eq!(&reversed, mirrored);
    }
}

#[test]
fn subset_offsets_indices_and_names_each_instance() {
    let scene = scene_with(&[
        (Primitive::Cube, "first"),
        (Primitive::Plane, "second"),
        (Primitive::Cylinder, "third"),
    ]);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let third = scene.nodes()[2].id();
    let subset: Vec<&ViewInstance> = view
        .instances()
        .iter()
        .filter(|instance| instance.transform() != third)
        .collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("subset.obj");
    let summary = export_native_new(&path, &scene, &subset, &|| true).unwrap();
    let parsed = parse(&fs::read_to_string(&path).unwrap());
    assert_eq!(parsed.names, ["first", "second"]);
    assert_eq!(summary.objects, 2);
    assert_eq!(summary.vertices, parsed.vertices.len());
    assert_eq!(summary.faces, parsed.faces.len());
    // The plane's faces index past the cube's eight vertices.
    assert!(parsed.faces[6..].iter().flatten().all(|index| *index > 8));
}

#[test]
fn occupied_destination_and_empty_selection_write_nothing() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("taken.obj");
    fs::write(&path, b"keep me").unwrap();
    assert!(matches!(
        export_native_new(&path, &scene, &all(&view), &|| true),
        Err(ObjError::Io { source, .. }) if source.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&path).unwrap(), b"keep me");
    let fresh = directory.path().join("fresh.obj");
    assert!(matches!(
        export_native_new(&fresh, &scene, &[], &|| true),
        Err(ObjError::NothingToExport)
    ));
    assert!(!fresh.exists());
    let missing_folder = directory.path().join("missing").join("x.obj");
    assert!(matches!(
        export_native_new(&missing_folder, &scene, &all(&view), &|| true),
        Err(ObjError::Io { .. })
    ));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn an_instance_without_faces_is_refused_and_writes_nothing() {
    let mut legacy = azteccraft::scene::Scene::default();
    legacy.create(Primitive::Cube, "hollow").unwrap();
    let mut json = serde_json::to_value(&legacy).unwrap();
    let mesh = &mut json["nodes"][0]["mesh"];
    mesh["vertices"] = serde_json::json!([]);
    mesh["faces"] = serde_json::json!([]);
    let hollow: azteccraft::scene::Scene = serde_json::from_value(json).unwrap();
    let scene = NativeScene::from_legacy(hollow).unwrap();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    assert_eq!(view.instances().len(), 1);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hollow.obj");
    assert!(matches!(
        export_native_new(&path, &scene, &all(&view), &|| true),
        Err(ObjError::NothingToExport)
    ));
    assert!(!path.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn a_refused_install_permission_stops_the_export_at_each_check_and_leaves_nothing() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cube.obj");
    for allowed in [0usize, 1] {
        let asked = std::cell::Cell::new(0usize);
        let may_install = || {
            asked.set(asked.get() + 1);
            asked.get() <= allowed
        };
        assert!(matches!(
            export_native_new(&path, &scene, &all(&view), &may_install),
            Err(ObjError::Cancelled)
        ));
        assert!(!path.exists(), "allowed {allowed}");
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        // Staging and linking are the only checks; a refusal at the link ends the export.
        assert_eq!(asked.get(), allowed + 1);
    }
    export_native_new(&path, &scene, &all(&view), &|| true).unwrap();
    assert!(path.exists());
}

#[test]
fn a_faceless_instance_beside_a_solid_one_is_written_as_a_vertex_only_object() {
    let mut legacy = azteccraft::scene::Scene::default();
    legacy.create(Primitive::Cube, "hollow").unwrap();
    legacy.create(Primitive::Cube, "solid").unwrap();
    let mut json = serde_json::to_value(&legacy).unwrap();
    json["nodes"][0]["mesh"]["faces"] = serde_json::json!([]);
    let mixed: azteccraft::scene::Scene = serde_json::from_value(json).unwrap();
    let scene = NativeScene::from_legacy(mixed).unwrap();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mixed.obj");
    let summary = export_native_new(&path, &scene, &all(&view), &|| true).unwrap();
    assert_eq!(
        summary,
        ExportSummary {
            objects: 2,
            vertices: 16,
            faces: 6
        }
    );
    let parsed = parse(&fs::read_to_string(&path).unwrap());
    assert_eq!(parsed.names, ["hollow", "solid"]);
    assert_eq!(parsed.vertices.len(), 16);
    assert_eq!(parsed.faces.len(), 6);
    assert!(parsed.faces.iter().flatten().all(|index| *index > 8));
}
