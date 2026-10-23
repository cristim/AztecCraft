use super::{NativeSaveError as Error, SaveDestination, SaveOutcome, SaveReceipt};
use crate::{
    document::{LinkStatus, MAX_DOCUMENT_BYTES, NativeOrigin, SourceBaseline},
    native_scene::{NativeScene, binary},
};
use craft_atomic::{AtomicFs, NewFilePermissions, RenameRetry, WriteOptions};
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    fs::{File, Metadata, OpenOptions},
    io::{self, Write},
    os::windows::fs::{FileExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};

// Win32 values; std exposes the flags and attribute words but not their names.
const FILE_SHARE_ALL: u32 = 0x1 | 0x2 | 0x4;
/// Read and write sharing without delete, so a bound folder cannot be moved or replaced.
const FILE_SHARE_PINNED: u32 = 0x1 | 0x2;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
/// Attributes a replacement cannot carry over, so Save refuses rather than drop them silently.
const KEPT_ATTRIBUTES: u32 =
    FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM;

/// Volume serial number and file index of an open handle: the Windows counterpart of the Unix
/// device and inode pair.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    volume: u64,
    index: u64,
}
impl Identity {
    fn of(file: &File) -> io::Result<Self> {
        let information = winapi_util::file::information(file)?;
        Ok(Self {
            volume: information.volume_serial_number(),
            index: information.file_index(),
        })
    }
}
fn io_error(error: impl Into<io::Error>) -> Error {
    let error = error.into();
    if locked(&error) {
        Error::Io(io::ErrorKind::ResourceBusy)
    } else {
        Error::Io(error.kind())
    }
}
fn changed() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidData)
}
/// Name-surrogate reparse points (symbolic links and junctions). Other reparse points, such as
/// OneDrive placeholders or deduplicated files, are ordinary files and folders here.
fn link(metadata: &Metadata) -> bool {
    metadata.file_type().is_symlink()
}
/// Another process holds the file without sharing it: a lock, not a change.
fn locked(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
    )
}
fn changed_or_locked(error: io::Error) -> Error {
    if locked(&error) {
        Error::Io(io::ErrorKind::ResourceBusy)
    } else {
        Error::SourceChanged
    }
}
/// Win32 file names cannot contain stream separators or wildcards, end with a dot or space, or
/// name a device (`CON`, `NUL`, `COM1`, ...); verbatim `\\?\` paths would otherwise create
/// alternate data streams or device-named files instead of refusing.
fn valid_leaf(leaf: &OsStr) -> bool {
    let Some(name) = leaf.to_str() else {
        return false;
    };
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c < ' ' || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
    {
        return false;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_ascii_uppercase();
    let device = matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || (stem.len() == 4
        && (stem.starts_with("COM") || stem.starts_with("LPT"))
        && stem.as_bytes()[3].is_ascii_digit()
        && stem.as_bytes()[3] != b'0');
    !device
}
fn kept_attributes(file: &File) -> io::Result<u32> {
    Ok(file.metadata()?.file_attributes() & KEPT_ATTRIBUTES)
}
/// Opens a directory, following links unless `exact` asks for the entry itself.
/// The `exact` handle is the one a destination holds: it does not share delete, so the folder
/// cannot be moved or replaced while bound.
fn open_directory(path: &Path, exact: bool) -> io::Result<File> {
    let (flags, share) = if exact {
        (
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_SHARE_PINNED,
        )
    } else {
        (FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_ALL)
    };
    let file = OpenOptions::new()
        .read(true)
        .share_mode(share)
        .custom_flags(flags)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_dir() || (exact && link(&metadata)) {
        return Err(changed());
    }
    Ok(file)
}

pub(in crate::document) struct Destination {
    requested_parent: PathBuf,
    canonical_parent: PathBuf,
    /// Held without delete sharing: the folder cannot be moved, and its file index cannot be
    /// reused by another folder, while bound.
    _parent: Arc<File>,
    identity: Identity,
    leaf: OsString,
}

impl Destination {
    pub(in crate::document) fn capture(path: &Path) -> io::Result<Self> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let leaf = absolute.file_name().ok_or_else(changed)?.to_os_string();
        if !valid_leaf(&leaf) {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        let requested_parent = absolute.parent().ok_or_else(changed)?.to_path_buf();
        let canonical_parent = requested_parent.canonicalize()?;
        let parent = open_directory(&canonical_parent, true)?;
        let identity = Identity::of(&parent)?;
        let destination = Self {
            requested_parent,
            canonical_parent,
            _parent: Arc::new(parent),
            identity,
            leaf,
        };
        destination.binding()?;
        Ok(destination)
    }
    fn path(&self) -> PathBuf {
        self.canonical_parent.join(&self.leaf)
    }
    /// Windows has no directory-relative calls in std, so every leaf operation goes through
    /// the canonical path and this check that both parent paths still name the captured folder.
    fn binding(&self) -> io::Result<()> {
        for path in [&self.requested_parent, &self.canonical_parent] {
            if Identity::of(&open_directory(path, false)?)? != self.identity {
                return Err(changed());
            }
        }
        if self.requested_parent.canonicalize()? != self.canonical_parent {
            return Err(changed());
        }
        Ok(())
    }
    fn stat(&self, leaf: &OsStr) -> io::Result<Metadata> {
        std::fs::symlink_metadata(self.canonical_parent.join(leaf))
    }
    fn open(&self, leaf: &OsStr) -> io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.canonical_parent.join(leaf))?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || link(&metadata) {
            return Err(changed());
        }
        self.binding()?;
        Ok(file)
    }
    pub(in crate::document) fn matches(&self, file: &File) -> io::Result<bool> {
        self.binding()?;
        let current = self.open(&self.leaf)?;
        Ok(Identity::of(&current)? == Identity::of(file)?)
    }
    pub(in crate::document) fn verify(&self, file: &File, bytes: &[u8]) -> Result<(), Error> {
        self.binding().map_err(|_| Error::SourceChanged)?;
        let current = self.open(&self.leaf).map_err(changed_or_locked)?;
        if Identity::of(&current).map_err(io_error)? != Identity::of(file).map_err(io_error)?
            || !equal_bytes(&current, bytes).map_err(io_error)?
        {
            return Err(Error::SourceChanged);
        }
        Ok(())
    }
    fn absent(&self) -> Result<(), Error> {
        self.binding().map_err(|_| Error::SourceChanged)?;
        match self.stat(&self.leaf) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(Error::DestinationExists),
            Err(error) => Err(io_error(error)),
        }
    }
    fn stage_leaf<'a>(&self, path: &'a Path) -> io::Result<&'a OsStr> {
        if path.parent() != Some(self.canonical_parent.as_path()) {
            return Err(changed());
        }
        let leaf = path.file_name().ok_or_else(changed)?;
        if leaf == self.leaf {
            return Err(changed());
        }
        let name = leaf.to_str().ok_or_else(changed)?;
        let stem: String = self.leaf.to_string_lossy().chars().take(64).collect();
        let prefix = format!(".{stem}.{}-", std::process::id());
        let counter = name
            .strip_prefix(&prefix)
            .and_then(|name| name.strip_suffix(".craft_tmp"))
            .ok_or_else(changed)?;
        if counter.is_empty()
            || !counter.bytes().all(|b| b.is_ascii_digit())
            || counter.parse::<u64>().is_err()
        {
            return Err(changed());
        }
        Ok(leaf)
    }
}

fn equal_bytes(file: &File, expected: &[u8]) -> io::Result<bool> {
    if expected.len() > MAX_DOCUMENT_BYTES || file.metadata()?.len() != expected.len() as u64 {
        return Ok(false);
    }
    let mut scratch = [0u8; 8192];
    let mut offset = 0;
    while offset < expected.len() {
        let count = scratch.len().min(expected.len() - offset);
        let read = match file.seek_read(&mut scratch[..count], offset as u64) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 || scratch[..read] != expected[offset..offset + read] {
            return Ok(false);
        }
        offset += read;
    }
    Ok(file.metadata()?.len() == expected.len() as u64)
}

struct OwnedStage {
    leaf: OsString,
    file: Arc<File>,
    identity: Identity,
}
#[derive(Default)]
struct State {
    stage: Option<OwnedStage>,
    installed: Option<Arc<File>>,
    synced: bool,
    failure: Option<Error>,
    post_error: Option<Error>,
}
struct Transaction<'a> {
    destination: Arc<Destination>,
    baseline: Option<&'a SourceBaseline>,
    bytes: Arc<[u8]>,
    parse_bytes: Arc<[u8]>,
    may_install: &'a dyn Fn() -> bool,
    state: RefCell<State>,
}

impl Transaction<'_> {
    /// Windows files carry attribute flags instead of a mode; the replacement must carry the
    /// authorized file's (none, since `prepare` refuses the ones a new file cannot inherit).
    fn check_mode(&self, file: &File) -> io::Result<()> {
        if let Some(baseline) = self.baseline
            && kept_attributes(file)? != kept_attributes(&baseline.file)?
        {
            return Err(self.fail(Error::SourceChanged));
        }
        Ok(())
    }
    fn writable(&self, path: &Path) -> io::Result<()> {
        if path != self.destination.path() {
            return Err(self.fail(Error::InvalidDestination));
        }
        let baseline = self.baseline.ok_or_else(|| self.fail(Error::Unsupported))?;
        self.destination
            .binding()
            .map_err(|_| self.fail(Error::SourceChanged))?;
        let file = OpenOptions::new()
            .write(true)
            .share_mode(FILE_SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| self.fail(io_error(error)))?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || link(&metadata)
            || Identity::of(&file)? != Identity::of(&baseline.file)?
        {
            return Err(self.fail(Error::SourceChanged));
        }
        Ok(())
    }
    fn fail(&self, error: Error) -> io::Error {
        let mut state = self.state.borrow_mut();
        if state.installed.is_some() {
            state.post_error = Some(error);
        } else {
            state.failure = Some(error);
        }
        if error == Error::Io(io::ErrorKind::ResourceBusy) {
            // Keep the raw code so the engine's rename retry still treats it as transient.
            return io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION);
        }
        io::Error::from(match error {
            Error::Cancelled => io::ErrorKind::Interrupted,
            Error::DestinationExists => io::ErrorKind::AlreadyExists,
            Error::Io(kind) => kind,
            _ => io::ErrorKind::InvalidData,
        })
    }
    fn stage(&self, path: &Path) -> io::Result<Arc<File>> {
        self.destination
            .binding()
            .map_err(|_| self.fail(Error::SourceChanged))?;
        let state = self.state.borrow();
        let stage = state.stage.as_ref().ok_or_else(changed)?;
        if path.parent() != Some(self.destination.canonical_parent.as_path())
            || path.file_name() != Some(stage.leaf.as_os_str())
        {
            return Err(changed());
        }
        let current = self.destination.open(&stage.leaf)?;
        if Identity::of(&current)? != stage.identity {
            return Err(changed());
        }
        Ok(stage.file.clone())
    }
    fn preinstall(&self, from: &Path, to: &Path, exclusive: bool) -> io::Result<Arc<File>> {
        if to != self.destination.path() || exclusive != self.baseline.is_none() {
            return Err(self.fail(Error::InvalidDestination));
        }
        let file = self
            .stage(from)
            .map_err(|_| self.fail(Error::SourceChanged))?;
        if !equal_bytes(&file, &self.bytes)? {
            return Err(self.fail(Error::SourceChanged));
        }
        if let Some(baseline) = self.baseline {
            self.destination
                .verify(&baseline.file, &baseline.bytes)
                .map_err(|error| self.fail(error))?;
        } else {
            self.destination
                .absent()
                .map_err(|error| self.fail(error))?;
        }
        self.check_mode(&file)?;
        if !(self.may_install)() {
            return Err(self.fail(Error::Cancelled));
        }
        Ok(file)
    }
    fn install(&self, from: &Path, to: &Path, exclusive: bool) -> io::Result<()> {
        let file = self.preinstall(from, to, exclusive)?;
        let target = self.destination.path();
        // CreateHardLinkW fails when the name exists, so a new destination is never replaced;
        // std's rename replaces the authorized file in one step (POSIX semantics where the
        // volume supports them).
        let result = if exclusive {
            std::fs::hard_link(from, &target)
        } else {
            std::fs::rename(from, &target)
        };
        result.map_err(|error| {
            self.fail(if error.kind() == io::ErrorKind::AlreadyExists {
                Error::DestinationExists
            } else {
                io_error(error)
            })
        })?;
        let mut state = self.state.borrow_mut();
        state.installed = Some(file);
        state.failure = None;
        Ok(())
    }
    fn run(&self, fs: &dyn AtomicFs, retry: RenameRetry) -> Result<SaveOutcome, Error> {
        let path = self.destination.path();
        let options = WriteOptions::default();
        let fill = |file: &mut File| file.write_all(&self.bytes);
        let result = if self.baseline.is_some() {
            options.atomic_write_with_custom(fs, retry, &path, fill)
        } else {
            options.write_new_with_custom(fs, &path, fill)
        };
        let state = self.state.borrow();
        let Some(file) = state.installed.as_ref() else {
            return Err(state.failure.unwrap_or_else(|| {
                result
                    .err()
                    .map(io_error)
                    .unwrap_or(Error::Io(io::ErrorKind::Other))
            }));
        };
        let receipt = Receipt {
            destination: self.destination.clone(),
            bytes: self.bytes.clone(),
            parse_bytes: self.parse_bytes.clone(),
            file: file.clone(),
        };
        let cause = state
            .post_error
            .or_else(|| result.err().map(io_error))
            .or_else(|| (!state.synced).then_some(Error::Io(io::ErrorKind::Other)));
        drop(state);
        match cause.map_or_else(|| receipt.verified(), Err) {
            Ok((origin, path)) => Ok(SaveOutcome::Saved { origin, path }),
            Err(cause) => Ok(SaveOutcome::Uncertain {
                receipt: SaveReceipt { inner: receipt },
                cause,
            }),
        }
    }
}

impl AtomicFs for Transaction<'_> {
    fn check_writable(&self, path: &Path) -> io::Result<()> {
        self.writable(path)
    }
    fn create_new(&self, path: &Path, _: NewFilePermissions) -> io::Result<File> {
        let leaf = self.destination.stage_leaf(path).map_err(|_| {
            let error = self.state.borrow().failure.unwrap_or(Error::Unsupported);
            self.fail(error)
        })?;
        if let Some(baseline) = self.baseline {
            self.destination
                .verify(&baseline.file, &baseline.bytes)
                .map_err(|error| self.fail(error))?;
            self.writable(&self.destination.path())?;
        } else {
            self.destination
                .absent()
                .map_err(|error| self.fail(error))?;
        }
        if !(self.may_install)() {
            return Err(self.fail(Error::Cancelled));
        }
        if self.state.borrow().stage.is_some() {
            return Err(self.fail(Error::Unsupported));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_ALL)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.destination.canonical_parent.join(leaf))?;
        let owned = file.try_clone()?;
        let identity = Identity::of(&file)?;
        self.state.borrow_mut().stage = Some(OwnedStage {
            leaf: leaf.to_os_string(),
            file: Arc::new(owned),
            identity,
        });
        Ok(file)
    }
    fn sync(&self, file: &File) -> io::Result<()> {
        self.check_mode(file)?;
        craft_atomic::sync_file(file)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.install(from, to, false)
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.install(from, to, true)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        self.stage(path)
            .map_err(|_| self.fail(Error::SourceChanged))?;
        std::fs::remove_file(path).map_err(|error| self.fail(io_error(error)))
    }
    /// Windows cannot flush a directory handle without administrator rights. NTFS journals the
    /// name change, and flushing the installed file commits the journal up to it; this is the
    /// closest durability step available to an ordinary user (PROVISIONAL, see known-issues).
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        if path != self.destination.canonical_parent {
            return Err(self.fail(Error::SourceChanged));
        }
        self.destination
            .binding()
            .map_err(|_| self.fail(Error::SourceChanged))?;
        let installed = self.state.borrow().installed.clone();
        if let Some(file) = installed {
            craft_atomic::sync_file(&file).map_err(|error| self.fail(io_error(error)))?;
        }
        self.state.borrow_mut().synced = true;
        Ok(())
    }
    fn set_permissions(&self, file: &File, permissions: std::fs::Permissions) -> io::Result<()> {
        if let Some(baseline) = self.baseline
            && permissions.readonly() != baseline.file.metadata()?.permissions().readonly()
        {
            return Err(self.fail(Error::SourceChanged));
        }
        file.set_permissions(permissions)
    }
}

pub(super) struct Receipt {
    destination: Arc<Destination>,
    bytes: Arc<[u8]>,
    parse_bytes: Arc<[u8]>,
    file: Arc<File>,
}
impl Receipt {
    fn verified(&self) -> Result<(NativeOrigin, PathBuf), Error> {
        self.destination.verify(&self.file, &self.bytes)?;
        let baseline = SourceBaseline {
            bytes: self.bytes.clone(),
            parse_bytes: self.parse_bytes.clone(),
            metadata: self.file.metadata().map_err(io_error)?,
            link: LinkStatus::Direct,
            file: self.file.clone(),
            destination: Some(self.destination.clone()),
        };
        Ok((NativeOrigin::Binary(baseline), self.destination.path()))
    }
    pub(super) fn reconcile(&self) -> Result<(NativeOrigin, PathBuf), Error> {
        self.destination.verify(&self.file, &self.bytes)?;
        craft_atomic::sync_file(&self.file).map_err(io_error)?;
        self.verified()
    }
}

fn prepare<'a>(
    origin: &'a NativeOrigin,
    scene: &NativeScene,
    destination: SaveDestination<'_>,
    may_install: &'a dyn Fn() -> bool,
) -> Result<Transaction<'a>, Error> {
    let NativeOrigin::Binary(source) = origin else {
        return Err(Error::Unsupported);
    };
    if !scene.has_binary_source(&source.parse_bytes) {
        return Err(Error::SourceChanged);
    }
    let (destination, baseline) = match destination {
        SaveDestination::Active => (
            source
                .destination
                .clone()
                .filter(|_| matches!(source.link, LinkStatus::Direct))
                .ok_or(Error::Unsupported)?,
            Some(source),
        ),
        SaveDestination::As(path) => {
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mb"))
                || !path.file_name().is_some_and(valid_leaf)
            {
                return Err(Error::InvalidDestination);
            }
            (
                Arc::new(Destination::capture(path).map_err(io_error)?),
                None,
            )
        }
    };
    if let Some(baseline) = baseline {
        destination.verify(&baseline.file, &baseline.bytes)?;
        let attributes = kept_attributes(&baseline.file).map_err(io_error)?;
        if attributes & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 {
            return Err(Error::Unsupported);
        }
    } else {
        destination.absent()?;
    }
    let bytes: Arc<[u8]> = binary::encode(scene).map_err(Error::Encode)?.into();
    Ok(Transaction {
        destination,
        baseline,
        bytes,
        parse_bytes: source.parse_bytes.clone(),
        may_install,
        state: RefCell::new(State::default()),
    })
}
pub(super) fn save_native(
    origin: &NativeOrigin,
    scene: &NativeScene,
    destination: SaveDestination<'_>,
    may_install: &dyn Fn() -> bool,
) -> Result<SaveOutcome, Error> {
    let transaction = prepare(origin, scene, destination, may_install)?;
    transaction.run(&transaction, RenameRetry::platform())
}

#[cfg(test)]
mod tests;
