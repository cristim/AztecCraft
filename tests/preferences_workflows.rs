use azteccraft::preferences::{Os, PreferencesError, platform_path};
use std::ffi::OsString;
use std::path::Path;

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
