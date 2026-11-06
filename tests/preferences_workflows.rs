use azteccraft::compatibility::MayaVersion;
use azteccraft::preferences::{
    Baseline, MAX_FILE_BYTES, Os, Preferences, PreferencesError, ResetOutcome, Store, load,
    platform_path,
};
use std::ffi::OsString;
use std::fs;
use std::path::Path;

fn year(p: &Preferences) -> u16 {
    p.maya_compatibility_version.year()
}

fn blocked(store: &Store) -> (&PreferencesError, Option<&[u8]>) {
    match store {
        Store::Blocked { error, seen, .. } => (error, seen.as_deref()),
        other => panic!("expected Blocked, got {other:?}"),
    }
}

fn assert_blocked_with(dir: &Path, content: &[u8], expected: PreferencesError) {
    let path = dir.join("preferences.json");
    fs::write(&path, content).unwrap();
    let loaded = load(&path);
    let (error, seen) = blocked(&loaded.store);
    assert_eq!(*error, expected, "{}", String::from_utf8_lossy(content));
    assert_eq!(seen, Some(content));
    assert_eq!(year(&loaded.preferences), 2027);
    assert_eq!(fs::read(&path).unwrap(), content);
    let dir_text = dir.to_string_lossy();
    assert!(!format!("{error}").contains(&*dir_text));
    assert!(!format!("{error:?}").contains(&*dir_text));
}

#[test]
fn missing_file_gives_defaults_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let loaded = load(&path);
    assert_eq!(year(&loaded.preferences), 2027);
    assert!(matches!(loaded.store, Store::Writable { .. }));
    assert!(!path.exists());
}

#[test]
fn a_schema_one_file_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::write(&path, br#"{"schema":1,"maya_compatibility_version":2015}"#).unwrap();
    let loaded = load(&path);
    assert_eq!(
        loaded.preferences.maya_compatibility_version,
        MayaVersion::try_from(2015).unwrap()
    );
    assert!(matches!(loaded.store, Store::Writable { .. }));
}

#[test]
fn invalid_content_blocks_with_a_typed_error_and_keeps_the_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    assert_blocked_with(d, b"{not json", PreferencesError::Malformed);
    assert_blocked_with(d, b"[]", PreferencesError::Malformed);
    assert_blocked_with(
        d,
        br#"{"schema":2,"maya_compatibility_version":2027}"#,
        PreferencesError::UnknownSchema,
    );
    assert_blocked_with(
        d,
        br#"{"schema":2,"other":true}"#,
        PreferencesError::UnknownSchema,
    );
    for v in ["2010", "2028", "2027.9", "\"2027\"", "null", "-1"] {
        let body = format!(r#"{{"schema":1,"maya_compatibility_version":{v}}}"#);
        assert_blocked_with(d, body.as_bytes(), PreferencesError::VersionOutOfRange);
    }
    assert_blocked_with(
        d,
        br#"{"schema":1,"maya_compatibility_version":2027,"x":1}"#,
        PreferencesError::Malformed,
    );
    assert_blocked_with(d, br#"{"schema":1}"#, PreferencesError::Malformed);
}

#[test]
fn a_file_over_the_cap_blocks_as_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let cap = usize::try_from(MAX_FILE_BYTES).unwrap();
    fs::write(&path, vec![b' '; cap + 10]).unwrap();
    let loaded = load(&path);
    let (error, seen) = blocked(&loaded.store);
    assert_eq!(*error, PreferencesError::TooLarge);
    assert_eq!(seen.map(<[u8]>::len), Some(cap + 1));
}

#[test]
fn a_file_exactly_at_the_cap_is_still_parsed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let mut body = br#"{"schema":1,"maya_compatibility_version":2020}"#.to_vec();
    body.resize(usize::try_from(MAX_FILE_BYTES).unwrap(), b' ');
    fs::write(&path, &body).unwrap();
    assert_eq!(year(&load(&path).preferences), 2020);
}

#[test]
fn a_directory_blocks_as_not_a_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::create_dir(&path).unwrap();
    let loaded = load(&path);
    let (error, seen) = blocked(&loaded.store);
    assert_eq!(*error, PreferencesError::NotRegularFile);
    assert_eq!(seen, None);
    assert!(path.is_dir());
}

// Windows needs a privilege to create symlinks, so the test cannot run on a stock account.
#[cfg(unix)]
#[test]
fn a_symlink_blocks_and_is_never_followed() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.json");
    let good = br#"{"schema":1,"maya_compatibility_version":2015}"#;
    fs::write(&target, good).unwrap();
    let path = dir.path().join("preferences.json");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let loaded = load(&path);
    let (error, seen) = blocked(&loaded.store);
    assert_eq!(*error, PreferencesError::Symlink);
    assert_eq!(seen, None);
    assert_eq!(year(&loaded.preferences), 2027);
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), good);
}

fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| OsString::from(*v))
    }
}

#[test]
fn platform_paths_per_os() {
    let p = |os, pairs: &[(&str, &str)]| platform_path(&env(pairs), os);
    assert_eq!(
        p(Os::Macos, &[("HOME", "/Users/u")]).unwrap(),
        Path::new("/Users/u/Library/Application Support/AztecCraft/preferences.json")
    );
    assert_eq!(
        p(
            Os::Linux,
            &[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/home/u")]
        )
        .unwrap(),
        Path::new("/xdg/azteccraft/preferences.json")
    );
    assert_eq!(
        p(Os::Linux, &[("HOME", "/home/u")]).unwrap(),
        Path::new("/home/u/.config/azteccraft/preferences.json")
    );
    assert_eq!(
        p(Os::Linux, &[("XDG_CONFIG_HOME", ""), ("HOME", "/home/u")]).unwrap(),
        Path::new("/home/u/.config/azteccraft/preferences.json")
    );
    assert_eq!(
        p(
            Os::Windows,
            &[("APPDATA", "C:\\Users\\u\\AppData\\Roaming")]
        )
        .unwrap(),
        Path::new("C:\\Users\\u\\AppData\\Roaming")
            .join("AztecCraft")
            .join("preferences.json")
    );
}

#[test]
fn empty_or_relative_roots_have_no_config_dir() {
    let p = |os, pairs: &[(&str, &str)]| platform_path(&env(pairs), os);
    assert_eq!(p(Os::Macos, &[]), Err(PreferencesError::NoConfigDir));
    assert_eq!(
        p(Os::Macos, &[("HOME", "")]),
        Err(PreferencesError::NoConfigDir)
    );
    assert_eq!(
        p(Os::Macos, &[("HOME", "rel")]),
        Err(PreferencesError::NoConfigDir)
    );
    assert_eq!(
        p(Os::Linux, &[("HOME", "rel")]),
        Err(PreferencesError::NoConfigDir)
    );
    assert_eq!(
        p(Os::Linux, &[("XDG_CONFIG_HOME", "rel"), ("HOME", "/h")]),
        Ok(Path::new("/h/.config/azteccraft/preferences.json").to_path_buf())
    );
    assert_eq!(
        p(Os::Windows, &[("APPDATA", "rel")]),
        Err(PreferencesError::NoConfigDir)
    );
}

mod save {
    use super::*;
    use craft_atomic::{AtomicFs, NewFilePermissions, RealFs};
    use std::fs::File;
    use std::io;

    pub fn v(year: u64) -> Preferences {
        Preferences {
            maya_compatibility_version: MayaVersion::try_from(year).unwrap(),
        }
    }

    /// Real file system except for one operation, which fails with `kind`.
    #[derive(Default)]
    pub struct FailingFs {
        pub fail_create: bool,
        pub fail_rename: bool,
        pub fail_remove: bool,
        pub fail_link: bool,
    }

    impl AtomicFs for FailingFs {
        fn create_new(&self, path: &Path, p: NewFilePermissions) -> io::Result<File> {
            if self.fail_create {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealFs.create_new(path, p)
        }
        fn sync(&self, file: &File) -> io::Result<()> {
            RealFs.sync(file)
        }
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.fail_rename {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealFs.rename(from, to)
        }
        fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
            if self.fail_link {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealFs.hard_link(from, to)
        }
        fn remove(&self, path: &Path) -> io::Result<()> {
            // Only the original fails; the backup link must stay removable.
            if self.fail_remove && !path.to_string_lossy().contains(".bad-") {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealFs.remove(path)
        }
        fn sync_dir(&self, dir: &Path) -> io::Result<()> {
            RealFs.sync_dir(dir)
        }
        fn set_permissions(&self, file: &File, perms: fs::Permissions) -> io::Result<()> {
            RealFs.set_permissions(file, perms)
        }
    }

    pub fn leftovers(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

#[test]
fn save_then_load_round_trips_and_creates_the_parent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("preferences.json");
    let mut store = load(&path).store;
    store.save(&save::v(2015)).unwrap();
    assert_eq!(
        fs::read(&path).unwrap(),
        b"{\"schema\":1,\"maya_compatibility_version\":2015}\n"
    );
    let loaded = load(&path);
    assert_eq!(year(&loaded.preferences), 2015);
    store.save(&save::v(2018)).unwrap();
    assert_eq!(year(&load(&path).preferences), 2018);
}

#[test]
fn a_failing_stage_or_rename_keeps_the_old_bytes_and_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let old = br#"{"schema":1,"maya_compatibility_version":2015}"#;
    fs::write(&path, old).unwrap();
    let mut store = load(&path).store;
    let before = store.clone();
    for (create, rename) in [(true, false), (false, true)] {
        let fs_ = save::FailingFs {
            fail_create: create,
            fail_rename: rename,
            ..Default::default()
        };
        let err = store.save_with(&save::v(2020), &fs_).unwrap_err();
        assert_eq!(
            err,
            PreferencesError::Write(std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(fs::read(&path).unwrap(), old);
        assert_eq!(store, before);
        assert_eq!(save::leftovers(dir.path()), ["preferences.json"]);
        let dir_text = dir.path().to_string_lossy().into_owned();
        assert!(!format!("{err} {err:?}").contains(&dir_text));
    }
    store.save(&save::v(2020)).unwrap();
    assert_eq!(year(&load(&path).preferences), 2020);
}

#[test]
fn a_file_changed_since_load_is_not_overwritten_and_blocks_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::write(&path, br#"{"schema":1,"maya_compatibility_version":2015}"#).unwrap();
    let mut store = load(&path).store;
    let newer = br#"{"schema":2,"maya_compatibility_version":2015}"#;
    fs::write(&path, newer).unwrap();
    assert_eq!(
        store.save(&save::v(2020)),
        Err(PreferencesError::ChangedOnDisk)
    );
    assert_eq!(fs::read(&path).unwrap(), newer);
    let (error, seen) = blocked(&store);
    assert_eq!(*error, PreferencesError::ChangedOnDisk);
    assert_eq!(seen, Some(&newer[..]));
    assert_eq!(
        store.save(&save::v(2020)),
        Err(PreferencesError::NotWritable)
    );
}

#[test]
fn a_file_created_after_a_missing_load_is_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let mut store = load(&path).store;
    let created = br#"{"schema":1,"maya_compatibility_version":2012}"#;
    fs::write(&path, created).unwrap();
    assert_eq!(
        store.save(&save::v(2020)),
        Err(PreferencesError::ChangedOnDisk)
    );
    assert_eq!(fs::read(&path).unwrap(), created);
    assert!(matches!(store, Store::Blocked { .. }));
}

#[test]
fn a_file_removed_after_load_is_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::write(&path, br#"{"schema":1,"maya_compatibility_version":2015}"#).unwrap();
    let mut store = load(&path).store;
    fs::remove_file(&path).unwrap();
    assert_eq!(
        store.save(&save::v(2020)),
        Err(PreferencesError::ChangedOnDisk)
    );
    assert!(!path.exists());
}

#[test]
fn saving_into_a_blocked_or_session_only_store_is_a_typed_error() {
    let mut s = Store::SessionOnly;
    assert_eq!(s.save(&save::v(2020)), Err(PreferencesError::NotWritable));
}

mod reset {
    use super::*;

    pub const BAD: &[u8] = b"{not json";

    pub fn blocked_store(dir: &Path) -> (std::path::PathBuf, Store) {
        let path = dir.join("preferences.json");
        fs::write(&path, BAD).unwrap();
        let store = load(&path).store;
        assert!(matches!(store, Store::Blocked { .. }));
        (path, store)
    }
}

#[test]
fn reset_moves_the_bad_file_to_the_first_free_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut store) = reset::blocked_store(dir.path());
    let outcome = store.reset().unwrap();
    assert_eq!(outcome, ResetOutcome::MovedAside { backup_index: 1 });
    assert!(!path.exists());
    assert_eq!(
        fs::read(dir.path().join("preferences.json.bad-1")).unwrap(),
        reset::BAD
    );
    assert!(matches!(
        &store,
        Store::Writable {
            baseline: Baseline::Missing,
            ..
        }
    ));
    store.save(&save::v(2015)).unwrap();
    assert_eq!(year(&load(&path).preferences), 2015);
}

#[test]
fn reset_never_clobbers_an_existing_backup() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("preferences.json.bad-1"), b"older").unwrap();
    let (_, mut store) = reset::blocked_store(dir.path());
    assert_eq!(
        store.reset().unwrap(),
        ResetOutcome::MovedAside { backup_index: 2 }
    );
    assert_eq!(
        fs::read(dir.path().join("preferences.json.bad-1")).unwrap(),
        b"older"
    );
    assert_eq!(
        fs::read(dir.path().join("preferences.json.bad-2")).unwrap(),
        reset::BAD
    );
}

#[test]
fn reset_refuses_a_file_edited_since_it_was_seen() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut store) = reset::blocked_store(dir.path());
    let before = store.clone();
    fs::write(&path, b"{edited").unwrap();
    assert_eq!(store.reset(), Err(PreferencesError::ResetChanged));
    assert_eq!(fs::read(&path).unwrap(), b"{edited");
    assert_eq!(save::leftovers(dir.path()), ["preferences.json"]);
    assert_eq!(store, before);
    fs::remove_file(&path).unwrap();
    assert_eq!(store.reset(), Err(PreferencesError::ResetChanged));
}

#[test]
fn store_debug_never_names_the_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let text = dir.path().to_string_lossy().into_owned();
    let (_, blocked) = reset::blocked_store(dir.path());
    let missing = load(&dir.path().join("other.json"));
    for shown in [format!("{blocked:?}"), format!("{missing:?}")] {
        assert!(!shown.contains(&text), "{shown}");
    }
}

#[test]
fn a_failing_remove_leaves_the_original_and_older_backups_and_drops_the_new_link() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("preferences.json.bad-1"), b"older").unwrap();
    let (path, mut store) = reset::blocked_store(dir.path());
    let before = store.clone();
    let fs_ = save::FailingFs {
        fail_remove: true,
        ..Default::default()
    };
    assert_eq!(
        store.reset_with(&fs_),
        Err(PreferencesError::ResetRemove(
            std::io::ErrorKind::PermissionDenied
        ))
    );
    assert_eq!(fs::read(&path).unwrap(), reset::BAD);
    assert_eq!(
        fs::read(dir.path().join("preferences.json.bad-1")).unwrap(),
        b"older"
    );
    assert_eq!(
        save::leftovers(dir.path()),
        ["preferences.json", "preferences.json.bad-1"]
    );
    assert_eq!(store, before);
}

#[test]
fn a_failing_link_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (path, mut store) = reset::blocked_store(dir.path());
    let before = store.clone();
    let fs_ = save::FailingFs {
        fail_link: true,
        ..Default::default()
    };
    assert_eq!(
        store.reset_with(&fs_),
        Err(PreferencesError::ResetBackup(
            std::io::ErrorKind::PermissionDenied
        ))
    );
    assert_eq!(fs::read(&path).unwrap(), reset::BAD);
    assert_eq!(save::leftovers(dir.path()), ["preferences.json"]);
    assert_eq!(store, before);
}

#[cfg(unix)]
#[test]
fn reset_refuses_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.json");
    fs::write(&target, reset::BAD).unwrap();
    let path = dir.path().join("preferences.json");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let mut store = load(&path).store;
    assert_eq!(store.reset(), Err(PreferencesError::ResetSymlink));
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        save::leftovers(dir.path()),
        ["preferences.json", "target.json"]
    );
}

#[test]
fn reset_of_a_store_that_is_not_blocked_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    fs::write(&path, br#"{"schema":1,"maya_compatibility_version":2015}"#).unwrap();
    let mut store = load(&path).store;
    let before = store.clone();
    assert_eq!(store.reset(), Ok(ResetOutcome::Unchanged));
    assert_eq!(store, before);
    assert_eq!(Store::SessionOnly.reset(), Ok(ResetOutcome::Unchanged));
    assert_eq!(save::leftovers(dir.path()), ["preferences.json"]);
}

#[test]
fn the_reset_after_a_changed_on_disk_block_moves_the_newer_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("preferences.json");
    let mut store = load(&path).store;
    let newer = br#"{"schema":2}"#;
    fs::write(&path, newer).unwrap();
    assert_eq!(
        store.save(&save::v(2020)),
        Err(PreferencesError::ChangedOnDisk)
    );
    assert_eq!(
        store.reset().unwrap(),
        ResetOutcome::MovedAside { backup_index: 1 }
    );
    assert_eq!(
        fs::read(dir.path().join("preferences.json.bad-1")).unwrap(),
        newer
    );
}
