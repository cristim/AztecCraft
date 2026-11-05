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
