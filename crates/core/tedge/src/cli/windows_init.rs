/// First-run initialisation for the Windows MSIX package.
///
/// Creates the `C:\ProgramData\tedge\` directory tree and seeds default files
/// that are bundled in the package but must live outside the read-only install
/// location.  Every operation is idempotent: existing files are never
/// overwritten, so user edits survive service upgrades.
///
/// Called before any command on Windows.
/// `refresh_plugin_wrappers` is set when a component (agent, mapper, ...) starts:
/// the `file.cmd` plugin wrappers are then pointed at the running `tedge.exe`.
/// It is not set for CLI commands, so running another `tedge.exe` (e.g. a development build)
/// does not redirect the plugins of the installed services.
#[cfg(windows)]
pub fn ensure_windows_data_dirs(config_dir: &std::path::Path, refresh_plugin_wrappers: bool) {
    use std::fs;

    if !config_dir.exists() && fs::create_dir_all(config_dir).is_ok() {
        restrict_to_system_and_administrators(config_dir);
    }

    for subdir in &[
        "data",
        "log",
        "tmp",
        "sm-plugins",
        "config-plugins",
        "log-plugins",
        "device-certs",
        "operations",
    ] {
        let _ = fs::create_dir_all(config_dir.join(subdir));
    }

    write_if_absent(
        &config_dir.join("tedge.toml"),
        default_tedge_toml(config_dir).as_bytes(),
    );

    if let Ok(exe) = std::env::current_exe() {
        // Copy winget.ps1 from the package directory (two levels up from the
        // service executable: {package_root}\bin\tedge.exe → {package_root}).
        if let Some(package_root) = exe.parent().and_then(|p| p.parent()) {
            let src = package_root.join("sm-plugins").join("winget-exe.ps1");
            let dst = config_dir.join("sm-plugins").join("winget.ps1");
            if src.exists() && !dst.exists() {
                let _ = fs::copy(&src, &dst);
            }
        }

        // Always regenerate file.cmd wrappers with the absolute path to the current
        // tedge.exe. The MSIX install location changes on every upgrade, so the path
        // must be kept current rather than written once and left stale.
        let exe_path = exe.to_string_lossy();
        let wrappers: &[(&str, &str)] = if refresh_plugin_wrappers {
            &[
                ("config-plugins", "tedge-file-config-plugin"),
                ("log-plugins", "tedge-file-log-plugin"),
            ]
        } else {
            &[]
        };
        for (subdir, subcommand) in wrappers {
            let content = format!("@echo off\r\n\"{exe_path}\" run {subcommand} %*\r\n");
            let _ = fs::write(config_dir.join(subdir).join("file.cmd"), content.as_bytes());
        }
    }
}

/// Restricts a directory to `SYSTEM` and `BUILTIN\Administrators`,
/// the Windows equivalent of the `tedge:tedge` ownership and restrictive modes used on Linux.
///
/// Full control is granted to both principals and inherited by every file and sub-directory,
/// and the permissions inherited from the parent directory (which grant read access to all users
/// under `C:\ProgramData`) are removed.
/// Principals are given by SID, as their names depend on the Windows display language.
///
/// This is applied only when the directory is created, so an administrator can relax it.
#[cfg(windows)]
fn restrict_to_system_and_administrators(dir: &std::path::Path) {
    let status = std::process::Command::new("icacls")
        .arg(dir)
        .args(icacls_restrict_args())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!(
            "Failed to restrict the access to {}: icacls {status}",
            dir.display()
        ),
        Err(err) => eprintln!("Failed to restrict the access to {}: {err}", dir.display()),
    }
}

#[cfg(windows)]
/// The `icacls` arguments granting full control to `SYSTEM` (S-1-5-18) and `Administrators` (S-1-5-32-544) only
fn icacls_restrict_args() -> [&'static str; 5] {
    [
        "/inheritance:r",
        "/grant:r",
        "*S-1-5-18:(OI)(CI)F",
        "/grant:r",
        "*S-1-5-32-544:(OI)(CI)F",
    ]
}

#[cfg(windows)]
fn write_if_absent(path: &std::path::Path, contents: &[u8]) {
    if !path.exists() {
        let _ = std::fs::write(path, contents);
    }
}

/// The `tedge.toml` created on the first run on Windows.
///
/// Paths are written with forward slashes in TOML literal strings, so they are not read as escape sequences.
/// The built-in bridge is enabled, as the Mosquitto bridge is not supported on Windows.
#[cfg_attr(not(windows), allow(dead_code))]
fn default_tedge_toml(config_dir: &std::path::Path) -> String {
    let path = |subdir: &str| {
        config_dir
            .join(subdir)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    format!(
        "[data]\npath = '{}'\n\n[logs]\npath = '{}'\n\n[tmp]\npath = '{}'\n\n\
         [configuration]\nplugin_paths = '{}'\n\n\
         [log]\nplugin_paths = '{}'\n\n\
         [mqtt.bridge]\nbuilt_in = true\n",
        path("data"),
        path("log"),
        path("tmp"),
        path("config-plugins"),
        path("log-plugins"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tedge_toml_is_a_valid_configuration() {
        let config_dir = std::path::Path::new(r"C:\ProgramData\tedge");
        let toml = default_tedge_toml(config_dir);
        let config = tedge_config::TEdgeConfig::load_toml_str(&toml);

        assert!(config.mqtt.bridge.built_in);
        assert!(config.data.path.as_str().ends_with("/data"));
        assert!(
            !toml.contains('\\'),
            "backslashes would be read as TOML escapes: {toml}"
        );
    }
}
