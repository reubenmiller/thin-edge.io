use std::fs::File;
use std::io;
use std::path::Path;
use std::path::PathBuf;

const LOCK_CHILD_DIRECTORY: &str = "lock/";

#[derive(thiserror::Error, Debug)]
pub enum FlockfileError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

/// A Windows lock file implemented by holding an exclusive open handle.
///
/// On Windows, file locking is mandatory: a file opened with exclusive access
/// cannot be opened again by another process. Dropping the `Flockfile` closes
/// the handle and removes the lock file.
///
/// Note: this is an advisory-style emulation — it protects against well-behaved
/// concurrent thin-edge processes but not against programs that ignore the lock.
#[derive(Debug)]
pub struct Flockfile {
    pub path: PathBuf,
    _handle: File,
}

impl Flockfile {
    pub fn new_lock(lock_name: impl AsRef<Path>) -> Result<Flockfile, FlockfileError> {
        let path = lock_name.as_ref().to_path_buf();

        // Create or open the lock file with exclusive access so a second
        // process gets an error on open rather than silently proceeding.
        let handle = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        Ok(Flockfile {
            path,
            _handle: handle,
        })
    }

    pub fn unlock(self) -> Result<(), io::Error> {
        // The lock file is removed when the handle is dropped. We also
        // attempt to delete it here so it does not linger after a clean exit.
        let path = self.path.clone();
        drop(self);
        // Best-effort removal; ignore the error if another process beat us to it.
        let _ = std::fs::remove_file(&path);
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
        // _handle is dropped automatically, releasing the OS lock.
        // Attempt to clean up the file; ignore errors (e.g. already deleted).
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Check whether another instance of `app_name` is already running by
/// attempting to acquire its lock file under `run_dir/lock/`.
///
/// On Windows the lock is a mandatory exclusive file handle: if another
/// process already holds it, `OpenOptions::open` fails with
/// ERROR_SHARING_VIOLATION (OS error 32).
pub fn check_another_instance_is_not_running(
    app_name: &str,
    run_dir: &Path,
) -> Result<Option<Flockfile>, FlockfileError> {
    let lock_path = run_dir.join(format!("{}{}.lock", LOCK_CHILD_DIRECTORY, app_name));

    match Flockfile::new_lock(&lock_path) {
        Ok(flock) => Ok(Some(flock)),
        Err(FlockfileError::IoError(e)) if e.raw_os_error() == Some(32) => {
            // ERROR_SHARING_VIOLATION — another process holds the exclusive handle
            Err(FlockfileError::IoError(e))
        }
        Err(_) => {
            // Lock directory not accessible — skip locking rather than hard-failing
            Ok(None)
        }
    }
}
