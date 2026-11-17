//! File names built from names which can contain characters that are invalid in file names on Windows,
//! such as Cumulocity external ids (`main:device:child1`) or log types (`robot_log::robotlog`).
//!
//! On Windows, these characters are percent-encoded (`:` becomes `%3A`) and decoded back when the file is read.
//! On other platforms, the file name is the raw name.

use std::borrow::Cow;

/// The characters that cannot be used in a Windows file name, plus `%` which is the escape character
const WINDOWS_ESCAPED_CHARS: [char; 10] = ['%', '<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Returns a valid file name for the given name
///
/// On Windows, the characters that are invalid in file names are percent-encoded.
/// On other platforms, the name is returned unchanged.
pub fn to_file_name(name: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        percent_encode(name)
    } else {
        Cow::Borrowed(name)
    }
}

/// Returns the name encoded by [`to_file_name`]
pub fn from_file_name(file_name: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        percent_decode(file_name)
    } else {
        Cow::Borrowed(file_name)
    }
}

fn percent_encode(name: &str) -> Cow<'_, str> {
    if !name.contains(WINDOWS_ESCAPED_CHARS) {
        return Cow::Borrowed(name);
    }
    let mut encoded = String::with_capacity(name.len() + 8);
    for c in name.chars() {
        if WINDOWS_ESCAPED_CHARS.contains(&c) {
            encoded.push_str(&format!("%{:02X}", c as u32));
        } else {
            encoded.push(c);
        }
    }
    Cow::Owned(encoded)
}

/// Decodes the escape sequences produced by [`percent_encode`].
///
/// A `%` that is not followed by the code of an escaped character is kept as is,
/// so a file created by hand with a literal `%` keeps its name.
fn percent_decode(file_name: &str) -> Cow<'_, str> {
    if !file_name.contains('%') {
        return Cow::Borrowed(file_name);
    }
    let mut decoded = String::with_capacity(file_name.len());
    let mut rest = file_name;
    while let Some(pos) = rest.find('%') {
        decoded.push_str(&rest[..pos]);
        let escaped = rest
            .get(pos + 1..pos + 3)
            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            .map(char::from)
            .filter(|c| WINDOWS_ESCAPED_CHARS.contains(c));
        match escaped {
            Some(c) => {
                decoded.push(c);
                rest = &rest[pos + 3..];
            }
            None => {
                decoded.push('%');
                rest = &rest[pos + 1..];
            }
        }
    }
    decoded.push_str(rest);
    Cow::Owned(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    #[test_case("child1", "child1")]
    #[test_case("main:device:child1", "main%3Adevice%3Achild1")]
    #[test_case("a%3Ab", "a%253Ab")]
    #[test_case(r#"<>:"/\|?*"#, "%3C%3E%3A%22%2F%5C%7C%3F%2A")]
    fn windows_names_are_percent_encoded(name: &str, file_name: &str) {
        assert_eq!(percent_encode(name), file_name);
        assert_eq!(percent_decode(file_name), name);
    }

    #[test_case("100%")]
    #[test_case("a%zzb")]
    #[test_case("a%41b")]
    fn unknown_escape_sequences_are_kept(file_name: &str) {
        assert_eq!(percent_decode(file_name), file_name);
    }

    #[test]
    #[cfg(windows)]
    fn windows_file_names_are_encoded() {
        assert_eq!(
            to_file_name("robot_log::robotlog"),
            "robot_log%3A%3Arobotlog"
        );
        assert_eq!(
            from_file_name("robot_log%3A%3Arobotlog"),
            "robot_log::robotlog"
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_file_names_are_unchanged() {
        assert_eq!(to_file_name("robot_log::robotlog"), "robot_log::robotlog");
        assert_eq!(from_file_name("a%3Ab"), "a%3Ab");
    }
}
