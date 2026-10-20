use azteccraft::document::{DocumentError, MAX_DOCUMENT_BYTES, load, save_new};
use azteccraft::history::{Command, History};
use azteccraft::scene::{Primitive, Scene, Transform};
use glam::Vec3;
use serde_json::{Value, json};
use std::fs;
mod common;

fn fixture() -> Scene {
    let mut scene = Scene::default();
    let cube = scene.create(Primitive::Cube, "cube").unwrap();
    let plane = scene.create(Primitive::Plane, "ground").unwrap();
    scene
        .set_transform(
            cube,
            Transform {
                translation: Vec3::new(2.0, -3.0, 4.0),
                rotation_degrees: Vec3::new(90.0, 45.0, 20.0),
                scale: Vec3::new(-2.0, 3.0, 0.0),
            },
        )
        .unwrap();
    scene.remove(plane).unwrap();
    scene
}

fn value(scene: &Scene) -> Value {
    json!({"version": 1, "scene": scene})
}

#[test]
fn save_open_round_trip_preserves_scene_and_allocator() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scene.aztec");
    let scene = fixture();
    save_new(&path, &scene).unwrap();
    let mut loaded = load(&path).unwrap();
    assert_eq!(loaded, scene);
    let mut original = scene.clone();
    assert_eq!(
        loaded.create(Primitive::Plane, "next").unwrap(),
        original.create(Primitive::Plane, "next").unwrap()
    );
    let empty_path = dir.path().join("empty.aztec");
    save_new(&empty_path, &Scene::default()).unwrap();
    assert_eq!(load(&empty_path).unwrap(), Scene::default());
}

#[test]
fn exhausted_loaded_allocator_preserves_scene_history_and_saved_reservations() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("exhausted.aztec");
    let mut document = value(&fixture());
    document["scene"]["next_id"] = json!(u64::MAX);
    fs::write(&input, document.to_string()).unwrap();
    let mut scene = load(&input).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.create(Primitive::Cube, "new"),
        Err(azteccraft::scene::SceneError::IdsExhausted)
    );
    assert_eq!(scene, before);
    let saved = dir.path().join("saved.aztec");
    save_new(&saved, &scene).unwrap();
    assert_eq!(load(&saved).unwrap(), before);
    let id = scene.nodes()[0].id();
    let mut history =
        History::new(azteccraft::native_scene::NativeScene::from_legacy(scene).unwrap());
    history
        .execute(Command::Rename {
            id,
            name: "renamed".into(),
        })
        .unwrap();
    assert!(history.undo());
    let before = history.scene().clone();
    assert_eq!(
        history.execute(Command::Create {
            primitive: Primitive::Cube,
            options: None,
            name: "new".into()
        }),
        Err(azteccraft::native_scene::NativeError::Legacy(
            azteccraft::scene::SceneError::IdsExhausted
        ))
    );
    assert_eq!(history.scene(), &before);
    assert!(!history.can_undo());
    assert!(history.can_redo());
    assert!(history.redo());
    assert_eq!(history.scene().node(id).unwrap().name(), "renamed");
}

#[test]
fn occupied_destination_retains_original_bytes_and_no_staging_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("occupied.aztec");
    fs::write(&path, b"original user data").unwrap();
    let error = save_new(&path, &fixture()).unwrap_err();
    assert!(
        matches!(error, DocumentError::Io { source, .. } if source.kind() == std::io::ErrorKind::AlreadyExists)
    );
    assert_eq!(fs::read(&path).unwrap(), b"original user data");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn invalid_parent_and_directory_destination_leave_state_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let scene = fixture();
    let before = scene.clone();
    assert!(save_new(&dir.path().join("missing/scene.aztec"), &scene).is_err());
    assert!(save_new(dir.path(), &scene).is_err());
    assert_eq!(scene, before);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    assert!(load(dir.path()).is_err());
    assert!(load(&dir.path().join("missing")).is_err());
}

#[test]
fn malformed_json_unknown_version_and_fields_are_rejected_without_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.aztec");
    let current = fixture();
    let before = current.clone();
    for bytes in [b"{".as_slice(), b"null", b"{}"] {
        fs::write(&path, bytes).unwrap();
        assert!(load(&path).is_err());
        assert_eq!(current, before);
    }
    let mut unknown = value(&current);
    unknown["version"] = json!(99);
    fs::write(&path, unknown.to_string()).unwrap();
    assert!(matches!(
        load(&path),
        Err(DocumentError::UnsupportedVersion(99))
    ));
    let mut extra = value(&current);
    extra["scene"]["unexpected"] = json!(true);
    fs::write(&path, extra.to_string()).unwrap();
    assert!(load(&path).is_err());
    assert_eq!(current, before);
}

#[test]
fn invalid_decoded_ids_names_transforms_and_meshes_never_become_scenes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid.aztec");
    let baseline = value(&fixture());
    let mut candidates = Vec::new();
    for (pointer, replacement) in [
        ("/scene/next_id", json!(0)),
        ("/scene/next_id", json!(1)),
        ("/scene/nodes/0/id", json!(0)),
        ("/scene/nodes/0/name", json!("\n")),
        ("/scene/nodes/0/transform/translation/0", json!(null)),
        ("/scene/nodes/0/transform/scale/0", json!(1e100)),
        ("/scene/nodes/0/mesh/vertices/0/0", json!(null)),
        ("/scene/nodes/0/mesh/faces/0/0", json!(999)),
        ("/scene/nodes/0/mesh/faces/0/0", json!(3)),
        ("/scene/nodes/0/mesh/faces/0", json!([0, 1, 2])),
        ("/scene/nodes/0/mesh/faces/0", json!([0, 1, 2, 3, 4])),
    ] {
        let mut bad = baseline.clone();
        *bad.pointer_mut(pointer).unwrap() = replacement;
        candidates.push(bad);
    }
    let mut duplicate = baseline.clone();
    let node = duplicate["scene"]["nodes"][0].clone();
    duplicate["scene"]["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node);
    candidates.push(duplicate);
    let mut overflow = baseline.clone();
    let extreme = [f32::MAX; 3];
    overflow["scene"]["nodes"][0]["transform"]["scale"] = json!(extreme);
    overflow["scene"]["nodes"][0]["transform"]["translation"] = json!(extreme);
    candidates.push(overflow);
    for bad in candidates {
        fs::write(&path, bad.to_string()).unwrap();
        assert!(load(&path).is_err(), "accepted {bad}");
        assert!(serde_json::from_value::<Scene>(bad["scene"].clone()).is_err());
    }
}

#[test]
fn oversized_load_and_save_are_rejected_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("large.aztec");
    let file = fs::File::create(&input).unwrap();
    file.set_len(MAX_DOCUMENT_BYTES as u64 + 1).unwrap();
    assert!(matches!(load(&input), Err(DocumentError::TooLarge)));
    let mut scene = Scene::default();
    scene
        .create(Primitive::Cube, &"x".repeat(MAX_DOCUMENT_BYTES))
        .unwrap();
    let output = dir.path().join("output.aztec");
    assert!(matches!(
        save_new(&output, &scene),
        Err(DocumentError::TooLarge)
    ));
    assert!(!output.exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn dangling_symlink_is_an_occupied_destination() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("link.aztec");
    let target = dir.path().join("missing.aztec");
    std::os::unix::fs::symlink(&target, &destination).unwrap();
    assert!(save_new(&destination, &fixture()).is_err());
    assert!(!target.exists());
    assert_eq!(fs::read_link(&destination).unwrap(), target);
}

#[test]
fn edited_and_empty_quad_meshes_round_trip_without_geometry_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("edited.aztec");
    for (index, mesh) in [
        json!({"vertices": [[0,0,0],[3,0,0],[3,2,1],[0,2,0],[99,99,99]], "faces": [[3,2,1,0],[3,2,1,0],[3,2,1,0]]}),
        json!({"vertices": [[0,0,0],[0,0,0],[0,0,0],[0,0,0]], "faces": [[0,1,2,3]]}),
        json!({"vertices": [[42,0,0]], "faces": []}),
        json!({"vertices": [], "faces": []}),
    ].into_iter().enumerate() {
        let mut document = value(&fixture());
        document["scene"]["nodes"][0]["mesh"] = mesh;
        let expected: azteccraft::scene::Mesh = serde_json::from_value(document["scene"]["nodes"][0]["mesh"].clone()).unwrap();
        fs::write(&input, document.to_string()).unwrap();
        let loaded = load(&input).unwrap();
        let output = dir.path().join(format!("round-trip-{index}.aztec"));
        save_new(&output, &loaded).unwrap();
        assert_eq!(load(&output).unwrap(), loaded);
        assert_eq!(loaded.nodes()[0].mesh(), &expected);
    }
}

#[test]
fn extruded_and_zero_distance_topology_round_trips_and_rejects_malformed_edits() {
    let dir = tempfile::tempdir().unwrap();
    let (mut scene, id) = common::extruded_corner();
    scene.extrude_face(id, 1, 0.0).unwrap();
    let path = dir.path().join("extruded.aztec");
    save_new(&path, &scene).unwrap();
    let bytes = fs::read(&path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded, scene);
    assert_eq!(loaded.node(id).unwrap().mesh().vertices().len(), 20);
    assert_eq!(loaded.node(id).unwrap().mesh().faces().len(), 18);
    assert!(save_new(&path, &scene).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let malformed = dir.path().join("malformed.aztec");
    for face in [json!([0, 1, 2, 20]), json!([0, 1, 1, 3])] {
        let mut document = value(&scene);
        document["scene"]["nodes"][0]["mesh"]["faces"][1] = face;
        fs::write(&malformed, document.to_string()).unwrap();
        assert!(load(&malformed).is_err());
        assert_eq!(scene, loaded);
    }
}
