use super::*;
use crate::{document::load_native, native_scene::Channel};
use std::cell::Cell;
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
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

fn identity_of(path: &Path) -> Identity {
    Identity::of(&File::open(path).unwrap()).unwrap()
}
/// Creating a symbolic link needs Developer Mode or elevation; tests that need one skip
/// themselves where the account cannot create it.
fn symlink(target: &Path, link: &Path) -> bool {
    let created = std::os::windows::fs::symlink_file(target, link).is_ok();
    if !created {
        eprintln!("SKIPPED: this account cannot create symbolic links");
    }
    created
}
fn set_readonly(path: &Path, readonly: bool) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(readonly);
    std::fs::set_permissions(path, permissions).unwrap();
}
fn mklink_junction(link: &Path, target: &Path) {
    assert!(
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    );
}
fn attrib(flag: &str, path: &Path) {
    assert!(
        std::process::Command::new("attrib")
            .arg(flag)
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
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
    AfterInstallCopy,
    BeforeInstallEdit,
    BeforeInstallReadonly,
    HookSymlink,
}
struct FaultFs<'a> {
    inner: &'a Transaction<'a>,
    fault: Fault,
    renames: Cell<usize>,
    after_sync: Option<&'a Cell<bool>>,
}
impl AtomicFs for FaultFs<'_> {
    fn check_writable(&self, path: &Path) -> io::Result<()> {
        if self.fault == Fault::HookSymlink {
            std::fs::rename(path, path.with_extension("retained"))?;
            std::os::windows::fs::symlink_file(path.with_extension("retained"), path)?;
        }
        self.inner.check_writable(path)
    }
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        if self.fault == Fault::Create {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
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
        if self.fault == Fault::BeforeInstallReadonly {
            set_readonly(to, true);
        }
        self.inner.rename(from, to)?;
        if self.fault == Fault::AfterInstall {
            std::fs::rename(to, to.with_extension("retained"))?;
            std::fs::write(to, b"arrival")?;
        }
        if self.fault == Fault::AfterInstallCopy {
            std::fs::rename(to, to.with_extension("retained"))?;
            std::fs::copy(to.with_extension("retained"), to)?;
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
fn hidden_or_system_authorized_file_refuses_save_but_allows_save_as() {
    for flag in ["+h", "+s"] {
        let directory = tempfile::tempdir().unwrap();
        let (path, document) = opened(directory.path());
        attrib(flag, &path);
        let original = std::fs::read(&path).unwrap();
        assert!(matches!(
            save_native(
                &document.origin,
                &document.scene,
                SaveDestination::Active,
                &|| true
            ),
            Err(Error::Unsupported)
        ));
        assert!(std::fs::read(&path).unwrap() == original);
        let destination = directory.path().join("visible.mb");
        saved(save_native(
            &document.origin,
            &document.scene,
            SaveDestination::As(&destination),
            &|| true,
        ));
        attrib(&flag.replace('+', "-"), &path);
    }
}

#[test]
fn identical_copy_replacing_the_installed_file_is_not_confirmed() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let transaction = prepare(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    )
    .unwrap();
    let fs = FaultFs {
        inner: &transaction,
        fault: Fault::AfterInstallCopy,
        renames: Cell::new(0),
        after_sync: None,
    };
    let SaveOutcome::Uncertain { receipt, .. } =
        transaction.run(&fs, RenameRetry::platform()).unwrap()
    else {
        panic!("a same-bytes replacement after install must be uncertain");
    };
    assert!(matches!(receipt.reconcile(), Err(Error::SourceChanged)));
    assert!(std::fs::read(&path).unwrap() == wire::specimen());
}

#[test]
fn a_bound_save_as_folder_cannot_be_moved_until_released() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("work");
    std::fs::create_dir(&parent).unwrap();
    let destination = Destination::capture(&parent.join("new.mb")).unwrap();
    let moved = directory.path().join("moved");
    assert!(std::fs::rename(&parent, &moved).is_err());
    assert!(destination.absent().is_ok());
    drop(destination);
    std::fs::rename(&parent, &moved).unwrap();
}

#[test]
fn stream_device_and_trailing_dot_names_are_refused_before_writing() {
    let directory = tempfile::tempdir().unwrap();
    let (_, document) = opened(directory.path());
    for name in [
        "foreign:y.mb",
        "CON.mb",
        "nul.mb",
        "com1.mb",
        "LPT9.mb",
        "wild*.mb",
    ] {
        let destination = directory.path().join(name);
        assert!(
            matches!(
                save_native(
                    &document.origin,
                    &document.scene,
                    SaveDestination::As(&destination),
                    &|| true
                ),
                Err(Error::InvalidDestination)
            ),
            "{name}"
        );
    }
    let names: Vec<_> = std::fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["scene.mb"]);
    for name in ["com0.mb", "console.mb", "CONx.mb", "a.b.mb"] {
        assert!(valid_leaf(OsStr::new(name)), "{name}");
    }
    for name in ["x.mb.", "x.mb ", "a	b.mb"] {
        assert!(!valid_leaf(OsStr::new(name)), "{name}");
    }
}

#[test]
fn a_file_held_without_delete_sharing_is_refused_not_reported_changed() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    // Read and write sharing only: a sync or backup tool holding the scene this way blocks
    // the replacing rename (the editor's own read handle rules out denying read sharing).
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&path)
        .unwrap();
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::Io(io::ErrorKind::PermissionDenied))
    ));
    drop(lock);
    assert!(std::fs::read(&path).unwrap() == wire::specimen());
    saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
}

#[test]
fn readonly_arrival_before_install_refuses_the_rename() {
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
        fault: Fault::BeforeInstallReadonly,
        renames: Cell::new(0),
        after_sync: None,
    };
    assert!(matches!(
        transaction.run(&fs, RenameRetry::platform()),
        Err(Error::SourceChanged)
    ));
    assert!(transaction.state.borrow().installed.is_none());
    assert!(std::fs::read(&path).unwrap() == original);
    set_readonly(&path, false);
}

#[test]
fn source_open_in_another_handle_still_saves_and_save_as_never_replaces() {
    let directory = tempfile::tempdir().unwrap();
    let (path, mut document) = opened(directory.path());
    let reader = File::open(&path).unwrap();
    let id = document.scene.nodes()[0].id();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 7.5)
        .unwrap();
    let (origin, installed) = saved(save_native(
        &document.origin,
        &document.scene,
        SaveDestination::Active,
        &|| true,
    ));
    assert_eq!(installed, path.canonicalize().unwrap());
    drop(reader);
    let occupied = directory.path().join("occupied.mb");
    std::fs::write(&occupied, b"someone else").unwrap();
    assert!(matches!(
        save_native(
            &origin,
            &document.scene,
            SaveDestination::As(&occupied),
            &|| true
        ),
        Err(Error::DestinationExists)
    ));
    assert!(std::fs::read(&occupied).unwrap() == b"someone else");
    assert_eq!(
        load_native(&path)
            .unwrap()
            .scene
            .node(id)
            .unwrap()
            .transform()
            .translation[0],
        7.5
    );
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
    let identity = identity_of(&destination);
    let written = std::fs::read(&destination).unwrap();
    document
        .scene
        .set_channel(id, Channel::TranslateX, 23.75)
        .unwrap();
    let (origin, path) = receipt.reconcile().unwrap();
    assert_eq!(path, destination.canonicalize().unwrap());
    assert!(identity_of(&destination) == identity);
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
    let identity = identity_of(&path);
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
    assert!(identity_of(&path) == identity);
    assert!(std::fs::read(path).unwrap() == expected);
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

#[test]
fn same_size_and_timestamp_content_change_is_still_a_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let (path, document) = opened(directory.path());
    let file = File::options().write(true).open(&path).unwrap();
    let metadata = file.metadata().unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[24] ^= 1;
    file.seek_write(&bytes, 0).unwrap();
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
    // Windows refuses to rename a folder while the opened scene's handle is inside it.
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("work");
    std::fs::create_dir(&parent).unwrap();
    let (_, document) = opened(&parent);
    let error = std::fs::rename(&parent, directory.path().join("moved")).unwrap_err();
    // Access denied (open child) or sharing violation (the pinned folder handle).
    assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error:?}");
    drop(document);
    // A junction retargeted after Open redirects the requested folder elsewhere.
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let junction = directory.path().join("junction");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    std::fs::write(second.join("scene.mb"), b"arrival").unwrap();
    mklink_junction(&junction, &first);
    let (_, document) = opened(&junction);
    std::fs::remove_dir(&junction).unwrap();
    mklink_junction(&junction, &second);
    assert!(matches!(
        save_native(
            &document.origin,
            &document.scene,
            SaveDestination::Active,
            &|| true
        ),
        Err(Error::SourceChanged)
    ));
    assert!(std::fs::read(second.join("scene.mb")).unwrap() == b"arrival");
    assert!(std::fs::read(first.join("scene.mb")).unwrap() == wire::specimen());
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
    if !symlink(&directory.path().join("missing"), &link) {
        return;
    }
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
fn symlink_source_is_readable_but_requires_save_as() {
    let directory = tempfile::tempdir().unwrap();
    let (path, _) = opened(directory.path());
    let link = directory.path().join("link.mb");
    if !symlink(&path, &link) {
        return;
    }
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
fn writable_hook_refuses_a_symlink_arrival_before_staging() {
    let directory = tempfile::tempdir().unwrap();
    if !symlink(
        &directory.path().join("probe-target"),
        &directory.path().join("probe"),
    ) {
        return;
    }
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
    set_readonly(&path, true);
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
    set_readonly(&path, false);
}
