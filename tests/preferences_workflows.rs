use azteccraft::compatibility::MayaVersion;
use azteccraft::preferences::{
    MAX_FILE_BYTES, Os, Preferences, PreferencesError, Store, load, platform_path,
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
            RealFs.hard_link(from, to)
        }
        fn remove(&self, path: &Path) -> io::Result<()> {
            if self.fail_remove {
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
