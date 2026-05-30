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
    use std::path::Path;

    for subdir in &["data", "log", "tmp", "sm-plugins"] {
        let _ = fs::create_dir_all(config_dir.join(subdir));
    }

    write_if_absent(
        &config_dir.join("tedge.toml"),
        default_tedge_toml(config_dir).as_bytes(),
    );

    // Copy winget.ps1 from the package directory (two levels up from the
    // service executable: {package_root}\bin\tedge.exe → {package_root}).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(package_root) = exe.parent().and_then(|p| p.parent()) {
            let src = package_root.join("sm-plugins").join("winget.ps1");
            let dst = config_dir.join("sm-plugins").join("winget.ps1");
            if src.exists() && !dst.exists() {
                let _ = fs::copy(&src, &dst);
            }
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
    format!(
        "[data]\npath = '{data}'\n\n[logs]\npath = '{log}'\n\n[tmp]\npath = '{tmp}'\n"
    )
}
