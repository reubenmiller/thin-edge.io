//! Names of the child-device directories of the c8y operations directory.
//!
//! A child device directory is named after the device external id.
//! External ids, such as `main:device:child1`, can contain characters that are invalid in
//! Windows file names: on Windows, they are percent-encoded (`:` becomes `%3A`),
//! see [`tedge_utils::file_name`].
//! On other platforms, the directory name is the raw external id.

use std::borrow::Cow;
use tedge_utils::file_name::from_file_name;
use tedge_utils::file_name::to_file_name;

/// Returns the name of the directory holding the operations of the device with the given external id
pub fn dir_name_from_xid(xid: &str) -> Cow<'_, str> {
    to_file_name(xid)
}

/// Returns the external id of the device whose operations are stored in the directory with the given name
pub fn xid_from_dir_name(dir_name: &str) -> Cow<'_, str> {
    from_file_name(dir_name)
}

#[cfg(test)]
mod tests {
    use super::*;

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
