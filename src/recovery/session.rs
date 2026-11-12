//! Session markers: an advisory lock on a per-pid file tells a crashed session from a live one.

use super::*;

const MARKER_PREFIX: &str = "session-";
const MARKER_SUFFIX: &str = ".marker";

/// A running session. It holds an advisory lock on its marker file; the OS drops the lock if the
/// process dies, so a marker nobody holds marks a crashed session.
#[derive(Debug)]
pub struct SessionGuard {
    path: PathBuf,
    file: Option<File>,
    /// A marker for this pid already existed (an unclean prior run) and was taken over.
    pub replaced_stale_marker: bool,
}

impl SessionGuard {
    /// Clean shutdown: remove the marker.
    pub fn end(mut self) -> Result<(), RecoveryError> {
        let result = remove_regular(&self.path);
        self.file = None;
        result
    }
}

fn marker_name(pid: u32) -> String {
    format!("{MARKER_PREFIX}{pid}{MARKER_SUFFIX}")
}

fn parse_marker(name: &str) -> Option<u32> {
    canonical_pid(
        name.strip_prefix(MARKER_PREFIX)?
            .strip_suffix(MARKER_SUFFIX)?,
    )
}

impl RecoveryStore {
    /// Mark this process as running by locking its marker file for the session's lifetime.
    pub fn begin_session(&self) -> Result<SessionGuard, RecoveryError> {
        self.ensure_dir()?;
        let path = self.dir.join(marker_name(self.pid));
        let replaced_stale_marker = fs::symlink_metadata(&path).is_ok();
        if replaced_stale_marker && !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
            return Err(RecoveryError::NotInStore);
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let file = options.open(&path).map_err(io_error("begin session"))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(fs::TryLockError::WouldBlock) => return Err(RecoveryError::Contended),
            Err(fs::TryLockError::Error(source)) => return Err(io_error("lock session")(source)),
        }
        Ok(SessionGuard {
            path,
            file: Some(file),
            replaced_stale_marker,
        })
    }

    /// Pids whose marker exists but is held by no process. Read-only. A marker created a moment
    /// ago by a session that has not locked it yet can be reported; call this once at launch.
    pub fn crashed_sessions(&self) -> Result<Vec<u32>, RecoveryError> {
        let mut pids = Vec::new();
        for item in self.read_dir()? {
            let item = item.map_err(io_error("scan sessions"))?;
            let Some(pid) = item.file_name().to_str().and_then(parse_marker) else {
                continue;
            };
            if !fs::symlink_metadata(item.path()).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let Ok(file) = File::open(item.path()) else {
                continue;
            };
            if file.try_lock().is_ok() {
                pids.push(pid);
            }
        }
        pids.sort_unstable();
        Ok(pids)
    }

    /// Acknowledge a crashed session by removing its marker, unless a process holds it. Recovery
    /// files are never touched.
    pub fn clear_crashed_session(&self, pid: u32) -> Result<(), RecoveryError> {
        let path = self.dir.join(marker_name(pid));
        match File::open(&path) {
            Ok(file) => match file.try_lock() {
                Ok(()) => remove_regular(&path),
                Err(fs::TryLockError::WouldBlock) => Err(RecoveryError::Contended),
                Err(fs::TryLockError::Error(source)) => Err(io_error("lock session")(source)),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(io_error("open marker")(source)),
        }
    }
}
