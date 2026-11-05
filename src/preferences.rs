//! The persisted application preferences file.
//!
//! The location, JSON format and backup naming are AztecCraft's own (PROVISIONAL: Maya keeps a
//! per-version folder of MEL files). A file this build cannot read is never overwritten: the
//! store becomes `Blocked` and the session runs on defaults until the user resets it.
//! Errors carry an `io::ErrorKind` at most, never a path.

use crate::compatibility::MayaVersion;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "preferences.json";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Preferences {
    pub maya_compatibility_version: MayaVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferencesError {
    NoConfigDir,
    Symlink,
    NotRegularFile,
    TooLarge,
    Unreadable(io::ErrorKind),
    Malformed,
    UnknownSchema,
    VersionOutOfRange,
}

impl fmt::Display for PreferencesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NoConfigDir => "no usable configuration folder",
            Self::Symlink => "the preferences file is a symbolic link",
            Self::NotRegularFile => "the preferences file is not a regular file",
            Self::TooLarge => "the preferences file is too large",
            Self::Unreadable(_) => "the preferences file could not be read",
            Self::Malformed => "the preferences file is not valid",
            Self::UnknownSchema => "the preferences file has an unknown schema",
            Self::VersionOutOfRange => "the preferences file names an unsupported Maya version",
        })
    }
}

impl std::error::Error for PreferencesError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Macos,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Linux
        }
    }
}

fn absolute_root(os: Os, value: Option<OsString>) -> Option<PathBuf> {
    let value = value.filter(|v| !v.is_empty())?;
    let absolute = match (os, value.to_str()) {
        (Os::Windows, Some(s)) => {
            let b = s.as_bytes();
            s.starts_with("\\\\")
                || (b.len() >= 3
                    && b[0].is_ascii_alphabetic()
                    && b[1] == b':'
                    && matches!(b[2], b'\\' | b'/'))
        }
        _ => Path::new(&value).is_absolute(),
    };
    absolute.then(|| PathBuf::from(value))
}

pub fn platform_path(
    env: &dyn Fn(&str) -> Option<OsString>,
    os: Os,
) -> Result<PathBuf, PreferencesError> {
    let root = |name: &str| absolute_root(os, env(name));
    let path = match os {
        Os::Macos => root("HOME").map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("AztecCraft")
        }),
        Os::Linux => root("XDG_CONFIG_HOME")
            .or_else(|| root("HOME").map(|h| h.join(".config")))
            .map(|c| c.join("azteccraft")),
        Os::Windows => root("APPDATA").map(|a| a.join("AztecCraft")),
    };
    path.map(|dir| dir.join(FILE_NAME))
        .ok_or(PreferencesError::NoConfigDir)
}
