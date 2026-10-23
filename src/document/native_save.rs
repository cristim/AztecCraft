use super::{NativeOrigin, NativeScene};
use crate::native_scene::binary;
use std::{
    fmt, io,
    path::{Path, PathBuf},
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(super) use unix::Destination;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(super) use windows::Destination;

pub enum SaveDestination<'a> {
    Active,
    As(&'a Path),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSaveError {
    Unsupported,
    InvalidDestination,
    SourceChanged,
    DestinationExists,
    Cancelled,
    Io(io::ErrorKind),
    Encode(binary::BinaryError),
}

impl fmt::Display for NativeSaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("this native scene or platform cannot be written"),
            Self::InvalidDestination => f.write_str("Save As requires a new .mb destination"),
            Self::SourceChanged => {
                f.write_str("scene file or directory identity or contents changed")
            }
            Self::DestinationExists => f.write_str("destination already exists"),
            Self::Cancelled => f.write_str("save cancelled before installation"),
            Self::Io(kind) => write!(f, "native scene IO failed: {kind:?}"),
            Self::Encode(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for NativeSaveError {}

#[non_exhaustive]
pub struct SaveReceipt {
    #[cfg(unix)]
    inner: unix::Receipt,
    #[cfg(windows)]
    inner: windows::Receipt,
}

impl fmt::Debug for SaveReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SaveReceipt { installed: true }")
    }
}

pub enum SaveOutcome {
    Saved {
        origin: NativeOrigin,
        path: PathBuf,
    },
    Uncertain {
        receipt: SaveReceipt,
        cause: NativeSaveError,
    },
}

impl fmt::Debug for SaveOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Saved { .. } => f.write_str("SaveOutcome::Saved"),
            Self::Uncertain { cause, .. } => f
                .debug_struct("SaveOutcome::Uncertain")
                .field("cause", cause)
                .finish(),
        }
    }
}

impl SaveReceipt {
    pub fn reconcile(&self) -> Result<(NativeOrigin, PathBuf), NativeSaveError> {
        #[cfg(any(unix, windows))]
        {
            self.inner.reconcile()
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(NativeSaveError::Unsupported)
        }
    }
}

pub fn save_native(
    origin: &NativeOrigin,
    scene: &NativeScene,
    destination: SaveDestination<'_>,
    may_install: &dyn Fn() -> bool,
) -> Result<SaveOutcome, NativeSaveError> {
    #[cfg(unix)]
    {
        unix::save_native(origin, scene, destination, may_install)
    }
    #[cfg(windows)]
    {
        windows::save_native(origin, scene, destination, may_install)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (origin, scene, destination, may_install);
        Err(NativeSaveError::Unsupported)
    }
}
