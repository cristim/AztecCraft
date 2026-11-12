//! Crash-recovery files, kept apart from the user's scene files.
//!
//! A [`RecoveryStore`] owns one directory. It never receives a scene path, so a recovery write
//! cannot target the user's file, and every file it creates ends in `.recovery`, so the data is
//! inert until a user action loads it. Writes are staged and installed without replacing an
//! occupied destination (`craft_atomic::write_new`), and a failed write leaves earlier recovery
//! files as they were. The payload is opaque bytes: the caller serializes the scene.
//!
//! Files carry the writing session's pid, and retention only prunes files this store instance wrote, so a
//! live session never deletes what a crashed one left. Known limits: on volumes without hard links
//! the install falls back to a copy that a power loss can leave truncated, so callers must treat a
//! payload as untrusted until it parses; the store directory is checked, not pinned by file
//! descriptor, so a concurrent swap of the directory itself is not defended against; orphaned
//! `.craft_tmp` staging files from a crash are not swept yet.
//!
//! A marker can be unlinked by a concurrent `clear_crashed_session` between a restarting session's
//! open and lock; that session then runs unmarked and a later crash of it goes undetected, though
//! its recovery files are still listed.
//!
//! Every default here is PROVISIONAL: the Maya autosave defaults are not verified against the
//! pinned reference (#5), and nothing in this module claims parity.

mod session;
pub use session::SessionGuard;

use craft_atomic::{AtomicFs, NewFilePermissions, RealFs, WriteOptions};
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt, fs,
    fs::File,
    io,
    path::{Path, PathBuf},
};

/// PROVISIONAL default for the number of recovery files kept per source.
pub const DEFAULT_MAX_BACKUPS: usize = 10;
/// PROVISIONAL upper bound for one recovery payload.
pub const MAX_RECOVERY_BYTES: usize = 512 * 1024 * 1024;
const MAX_STEM_CHARS: usize = 64;
const MAX_SEQUENCE: u32 = 999_999;
const WRITE_ATTEMPTS: u32 = 8;
const INFIX: &str = ".autosave";
const SUFFIX: &str = ".recovery";

#[derive(Debug)]
pub enum RecoveryError {
    InvalidRetention,
    InvalidPid,
    EmptyPayload,
    TooLarge,
    /// The store directory exists but is a symlink or not a directory.
    DirectoryNotUsable,
    SequenceExhausted,
    /// Every attempt found its destination taken by a concurrent writer.
    Contended,
    /// The entry does not name a regular recovery file of this store.
    NotInStore,
    Io {
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for RecoveryError {
    // Deliberately omits `source`: its message can embed absolute local paths.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRetention => f.write_str("recovery retention must be at least 1"),
            Self::InvalidPid => f.write_str("session pid must be a positive i32"),
            Self::EmptyPayload => f.write_str("recovery payload is empty"),
            Self::TooLarge => f.write_str("recovery payload exceeds the size limit"),
            Self::DirectoryNotUsable => f.write_str("recovery directory is not a plain directory"),
            Self::SequenceExhausted => f.write_str("recovery sequence numbers are exhausted"),
            Self::Contended => f.write_str("recovery destination kept being taken"),
            Self::NotInStore => f.write_str("not a recovery file of this store"),
            Self::Io { operation, source } => {
                write!(f, "recovery {operation} failed ({:?})", source.kind())
            }
        }
    }
}

impl std::error::Error for RecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn io_error(operation: &'static str) -> impl FnOnce(io::Error) -> RecoveryError {
    move |source| RecoveryError::Io { operation, source }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryEntry {
    pub stem: String,
    pub pid: u32,
    pub sequence: u32,
    pub file_name: String,
    pub len: u64,
}

#[derive(Debug)]
pub struct WriteOutcome {
    pub entry: RecoveryEntry,
    pub pruned: usize,
    pub prune_failures: usize,
}

/// Reduce a source name to `[A-Za-z0-9_-]`, at most 64 chars, `untitled` when nothing is left.
/// Distinct names can collapse to one stem; they then share a sequence and retention.
pub fn sanitize_stem(name: &str) -> String {
    let stem: String = name
        .chars()
        .take(MAX_STEM_CHARS)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if stem.is_empty() {
        "untitled".into()
    } else {
        stem
    }
}

fn file_name(stem: &str, pid: u32, sequence: u32) -> String {
    format!("{stem}{INFIX}.p{pid}.{sequence:06}{SUFFIX}")
}

fn canonical_pid(text: &str) -> Option<u32> {
    let pid: u32 = text.parse().ok()?;
    (pid > 0 && i32::try_from(pid).is_ok() && pid.to_string() == text).then_some(pid)
}

fn parse_name(name: &str) -> Option<(&str, u32, u32)> {
    let body = name.strip_suffix(SUFFIX)?;
    let (rest, digits) = body.rsplit_once('.')?;
    let (rest, pid) = rest.rsplit_once('.')?;
    let stem = rest.strip_suffix(INFIX)?;
    let valid_stem = !stem.is_empty()
        && stem.chars().count() <= MAX_STEM_CHARS
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid_stem || digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((
        stem,
        canonical_pid(pid.strip_prefix('p')?)?,
        digits.parse().ok()?,
    ))
}

#[derive(Debug, Clone)]
pub struct RecoveryStore {
    dir: PathBuf,
    max_backups: usize,
    pid: u32,
    /// First sequence this store wrote per stem; older same-pid files belong to an earlier
    /// process that reused the pid and are never pruned.
    floors: RefCell<HashMap<String, u32>>,
}

impl RecoveryStore {
    /// `pid` names the writing session in every file this store creates.
    pub fn new(
        dir: impl Into<PathBuf>,
        max_backups: usize,
        pid: u32,
    ) -> Result<Self, RecoveryError> {
        if max_backups == 0 {
            return Err(RecoveryError::InvalidRetention);
        }
        if canonical_pid(&pid.to_string()).is_none() {
            return Err(RecoveryError::InvalidPid);
        }
        Ok(Self {
            dir: dir.into(),
            max_backups,
            pid,
            floors: RefCell::default(),
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Write one recovery file for `stem` (see [`sanitize_stem`]) and prune older ones.
    pub fn write(&self, stem: &str, bytes: &[u8]) -> Result<WriteOutcome, RecoveryError> {
        self.write_with(&RealFs, stem, bytes)
    }

    /// [`Self::write`] through an injected filesystem, for failure tests.
    pub fn write_with(
        &self,
        fs: &dyn AtomicFs,
        stem: &str,
        bytes: &[u8],
    ) -> Result<WriteOutcome, RecoveryError> {
        if bytes.is_empty() {
            return Err(RecoveryError::EmptyPayload);
        }
        if bytes.len() > MAX_RECOVERY_BYTES {
            return Err(RecoveryError::TooLarge);
        }
        let stem = sanitize_stem(stem);
        self.ensure_dir()?;
        let options =
            WriteOptions::default().with_new_file_permissions(NewFilePermissions::Private);
        for _ in 0..WRITE_ATTEMPTS {
            let sequence = self.next_sequence(&stem)?;
            let name = file_name(&stem, self.pid, sequence);
            if fs::symlink_metadata(self.dir.join(&name)).is_ok() {
                continue;
            }
            match options.write_new_custom(fs, &self.dir.join(&name), bytes) {
                Ok(()) => {
                    self.floors
                        .borrow_mut()
                        .entry(stem.clone())
                        .or_insert(sequence);
                    let entry = RecoveryEntry {
                        stem,
                        pid: self.pid,
                        sequence,
                        file_name: name,
                        len: bytes.len() as u64,
                    };
                    let (pruned, prune_failures) = self.prune(fs, &entry);
                    return Ok(WriteOutcome {
                        entry,
                        pruned,
                        prune_failures,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => return Err(io_error("write")(source)),
            }
        }
        Err(RecoveryError::Contended)
    }

    /// Regular recovery files, sorted by stem then sequence. A missing directory is empty.
    pub fn list(&self) -> Result<Vec<RecoveryEntry>, RecoveryError> {
        let mut entries = Vec::new();
        for item in self.read_dir()? {
            let item = item.map_err(io_error("list"))?;
            let name = item.file_name();
            let Some((stem, pid, sequence)) = name.to_str().and_then(parse_name) else {
                continue;
            };
            let Ok(meta) = fs::symlink_metadata(item.path()) else {
                continue;
            };
            if meta.is_file() {
                entries.push(RecoveryEntry {
                    stem: stem.into(),
                    pid,
                    sequence,
                    file_name: name.to_string_lossy().into_owned(),
                    len: meta.len(),
                });
            }
        }
        entries.sort_by(|a, b| (&a.stem, a.pid, a.sequence).cmp(&(&b.stem, b.pid, b.sequence)));
        Ok(entries)
    }

    /// Read one recovery payload. Forged entries and non-regular files are refused.
    pub fn read(&self, entry: &RecoveryEntry) -> Result<Vec<u8>, RecoveryError> {
        let named = parse_name(&entry.file_name);
        if named != Some((entry.stem.as_str(), entry.pid, entry.sequence)) {
            return Err(RecoveryError::NotInStore);
        }
        let path = self.dir.join(&entry.file_name);
        let meta = fs::symlink_metadata(&path).map_err(io_error("read"))?;
        if !meta.is_file() {
            return Err(RecoveryError::NotInStore);
        }
        if meta.len() > MAX_RECOVERY_BYTES as u64 {
            return Err(RecoveryError::TooLarge);
        }
        fs::read(&path).map_err(io_error("read"))
    }

    /// Remove one recovery file, after recovery or when the user discards it.
    pub fn remove(&self, entry: &RecoveryEntry) -> Result<(), RecoveryError> {
        if parse_name(&entry.file_name) != Some((entry.stem.as_str(), entry.pid, entry.sequence)) {
            return Err(RecoveryError::NotInStore);
        }
        remove_regular(&self.dir.join(&entry.file_name))
    }

    fn read_dir(&self) -> Result<Vec<io::Result<fs::DirEntry>>, RecoveryError> {
        match fs::symlink_metadata(&self.dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(io_error("inspect directory")(source)),
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(RecoveryError::DirectoryNotUsable),
        }
        Ok(fs::read_dir(&self.dir)
            .map_err(io_error("read directory"))?
            .collect())
    }

    fn ensure_dir(&self) -> Result<(), RecoveryError> {
        match fs::symlink_metadata(&self.dir) {
            Ok(meta) if meta.is_dir() => Ok(()),
            Ok(_) => Err(RecoveryError::DirectoryNotUsable),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut builder = fs::DirBuilder::new();
                builder.recursive(true);
                #[cfg(unix)]
                std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
                builder
                    .create(&self.dir)
                    .map_err(io_error("create directory"))?;
                Ok(())
            }
            Err(source) => Err(io_error("inspect directory")(source)),
        }
    }

    /// One past the highest sequence named for `stem`, whatever kind of entry holds the name.
    fn next_sequence(&self, stem: &str) -> Result<u32, RecoveryError> {
        let mut highest = 0;
        for item in self.read_dir()? {
            let item = item.map_err(io_error("scan sequence"))?;
            if let Some((found, pid, sequence)) = item.file_name().to_str().and_then(parse_name)
                && found == stem
                && pid == self.pid
            {
                highest = highest.max(sequence);
            }
        }
        match highest.checked_add(1) {
            Some(next) if next <= MAX_SEQUENCE => Ok(next),
            _ => Err(RecoveryError::SequenceExhausted),
        }
    }

    /// Remove the oldest regular recovery files of `kept`'s stem beyond the retention limit.
    /// Best effort: a failure is counted, never raised, and `kept` is never removed.
    fn prune(&self, fs: &dyn AtomicFs, kept: &RecoveryEntry) -> (usize, usize) {
        let Ok(entries) = self.list() else {
            return (0, 1);
        };
        let same: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.stem == kept.stem
                    && e.pid == kept.pid
                    && self
                        .floors
                        .borrow()
                        .get(&e.stem)
                        .is_some_and(|floor| e.sequence >= *floor)
            })
            .collect();
        let excess = same.len().saturating_sub(self.max_backups);
        let (mut pruned, mut failures) = (0, 0);
        for old in same
            .iter()
            .filter(|e| e.sequence != kept.sequence)
            .take(excess)
        {
            match fs.remove(&self.dir.join(&old.file_name)) {
                Ok(()) => pruned += 1,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => failures += 1,
            }
        }
        (pruned, failures)
    }
}

fn remove_regular(path: &Path) -> Result<(), RecoveryError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => fs::remove_file(path).map_err(io_error("remove")),
        Ok(_) => Err(RecoveryError::NotInStore),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(io_error("remove")(source)),
    }
}
