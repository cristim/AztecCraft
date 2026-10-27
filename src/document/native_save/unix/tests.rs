use super::*;
use crate::{document::load_native, native_scene::Channel};
use std::cell::Cell;
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[allow(dead_code)]
mod sited {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sites.rs"
    ));
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sited.rs"
    ));
}

fn opened(directory: &Path) -> (PathBuf, crate::document::NativeDocument) {
    let path = directory.join("scene.mb");
    std::fs::write(&path, wire::specimen()).unwrap();
    let document = load_native(&path).unwrap();
    (path, document)
}
fn saved(result: Result<SaveOutcome, Error>) -> (NativeOrigin, PathBuf) {
    match result.unwrap() {
        SaveOutcome::Saved { origin, path } => (origin, path),
        other => panic!("expected confirmed installation: {other:?}"),
    }
}

/// A sited scene on disk: the `object` transform has no translate leaf, but its site is recorded.
fn opened_sited(directory: &Path) -> (PathBuf, Vec<u8>, crate::document::NativeDocument) {
    let path = directory.join("sited.mb");
    let bytes = sited::sited_scene(&wire::mesh_payload(&[0, 1, 0x6000_0002]));
    std::fs::write(&path, &bytes).unwrap();
    let document = load_native(&path).unwrap();
    (path, bytes, document)
}

fn has_record(bytes: &[u8], values: [f64; 3]) -> bool {
    let record = sited::triple_aux(b"t", sited::SITE_AUX, values);
    bytes.windows(record.len()).any(|window| window == record)
}

#[test]
fn inserted_leaf_save_as_repeat_save_and_reopen_are_bit_equal() {
    let directory = tempfile::tempdir().unwrap();
    let (path, original, mut document) = opened_sited(directory.path());
    let id = document
        .scene
        .nodes()
        .iter()
        .find(|node| node.name() == "object")
        .unwrap()
        .id();
    assert_eq!(
        document.scene.node(id).unwrap().transform().translation,
        [0.0; 3]
    );
    // 16777217.125 needs more than f32 precision; -0.0 is a real edit that must survive.
    document
        .scene
        .set_channel(id, Channel::TranslateX, 16777217.125)
        .unwrap();
    document
        .scene
        .set_channel(id, Channel::TranslateY, -0.0)
        .unwrap();
    let destination = directory.path().join("inserted.mb");
    let (origin, installed) = saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    ));
    assert_eq!(installed, destination.canonicalize().unwrap());
    let first = std::fs::read(&destination).unwrap();
    assert_eq!(first.len(), original.len() + 48);
    assert!(has_record(&first, [16777217.125, -0.0, 0.0]));
    // Save As never touches the source.
    assert!(std::fs::read(&path).unwrap() == original);
    // A repeat Save of the same scene against the returned origin rewrites identical bytes.
    let (origin, _) = saved(save_native(
        &origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert!(std::fs::read(&destination).unwrap() == first);
    // A later edit is derived from the same retained source, so the record is rewritten, not doubled.
    document
        .scene
        .set_channel(id, Channel::TranslateZ, 2.5)
        .unwrap();
    saved(save_native(
        &origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    let second = std::fs::read(&destination).unwrap();
    assert_eq!(second.len(), original.len() + 48);
    assert!(has_record(&second, [16777217.125, -0.0, 2.5]));
    let reopened = load_native(&destination).unwrap();
    let translation = reopened.scene.node(id).unwrap().transform().translation;
    assert_eq!(
        translation.map(f64::to_bits),
        [
            16777217.125f64.to_bits(),
            (-0.0f64).to_bits(),
            2.5f64.to_bits()
        ]
    );
    // The reopened document now has the leaf, so a further edit changes words in place.
    let mut again = reopened;
    again
        .scene
        .set_channel(id, Channel::TranslateX, 1.0)
        .unwrap();
    saved(save_native(
        &again.origin,
        &again.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(std::fs::read(&destination).unwrap().len(), second.len());
    let last = load_native(&destination).unwrap();
    assert_eq!(
        last.scene
            .node(id)
            .unwrap()
            .transform()
            .translation
            .map(f64::to_bits),
        [1.0f64.to_bits(), (-0.0f64).to_bits(), 2.5f64.to_bits()]
    );
}

#[test]
fn inserted_leaf_save_elides_positive_zero_and_writes_negative_zero() {
    let directory = tempfile::tempdir().unwrap();
    let (_, original, mut document) = opened_sited(directory.path());
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 5.0)
        .unwrap();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 0.0)
        .unwrap();
    let elided = directory.path().join("elided.mb");
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&elided),
        &|| true,
    ));
    assert!(std::fs::read(&elided).unwrap() == original);
    document
        .scene
        .set_channel(id, Channel::TranslateX, -0.0)
        .unwrap();
    let written = directory.path().join("negative-zero.mb");
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&written),
        &|| true,
    ));
    let bytes = std::fs::read(&written).unwrap();
    assert_eq!(bytes.len(), original.len() + 48);
    assert!(has_record(&bytes, [-0.0, 0.0, 0.0]));
    let reopened = load_native(&written).unwrap();
    assert_eq!(
        reopened
            .scene
            .node(id)
            .unwrap()
            .transform()
            .translation
            .map(f64::to_bits),
        [(-0.0f64).to_bits(), 0, 0]
    );
}

#[test]
fn inserted_leaf_save_is_rejected_when_the_source_changed_on_disk() {
    let directory = tempfile::tempdir().unwrap();
    let (path, original, mut document) = opened_sited(directory.path());
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 3.0)
        .unwrap();
    let mut replacement = original.clone();
    let last = replacement.len() - 1;
    replacement[last] ^= 1;
    std::fs::write(&path, &replacement).unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    assert!(std::fs::read(&path).unwrap() == replacement);
    // Save As derives from the bytes parsed at Open, so it still succeeds and leaves the changed source alone.
    let destination = directory.path().join("rescued.mb");
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    ));
    assert!(has_record(
        &std::fs::read(&destination).unwrap(),
        [3.0, 0.0, 0.0]
    ));
    assert!(std::fs::read(&path).unwrap() == replacement);
}

#[test]
#[ignore = "requires explicit private manifest; writes only independent temporary copies"]
fn private_models_save_as_and_repeat_on_working_copies_only() {
    let manifest = std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST").unwrap();
    let manifest = Path::new(&manifest);
    let directory = manifest.parent().unwrap().canonicalize().unwrap();
    let entries: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 5);
    for (ordinal, entry) in entries.as_array().unwrap().iter().enumerate() {
        let original = directory
            .join(entry["relative_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(original.starts_with(&directory));
        let before = std::fs::read(&original).unwrap();
        assert_eq!(before.len(), entry["bytes"].as_u64().unwrap() as usize);
        let mut document = load_native(&original).unwrap();
        let copy_directory = tempfile::tempdir().unwrap();
        let first_copy = copy_directory.path().join("first.mb");
        let second_copy = copy_directory.path().join("second.mb");
        let NativeOrigin::Binary(original_origin) = &document.origin else {
            panic!("fixture {ordinal} needs binary origin");
        };
        let parse = original_origin.parse_bytes.clone();
        let edit = document.scene.nodes().iter().find_map(|node| {
            let id = node.id();
            [
                (Channel::TranslateX, node.transform().translation[0]),
                (Channel::RotateX, node.transform().rotation[0]),
            ]
            .into_iter()
            .find_map(|(channel, original)| {
                let mut candidate = document.scene.clone();
                candidate.set_channel(id, channel, original + 0.125).ok()?;
                (candidate != document.scene && binary::encode(&candidate).is_ok()).then_some((
                    candidate,
                    id,
                    channel,
                    original + 0.5,
                ))
            })
        });
        let (first_edit, id, channel, later_value) =
            edit.unwrap_or_else(|| panic!("fixture {ordinal} needs a supported existing word"));
        println!("fixture {ordinal} repeated channel {channel:?}");
        let mut origin = document.origin.clone();
        let mut first_installed = Vec::new();
        for cycle in 0..6 {
            if cycle == 1 {
                document.scene = first_edit.clone();
            } else if cycle == 4 {
                document
                    .scene
                    .set_channel(id, channel, later_value)
                    .unwrap();
            }
            let expected = binary::encode(&document.scene).unwrap();
            if cycle == 0 {
                assert!(
                    expected == before,
                    "fixture {ordinal} unedited output differs"
                );
            }
            let destination = match cycle {
                0 => SaveDestination::As(&first_copy),
                3 => SaveDestination::As(&second_copy),
                _ => SaveDestination::Active,
            };
            let (next_origin, installed) =
                saved(save_native(&origin, &document.scene, destination, &|| true));
            origin = next_origin;
            let expected_path = if cycle < 3 { &first_copy } else { &second_copy };
            assert!(installed == expected_path.canonicalize().unwrap());
            assert!(
                std::fs::read(expected_path).unwrap() == expected,
                "fixture {ordinal} cycle {cycle} disk bytes differ"
            );
            assert!(
                origin.source_bytes().unwrap().as_ref() == expected,
                "fixture {ordinal} cycle {cycle} disk baseline differs"
            );
            let NativeOrigin::Binary(current) = &origin else {
                panic!("fixture {ordinal} cycle {cycle} lost binary origin");
            };
            assert!(Arc::ptr_eq(&parse, &current.parse_bytes));
            assert!(document.scene.has_binary_source(&parse));
            let reopened = load_native(expected_path).unwrap();
            let actual = reopened.scene.internal_transform(id).unwrap();
            let expected_pose = document.scene.internal_transform(id).unwrap();
            let (actual_word, expected_word) = match channel {
                Channel::TranslateX => (actual.translation_cm[0], expected_pose.translation_cm[0]),
                Channel::RotateX => (
                    actual.rotation_radians[0],
                    expected_pose.rotation_radians[0],
                ),
                _ => unreachable!(),
            };
            assert!(
                actual_word.to_bits() == expected_word.to_bits(),
                "fixture {ordinal} cycle {cycle} reopened word differs"
            );
            if cycle == 2 {
                first_installed = expected;
            } else if cycle >= 3 {
                assert!(
                    std::fs::read(&first_copy).unwrap() == first_installed,
                    "fixture {ordinal} cycle {cycle} previous copy changed"
                );
            }
        }
        assert!(
            std::fs::read(&original).unwrap() == before,
            "fixture {ordinal} original changed"
        );
    }
}

#[test]
fn active_save_and_repeat_preserve_parse_source_and_reopen_written_words() {
    let directory = tempfile::tempdir().unwrap();
    let (path, mut document) = opened(directory.path());
    let NativeOrigin::Binary(original) = &document.origin else {
        unreachable!()
    };
    let parse = original.parse_bytes.clone();
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 16777219.125)
        .unwrap();
    let (origin, installed) = saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(installed, path.canonicalize().unwrap());
    let NativeOrigin::Binary(baseline) = &origin else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(&parse, &baseline.parse_bytes));
    assert!(document.scene.has_binary_source(&parse));
    let first = std::fs::read(&path).unwrap();
    let (origin, _) = saved(save_native(
        &origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert!(std::fs::read(&path).unwrap() == first);
    document
        .scene
        .set_channel(id, Channel::TranslateX, 16777221.375)
        .unwrap();
    saved(save_native(
        &origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    let reopened = load_native(&path).unwrap();
    assert_eq!(
        reopened.scene.node(id).unwrap().transform().translation[0],
        16777221.375
    );
}

#[test]
fn save_as_no_clobber_and_hardlink_alias_retains_old_inode() {
    let directory = tempfile::tempdir().unwrap();
    let (path, mut document) = opened(directory.path());
    let alias = directory.path().join("alias.mb");
    std::fs::hard_link(&path, &alias).unwrap();
    let old = std::fs::read(&alias).unwrap();
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 4.125)
        .unwrap();
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert!(std::fs::read(&alias).unwrap() == old);
    let destination = directory.path().join("new.mb");
    let (origin, _) = saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    ));
    assert!(matches!(
        save_native(
            &origin,
            &document.scene,
            SaveDestination::As(&destination),
            &|| true
        ),
        Err(Error::DestinationExists)
    ));
    let link = directory.path().join("dangling.mb");
    std::os::unix::fs::symlink(directory.path().join("missing"), &link).unwrap();
    assert!(matches!(
        save_native(
            &origin,
            &document.scene,
            SaveDestination::As(&link),
            &|| true
        ),
        Err(Error::DestinationExists)
    ));
}

#[test]
fn mismatched_scene_conflict_and_cancellation_preserve_destinations() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let second = load_native(&path).unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &second.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| false
        ),
        Err(Error::Cancelled)
    ));
    assert!(std::fs::read(&path).unwrap() == before);
    let mut replacement = before.clone();
    let last = replacement.len() - 1;
    replacement[last] ^= 1;
    std::fs::write(&path, &replacement).unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    assert!(std::fs::read(&path).unwrap() == replacement);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    None,
    Create,
    Sync,
    Rename,
    Link,
    DirectorySync,
    CleanupCollision,
    AfterInstall,
    BeforeInstallEdit,
    BeforeStageMode,
    BeforeInstallMode,
    HookFifo,
    HookSymlink,
    HookMode,
    HookRestore,
}
struct FaultFs<'a> {
    inner: &'a Transaction<'a>,
    fault: Fault,
    renames: Cell<usize>,
    after_sync: Option<&'a Cell<bool>>,
}
impl AtomicFs for FaultFs<'_> {
    fn check_writable(&self, path: &Path) -> io::Result<()> {
        match self.fault {
            Fault::HookFifo | Fault::HookSymlink => {
                std::fs::rename(path, path.with_extension("retained"))?;
                if self.fault == Fault::HookFifo {
                    if !std::process::Command::new("mkfifo")
                        .arg(path)
                        .status()?
                        .success()
                    {
                        return Err(io::Error::from(io::ErrorKind::Other));
                    }
                } else {
                    std::os::unix::fs::symlink(path.with_extension("retained"), path)?;
                }
            }
            Fault::HookMode => {
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            }
            Fault::HookRestore => {
                std::fs::rename(path, path.with_extension("arrival"))?;
                std::fs::rename(path.with_extension("retained"), path)?;
            }
            _ => {}
        }
        self.inner.check_writable(path)
    }
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        if self.fault == Fault::Create {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        if self.fault == Fault::BeforeStageMode {
            std::fs::set_permissions(
                self.inner.destination.path(),
                std::fs::Permissions::from_mode(0o600),
            )?;
        }
        self.inner.create_new(path, permissions)
    }
    fn sync(&self, file: &File) -> io::Result<()> {
        if self.fault == Fault::Sync {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        self.inner.sync(file)?;
        if let Some(signal) = self.after_sync {
            signal.set(true);
        }
        Ok(())
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.renames.set(self.renames.get() + 1);
        if self.fault == Fault::Rename {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        if self.fault == Fault::CleanupCollision {
            std::fs::rename(from, from.with_extension("retained"))?;
            std::fs::write(from, b"arrival")?;
        }
        if self.fault == Fault::BeforeInstallEdit {
            let mut bytes = std::fs::read(to)?;
            bytes[24] ^= 1;
            std::fs::write(to, bytes)?;
        }
        if self.fault == Fault::BeforeInstallMode {
            std::fs::set_permissions(to, std::fs::Permissions::from_mode(0o600))?;
        }
        self.inner.rename(from, to)?;
        if self.fault == Fault::AfterInstall {
            std::fs::rename(to, to.with_extension("retained"))?;
            std::fs::write(to, b"arrival")?;
        }
        Ok(())
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        if self.fault == Fault::Link {
            return Err(io::Error::from(io::ErrorKind::Unsupported));
        }
        if self.fault == Fault::CleanupCollision {
            std::fs::rename(from, from.with_extension("retained"))?;
            std::fs::write(from, b"arrival")?;
        }
        self.inner.hard_link(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.inner.remove(path)
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        if self.fault == Fault::DirectorySync {
            return Err(self.inner.fail(Error::Io(io::ErrorKind::Other)));
        }
        self.inner.sync_dir(path)
    }
    fn set_permissions(&self, file: &File, permissions: std::fs::Permissions) -> io::Result<()> {
        self.inner.set_permissions(file, permissions)
    }
}

#[test]
fn preinstall_faults_preserve_bytes_and_forbidden_copy_fallback_creates_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (path, mut document) = opened(directory.path());
    let original = std::fs::read(&path).unwrap();
    document
        .scene
        .set_channel(document.scene.nodes()[0].id(), Channel::TranslateX, 19.25)
        .unwrap();
    for fault in [Fault::Create, Fault::Sync, Fault::Rename] {
        let transaction = prepare(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true,
        )
        .unwrap();
        let fs = FaultFs {
            inner: &transaction,
            fault,
            renames: Cell::new(0),
            after_sync: None,
        };
        assert!(
            transaction
                .run(
                    &fs,
                    RenameRetry {
                        attempts: 0,
                        first_delay_ms: 0
                    }
                )
                .is_err()
        );
        assert!(std::fs::read(&path).unwrap() == original);
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    let destination = directory.path().join("new.mb");
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::Link,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::Unsupported)
    ));
    assert!(!destination.exists());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn directory_sync_uncertainty_reconciles_without_reinstalling_or_rebasing_later_scene() {
    let directory = tempfile::tempdir().unwrap();
    let (_, mut document) = opened(directory.path());
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 19.25)
        .unwrap();
    let destination = directory.path().join("new.mb");
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::DirectorySync,
        renames: Cell::new(0),
        after_sync: None,
    };
    let SaveOutcome::Uncertain { receipt, .. } =
        transaction.run(&fs, RenameRetry::platform()).unwrap()
    else {
        panic!("sync failure must retain receipt");
    };
    let identity = Identity::of(&std::fs::metadata(&destination).unwrap());
    let written = std::fs::read(&destination).unwrap();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 23.75)
        .unwrap();
    let (origin, path) = receipt.reconcile().unwrap();
    assert_eq!(path, destination.canonicalize().unwrap());
    assert!(Identity::of(&std::fs::metadata(&destination).unwrap()) == identity);
    assert!(std::fs::read(&destination).unwrap() == written);
    assert!(origin.source_bytes().unwrap().as_ref() == written);
    assert_eq!(
        load_native(&destination)
            .unwrap()
            .scene
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        19.25
    );
    assert_eq!(
        document.scene.node(id).unwrap().transform().translation[0],
        23.75
    );
}

#[test]
fn detected_stage_and_postinstall_arrivals_are_never_removed() {
    for fault in [Fault::CleanupCollision, Fault::AfterInstall] {
        let directory = tempfile::tempdir().unwrap();
        let (path, mut document) = opened(directory.path());
        let original = std::fs::read(&path).unwrap();
        document
            .scene
            .set_channel(document.scene.nodes()[0].id(), Channel::TranslateX, 19.25)
            .unwrap();
        let transaction = prepare(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true,
        )
        .unwrap();
        let fs = FaultFs {
            inner: &transaction,
            fault,
            renames: Cell::new(0),
            after_sync: None,
        };
        let result = transaction.run(&fs, RenameRetry::platform());
        if fault == Fault::CleanupCollision {
            assert!(result.is_err());
            assert!(std::fs::read(&path).unwrap() == original);
            let state = transaction.state.borrow();
            let stage = state.stage.as_ref().unwrap();
            assert!(std::fs::read(directory.path().join(&stage.leaf)).unwrap() == b"arrival");
        } else {
            let SaveOutcome::Uncertain { receipt, .. } = result.unwrap() else {
                panic!("replacement after install must be uncertain");
            };
            assert!(matches!(receipt.reconcile(), Err(Error::SourceChanged)));
            assert!(std::fs::read(&path).unwrap() == b"arrival");
        }
    }
}

#[test]
fn a_guard_is_rechecked_on_every_actual_rename_retry() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let failed = Cell::new(false);
    let attempts = Cell::new(0);
    let guard = || !failed.get();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &guard,
    )
    .unwrap();
    struct Retry<'a>(&'a Transaction<'a>, &'a Cell<bool>, &'a Cell<usize>);
    impl AtomicFs for Retry<'_> {
        fn check_writable(&self, path: &Path) -> io::Result<()> {
            self.0.check_writable(path)
        }
        fn create_new(&self, p: &Path, n: NewFilePermissions) -> io::Result<File> {
            self.0.create_new(p, n)
        }
        fn sync(&self, f: &File) -> io::Result<()> {
            self.0.sync(f)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.2.set(self.2.get() + 1);
            self.0.preinstall(from, to, false)?;
            self.1.set(true);
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
        fn hard_link(&self, f: &Path, t: &Path) -> io::Result<()> {
            self.0.hard_link(f, t)
        }
        fn remove(&self, p: &Path) -> io::Result<()> {
            self.0.remove(p)
        }
        fn sync_dir(&self, p: &Path) -> io::Result<()> {
            self.0.sync_dir(p)
        }
        fn set_permissions(&self, f: &File, p: std::fs::Permissions) -> io::Result<()> {
            self.0.set_permissions(f, p)
        }
    }
    assert!(matches!(
        transaction.run(
            &Retry(&transaction, &failed, &attempts),
            RenameRetry {
                attempts: 1,
                first_delay_ms: 0
            }
        ),
        Err(Error::Cancelled)
    ));
    assert_eq!(attempts.get(), 2);
    assert!(failed.get());
    assert!(transaction.state.borrow().installed.is_none());
    assert!(std::fs::read(&path).unwrap() == wire::specimen());
}

#[test]
fn replaced_deleted_and_redirected_parent_bindings_refuse_without_touching_arrivals() {
    for removed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (path, document) = opened(directory.path());
        std::fs::rename(&path, path.with_extension("retained")).unwrap();
        if !removed {
            std::fs::write(&path, wire::specimen()).unwrap();
        }
        assert!(matches!(
            save_native(
                &document.origin,
                &document.scene,
                SaveDestination::Active,
                &|| true
            ),
            Err(Error::SourceChanged)
        ));
        assert_eq!(path.exists(), !removed);
    }
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("work");
    std::fs::create_dir(&parent).unwrap();
    let (_, document) = opened(&parent);
    let moved = directory.path().join("moved");
    std::fs::rename(&parent, &moved).unwrap();
    std::fs::create_dir(&parent).unwrap();
    std::fs::write(parent.join("scene.mb"), b"arrival").unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    assert!(std::fs::read(parent.join("scene.mb")).unwrap() == b"arrival");
    assert!(std::fs::read(moved.join("scene.mb")).unwrap() == wire::specimen());
}

#[test]
fn symlink_source_is_readable_but_requires_save_as() {
    let directory = tempfile::tempdir().unwrap();
    let (path, _) = opened(directory.path());
    let link = directory.path().join("link.mb");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    let document = load_native(&link).unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::Unsupported)
    ));
    let destination = directory.path().join("new.mb");
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    ));
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn same_size_and_timestamp_content_change_is_still_a_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let file = File::options().write(true).open(&path).unwrap();
    let metadata = file.metadata().unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[24] ^= 1;
    file.write_at(&bytes, 0).unwrap();
    file.set_times(
        std::fs::FileTimes::new()
            .set_accessed(metadata.accessed().unwrap())
            .set_modified(metadata.modified().unwrap()),
    )
    .unwrap();
    assert_eq!(
        file.metadata().unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    assert!(std::fs::read(&path).unwrap() == bytes);
}

#[test]
fn staged_save_as_cancellation_and_stage_conflicts_keep_their_typed_cause() {
    let directory = tempfile::tempdir().unwrap();
    let (_, document) = opened(directory.path());
    let destination = directory.path().join("new.mb");
    let synced = Cell::new(false);
    let guard = || !synced.get();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &guard,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::None,
        renames: Cell::new(0),
        after_sync: Some(&synced),
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::Cancelled)
    ));
    assert!(synced.get());
    assert!(!destination.exists());
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::As(&destination),
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::CleanupCollision,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::SourceChanged)
    ));
    assert!(!destination.exists());
    let state = transaction.state.borrow();
    assert!(
        std::fs::read(directory.path().join(&state.stage.as_ref().unwrap().leaf)).unwrap()
            == b"arrival"
    );
}

#[test]
fn same_inode_content_change_after_staging_refuses_actual_rename() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let identity = Identity::of(&std::fs::metadata(&path).unwrap());
    let mut expected = std::fs::read(&path).unwrap();
    expected[24] ^= 1;
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::BeforeInstallEdit,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::SourceChanged)
    ));
    assert!(Identity::of(&std::fs::metadata(&path).unwrap()) == identity);
    assert!(std::fs::read(path).unwrap() == expected);
}

#[test]
fn pinned_current_mode_must_match_upstream_and_staged_modes() {
    for fault in [
        Fault::BeforeStageMode,
        Fault::BeforeInstallMode,
        Fault::HookMode,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let (path, document) = opened(directory.path());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        let original = std::fs::read(&path).unwrap();
        let transaction = prepare(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true,
        )
        .unwrap();
        let fs = FaultFs {
            inner: &transaction,
            fault,
            renames: Cell::new(0),
            after_sync: None,
        };
        assert!(matches!(
            transaction.run(&fs, RenameRetry::platform()),
            Err(Error::SourceChanged)
        ));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(std::fs::read(path).unwrap() == original);
    }
}

#[test]
fn stale_supplied_mode_refuses_before_changing_owned_stage() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    let stage_path = transaction
        .destination
        .canonical_parent
        .join(craft_atomic::temp_name("scene.mb"));
    let stage = transaction
        .create_new(&stage_path, NewFilePermissions::Private)
        .unwrap();
    transaction
        .set_permissions(&stage, std::fs::metadata(&path).unwrap().permissions())
        .unwrap();
    let mut stale_mode = std::fs::metadata(&path).unwrap().permissions();
    stale_mode.set_mode((stale_mode.mode() & !0o777) | 0o600);
    assert!(transaction.set_permissions(&stage, stale_mode).is_err());
    assert!(matches!(
        transaction.state.borrow().failure,
        Some(Error::SourceChanged)
    ));
    assert_eq!(
        stage.metadata().unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn transient_path_mode_cannot_change_the_authorized_file_mode() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let original = std::fs::read(&path).unwrap();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    std::fs::rename(&path, path.with_extension("retained")).unwrap();
    std::fs::write(&path, b"arrival").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::HookRestore,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::SourceChanged)
    ));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(std::fs::read(&path).unwrap() == original);
    assert!(std::fs::read(path.with_extension("arrival")).unwrap() == b"arrival");
    assert!(transaction.state.borrow().installed.is_none());
}

#[test]
fn skipped_writable_hook_does_not_bypass_stage_admission() {
    for directory_arrival in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (path, document) = opened(directory.path());
        let original = std::fs::read(&path).unwrap();
        let transaction = prepare(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true,
        )
        .unwrap();
        std::fs::rename(&path, path.with_extension("retained")).unwrap();
        if directory_arrival {
            std::fs::create_dir(&path).unwrap();
        }
        assert!(matches!(
            transaction.run(&transaction, RenameRetry::platform()),
            Err(Error::SourceChanged)
        ));
        assert!(transaction.state.borrow().stage.is_none());
        assert!(transaction.state.borrow().installed.is_none());
        assert!(std::fs::read(path.with_extension("retained")).unwrap() == original);
        assert_eq!(path.is_dir(), directory_arrival);
    }
}

#[test]
fn writable_hook_refuses_a_symlink_arrival_before_staging() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let original = std::fs::read(&path).unwrap();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::HookSymlink,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(transaction.run(&fs, RenameRetry::platform()).is_err());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(std::fs::read(path.with_extension("retained")).unwrap() == original);
    assert!(transaction.state.borrow().stage.is_none());
}

#[test]
fn readonly_authorized_file_refuses_before_stage_creation() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let original = std::fs::read(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    assert!(matches!(
        transaction.run(&transaction, RenameRetry::platform()),
        Err(Error::Io(io::ErrorKind::PermissionDenied))
    ));
    assert!(transaction.state.borrow().stage.is_none());
    assert!(std::fs::read(&path).unwrap() == original);
}

#[test]
fn fifo_substitution_at_actual_writable_hook_is_bounded() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "document::native_save::unix::tests::fifo_writable_hook_child",
            "--ignored",
        ])
        .env("AZTECCRAFT_SAVE_FIFO_CHILD", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Liveness guard only: a regression blocks forever in the child's FIFO open, so any
    // finite bound fails it; the bound must exceed process start-up time under load.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "actual writable-hook FIFO child must refuse"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("actual writable hook blocked on FIFO substitution");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "bounded subprocess of the actual writable-hook FIFO regression"]
fn fifo_writable_hook_child() {
    if std::env::var_os("AZTECCRAFT_SAVE_FIFO_CHILD").is_none() {
        return;
    }
    use std::os::unix::fs::FileTypeExt;
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let original = std::fs::read(&path).unwrap();
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::HookFifo,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(transaction.run(&fs, RenameRetry::platform()).is_err());
    assert!(
        std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_fifo()
    );
    assert!(std::fs::read(path.with_extension("retained")).unwrap() == original);
    assert!(transaction.state.borrow().stage.is_none());
    assert!(transaction.state.borrow().installed.is_none());
}

#[test]
fn exclusive_stage_collision_never_grants_ownership_or_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let (_, document) = opened(directory.path());
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    struct Collision<'a> {
        inner: &'a Transaction<'a>,
        calls: Cell<usize>,
        arrival: RefCell<Option<PathBuf>>,
    }
    impl AtomicFs for Collision<'_> {
        fn check_writable(&self, path: &Path) -> io::Result<()> {
            self.inner.check_writable(path)
        }
        fn create_new(&self, p: &Path, n: NewFilePermissions) -> io::Result<File> {
            self.calls.set(self.calls.get() + 1);
            if self.calls.get() == 1 {
                std::fs::write(p, b"arrival")?;
                *self.arrival.borrow_mut() = Some(p.to_path_buf());
            }
            self.inner.create_new(p, n)
        }
        fn sync(&self, f: &File) -> io::Result<()> {
            self.inner.sync(f)
        }
        fn rename(&self, f: &Path, t: &Path) -> io::Result<()> {
            self.inner.rename(f, t)
        }
        fn hard_link(&self, f: &Path, t: &Path) -> io::Result<()> {
            self.inner.hard_link(f, t)
        }
        fn remove(&self, p: &Path) -> io::Result<()> {
            self.inner.remove(p)
        }
        fn sync_dir(&self, p: &Path) -> io::Result<()> {
            self.inner.sync_dir(p)
        }
        fn set_permissions(&self, f: &File, p: std::fs::Permissions) -> io::Result<()> {
            self.inner.set_permissions(f, p)
        }
    }
    let fs = Collision {
        inner: &transaction,
        calls: Cell::new(0),
        arrival: RefCell::new(None),
    };
    saved(transaction.run(&fs, RenameRetry::platform()));
    assert_eq!(fs.calls.get(), 2);
    let arrival = fs.arrival.borrow();
    assert!(std::fs::read(arrival.as_ref().unwrap()).unwrap() == b"arrival");
    assert!(
        transaction.state.borrow().stage.as_ref().unwrap().leaf
            != arrival.as_ref().unwrap().file_name().unwrap()
    );
}
