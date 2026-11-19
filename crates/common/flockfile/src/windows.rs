use std::fs::File;
use std::fs::TryLockError;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

const LOCK_CHILD_DIRECTORY: &str = "lock/";

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error("Couldn't create file lock.")]
    FromIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Couldn't acquire file lock: {path:?} is locked by another process.")]
    AlreadyLocked { path: PathBuf },
}

/// A lock file holding an exclusive lock (`LockFileEx`) for the lifetime of the value.
///
/// The lock is released by the OS when the process exits, even after a crash,
/// so a stale lock file left on disk does not block the next start.
/// The lock file contains the id of the process holding the lock.
#[derive(Debug)]
pub struct Flockfile {
    path: PathBuf,
    _file: File,
}

impl Flockfile {
    /// Create an exclusive lock file with the given path, creating its directory if needed
    pub fn new_lock(path: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        let path = path.as_ref().to_path_buf();
        let io_error = |source| FlockfileError::FromIo {
            path: path.clone(),
            source,
        };

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_error)?;
        }

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(io_error)?;

        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(FlockfileError::AlreadyLocked { path });
            }
            Err(TryLockError::Error(source)) => return Err(io_error(source)),
        }

        // Writing through the locked handle is allowed; other processes can no longer write.
        file.set_len(0).map_err(io_error)?;
        file.write_all(std::process::id().to_string().as_bytes())
            .map_err(io_error)?;

        Ok(Flockfile { path, _file: file })
    }

    pub fn unlock(self) -> Result<(), io::Error> {
        drop(self);
        Ok(())
    }
}

impl AsRef<Path> for Flockfile {
    fn as_ref(&self) -> &Path {
        self.path.as_ref()
    }
}

impl Drop for Flockfile {
    fn drop(&mut self) {
        // Rust opens files with FILE_SHARE_DELETE, so the file can be removed while locked:
        // it is deleted when the handle is closed, right after this.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Check `run_dir`/lock/ for a lock file of a given `app_name`
pub fn check_another_instance_is_not_running(
    app_name: &str,
    run_dir: &Path,
) -> Result<Option<Flockfile>, FlockfileError> {
    let lock_path = run_dir.join(format!("{}{}.lock", LOCK_CHILD_DIRECTORY, app_name));

    match Flockfile::new_lock(&lock_path) {
        Ok(flock) => Ok(Some(flock)),
        Err(err @ FlockfileError::AlreadyLocked { .. }) => {
            tracing::error!("Another instance of {} is running.", app_name);
            Err(err)
        }
        Err(err) => {
            tracing::warn!(
                "Single-instance locking is NOT active for {app_name}: {err:?}. \
                 Another instance running concurrently will not be detected."
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn a_second_lock_on_the_same_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock").join("app.lock");

        let _lock = Flockfile::new_lock(&path).unwrap();

        assert_matches!(
            Flockfile::new_lock(&path),
            Err(FlockfileError::AlreadyLocked { .. })
        );
    }

    #[test]
    fn a_second_instance_is_detected() {
        let dir = tempfile::tempdir().unwrap();

        let first = check_another_instance_is_not_running("tedge-agent", dir.path()).unwrap();

        assert!(first.is_some());
        assert_matches!(
            check_another_instance_is_not_running("tedge-agent", dir.path()),
            Err(FlockfileError::AlreadyLocked { .. })
        );
    }

    #[test]
    fn the_lock_is_released_when_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.lock");

        drop(Flockfile::new_lock(&path).unwrap());

        assert!(!path.exists());
        assert!(Flockfile::new_lock(&path).is_ok());
    }
}
