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

/// The interpreter and its leading arguments needed to run a script on Windows, if any.
///
/// `CreateProcessW` cannot start non-PE executables directly:
/// `.bat` and `.cmd` scripts run with `cmd.exe /c`,
/// `.ps1` scripts with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File`.
/// Returns `None` for any other program, which is started directly.
pub fn windows_interpreter(program: &Path) -> Option<(&'static str, &'static [&'static str])> {
    let ext = program.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("bat") || ext.eq_ignore_ascii_case("cmd") {
        Some(("cmd.exe", &["/c"]))
    } else if ext.eq_ignore_ascii_case("ps1") {
        Some((
            "powershell.exe",
            &["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"],
        ))
    } else {
        None
    }
}

/// Splits a command line into a program and its arguments, as a POSIX shell would.
///
/// On Windows, backslashes are kept as is, as they are path separators and not escape characters:
/// `C:\scripts\op.cmd "a b"` is split into `C:\scripts\op.cmd` and `a b`.
/// Use double quotes to group words containing spaces.
pub fn split_command_line(command_line: &str) -> Result<Vec<String>, shell_words::ParseError> {
    if cfg!(windows) {
        shell_words::split(&command_line.replace('\\', "\\\\"))
    } else {
        shell_words::split(command_line)
    }
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

    #[test_case("plugin.bat", Some("cmd.exe"))]
    #[test_case("plugin.CMD", Some("cmd.exe"))]
    #[test_case("plugin.ps1", Some("powershell.exe"))]
    #[test_case("plugin.exe", None)]
    #[test_case("plugin", None)]
    fn windows_interpreters(file: &str, interpreter: Option<&str>) {
        assert_eq!(
            windows_interpreter(Path::new(file)).map(|(program, _)| program),
            interpreter
        );
    }

    #[test]
    #[cfg(windows)]
    fn windows_command_lines_keep_backslashes() {
        assert_eq!(
            split_command_line(r#"C:\scripts\op.cmd "a b" c:\tmp"#).unwrap(),
            vec![r"C:\scripts\op.cmd", "a b", r"c:\tmp"]
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_command_lines_are_split_as_by_a_shell() {
        assert_eq!(
            split_command_line(r#"/bin/op "a b" c\ d"#).unwrap(),
            vec!["/bin/op", "a b", "c d"]
        );
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
