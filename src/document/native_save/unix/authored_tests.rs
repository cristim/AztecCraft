//! Saving a scene that was authored in memory (an Untitled document) as a new Maya Binary
//! file, and saving it again against the baseline written by this process.

use super::{
    tests::{opened, saved},
    *,
};
use crate::{
    document::load_native,
    native_scene::{Channel, binary::AuthoredRefusalCode},
    scene::Primitive,
};

fn authored() -> NativeScene {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "box").unwrap();
    scene
}
fn saved_as(scene: &NativeScene, path: &Path, origin: &NativeOrigin) -> Result<SaveOutcome, Error> {
    save_native(origin, scene, SaveDestination::As(path), &|| true)
}
fn names(scene: &NativeScene) -> Vec<String> {
    scene.nodes().iter().map(|n| n.name().to_string()).collect()
}

#[test]
fn an_untitled_scene_saves_as_a_new_mb_that_reopens() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.mb");
    let scene = authored();
    let (origin, installed) = saved(saved_as(&scene, &path, &NativeOrigin::Untitled));
    assert_eq!(installed, path.canonicalize().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(&bytes[..4], b"FOR8");
    assert_eq!(origin.source_bytes().unwrap().as_ref(), bytes.as_slice());
    assert_eq!(origin.kind(), "maya_binary");
    assert!(origin.active_save_available());
    let reopened = load_native(&path).unwrap();
    assert_eq!(names(&reopened.scene), names(&scene));
    assert_eq!(
        reopened.scene.shapes()[0].mesh().edges().len(),
        scene.shapes()[0].mesh().edges().len()
    );
}

#[test]
fn untitled_availability_is_save_as_only() {
    assert!(NativeOrigin::Untitled.save_as_available());
    assert!(!NativeOrigin::Untitled.active_save_available());
}

#[test]
fn save_on_an_untitled_scene_has_no_destination_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let result = save_native(
        &NativeOrigin::Untitled,
        &authored(),
        SaveDestination::Active,
        &|| true,
    );
    assert_eq!(result.unwrap_err(), Error::NoDestination);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn an_occupied_or_misnamed_destination_is_refused_untouched() {
    let directory = tempfile::tempdir().unwrap();
    let occupied = directory.path().join("taken.mb");
    std::fs::write(&occupied, b"keep").unwrap();
    assert_eq!(
        saved_as(&authored(), &occupied, &NativeOrigin::Untitled).unwrap_err(),
        Error::DestinationExists
    );
    assert_eq!(std::fs::read(&occupied).unwrap(), b"keep");
    let wrong = directory.path().join("new.ma");
    assert_eq!(
        saved_as(&authored(), &wrong, &NativeOrigin::Untitled).unwrap_err(),
        Error::InvalidDestination
    );
    assert!(!wrong.exists());
}

#[test]
fn a_cancelled_install_leaves_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.mb");
    let result = save_native(
        &NativeOrigin::Untitled,
        &authored(),
        SaveDestination::As(&path),
        &|| false,
    );
    assert_eq!(result.unwrap_err(), Error::Cancelled);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn a_scene_the_writer_refuses_is_reported_by_code_and_writes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (_, opened) = opened(directory.path());
    let path = directory.path().join("copy.mb");
    // Parsed source is not authored content, whatever origin the caller claims for it.
    assert_eq!(
        saved_as(&opened.scene, &path, &NativeOrigin::Untitled).unwrap_err(),
        Error::Unwritable(AuthoredRefusalCode::SourcedScene)
    );
    assert!(!path.exists());
    let mut named = authored();
    named.rename(named.nodes()[0].id(), "persp").unwrap();
    assert_eq!(
        saved_as(&named, &path, &NativeOrigin::Untitled).unwrap_err(),
        Error::Unwritable(AuthoredRefusalCode::ReservedName)
    );
    assert!(!path.exists());
}

#[test]
fn the_authored_baseline_saves_edits_in_place_and_detects_outside_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.mb");
    let mut scene = authored();
    let (origin, _) = saved(saved_as(&scene, &path, &NativeOrigin::Untitled));
    let first = std::fs::read(&path).unwrap();

    let id = scene.nodes()[0].id();
    scene.set_channel(id, Channel::TranslateX, 2.5).unwrap();
    let (origin, again) = saved(save_native(
        &origin,
        &scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(again, path.canonicalize().unwrap());
    let second = std::fs::read(&path).unwrap();
    assert_ne!(first, second);
    let reopened = load_native(&path).unwrap();
    assert_eq!(reopened.scene.nodes()[0].transform().translation[0], 2.5);

    // A scene that retains parsed source does not match a baseline we authored.
    assert_eq!(
        save_native(&origin, &reopened.scene, SaveDestination::Active, &|| true).unwrap_err(),
        Error::SourceChanged
    );
    // Another process rewriting the file is detected before anything is replaced.
    let mut outside = second.clone();
    outside.push(0);
    std::fs::write(&path, &outside).unwrap();
    assert_eq!(
        save_native(&origin, &scene, SaveDestination::Active, &|| true).unwrap_err(),
        Error::SourceChanged
    );
    assert_eq!(std::fs::read(&path).unwrap(), outside);
}

#[test]
fn an_authored_baseline_can_save_as_a_second_file() {
    let directory = tempfile::tempdir().unwrap();
    let (first, second) = (directory.path().join("a.mb"), directory.path().join("b.mb"));
    let scene = authored();
    let (origin, _) = saved(saved_as(&scene, &first, &NativeOrigin::Untitled));
    let (next, installed) = saved(saved_as(&scene, &second, &origin));
    assert_eq!(installed, second.canonicalize().unwrap());
    assert_eq!(
        std::fs::read(&first).unwrap(),
        std::fs::read(&second).unwrap()
    );
    assert!(next.active_save_available());
}
