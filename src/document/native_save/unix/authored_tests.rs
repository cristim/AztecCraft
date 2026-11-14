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
    let wrong = directory.path().join("new.txt");
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

// #375: `prepare`'s Untitled branch refuses with `SourceChanged` when `authored_document_id` is
// `None`. Two tests together show it cannot happen. This one: the id is `None` exactly when the
// scene has a parsed source. And `a_scene_the_writer_refuses_is_reported_by_code_and_writes_nothing`:
// the writer refuses such a scene with `SourcedScene` before the id is read. So the refusal only
// guards that invariant.
#[test]
fn the_authored_document_id_is_present_exactly_when_the_scene_has_no_source() {
    let directory = tempfile::tempdir().unwrap();
    let (_, opened) = opened(directory.path());
    assert!(opened.scene.authored_document_id().is_none());
    assert!(authored().authored_document_id().is_some());
    assert!(NativeScene::default().authored_document_id().is_some());
}

mod maya_ascii {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn ascii_only_origin(directory: &Path) -> NativeOrigin {
    let path = directory.join("imported.ma");
    std::fs::write(&path, maya_ascii::specimen()).unwrap();
    load_native(&path).unwrap().origin
}
fn legacy_origin(directory: &Path) -> NativeOrigin {
    let path = directory.join("old.aztec");
    crate::document::save_new(&path, &crate::scene::Scene::default()).unwrap();
    load_native(&path).unwrap().origin
}

/// What one Save As of the authored scene to `extension` does from `origin`: the kind of the
/// resulting origin, or the refusal. Every origin crossed with every extension.
/// (name, origin, scene, expected results for .ma, .mb and .txt)
type Case = (
    &'static str,
    NativeOrigin,
    NativeScene,
    [Result<&'static str, Error>; 3],
);

fn save_as_kind(
    origin: &NativeOrigin,
    scene: &NativeScene,
    directory: &Path,
    extension: &str,
) -> Result<&'static str, Error> {
    let target = directory.join(format!("target.{extension}"));
    let kind = saved_as(scene, &target, origin).map(|outcome| match outcome {
        SaveOutcome::Saved { origin, .. } => origin.kind(),
        other => panic!("expected Saved: {other:?}"),
    });
    assert_eq!(target.exists(), kind.is_ok(), "{extension}");
    kind
}

#[test]
fn save_as_picks_the_writer_by_extension_for_every_origin() {
    let scratch = tempfile::tempdir().unwrap();
    let scene = authored();
    let (ascii_origin, _) = saved(saved_as(
        &scene,
        &scratch.path().join("a.ma"),
        &NativeOrigin::Untitled,
    ));
    assert_eq!(ascii_origin.kind(), "maya_ascii");
    let (binary_origin, _) = saved(saved_as(
        &scene,
        &scratch.path().join("b.mb"),
        &NativeOrigin::Untitled,
    ));
    let opened_document = opened(scratch.path()).1;
    let invalid = Err(Error::InvalidDestination);
    let unsupported = Err(Error::Unsupported);
    let sourced = Err(Error::AsciiWrite(AsciiWriteRefusal::SourcedScene));
    let cases: Vec<Case> = vec![
        (
            "untitled",
            NativeOrigin::Untitled,
            scene.clone(),
            [Ok("maya_ascii"), Ok("maya_binary"), invalid.clone()],
        ),
        (
            "authored ascii",
            ascii_origin,
            scene.clone(),
            [Ok("maya_ascii"), Ok("maya_binary"), invalid.clone()],
        ),
        (
            "authored binary",
            binary_origin,
            scene.clone(),
            [Ok("maya_ascii"), Ok("maya_binary"), invalid.clone()],
        ),
        (
            "opened binary",
            opened_document.origin,
            opened_document.scene,
            [sourced, Ok("maya_binary"), invalid],
        ),
        (
            "opened ascii",
            ascii_only_origin(scratch.path()),
            scene.clone(),
            [
                unsupported.clone(),
                unsupported.clone(),
                unsupported.clone(),
            ],
        ),
        (
            "legacy",
            legacy_origin(scratch.path()),
            scene.clone(),
            [unsupported.clone(), unsupported.clone(), unsupported],
        ),
    ];
    for (name, origin, scene, expected) in cases {
        for (extension, expected) in ["ma", "mb", "txt"].into_iter().zip(expected) {
            let directory = tempfile::tempdir().unwrap();
            assert_eq!(
                save_as_kind(&origin, &scene, directory.path(), extension),
                expected,
                "{name} -> .{extension}"
            );
        }
    }
}

#[test]
fn a_scene_saved_as_ma_can_be_saved_as_mb_and_back_but_active_save_keeps_its_file() {
    let directory = tempfile::tempdir().unwrap();
    let scene = authored();
    let ma = directory.path().join("one.ma");
    let mb = directory.path().join("two.mb");
    let (origin, _) = saved(saved_as(&scene, &ma, &NativeOrigin::Untitled));
    let (origin, _) = saved(saved_as(&scene, &mb, &origin));
    assert_eq!(&std::fs::read(&mb).unwrap()[..4], b"FOR8");
    assert_eq!(origin.kind(), "maya_binary");
    // The new origin's Active Save rewrites the .mb, not the .ma.
    let ma_before = std::fs::read(&ma).unwrap();
    saved(save_native(
        &origin,
        &scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(std::fs::read(&ma).unwrap(), ma_before);
    let again = directory.path().join("three.ma");
    saved(saved_as(&scene, &again, &origin));
    assert!(
        std::fs::read_to_string(&again)
            .unwrap()
            .starts_with("//Maya ASCII")
    );
}

#[test]
fn an_ascii_origin_cannot_active_save_over_a_binary_writer() {
    let directory = tempfile::tempdir().unwrap();
    let scene = authored();
    let (origin, _) = saved(saved_as(
        &scene,
        &directory.path().join("one.ma"),
        &NativeOrigin::Untitled,
    ));
    let transaction = prepare(&origin, &scene, SaveDestination::Active, &|| true);
    assert_eq!(transaction.err(), Some(Error::Unsupported));
}

#[test]
fn the_save_as_dialog_offers_the_formats_each_origin_can_write() {
    let scratch = tempfile::tempdir().unwrap();
    let scene = authored();
    let (ascii_origin, _) = saved(saved_as(
        &scene,
        &scratch.path().join("a.ma"),
        &NativeOrigin::Untitled,
    ));
    let (binary_authored, _) = saved(saved_as(
        &scene,
        &scratch.path().join("b.mb"),
        &NativeOrigin::Untitled,
    ));
    let both: &[&str] = &["ma", "mb"];
    assert_eq!(NativeOrigin::Untitled.save_as_extensions(), both);
    assert_eq!(ascii_origin.save_as_extensions(), both);
    assert_eq!(binary_authored.save_as_extensions(), both);
    let opened_binary = opened(scratch.path()).1.origin;
    assert_eq!(opened_binary.save_as_extensions(), &["mb"]);
    assert!(
        ascii_only_origin(scratch.path())
            .save_as_extensions()
            .is_empty()
    );
    assert!(
        legacy_origin(scratch.path())
            .save_as_extensions()
            .is_empty()
    );
}

fn with_uvs() -> (NativeScene, crate::native_scene::NativeId) {
    let mut scene = authored();
    let shape = scene.shapes()[0].id();
    scene
        .project_uv_planar(shape, crate::native_scene::UvProjectAxis::Z)
        .unwrap();
    (scene, shape)
}

#[test]
fn a_scene_with_uvs_saves_as_ma_reopens_with_the_same_uvs_and_active_saves() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("uv.ma");
    let (scene, shape) = with_uvs();
    let (origin, _) = saved(saved_as(&scene, &path, &NativeOrigin::Untitled));
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains(" mu 0 4 ")
    );
    let reopened = load_native(&path).unwrap();
    let reopened_shape = reopened.scene.shapes()[0].id();
    assert_eq!(
        reopened.scene.uv_set(reopened_shape),
        scene.uv_set(shape),
        "UVs must round-trip f32-exactly"
    );
    assert!(matches!(reopened.origin, NativeOrigin::AuthoredAscii(_)));
    // Active Save of the reopened document rewrites the same bytes; each Save replaces the file,
    // so the original origin's baseline is checked first and the file is opened again for the next.
    saved(save_native(
        &origin,
        &scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let again = load_native(&path).unwrap();
    saved(save_native(
        &again.origin,
        &again.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn a_scene_with_uvs_still_refuses_mb_and_a_material_scene_saves_ma_but_still_refuses_mb() {
    let directory = tempfile::tempdir().unwrap();
    let (scene, shape) = with_uvs();
    let mb = directory.path().join("uv.mb");
    assert_eq!(
        saved_as(&scene, &mb, &NativeOrigin::Untitled).unwrap_err(),
        Error::Unwritable(AuthoredRefusalCode::SessionUvs)
    );
    assert!(!mb.exists());
    let mut material = scene.clone();
    material
        .assign_session_material(
            shape,
            crate::native_scene::MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        )
        .unwrap();
    let ma = directory.path().join("material.ma");
    saved(saved_as(&material, &ma, &NativeOrigin::Untitled));
    let bytes = std::fs::read(&ma).unwrap();
    let reopened = load_native(&ma).unwrap();
    assert!(matches!(reopened.origin, NativeOrigin::AuthoredAscii(_)));
    assert_eq!(
        reopened.scene.session_materials(),
        material.session_materials()
    );
    assert_eq!(
        reopened.scene.session_material(&reopened.scene.shapes()[0]),
        material.session_material(&material.shapes()[0])
    );
    // Save, reopen and save again is byte-identical.
    saved(save_native(
        &reopened.origin,
        &reopened.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(std::fs::read(&ma).unwrap(), bytes);
    let mut only = authored();
    let target = only.shapes()[0].id();
    only.assign_session_material(
        target,
        crate::native_scene::MaterialChoice::New {
            color: [1.0, 0.0, 0.0],
        },
    )
    .unwrap();
    let mb_material = directory.path().join("material.mb");
    assert_eq!(
        saved_as(&only, &mb_material, &NativeOrigin::Untitled).unwrap_err(),
        Error::Unwritable(AuthoredRefusalCode::SessionMaterials)
    );
    assert!(!mb_material.exists());
}
