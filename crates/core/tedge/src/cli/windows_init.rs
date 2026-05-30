/// First-run initialisation for the Windows MSIX package.
///
/// Creates the `C:\ProgramData\tedge\` directory tree and seeds default files
/// that are bundled in the package but must live outside the read-only install
/// location.  Every operation is idempotent: existing files are never
/// overwritten, so user edits survive service upgrades.
///
/// Called once before any `tedge run <service>` dispatch on Windows.
#[cfg(windows)]
pub fn ensure_windows_data_dirs(config_dir: &std::path::Path) {
    use std::fs;

    for subdir in &["data", "log", "tmp", "sm-plugins", "config-plugins", "log-plugins"] {
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
        for (subdir, subcommand) in &[
            ("config-plugins", "tedge-file-config-plugin"),
            ("log-plugins", "tedge-file-log-plugin"),
        ] {
            let content =
                format!("@echo off\r\n\"{exe_path}\" run {subcommand} %*\r\n");
            let _ = fs::write(config_dir.join(subdir).join("file.cmd"), content.as_bytes());
        }
    }
}

#[cfg(windows)]
fn write_if_absent(path: &std::path::Path, contents: &[u8]) {
    if !path.exists() {
        let _ = std::fs::write(path, contents);
    }
}

#[cfg(windows)]
fn default_tedge_toml(config_dir: &std::path::Path) -> String {
    let data = config_dir.join("data").display().to_string().replace('\\', "/");
    let log = config_dir.join("log").display().to_string().replace('\\', "/");
    let tmp = config_dir.join("tmp").display().to_string().replace('\\', "/");
    let config_plugins = config_dir
        .join("config-plugins")
        .display()
        .to_string()
        .replace('\\', "/");
    let log_plugins = config_dir
        .join("log-plugins")
        .display()
        .to_string()
        .replace('\\', "/");
    format!(
        "[data]\npath = '{data}'\n\n[logs]\npath = '{log}'\n\n[tmp]\npath = '{tmp}'\n\n\
         [configuration]\nplugin_paths = '{config_plugins}'\n\n\
         [log]\nplugin_paths = '{log_plugins}'\n"
    )
}
