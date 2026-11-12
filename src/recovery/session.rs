//! Session markers: an advisory lock on a per-pid file tells a crashed session from a live one.

use super::*;

const MARKER_PREFIX: &str = "session-";
const MARKER_SUFFIX: &str = ".marker";

/// A running session. It holds an advisory lock on its marker file; the OS drops the lock if the
/// process dies, so a marker nobody holds marks a crashed session.
pub struct SessionGuard {
    path: PathBuf,
    file: Option<File>,
    active: Rc<Cell<bool>>,
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

impl fmt::Debug for SessionGuard {
    // Omits the marker path (an absolute local path).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionGuard")
            .field("replaced_stale_marker", &self.replaced_stale_marker)
            .finish_non_exhaustive()
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.active.set(false);
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
        if self.active.get() {
            return Err(RecoveryError::Contended);
        }
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
        self.active.set(true);
        if replaced_stale_marker {
            self.stale_takeover.set(true);
        }
        Ok(SessionGuard {
            path,
            file: Some(file),
            active: Rc::clone(&self.active),
            replaced_stale_marker,
        })
    }

    /// Pids whose marker exists but is held by no process, plus this pid when `begin_session`
    /// took over its unheld marker (until acknowledged). Read-only. A marker created a moment
    /// ago by a session that has not locked it yet can be reported; call this once at launch.
    /// While this store's session is active its own marker is never probed.
    pub fn crashed_sessions(&self) -> Result<Vec<u32>, RecoveryError> {
        let mut pids = Vec::new();
        for item in self.read_dir()? {
            let item = item.map_err(io_error("scan sessions"))?;
            let Some(pid) = item.file_name().to_str().and_then(parse_marker) else {
                continue;
            };
            if (pid == self.pid && self.active.get())
                || !fs::symlink_metadata(item.path()).is_ok_and(|m| m.is_file())
            {
                continue;
            }
            let Ok(file) = File::open(item.path()) else {
                continue;
            };
            if file.try_lock().is_ok() {
                pids.push(pid);
            }
        }
        if self.stale_takeover.get() && !pids.contains(&self.pid) {
            pids.push(self.pid);
        }
        pids.sort_unstable();
        Ok(pids)
    }

    /// Acknowledge a crashed session by removing its marker, unless a process holds it. Recovery
    /// files are never touched.
    pub fn clear_crashed_session(&self, pid: u32) -> Result<(), RecoveryError> {
        if pid == self.pid && self.active.get() {
            // The live marker is ours: only the takeover report is acknowledged.
            return if self.stale_takeover.replace(false) {
                Ok(())
            } else {
                Err(RecoveryError::Contended)
            };
        }
        let path = self.dir.join(marker_name(pid));
        match File::open(&path) {
            Ok(file) => match file.try_lock() {
                Ok(()) => {
                    remove_regular(&path)?;
                    self.forget_takeover(pid);
                    Ok(())
                }
                Err(fs::TryLockError::WouldBlock) => Err(RecoveryError::Contended),
                Err(fs::TryLockError::Error(source)) => Err(io_error("lock session")(source)),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.forget_takeover(pid);
                Ok(())
            }
            Err(source) => Err(io_error("open marker")(source)),
        }
    }

    fn forget_takeover(&self, pid: u32) {
        if pid == self.pid {
            self.stale_takeover.set(false);
        }
    }
}
