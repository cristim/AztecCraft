use super::*;
use std::{cell::Cell, fs::File};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    CreateNew,
    Sync,
    HardLink,
    SyncDir,
    Remove,
}

/// Real filesystem that fails the `nth` (1-based) call of one operation, or all calls from it on.
struct FaultFs {
    op: Op,
    nth: u32,
    sticky: bool,
    seen: Cell<u32>,
}

impl FaultFs {
    fn once(op: Op, nth: u32) -> Self {
        Self {
            op,
            nth,
            sticky: false,
            seen: Cell::new(0),
        }
    }
    fn always(op: Op) -> Self {
        Self {
            op,
            nth: 1,
            sticky: true,
            seen: Cell::new(0),
        }
    }
    fn gate(&self, op: Op) -> io::Result<()> {
        if op != self.op {
            return Ok(());
        }
        self.seen.set(self.seen.get() + 1);
        let hit = if self.sticky {
            self.seen.get() >= self.nth
        } else {
            self.seen.get() == self.nth
        };
        if hit {
            Err(io::Error::other("injected fault"))
        } else {
            Ok(())
        }
    }
}

impl AtomicFs for FaultFs {
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        self.gate(Op::CreateNew)?;
        RealFs.create_new(path, permissions)
    }
    fn sync(&self, file: &File) -> io::Result<()> {
        self.gate(Op::Sync)?;
        RealFs.sync(file)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.gate(Op::HardLink)?;
        RealFs.hard_link(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.gate(Op::Remove)?;
        RealFs.remove(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        self.gate(Op::SyncDir)?;
        RealFs.sync_dir(dir)
    }
    fn set_permissions(&self, file: &File, perms: fs::Permissions) -> io::Result<()> {
        RealFs.set_permissions(file, perms)
    }
}

#[test]
fn injected_failures_preserve_earlier_files_and_leave_no_partial() {
    for (label, fault) in [
        ("create stage", FaultFs::always(Op::CreateNew)),
        ("fsync stage", FaultFs::once(Op::Sync, 1)),
        ("link and copy fallback fsync", FaultFs::once(Op::Sync, 2)),
    ] {
        let (_tmp, store) = store(10);
        let before = store.write("a", b"original").unwrap().entry;
        // For the copy-fallback case the link must fail first.
        let result = if label.starts_with("link") {
            struct LinkThenSync(FaultFs);
            impl AtomicFs for LinkThenSync {
                fn create_new(&self, p: &Path, m: NewFilePermissions) -> io::Result<File> {
                    self.0.create_new(p, m)
                }
                fn sync(&self, f: &File) -> io::Result<()> {
                    self.0.sync(f)
                }
                fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
                    self.0.rename(a, b)
                }
                fn hard_link(&self, _: &Path, _: &Path) -> io::Result<()> {
                    Err(io::Error::other("no hard links"))
                }
                fn remove(&self, p: &Path) -> io::Result<()> {
                    self.0.remove(p)
                }
                fn sync_dir(&self, d: &Path) -> io::Result<()> {
                    self.0.sync_dir(d)
                }
                fn set_permissions(&self, f: &File, p: fs::Permissions) -> io::Result<()> {
                    self.0.set_permissions(f, p)
                }
            }
            store.write_with(&LinkThenSync(fault), "a", b"replacement")
        } else {
            store.write_with(&fault, "a", b"replacement")
        };
        assert!(matches!(result, Err(RecoveryError::Io { .. })), "{label}");
        assert_eq!(
            names(&store),
            vec![before.file_name.clone()],
            "{label}: only the original remains"
        );
        assert_eq!(store.read(&before).unwrap(), b"original", "{label}");
    }
}

#[test]
fn concurrent_writer_taking_the_name_is_retried_then_bounded() {
    let (_tmp, store) = store(10);
    store.write("a", b"1").unwrap();
    // Sync fault at the first attempt only is a hard error (not AlreadyExists), see below;
    // here a hard_link that reports AlreadyExists on every attempt must end in Contended.
    struct Taken;
    impl AtomicFs for Taken {
        fn create_new(&self, p: &Path, m: NewFilePermissions) -> io::Result<File> {
            RealFs.create_new(p, m)
        }
        fn sync(&self, f: &File) -> io::Result<()> {
            RealFs.sync(f)
        }
        fn rename(&self, a: &Path, b: &Path) -> io::Result<()> {
            RealFs.rename(a, b)
        }
        fn hard_link(&self, _: &Path, _: &Path) -> io::Result<()> {
            Err(io::ErrorKind::AlreadyExists.into())
        }
        fn remove(&self, p: &Path) -> io::Result<()> {
            RealFs.remove(p)
        }
        fn sync_dir(&self, d: &Path) -> io::Result<()> {
            RealFs.sync_dir(d)
        }
        fn set_permissions(&self, f: &File, p: fs::Permissions) -> io::Result<()> {
            RealFs.set_permissions(f, p)
        }
    }
    assert!(matches!(
        store.write_with(&Taken, "a", b"2"),
        Err(RecoveryError::Contended)
    ));
    assert_eq!(
        names(&store),
        vec![file_name("a", 4242, 1)],
        "no partial or staging file remains"
    );
}

#[test]
fn directory_sync_failure_after_install_is_still_success() {
    let (_tmp, store) = store(10);
    let out = store
        .write_with(&FaultFs::always(Op::SyncDir), "a", b"data")
        .unwrap();
    assert_eq!(store.read(&out.entry).unwrap(), b"data");
}

#[test]
fn error_display_omits_paths_and_payload() {
    let (_tmp, store) = store(10);
    let error = store
        .write_with(&FaultFs::always(Op::CreateNew), "secret-stem", b"PAYLOAD")
        .unwrap_err();
    let shown = format!("{error} {error:?}");
    assert!(!format!("{error}").contains("recovery/"), "{shown}");
    assert!(!format!("{error}").contains(store.dir().to_str().unwrap()));
    assert!(!format!("{error}").contains("PAYLOAD"));
}

#[test]
fn unusable_directories_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("plain");
    fs::write(&file, b"x").unwrap();
    let store = RecoveryStore::new(&file, 3, 4242).unwrap();
    assert!(matches!(
        store.write("a", b"1"),
        Err(RecoveryError::DirectoryNotUsable)
    ));
    assert!(matches!(
        store.list(),
        Err(RecoveryError::DirectoryNotUsable)
    ));
    assert_eq!(fs::read(&file).unwrap(), b"x");
    #[cfg(unix)]
    {
        let real = tmp.path().join("real");
        fs::create_dir(&real).unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let store = RecoveryStore::new(&link, 3, 4242).unwrap();
        assert!(matches!(
            store.write("a", b"1"),
            Err(RecoveryError::DirectoryNotUsable)
        ));
        assert_eq!(fs::read_dir(&real).unwrap().count(), 0);
    }
}

#[test]
fn list_ignores_orphan_staging_symlinks_and_missing_dir() {
    let (_tmp, store) = store(10);
    assert!(store.list().unwrap().is_empty());
    store.write("a", b"1").unwrap();
    fs::write(
        store
            .dir()
            .join(".a.autosave.p4242.000002.recovery.1-0.craft_tmp"),
        b"partial",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        store.dir().join("nowhere"),
        store.dir().join(file_name("a", 4242, 9)),
    )
    .unwrap();
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].sequence, 1);
}

#[test]
fn read_refuses_forged_and_non_regular_entries() {
    let (_tmp, store) = store(10);
    let good = store.write("a", b"1").unwrap().entry;
    let mut forged = good.clone();
    forged.file_name = "../outside.recovery".into();
    assert!(matches!(
        store.read(&forged),
        Err(RecoveryError::NotInStore)
    ));
    let mut mismatch = good.clone();
    mismatch.sequence = 2;
    assert!(matches!(
        store.read(&mismatch),
        Err(RecoveryError::NotInStore)
    ));
    fs::create_dir(store.dir().join(file_name("a", 4242, 5))).unwrap();
    let dir_entry = RecoveryEntry {
        stem: "a".into(),
        pid: 4242,
        sequence: 5,
        file_name: file_name("a", 4242, 5),
        len: 0,
    };
    assert!(matches!(
        store.read(&dir_entry),
        Err(RecoveryError::NotInStore)
    ));
}

#[cfg(unix)]
#[test]
fn dangling_symlink_at_the_next_name_is_skipped_not_followed() {
    let (tmp, store) = store(10);
    store.write("a", b"1").unwrap();
    let outside = tmp.path().join("outside.txt");
    std::os::unix::fs::symlink(&outside, store.dir().join(file_name("a", 4242, 2))).unwrap();
    let out = store.write("a", b"2").unwrap();
    assert_eq!(out.entry.sequence, 3);
    assert!(!outside.exists(), "nothing was created through the link");
}

#[test]
fn exhausted_sequence_is_a_typed_error() {
    let (_tmp, store) = store(10);
    fs::create_dir_all(store.dir()).unwrap();
    fs::write(store.dir().join(file_name("a", 4242, 999_999)), b"x").unwrap();
    assert!(matches!(
        store.write("a", b"1"),
        Err(RecoveryError::SequenceExhausted)
    ));
}

#[test]
fn remove_refuses_forged_entries() {
    let (_tmp, store) = store(10);
    let forged = RecoveryEntry {
        stem: "a".into(),
        pid: 1,
        sequence: 1,
        file_name: "../x".into(),
        len: 0,
    };
    assert!(matches!(
        store.remove(&forged),
        Err(RecoveryError::NotInStore)
    ));
}

#[test]
fn prune_failure_is_reported_and_write_survives() {
    let (_tmp, store) = store(1);
    store.write("a", b"1").unwrap();
    let out = store
        .write_with(&FaultFs::always(Op::Remove), "a", b"2")
        .unwrap();
    assert!(out.prune_failures >= 1);
    assert_eq!(out.pruned, 0);
    assert_eq!(store.read(&out.entry).unwrap(), b"2");
}
