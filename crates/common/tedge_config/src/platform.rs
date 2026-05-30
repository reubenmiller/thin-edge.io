/// Cross-platform default directory paths for thin-edge.io.
///
/// On Linux/macOS these resolve to the traditional FHS paths; on Windows they
/// resolve under `%ProgramData%\tedge` so that multiple users on the same
/// machine share a single installation without UAC prompts.
use std::path::PathBuf;

/// System-wide configuration root.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux / macOS | `/etc/tedge`                          |
/// | Windows       | `C:\ProgramData\tedge`                |
pub fn config_root() -> PathBuf {
    #[cfg(windows)]
    {
        // %PROGRAMDATA% resolves to C:\ProgramData (system-wide, not per-user).
        // dirs::data_dir() returns %APPDATA%\Roaming which is per-user — wrong for a service.
        PathBuf::from(
            std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_owned()),
        )
        .join("tedge")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/tedge")
    }
}

/// Mutable runtime data root.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux         | `/var/tedge`                          |
/// | Windows       | `C:\ProgramData\tedge\data`           |
pub fn data_root() -> PathBuf {
    #[cfg(windows)]
    {
        config_root().join("data")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/tedge")
    }
}

/// Log directory root.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux         | `/var/log/tedge`                      |
/// | Windows       | `C:\ProgramData\tedge\log`            |
pub fn log_root() -> PathBuf {
    #[cfg(windows)]
    {
        config_root().join("log")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/var/log/tedge")
    }
}

/// Default CA certificate bundle directory.
///
/// Returns `None` on Windows because Windows uses the system certificate store
/// rather than a PEM bundle on disk.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux / macOS | `/etc/ssl/certs`                      |
/// | Windows       | `None` (system certificate store)     |
pub fn ca_certs_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from("/etc/ssl/certs"))
    }
}

/// Temporary directory for downloads and in-progress operations.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux / macOS | `/tmp`                                |
/// | Windows       | `C:\ProgramData\tedge\tmp`            |
pub fn tmp_root() -> PathBuf {
    #[cfg(windows)]
    {
        config_root().join("tmp")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/tmp")
    }
}

/// Runtime socket / named-pipe directory.
///
/// On Windows, named pipes are addressed as `\\.\pipe\<name>`, so this
/// returns the pipe namespace root rather than a filesystem directory.
///
/// | Platform      | Default path                          |
/// |---------------|---------------------------------------|
/// | Linux         | `/run/tedge`                          |
/// | Windows       | `\\.\pipe`                            |
pub fn runtime_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"\\.\pipe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/run/tedge")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_root_is_absolute() {
        assert!(config_root().is_absolute());
    }

    #[test]
    fn data_root_is_absolute() {
        assert!(data_root().is_absolute());
    }

    #[test]
    fn log_root_is_absolute() {
        assert!(log_root().is_absolute());
    }

    #[test]
    fn data_root_is_under_config_root_on_windows() {
        #[cfg(windows)]
        assert!(data_root().starts_with(config_root()));
    }
}
