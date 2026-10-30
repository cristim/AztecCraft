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
    let summary = export_native_new(&path, &scene, &all(&view)).unwrap();
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
        export_native_new(&path, scene, &all(&view)).unwrap();
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
    let summary = export_native_new(&path, &scene, &subset).unwrap();
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
        export_native_new(&path, &scene, &all(&view)),
        Err(ObjError::Io { source, .. }) if source.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&path).unwrap(), b"keep me");
    let fresh = directory.path().join("fresh.obj");
    assert!(matches!(
        export_native_new(&fresh, &scene, &[]),
        Err(ObjError::NothingToExport)
    ));
    assert!(!fresh.exists());
    let missing_folder = directory.path().join("missing").join("x.obj");
    assert!(matches!(
        export_native_new(&missing_folder, &scene, &all(&view)),
        Err(ObjError::Io { .. })
    ));
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
