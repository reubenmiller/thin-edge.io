//! Names of the child-device directories of the c8y operations directory.
//!
//! A child device directory is named after the device external id.
//! External ids, such as `main:device:child1`, can contain characters that are invalid in
//! Windows file names. On Windows, these characters are percent-encoded (`:` becomes `%3A`),
//! and the directory names are decoded back to external ids when the directory is read.
//! On other platforms, the directory name is the raw external id.

use std::borrow::Cow;

/// The characters that cannot be used in a Windows file name, plus `%` which is the escape character
const WINDOWS_ESCAPED_CHARS: [char; 10] = ['%', '<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Returns the name of the directory holding the operations of the device with the given external id
pub fn dir_name_from_xid(xid: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        percent_encode(xid)
    } else {
        Cow::Borrowed(xid)
    }
}

/// Returns the external id of the device whose operations are stored in the directory with the given name
pub fn xid_from_dir_name(dir_name: &str) -> Cow<'_, str> {
    if cfg!(windows) {
        percent_decode(dir_name)
    } else {
        Cow::Borrowed(dir_name)
    }
}

fn percent_encode(xid: &str) -> Cow<'_, str> {
    if !xid.contains(WINDOWS_ESCAPED_CHARS) {
        return Cow::Borrowed(xid);
    }
    let mut encoded = String::with_capacity(xid.len() + 8);
    for c in xid.chars() {
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
/// so a directory created by hand with a literal `%` still maps to a device.
fn percent_decode(dir_name: &str) -> Cow<'_, str> {
    if !dir_name.contains('%') {
        return Cow::Borrowed(dir_name);
    }
    let mut decoded = String::with_capacity(dir_name.len());
    let mut rest = dir_name;
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
    fn windows_dir_names_are_percent_encoded(xid: &str, dir_name: &str) {
        assert_eq!(percent_encode(xid), dir_name);
        assert_eq!(percent_decode(dir_name), xid);
    }

    #[test_case("100%")]
    #[test_case("a%zzb")]
    #[test_case("a%41b")]
    fn unknown_escape_sequences_are_kept(dir_name: &str) {
        assert_eq!(percent_decode(dir_name), dir_name);
    }

    #[test]
    #[cfg(windows)]
    fn windows_directory_names_are_valid_file_names() {
        assert_eq!(
            dir_name_from_xid("main:device:child1"),
            "main%3Adevice%3Achild1"
        );
        assert_eq!(
            xid_from_dir_name("main%3Adevice%3Achild1"),
            "main:device:child1"
        );
    }

    #[test]
    #[cfg(unix)]
    fn unix_directory_names_are_external_ids() {
        assert_eq!(
            dir_name_from_xid("main:device:child1"),
            "main:device:child1"
        );
        assert_eq!(xid_from_dir_name("main%3Adevice"), "main%3Adevice");
    }
}
