//! Platform-specific rules to find the plugins in a plugin directory.
//!
//! On Unix, any file is a candidate plugin and its name is the file name.
//! Whether the file is executable is checked when the plugin is invoked.
//!
//! On Windows, there is no execute bit, so only files with an executable extension
//! (`.exe`, `.bat`, `.cmd`, `.ps1`, `.com`) are plugins.
//! The extension is stripped from the plugin name, so `my-plugin.bat` is named `my-plugin`,
//! keeping the software, log and config types published by a plugin platform-independent.

use std::path::Path;

/// The file extensions of the Windows plugins, in lower case
pub const WINDOWS_PLUGIN_EXTENSIONS: [&str; 5] = ["exe", "bat", "cmd", "ps1", "com"];

/// Returns the name of the plugin implemented by the given file
pub fn plugin_name(path: &Path) -> Option<&str> {
    if cfg!(windows) {
        path.file_stem()?.to_str()
    } else {
        path.file_name()?.to_str()
    }
}

/// Returns true if the given file can be a plugin on this platform
pub fn is_plugin_candidate(path: &Path) -> bool {
    !cfg!(windows) || has_windows_plugin_extension(path)
}

/// Returns true if the file extension is one of [`WINDOWS_PLUGIN_EXTENSIONS`], ignoring case
pub fn has_windows_plugin_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            WINDOWS_PLUGIN_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("plugin.exe", true)]
    #[test_case("plugin.EXE", true)]
    #[test_case("plugin.bat", true)]
    #[test_case("plugin.cmd", true)]
    #[test_case("plugin.ps1", true)]
    #[test_case("plugin.com", true)]
    #[test_case("plugin", false)]
    #[test_case("plugin.sh", false)]
    #[test_case("plugin.toml", false)]
    fn windows_plugin_extensions(file: &str, expected: bool) {
        assert_eq!(has_windows_plugin_extension(Path::new(file)), expected);
    }

    #[test]
    #[cfg(windows)]
    fn windows_plugin_names_have_no_extension() {
        assert_eq!(plugin_name(Path::new(r"C:\plugins\apt.bat")), Some("apt"));
        assert!(is_plugin_candidate(Path::new("apt.ps1")));
        assert!(!is_plugin_candidate(Path::new("apt")));
    }

    #[test]
    #[cfg(unix)]
    fn unix_plugin_names_are_file_names() {
        assert_eq!(plugin_name(Path::new("/plugins/apt.sh")), Some("apt.sh"));
        assert!(is_plugin_candidate(Path::new("apt")));
    }
}
