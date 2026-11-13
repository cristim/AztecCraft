//! Staged create-new file write shared by the glTF and USD exporters. The destination must not
//! exist, and the caller's `may_install` guard is asked before the staged file is created and
//! again before it is given the destination's name (like the OBJ exporter).

use craft_atomic::{AtomicFs, NewFilePermissions, RealFs, WriteOptions};
use std::{
    cell::Cell,
    fs::File,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) enum StagedWriteError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// `may_install` refused; nothing was installed.
    Cancelled,
}

/// Asks `may_install` before the staged file is created or linked, like the OBJ exporter.
struct GuardedFs<'a> {
    may_install: &'a dyn Fn() -> bool,
    refused: Cell<bool>,
}

impl GuardedFs<'_> {
    fn permit(&self) -> io::Result<()> {
        if (self.may_install)() {
            return Ok(());
        }
        self.refused.set(true);
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "export cancelled before install",
        ))
    }
}

impl AtomicFs for GuardedFs<'_> {
    fn create_new(&self, path: &Path, permissions: NewFilePermissions) -> io::Result<File> {
        self.permit()?;
        RealFs.create_new(path, permissions)
    }
    fn sync(&self, file: &File) -> io::Result<()> {
        RealFs.sync(file)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        RealFs.rename(from, to)
    }
    fn hard_link(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.permit()?;
        RealFs.hard_link(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        RealFs.remove(path)
    }
    fn sync_dir(&self, dir: &Path) -> io::Result<()> {
        RealFs.sync_dir(dir)
    }
    fn set_permissions(&self, file: &File, perms: std::fs::Permissions) -> io::Result<()> {
        RealFs.set_permissions(file, perms)
    }
}

fn refuse_occupied(path: &Path) -> Result<(), StagedWriteError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(StagedWriteError::Io {
            path: path.into(),
            source: io::Error::new(io::ErrorKind::AlreadyExists, "destination already exists"),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StagedWriteError::Io {
            path: path.into(),
            source,
        }),
    }
}

/// Writes `contents` to a new file at `path`; nothing is left behind on refusal or failure.
pub(crate) fn write_new_staged(
    path: &Path,
    contents: &[u8],
    may_install: &dyn Fn() -> bool,
) -> Result<(), StagedWriteError> {
    refuse_occupied(path)?;
    let fs = GuardedFs {
        may_install,
        refused: Cell::new(false),
    };
    WriteOptions::default()
        .write_new_custom(&fs, path, contents)
        .map_err(|source| {
            if fs.refused.get() {
                StagedWriteError::Cancelled
            } else {
                StagedWriteError::Io {
                    path: path.into(),
                    source,
                }
            }
        })
}
