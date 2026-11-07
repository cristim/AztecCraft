//! Export, import, export again: the import reads back what the exporter wrote, and a second
//! pass changes nothing further (the f32 rounding happens once, on the first import).

use azteccraft::{
    history::{Command, History},
    native_scene::{Channel, NativeMesh, NativeScene, Points},
    obj::{
        export_native_new,
        import::{ParsedObj, parse, read},
    },
    scene::Primitive,
    view_scene::{VIEW_BUDGET, ViewInstance, ViewScene},
};
use std::path::Path;

fn export_to(scene: &NativeScene, path: &Path) -> String {
    let view = ViewScene::from_native(scene, VIEW_BUDGET).unwrap();
    let instances: Vec<&ViewInstance> = view.instances().iter().collect();
    export_native_new(path, scene, &instances, &|| true).unwrap();
    std::fs::read_to_string(path).unwrap()
}

fn import_into_empty(path: &Path) -> (NativeScene, ParsedObj) {
    let parsed = read(path).unwrap();
    let mut history = History::new(NativeScene::default());
    let plan = history.scene().plan_obj_import(&parsed).unwrap();
    history
        .execute(Command::ImportObjects(Box::new(plan)))
        .unwrap();
    (history.scene().clone(), parsed)
}

/// The vertex positions of each face of a mesh, in loop order, starting where the loop starts.
fn face_positions(mesh: &NativeMesh) -> Vec<Vec<[f32; 3]>> {
    let Points::Float3(points) = mesh.points() else {
        panic!("imported meshes are single precision");
    };
    mesh.faces()
        .iter()
        .map(|face| {
            face.outer()
                .iter()
                .map(|directed| {
                    let [from, to] = mesh.edges()[directed.edge].endpoints();
                    points[if directed.reversed { to } else { from }]
                })
                .collect()
        })
        .collect()
}

fn scenes() -> Vec<(&'static str, NativeScene)> {
    let mut plain = NativeScene::default();
    plain.create(Primitive::Cube, "cube").unwrap();
    let mut flat = NativeScene::default();
    flat.create(Primitive::Plane, "plane").unwrap();
    let mut round = NativeScene::default();
    round.create(Primitive::Sphere, "ball").unwrap();
    let mut moved = NativeScene::default();
    let id = moved.create(Primitive::Cube, "moved").unwrap();
    for (channel, value) in [
        (Channel::TranslateX, 1.25),
        (Channel::TranslateY, -3.5),
        (Channel::RotateX, 30.0),
        (Channel::RotateY, 45.0),
        (Channel::ScaleZ, 2.0),
    ] {
        moved.set_channel(id, channel, value).unwrap();
    }
    let mut mirrored = NativeScene::default();
    let id = mirrored.create(Primitive::Cube, "mirror").unwrap();
    mirrored.set_channel(id, Channel::ScaleX, -1.0).unwrap();
    mirrored.create(Primitive::Cone, "cone").unwrap();
    vec![
        ("cube", plain),
        ("plane", flat),
        ("sphere", round),
        ("transformed", moved),
        ("mirrored", mirrored),
    ]
}

#[test]
fn imports_read_back_the_exported_names_vertices_and_face_loops() {
    let directory = tempfile::tempdir().unwrap();
    for (label, scene) in scenes() {
        let path = directory.path().join(format!("{label}.obj"));
        let text = export_to(&scene, &path);
        let exported = parse(text.as_bytes()).unwrap();
        let (imported, parsed) = import_into_empty(&path);
        assert_eq!(parsed, exported, "{label}");
        assert_eq!(imported.nodes().len(), exported.objects.len(), "{label}");
        for (index, object) in exported.objects.iter().enumerate() {
            assert_eq!(imported.nodes()[index].name(), object.name, "{label}");
            let expected: Vec<Vec<[f32; 3]>> = object
                .faces
                .iter()
                .map(|face| {
                    face.iter()
                        .map(|i| exported.vertices[*i as usize])
                        .collect()
                })
                .collect();
            let got = face_positions(imported.shapes()[index].mesh());
            assert_eq!(got, expected, "{label} object {index}");
        }
    }
}

#[test]
fn a_second_round_trip_is_byte_identical() {
    let directory = tempfile::tempdir().unwrap();
    for (label, scene) in scenes() {
        let first = directory.path().join(format!("{label}-1.obj"));
        export_to(&scene, &first);
        let (once, _) = import_into_empty(&first);
        let second = directory.path().join(format!("{label}-2.obj"));
        let text_two = export_to(&once, &second);
        let (twice, _) = import_into_empty(&second);
        let third = directory.path().join(format!("{label}-3.obj"));
        let text_three = export_to(&twice, &third);
        assert_eq!(text_two, text_three, "{label}");
    }
}

#[cfg(unix)]
#[test]
fn an_imported_scene_saves_as_a_binary_file_and_reopens_with_the_same_geometry() {
    use azteccraft::document::{NativeOrigin, SaveDestination, load_native, save_native};
    let directory = tempfile::tempdir().unwrap();
    for (label, scene) in scenes() {
        let obj = directory.path().join(format!("{label}.obj"));
        export_to(&scene, &obj);
        let (imported, _) = import_into_empty(&obj);
        let mb = directory.path().join(format!("{label}.mb"));
        save_native(
            &NativeOrigin::Untitled,
            &imported,
            SaveDestination::As(&mb),
            &|| true,
        )
        .unwrap();
        let reopened = load_native(&mb).unwrap().scene;
        assert_eq!(reopened.nodes().len(), imported.nodes().len(), "{label}");
        for (before, after) in imported.shapes().iter().zip(reopened.shapes()) {
            assert_eq!(
                before.mesh().point_count(),
                after.mesh().point_count(),
                "{label}"
            );
            assert_eq!(
                before.mesh().faces().len(),
                after.mesh().faces().len(),
                "{label}"
            );
            assert_eq!(before.mesh().points(), after.mesh().points(), "{label}");
        }
    }
}
