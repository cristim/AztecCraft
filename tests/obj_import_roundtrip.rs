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

mod uv {
    use super::*;
    use azteccraft::native_scene::{UvProjectAxis, UvSet};

    fn with_uvs(mut scene: NativeScene) -> NativeScene {
        let ids: Vec<_> = scene.shapes().iter().map(|shape| shape.id()).collect();
        for id in ids {
            // A plane has no extent along Z, so it is projected along Y.
            if scene.project_uv_planar(id, UvProjectAxis::Z).is_err() {
                scene.project_uv_planar(id, UvProjectAxis::Y).unwrap();
            }
        }
        scene
    }

    /// Per object, per face, the sorted (position bits, uv bits) corner pairs.
    type Pairs = Vec<Vec<Vec<([u32; 3], [u32; 2])>>>;

    fn uv_scenes() -> Vec<(&'static str, NativeScene)> {
        scenes()
            .into_iter()
            .map(|(label, scene)| (label, with_uvs(scene)))
            .collect()
    }

    /// Each face as the sorted (position bits, uv bits) pairs of its corners: independent of
    /// where the loop starts and of its direction.
    fn corner_pairs(scene: &NativeScene) -> Pairs {
        scene
            .shapes()
            .iter()
            .map(|shape| {
                let set = scene.uv_set(shape.id()).expect("every shape has UVs");
                face_positions(shape.mesh())
                    .iter()
                    .zip(set.faces())
                    .map(|(positions, uvs)| {
                        let mut pairs: Vec<_> = positions
                            .iter()
                            .zip(uvs)
                            .map(|(p, uv)| {
                                (
                                    p.map(f32::to_bits),
                                    [uv[0] as f32, uv[1] as f32].map(f32::to_bits),
                                )
                            })
                            .collect();
                        pairs.sort_unstable();
                        pairs
                    })
                    .collect()
            })
            .collect()
    }

    fn sorted_uv_bits(set: &UvSet) -> Vec<Vec<[u32; 2]>> {
        set.faces()
            .iter()
            .map(|face| {
                let mut bits: Vec<_> = face
                    .iter()
                    .map(|uv| [uv[0] as f32, uv[1] as f32].map(f32::to_bits))
                    .collect();
                bits.sort_unstable();
                bits
            })
            .collect()
    }

    /// The same pairs read from the parsed file, the oracle for what the exporter wrote.
    fn parsed_pairs(parsed: &ParsedObj) -> Pairs {
        parsed
            .objects
            .iter()
            .map(|object| {
                let uv_faces = object.uv_faces.as_ref().expect("complete UVs");
                object
                    .faces
                    .iter()
                    .zip(uv_faces)
                    .map(|(face, uvs)| {
                        let mut pairs: Vec<_> = face
                            .iter()
                            .zip(uvs)
                            .map(|(v, t)| {
                                (
                                    parsed.vertices[*v as usize].map(f32::to_bits),
                                    parsed.uvs[*t as usize].map(f32::to_bits),
                                )
                            })
                            .collect();
                        pairs.sort_unstable();
                        pairs
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn uvs_survive_export_then_import_bit_for_bit_per_corner() {
        let directory = tempfile::tempdir().unwrap();
        for (label, scene) in uv_scenes() {
            let path = directory.path().join(format!("{label}.obj"));
            export_to(&scene, &path);
            let (imported, parsed) = import_into_empty(&path);
            assert_eq!(corner_pairs(&imported), parsed_pairs(&parsed), "{label}");
            // Oracle outside the exported file: the source scene's own UVs, per face, ignoring
            // where the loop starts and a mirrored instance's reversal.
            for (index, (source, back)) in scene.shapes().iter().zip(imported.shapes()).enumerate()
            {
                let expected = sorted_uv_bits(scene.uv_set(source.id()).unwrap());
                let got = sorted_uv_bits(imported.uv_set(back.id()).unwrap());
                assert_eq!(got, expected, "{label} object {index}");
            }
            // Positions are only comparable where the object has no transform (local == world).
            if label != "transformed" && label != "mirrored" {
                for (source, back) in scene.shapes().iter().zip(imported.shapes()) {
                    let sorted = |mesh: &NativeMesh| -> Vec<Vec<[u32; 3]>> {
                        face_positions(mesh)
                            .into_iter()
                            .map(|face| {
                                let mut bits: Vec<_> =
                                    face.iter().map(|p| p.map(f32::to_bits)).collect();
                                bits.sort_unstable();
                                bits
                            })
                            .collect()
                    };
                    assert_eq!(sorted(back.mesh()), sorted(source.mesh()), "{label}");
                }
            }
            assert!(
                imported
                    .shapes()
                    .iter()
                    .all(|shape| imported.uv_set(shape.id()).is_some()),
                "{label}"
            );
        }
    }

    #[test]
    fn an_unmirrored_import_keeps_the_source_corner_values() {
        let directory = tempfile::tempdir().unwrap();
        let mut cube = NativeScene::default();
        cube.create(Primitive::Cube, "cube").unwrap();
        let cube = with_uvs(cube);
        let path = directory.path().join("cube.obj");
        export_to(&cube, &path);
        let (imported, _) = import_into_empty(&path);
        assert_eq!(
            imported.uv_set(imported.shapes()[0].id()).unwrap().faces(),
            cube.uv_set(cube.shapes()[0].id()).unwrap().faces()
        );
    }

    #[test]
    fn a_mirrored_instance_keeps_each_uv_on_its_vertex() {
        // Planar Z with scale X = -1: u follows the object x, so in world space it falls as x rises.
        let mut scene = NativeScene::default();
        let id = scene.create(Primitive::Cube, "mirror").unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
        scene.set_channel(id, Channel::ScaleX, -1.0).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mirror.obj");
        export_to(&scene, &path);
        let (imported, _) = import_into_empty(&path);
        let shape = &imported.shapes()[0];
        let set = imported.uv_set(shape.id()).unwrap();
        let xs: Vec<f32> = face_positions(shape.mesh())
            .concat()
            .iter()
            .map(|p| p[0])
            .collect();
        let (low, high) = (
            xs.iter().copied().fold(f32::INFINITY, f32::min),
            xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        );
        for (positions, uvs) in face_positions(shape.mesh()).iter().zip(set.faces()) {
            for (position, uv) in positions.iter().zip(uvs) {
                let expected = f64::from((high - position[0]) / (high - low));
                assert!((uv[0] - expected).abs() < 1e-6, "{position:?} {uv:?}");
            }
        }
    }

    #[test]
    // Compares export 2 with export 3: export 1 may normalise f64 to f32 once, so this shows the
    // round trip is stable after that, not that export 1 is lossless (the oracle test does that).
    fn a_second_uv_round_trip_is_byte_identical() {
        let directory = tempfile::tempdir().unwrap();
        for (label, scene) in uv_scenes() {
            let first = directory.path().join(format!("{label}-1.obj"));
            export_to(&scene, &first);
            let (once, _) = import_into_empty(&first);
            let second = directory.path().join(format!("{label}-2.obj"));
            let text_two = export_to(&once, &second);
            let (twice, _) = import_into_empty(&second);
            let third = directory.path().join(format!("{label}-3.obj"));
            let text_three = export_to(&twice, &third);
            assert!(text_two.lines().any(|l| l.starts_with("vt ")), "{label}");
            assert_eq!(text_two, text_three, "{label}");
        }
    }

    #[test]
    fn objects_with_different_uv_states_keep_theirs_and_the_offsets_line_up() {
        let mut scene = NativeScene::default();
        scene.create(Primitive::Cube, "a").unwrap();
        scene.create(Primitive::Plane, "plain").unwrap();
        scene.create(Primitive::Cube, "c").unwrap();
        let ids: Vec<_> = scene.shapes().iter().map(|shape| shape.id()).collect();
        scene.project_uv_planar(ids[0], UvProjectAxis::Z).unwrap();
        scene.project_uv_planar(ids[2], UvProjectAxis::X).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("mixed.obj");
        export_to(&scene, &path);
        let (imported, _) = import_into_empty(&path);
        let states: Vec<bool> = imported
            .shapes()
            .iter()
            .map(|shape| imported.uv_set(shape.id()).is_some())
            .collect();
        assert_eq!(states, [true, false, true]);
        for (original, back) in [(0, 0), (2, 2)] {
            assert_eq!(
                imported
                    .uv_set(imported.shapes()[back].id())
                    .unwrap()
                    .faces(),
                scene.uv_set(scene.shapes()[original].id()).unwrap().faces()
            );
        }
    }

    #[test]
    fn a_hand_written_file_with_v_vt_vn_negative_indices_and_an_ngon_imports_its_uvs() {
        let text = "v 0 0 0\nv 2 0 0\nv 3 1 0\nv 1 2 0\nv -1 1 0\n\
                    vt 0 0\nvt 1 0\nvt 1.5 0.5\nvt 0.5 1\nvt -0.5 0.5\n\
                    vn 0 0 1\n\
                    o pentagon\nf -5/-5/1 -4/-4/1 -3/-3/1 -2/-2/1 -1/-1/1\n";
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hand.obj");
        std::fs::write(&path, text).unwrap();
        let (imported, _) = import_into_empty(&path);
        let shape = &imported.shapes()[0];
        assert_eq!(shape.mesh().faces()[0].outer().len(), 5);
        let set = imported.uv_set(shape.id()).unwrap();
        assert_eq!(
            set.faces(),
            [vec![
                [0.0, 0.0],
                [1.0, 0.0],
                [1.5, 0.5],
                [0.5, 1.0],
                [-0.5, 0.5]
            ]]
        );
    }

    #[test]
    fn undoing_an_import_removes_the_shapes_and_their_uvs_and_redo_restores_both() {
        let mut cube = NativeScene::default();
        cube.create(Primitive::Cube, "cube").unwrap();
        let cube = with_uvs(cube);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cube.obj");
        export_to(&cube, &path);
        let parsed = read(&path).unwrap();
        let mut history = History::new(NativeScene::default());
        let plan = history.scene().plan_obj_import(&parsed).unwrap();
        history
            .execute(Command::ImportObjects(Box::new(plan)))
            .unwrap();
        let id = history.scene().shapes()[0].id();
        assert!(history.scene().uv_set(id).is_some());
        assert!(history.undo());
        assert!(history.scene().shapes().is_empty());
        assert!(history.scene().uv_set(id).is_none());
        assert!(history.redo());
        assert_eq!(
            history.scene().uv_set(id).unwrap().faces(),
            cube.uv_set(cube.shapes()[0].id()).unwrap().faces()
        );
    }
}
