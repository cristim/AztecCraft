//! Crash-recovery files, kept apart from the user's scene files.
//!
//! A [`RecoveryStore`] owns one directory. It never receives a scene path, so a recovery write
//! cannot target the user's file, and every file it creates ends in `.recovery`, so the data is
//! inert until a user action loads it. Writes are staged and installed without replacing an
//! occupied destination (`craft_atomic::write_new`), and a failed write leaves earlier recovery
//! files as they were. The payload is opaque bytes: the caller serializes the scene.
//!
//! Files carry the writing session's pid. Retention prunes only the exact files this store instance
//! created, so neither a crashed earlier session nor another live writer sharing the pid (pid
//! namespaces, two stores in one process, a synced directory) loses a file to it. A file this
//! store wrote and that was then removed through another store can have its name reused by a
//! third writer; `remove` through this store forgets the name, other stores cannot. Stems are
//! matched case-insensitively for sequencing, since volumes such as macOS APFS fold case.
//!
//! Known limits:
//! - On volumes without hard links the install falls back to a copy straight into the final
//!   name. A process crash or power loss mid-copy can leave a truncated newest file, so treat a
//!   payload as untrusted until it parses.
//! - The store directory is checked, not pinned by file descriptor; a concurrent swap of the
//!   directory itself is not defended against.
//! - Orphaned `.craft_tmp` staging files from a crash are not swept yet.
//! - Crash markers use advisory file locks, which are only reliable on a local disk. On NFS,
//!   other network mounts and synced folders (Dropbox, iCloud) a live session can look crashed
//!   to another process or host, or locks can be lost. Callers must put the directory on local
//!   storage; `clear_crashed_session` only removes a marker whose lock it can take, and never
//!   touches recovery files, so a false crash report costs a prompt, not data. This process
//!   never probes its own marker through a second descriptor (per store instance: a second
//!   store with the same pid in one process still does, which is correct with per-descriptor
//!   `flock` but not with POSIX-style NFS emulation).
//! - A marker can be unlinked by a concurrent `clear_crashed_session` between a restarting
//!   session's open and lock; that session then runs unmarked and a later crash of it goes
//!   undetected, though its recovery files are still listed.
//!
//! Every default here is PROVISIONAL: the Maya autosave defaults are not verified against the
//! pinned reference (#5), and nothing in this module claims parity.

mod session;
pub use session::SessionGuard;

use craft_atomic::{AtomicFs, NewFilePermissions, RealFs, WriteOptions};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    fmt, fs,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    rc::Rc,
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

// Neither Debug nor `source()` exposes the inner `io::Error`: craft-atomic puts the absolute
// destination path (which embeds the scene stem) in its message, and Debug, `{:#}` and error
// chains would otherwise carry it into logs. Only the operation and `ErrorKind` survive.
impl fmt::Debug for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => f
                .debug_struct("Io")
                .field("operation", operation)
                .field("kind", &source.kind())
                .finish(),
            Self::InvalidRetention => f.write_str("InvalidRetention"),
            Self::InvalidPid => f.write_str("InvalidPid"),
            Self::EmptyPayload => f.write_str("EmptyPayload"),
            Self::TooLarge => f.write_str("TooLarge"),
            Self::DirectoryNotUsable => f.write_str("DirectoryNotUsable"),
            Self::SequenceExhausted => f.write_str("SequenceExhausted"),
            Self::Contended => f.write_str("Contended"),
            Self::NotInStore => f.write_str("NotInStore"),
        }
    }
}

impl std::error::Error for RecoveryError {}

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

pub struct RecoveryStore {
    dir: PathBuf,
    max_backups: usize,
    pid: u32,
    /// Exact files this instance created, by case-folded stem. Retention prunes only these, so
    /// no other writer, however its pid and stem collide with ours, loses a file to us.
    written: RefCell<HashMap<String, BTreeMap<u32, String>>>,
    /// A session guard of this store is alive: its marker is never probed from this process.
    active: Rc<Cell<bool>>,
    /// `begin_session` took over an unheld marker for this pid; reported until cleared.
    stale_takeover: Cell<bool>,
}

impl fmt::Debug for RecoveryStore {
    // Omits the directory and stems: they are absolute paths and scene names.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecoveryStore")
            .field("max_backups", &self.max_backups)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
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
            written: RefCell::default(),
            active: Rc::default(),
            stale_takeover: Cell::default(),
        })
    }

    pub fn max_backups(&self) -> usize {
        self.max_backups
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
                    self.written
                        .borrow_mut()
                        .entry(stem.to_ascii_lowercase())
                        .or_default()
                        .insert(sequence, name.clone());
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
        // Bounded by the read itself, so a file that grows after the check stays under the cap.
        let mut bytes = Vec::new();
        File::open(&path)
            .and_then(|file| {
                file.take(MAX_RECOVERY_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
            })
            .map_err(io_error("read"))?;
        if bytes.len() > MAX_RECOVERY_BYTES {
            return Err(RecoveryError::TooLarge);
        }
        Ok(bytes)
    }

    /// Remove one recovery file, after recovery or when the user discards it.
    pub fn remove(&self, entry: &RecoveryEntry) -> Result<(), RecoveryError> {
        if parse_name(&entry.file_name) != Some((entry.stem.as_str(), entry.pid, entry.sequence)) {
            return Err(RecoveryError::NotInStore);
        }
        remove_regular(&self.dir.join(&entry.file_name))?;
        if let Some(files) = self
            .written
            .borrow_mut()
            .get_mut(&entry.stem.to_ascii_lowercase())
            && entry.pid == self.pid
        {
            files.remove(&entry.sequence);
        }
        Ok(())
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
                && found.eq_ignore_ascii_case(stem)
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

    /// Remove the oldest files this instance wrote beyond the retention limit.
    /// Best effort: a failure is counted, never raised, and `kept` is never removed.
    fn prune(&self, fs: &dyn AtomicFs, kept: &RecoveryEntry) -> (usize, usize) {
        let mut written = self.written.borrow_mut();
        let files = written.entry(kept.stem.to_ascii_lowercase()).or_default();
        let excess = files.len().saturating_sub(self.max_backups);
        let victims: Vec<(u32, String)> = files
            .iter()
            .filter(|(sequence, _)| **sequence != kept.sequence)
            .take(excess)
            .map(|(sequence, name)| (*sequence, name.clone()))
            .collect();
        let (mut pruned, mut failures) = (0, 0);
        for (sequence, name) in victims {
            match fs.remove(&self.dir.join(name)) {
                Ok(()) => {
                    pruned += 1;
                    files.remove(&sequence);
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    files.remove(&sequence);
                }
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

#[cfg(test)]
mod tests;
