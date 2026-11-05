//! The persisted application preferences file.
//!
//! The location, JSON format and backup naming are AztecCraft's own (PROVISIONAL: Maya keeps a
//! per-version folder of MEL files). A file this build cannot read is never overwritten: the
//! store becomes `Blocked` and the session runs on defaults until the user resets it.
//! Errors carry an `io::ErrorKind` at most, never a path.

use crate::compatibility::MayaVersion;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const MAX_FILE_BYTES: u64 = 64 * 1024;
const SCHEMA: u64 = 1;
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
    /// The file differs from what this store loaded or last wrote.
    ChangedOnDisk,
    Write(io::ErrorKind),
    /// Only a `Writable` store can save.
    NotWritable,
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
            Self::ChangedOnDisk => "the preferences file changed on disk since it was read",
            Self::Write(_) => "the preferences file could not be written",
            Self::NotWritable => "the preferences file is not writable in this session",
        })
    }
}

impl std::error::Error for PreferencesError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Baseline {
    Missing,
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Store {
    Writable {
        path: PathBuf,
        baseline: Baseline,
    },
    /// `seen` is the bytes read (at most the cap plus one), or `None` when none could be read.
    Blocked {
        path: PathBuf,
        error: PreferencesError,
        seen: Option<Vec<u8>>,
    },
    SessionOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub preferences: Preferences,
    pub store: Store,
}

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

enum Observed {
    Missing,
    Symlink,
    NotRegular,
    Unreadable(io::ErrorKind),
    Bytes(Vec<u8>),
}

fn observe(path: &Path) -> Observed {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Observed::Missing,
        Err(e) => return Observed::Unreadable(e.kind()),
    };
    if meta.file_type().is_symlink() {
        return Observed::Symlink;
    }
    if !meta.is_file() {
        return Observed::NotRegular;
    }
    let mut bytes = Vec::new();
    let read =
        std::fs::File::open(path).and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes));
    match read {
        Ok(_) => Observed::Bytes(bytes),
        Err(e) => Observed::Unreadable(e.kind()),
    }
}

fn parse(bytes: &[u8]) -> Result<Preferences, PreferencesError> {
    if u64::try_from(bytes.len()).map_or(true, |n| n > MAX_FILE_BYTES) {
        return Err(PreferencesError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| PreferencesError::Malformed)?;
    match value.get("schema").and_then(serde_json::Value::as_u64) {
        Some(SCHEMA) => {}
        Some(_) => return Err(PreferencesError::UnknownSchema),
        None => return Err(PreferencesError::Malformed),
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Schema1 {
        #[allow(dead_code)]
        schema: u64,
        maya_compatibility_version: serde_json::Value,
    }
    let parsed: Schema1 = serde_json::from_value(value).map_err(|_| PreferencesError::Malformed)?;
    let version = parsed
        .maya_compatibility_version
        .as_u64()
        .and_then(|y| MayaVersion::try_from(y).ok())
        .ok_or(PreferencesError::VersionOutOfRange)?;
    Ok(Preferences {
        maya_compatibility_version: version,
    })
}

/// Read the preferences at `path`. Never writes and never follows a symlink.
pub fn load(path: &Path) -> Loaded {
    let blocked = |error, seen| Loaded {
        preferences: Preferences::default(),
        store: Store::Blocked {
            path: path.to_path_buf(),
            error,
            seen,
        },
    };
    match observe(path) {
        Observed::Missing => Loaded {
            preferences: Preferences::default(),
            store: Store::Writable {
                path: path.to_path_buf(),
                baseline: Baseline::Missing,
            },
        },
        Observed::Symlink => blocked(PreferencesError::Symlink, None),
        Observed::NotRegular => blocked(PreferencesError::NotRegularFile, None),
        Observed::Unreadable(kind) => blocked(PreferencesError::Unreadable(kind), None),
        Observed::Bytes(bytes) => match parse(&bytes) {
            Ok(preferences) => Loaded {
                preferences,
                store: Store::Writable {
                    path: path.to_path_buf(),
                    baseline: Baseline::Bytes(bytes),
                },
            },
            Err(error) => blocked(error, Some(bytes)),
        },
    }
}

/// Reduce an `atomic_write` failure to its kind; its message names the path.
fn write_error(e: &io::Error) -> PreferencesError {
    PreferencesError::Write(e.kind())
}

impl Preferences {
    fn to_json(&self) -> Vec<u8> {
        format!(
            "{{\"schema\":{SCHEMA},\"maya_compatibility_version\":{}}}\n",
            self.maya_compatibility_version.year()
        )
        .into_bytes()
    }
}

impl Store {
    /// Persist `preferences`. Re-reads the file first: if it no longer matches what this store
    /// loaded or last wrote (another instance, possibly a newer schema), nothing is written and
    /// the store becomes `Blocked` so Reset can move the file aside. A failed write leaves the
    /// file and the store unchanged. A change between the re-read and the rename is not caught.
    pub fn save_with(
        &mut self,
        preferences: &Preferences,
        fs: &dyn craft_atomic::AtomicFs,
    ) -> Result<(), PreferencesError> {
        let Store::Writable { path, baseline } = self else {
            return Err(PreferencesError::NotWritable);
        };
        let seen = match observe(path) {
            Observed::Missing if *baseline == Baseline::Missing => None,
            Observed::Bytes(b) if *baseline == Baseline::Bytes(b.clone()) => None,
            Observed::Bytes(b) => Some(Some(b)),
            Observed::Symlink
            | Observed::NotRegular
            | Observed::Unreadable(_)
            | Observed::Missing => Some(None),
        };
        if let Some(seen) = seen {
            *self = Store::Blocked {
                path: std::mem::take(path),
                error: PreferencesError::ChangedOnDisk,
                seen,
            };
            return Err(PreferencesError::ChangedOnDisk);
        }
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| write_error(&e))?;
        }
        let bytes = preferences.to_json();
        craft_atomic::WriteOptions::default()
            .with_new_file_permissions(craft_atomic::NewFilePermissions::Private)
            .atomic_write_custom(fs, craft_atomic::RenameRetry::platform(), path, &bytes)
            .map_err(|e| write_error(&e))?;
        *baseline = Baseline::Bytes(bytes);
        Ok(())
    }

    pub fn save(&mut self, preferences: &Preferences) -> Result<(), PreferencesError> {
        self.save_with(preferences, &craft_atomic::RealFs)
    }
}
