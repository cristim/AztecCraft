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
        // Refused at staging: one check. Refused at the link: craft-atomic then tries a copy
        // into the destination, whose creation asks again and is refused again.
        assert_eq!(asked.get(), if allowed == 0 { 1 } else { 3 });
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

struct UvFile {
    uvs: Vec<[f32; 2]>,
    faces: Vec<Vec<(usize, Option<usize>)>>,
}

fn parse_uv(text: &str) -> UvFile {
    let mut file = UvFile {
        uvs: Vec::new(),
        faces: Vec::new(),
    };
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let mut fields = line.split_whitespace();
        match fields.next().unwrap() {
            "vt" => {
                let uv: Vec<f32> = fields.map(|s| s.parse().unwrap()).collect();
                file.uvs.push([uv[0], uv[1]]);
            }
            "f" => file.faces.push(
                fields
                    .map(|corner| match corner.split_once('/') {
                        Some((v, vt)) => (v.parse().unwrap(), Some(vt.parse().unwrap())),
                        None => (corner.parse().unwrap(), None),
                    })
                    .collect(),
            ),
            _ => {}
        }
    }
    file
}

fn export_text(scene: &NativeScene) -> String {
    let view = ViewScene::from_native(scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("out.obj");
    export_native_new(&path, scene, &all(&view), &|| true).unwrap();
    fs::read_to_string(path).unwrap()
}

/// Asserts that every instance's face rows carry exactly its shape's UV corners, in file order.
fn assert_uvs_written(scene: &NativeScene, file: &UvFile) {
    let view = ViewScene::from_native(scene, BUDGET).unwrap();
    let mut rows = file.faces.iter();
    for instance in view.instances() {
        let set = scene.uv_set(instance.shape());
        for face in instance.faces() {
            let row = rows.next().unwrap();
            assert_eq!(row.len(), face.corners().len());
            match set {
                Some(set) => {
                    let expected = &set.faces()[face.source_index()];
                    for ((_, vt), uv) in row.iter().zip(expected) {
                        let at = file.uvs[vt.unwrap() - 1];
                        assert_eq!(at, [uv[0] as f32, uv[1] as f32]);
                    }
                }
                None => assert!(row.iter().all(|(_, vt)| vt.is_none())),
            }
        }
    }
    assert!(rows.next().is_none());
}

#[test]
fn planar_mapped_cube_writes_one_vt_per_corner_and_v_vt_faces() {
    use azteccraft::native_scene::UvProjectAxis;
    let mut scene = scene_with(&[(Primitive::Cube, "mapped")]);
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    let file = parse_uv(&export_text(&scene));
    assert_eq!(file.uvs.len(), 24);
    assert_eq!(file.faces.len(), 6);
    assert_uvs_written(&scene, &file);
}

#[test]
fn polygon_faces_with_uvs_keep_every_corner() {
    use azteccraft::native_scene::UvProjectAxis;
    let mut scene = scene_with(&[(Primitive::Cylinder, "cap")]);
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let file = parse_uv(&export_text(&scene));
    assert!(file.faces.iter().any(|face| face.len() > 4));
    assert_uvs_written(&scene, &file);
}

#[test]
fn mixed_plain_and_mapped_objects_keep_their_own_vt_offsets() {
    use azteccraft::native_scene::UvProjectAxis;
    let mut scene = scene_with(&[
        (Primitive::Cube, "plain1"),
        (Primitive::Cube, "mapped1"),
        (Primitive::Plane, "plain2"),
        (Primitive::Plane, "mapped2"),
    ]);
    for (index, axis) in [(1, UvProjectAxis::Z), (3, UvProjectAxis::Y)] {
        let shape = scene.shapes()[index].id();
        scene.project_uv_planar(shape, axis).unwrap();
    }
    let file = parse_uv(&export_text(&scene));
    let mapped: usize = [1, 3]
        .iter()
        .map(|i| {
            scene
                .uv_set(scene.shapes()[*i].id())
                .unwrap()
                .corner_count()
        })
        .sum();
    assert_eq!(file.uvs.len(), mapped);
    assert_uvs_written(&scene, &file);
}

#[test]
fn mirrored_instance_reverses_vertices_and_uvs_together() {
    use azteccraft::native_scene::UvProjectAxis;
    let mut plain = scene_with(&[(Primitive::Cube, "c")]);
    let shape = plain.shapes()[0].id();
    plain.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    let mut mirrored = plain.clone();
    let id = mirrored.nodes()[0].id();
    mirrored.set_channel(id, Channel::ScaleX, -1.0).unwrap();
    let plain_file = parse_uv(&export_text(&plain));
    let mirrored_file = parse_uv(&export_text(&mirrored));
    // A cube vertex is shared by several faces with different UVs, so compare corner by corner:
    // the mirrored row is the plain row reversed, vertex and UV alike.
    assert_eq!(plain_file.faces.len(), mirrored_file.faces.len());
    for (plain_row, mirrored_row) in plain_file.faces.iter().zip(&mirrored_file.faces) {
        let plain_pairs: Vec<_> = plain_row
            .iter()
            .map(|(v, vt)| (*v, plain_file.uvs[vt.unwrap() - 1]))
            .collect();
        let mut mirrored_pairs: Vec<_> = mirrored_row
            .iter()
            .map(|(v, vt)| (*v, mirrored_file.uvs[vt.unwrap() - 1]))
            .collect();
        mirrored_pairs.reverse();
        assert_eq!(plain_pairs, mirrored_pairs);
    }
    assert_ne!(plain_file.faces, mirrored_file.faces);
}

#[test]
fn mapped_export_still_refuses_an_occupied_destination_and_a_revoked_install() {
    use azteccraft::native_scene::UvProjectAxis;
    let mut scene = scene_with(&[(Primitive::Cube, "mapped")]);
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let taken = directory.path().join("taken.obj");
    fs::write(&taken, b"keep me").unwrap();
    assert!(matches!(
        export_native_new(&taken, &scene, &all(&view), &|| true),
        Err(ObjError::Io { source, .. }) if source.kind() == io::ErrorKind::AlreadyExists
    ));
    assert_eq!(fs::read(&taken).unwrap(), b"keep me");
    let fresh = directory.path().join("fresh.obj");
    assert!(matches!(
        export_native_new(&fresh, &scene, &all(&view), &|| false),
        Err(ObjError::Cancelled)
    ));
    assert!(!fresh.exists());
}
