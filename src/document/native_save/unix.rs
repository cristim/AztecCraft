use super::{NativeSaveError as Error, SaveDestination, SaveOutcome, SaveReceipt};
use crate::{
    document::{LinkStatus, MAX_DOCUMENT_BYTES, NativeOrigin, SourceBaseline},
    native_scene::{NativeScene, ascii::write::emit_geometry_prefix, binary},
};
use craft_atomic::{AtomicFs, NewFilePermissions, RenameRetry, WriteOptions};
use rustix::fs::{self, AtFlags, Mode, OFlags};
use std::{
    cell::RefCell,
    ffi::{OsStr, OsString},
    fs::{File, Metadata},
    io::{self, Write},
    os::unix::fs::{FileExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    device: u64,
    inode: u64,
}
impl Identity {
    fn of(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}
fn io_error(error: impl Into<io::Error>) -> Error {
    Error::Io(error.into().kind())
}
fn changed() -> io::Error {
    io::Error::from(io::ErrorKind::InvalidData)
}

pub(in crate::document) struct Destination {
    requested_parent: PathBuf,
    canonical_parent: PathBuf,
    parent: Arc<File>,
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
        let requested_parent = absolute.parent().ok_or_else(changed)?.to_path_buf();
        let canonical_parent = requested_parent.canonicalize()?;
        let parent = File::from(fs::open(
            &canonical_parent,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let identity = Identity::of(&parent.metadata()?);
        let destination = Self {
            requested_parent,
            canonical_parent,
            parent: Arc::new(parent),
            identity,
            leaf,
        };
        destination.binding()?;
        Ok(destination)
    }
    pub(in crate::document) fn path(&self) -> PathBuf {
        self.canonical_parent.join(&self.leaf)
    }
    fn binding(&self) -> io::Result<()> {
        for path in [&self.requested_parent, &self.canonical_parent] {
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_dir() || Identity::of(&metadata) != self.identity {
                return Err(changed());
            }
        }
        if self.requested_parent.canonicalize()? != self.canonical_parent {
            return Err(changed());
        }
        Ok(())
    }
    fn stat(&self, leaf: &OsStr) -> io::Result<fs::Stat> {
        fs::statat(&*self.parent, leaf, AtFlags::SYMLINK_NOFOLLOW).map_err(Into::into)
    }
    fn open(&self, leaf: &OsStr) -> io::Result<File> {
        let file = File::from(fs::openat(
            &*self.parent,
            leaf,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        if !file.metadata()?.is_file() {
            return Err(changed());
        }
        Ok(file)
    }
    pub(in crate::document) fn matches(&self, metadata: &Metadata) -> io::Result<bool> {
        self.binding()?;
        let file = self.open(&self.leaf)?;
        Ok(Identity::of(&file.metadata()?) == Identity::of(metadata))
    }
    pub(in crate::document) fn verify(&self, file: &File, bytes: &[u8]) -> Result<(), Error> {
        self.binding().map_err(|_| Error::SourceChanged)?;
        let current = self.open(&self.leaf).map_err(|_| Error::SourceChanged)?;
        if Identity::of(&current.metadata().map_err(io_error)?)
            != Identity::of(&file.metadata().map_err(io_error)?)
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
        let read = match file.read_at(&mut scratch[..count], offset as u64) {
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
    authored_document_id: Option<crate::native_scene::AuthoredDocumentId>,
    may_install: &'a dyn Fn() -> bool,
    state: RefCell<State>,
}

impl Transaction<'_> {
    fn check_mode(&self, file: &File) -> io::Result<()> {
        if let Some(baseline) = self.baseline
            && file.metadata()?.permissions().mode()
                != baseline.file.metadata()?.permissions().mode()
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
        let file = File::from(
            fs::openat(
                &*self.destination.parent,
                &self.destination.leaf,
                OFlags::WRONLY | OFlags::NONBLOCK | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|error| self.fail(io_error(error)))?,
        );
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || Identity::of(&metadata) != Identity::of(&baseline.file.metadata()?)
        {
            return Err(self.fail(Error::SourceChanged));
        }
        Ok(())
    }
    fn fail(&self, error: Error) -> io::Error {
        let mut state = self.state.borrow_mut();
        if state.installed.is_some() {
            state.post_error = Some(error.clone());
        } else {
            state.failure = Some(error.clone());
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
        if Identity::of(&current.metadata()?) != stage.identity {
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
        let leaf = from.file_name().ok_or_else(changed)?;
        let result = if exclusive {
            fs::linkat(
                &*self.destination.parent,
                leaf,
                &*self.destination.parent,
                &self.destination.leaf,
                AtFlags::empty(),
            )
        } else {
            fs::renameat(
                &*self.destination.parent,
                leaf,
                &*self.destination.parent,
                &self.destination.leaf,
            )
        };
        result.map_err(|error| {
            self.fail(if error == rustix::io::Errno::EXIST {
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
            return Err(state.failure.clone().unwrap_or_else(|| {
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
            authored_document_id: self.authored_document_id,
            file: file.clone(),
        };
        let cause = state
            .post_error
            .clone()
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
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        let leaf = self.destination.stage_leaf(path).map_err(|_| {
            let error = self
                .state
                .borrow()
                .failure
                .clone()
                .unwrap_or(Error::Unsupported);
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
        let mode = if self.baseline.is_some() || permissions == NewFilePermissions::Private {
            Mode::RUSR | Mode::WUSR
        } else {
            Mode::from_raw_mode(0o666)
        };
        let file = File::from(fs::openat(
            &*self.destination.parent,
            leaf,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            mode,
        )?);
        let owned = file.try_clone()?;
        let identity = Identity::of(&file.metadata()?);
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
        let leaf = path.file_name().ok_or_else(changed)?;
        fs::unlinkat(&*self.destination.parent, leaf, AtFlags::empty())
            .map_err(|error| self.fail(io_error(error)))
    }
    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        if path != self.destination.canonical_parent {
            return Err(self.fail(Error::SourceChanged));
        }
        self.destination
            .binding()
            .map_err(|_| self.fail(Error::SourceChanged))?;
        craft_atomic::sync_file(&self.destination.parent)
            .map_err(|error| self.fail(io_error(error)))?;
        self.state.borrow_mut().synced = true;
        Ok(())
    }
    fn set_permissions(&self, file: &File, permissions: std::fs::Permissions) -> io::Result<()> {
        if let Some(baseline) = self.baseline
            && permissions.mode() != baseline.file.metadata()?.permissions().mode()
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
    authored_document_id: Option<crate::native_scene::AuthoredDocumentId>,
    file: Arc<File>,
}
impl Receipt {
    fn verified(&self) -> Result<(NativeOrigin, PathBuf), Error> {
        self.destination.verify(&self.file, &self.bytes)?;
        let path = self.destination.path();
        let baseline = SourceBaseline {
            bytes: self.bytes.clone(),
            parse_bytes: self.parse_bytes.clone(),
            authored_document_id: self.authored_document_id,
            metadata: self.file.metadata().map_err(io_error)?,
            link: LinkStatus::Direct,
            file: self.file.clone(),
            destination: Some(self.destination.clone()),
        };
        let origin = if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ma"))
        {
            NativeOrigin::AuthoredAscii(baseline)
        } else {
            NativeOrigin::Binary(baseline)
        };
        Ok((origin, path))
    }
    pub(super) fn reconcile(&self) -> Result<(NativeOrigin, PathBuf), Error> {
        self.destination.verify(&self.file, &self.bytes)?;
        craft_atomic::sync_file(&self.destination.parent).map_err(io_error)?;
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
    } else {
        destination.absent()?;
    }
    let bytes: Arc<[u8]> = binary::encode(scene).map_err(Error::Encode)?.into();
    Ok(Transaction {
        destination,
        baseline,
        bytes,
        parse_bytes: source.parse_bytes.clone(),
        authored_document_id: None,
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

fn prepare_authored<'a>(
    origin: &'a NativeOrigin,
    scene: &NativeScene,
    destination: SaveDestination<'_>,
    may_install: &'a dyn Fn() -> bool,
) -> Result<Transaction<'a>, Error> {
    // Authored output has unproven Maya acceptance; our ASCII reader cannot
    // reopen authored meshes. Bind every later save to this document.
    match origin {
        NativeOrigin::Untitled => {}
        NativeOrigin::AuthoredAscii(source) => {
            let id = scene.authored_document_id().ok_or(Error::SourceChanged)?;
            if source.authored_document_id != Some(id) {
                return Err(Error::SourceChanged);
            }
        }
        _ => return Err(Error::Unsupported),
    }

    // Resolve the destination path and determine if it is Save As or Active
    let (path, is_save_as) = match destination {
        SaveDestination::As(path) => (path.to_path_buf(), true),
        SaveDestination::Active => match origin {
            NativeOrigin::AuthoredAscii(source) => {
                (source.destination_path().ok_or(Error::Unsupported)?, false)
            }
            _ => return Err(Error::Unsupported),
        },
    };

    // Validate that the destination has a .ma extension (case-insensitive)
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ma"))
    {
        return Err(Error::InvalidDestination);
    }

    // Generate the bytes for the Maya ASCII scene. This uses the `emit_geometry_prefix` function
    let bytes: Arc<[u8]> = emit_geometry_prefix(scene)
        .map_err(Error::AsciiWrite)?
        .into();

    let document_id = scene.authored_document_id().ok_or(Error::SourceChanged)?;

    // Capture the destination
    let destination = Arc::new(Destination::capture(&path).map_err(io_error)?);

    // Determinate the baseline (using the boolean is_save_as)
    let baseline = if is_save_as {
        // Save As: the new destination MUST be absent.
        destination.absent()?;
        None
    } else {
        // Active: verify that it matches the origin.
        match origin {
            NativeOrigin::AuthoredAscii(source) => {
                if destination.verify(&source.file, &source.bytes).is_ok() {
                    Some(source)
                } else {
                    return Err(Error::SourceChanged);
                }
            }
            _ => return Err(Error::Unsupported),
        }
    };

    // Create the transaction
    let transaction = Transaction {
        destination,
        baseline,
        bytes,
        parse_bytes: Arc::new([]),
        authored_document_id: Some(document_id),
        may_install,
        state: RefCell::new(State::default()),
    };

    Ok(transaction)
}

pub(super) fn save_authored(
    origin: &NativeOrigin,
    scene: &NativeScene,
    destination: SaveDestination<'_>,
    may_install: &dyn Fn() -> bool,
) -> Result<SaveOutcome, Error> {
    let transaction = prepare_authored(origin, scene, destination, may_install)?;
    transaction.run(&transaction, RenameRetry::platform())
}

#[cfg(test)]
mod tests_authored {
    use super::*;
    use crate::native_scene::NativeScene;
    use crate::scene::Primitive;

    #[test]
    fn authored_scene_saves_as_maya_ascii() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("untitled.ma");

        // 1. Create a new legacy scene and add a cube
        let mut legacy = crate::scene::Scene::default();
        legacy.create(Primitive::Cube, "cube").unwrap();

        // 2. Convert to NativeScene
        let scene = NativeScene::from_legacy(legacy).unwrap();

        // 3. Verify that the scene is "without origin" (Untitled)
        let origin = NativeOrigin::Untitled;
        assert!(origin.save_as_available());

        // 4. Save
        let result = crate::document::save_native(
            &origin,
            &scene,
            SaveDestination::As(&destination),
            &|| true,
        );

        // 5. Verify that the file exists and is the expected path
        match result.expect("save should succeed") {
            SaveOutcome::Saved { path, .. } => {
                assert_eq!(path, destination.canonicalize().unwrap());
            }
            other => panic!("expected Saved, got {other:?}"),
        }

        // 6. Verify the content of the .ma file
        let contents = std::fs::read_to_string(&destination).unwrap();
        assert!(contents.contains("//Maya ASCII 2026 scene"));
        assert!(contents.contains("createNode transform"));
        assert!(contents.contains("createNode mesh"));

        // 7. Round-trip (Open) is not yet supported: the ASCII reader supports
        // only a narrower subset (curves) than the writer. This is tracked separately.
        // The file is valid Maya ASCII and can be opened in Maya.
    }
}

#[cfg(test)]
mod tests;
