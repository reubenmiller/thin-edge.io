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
        PathBuf::from(std::env::var("PROGRAMDATA").unwrap_or_else(|_| r"C:\ProgramData".to_owned()))
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

/// Directory, under the configuration root, of the extra trusted root certificates on Windows.
///
/// Windows keeps its root certificates in the OS certificate store, which is always trusted,
/// so `c8y.root_cert_path`, `az.root_cert_path` and `aws.root_cert_path` default to this
/// directory (empty by default) rather than to `/etc/ssl/certs`.
///
/// | Platform      | Default `root_cert_path`                    |
/// |---------------|---------------------------------------------|
/// | Linux / macOS | `/etc/ssl/certs`                            |
/// | Windows       | `C:\ProgramData\tedge\ca-certificates`     |
pub const WINDOWS_CA_CERTS_DIR: &str = "ca-certificates";

/// Default trusted root certificates of the cloud connections, given the configuration root
pub fn default_root_cert_path(config_root: &camino::Utf8Path) -> camino::Utf8PathBuf {
    if cfg!(windows) {
        config_root.join(WINDOWS_CA_CERTS_DIR)
    } else {
        camino::Utf8PathBuf::from("/etc/ssl/certs")
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
