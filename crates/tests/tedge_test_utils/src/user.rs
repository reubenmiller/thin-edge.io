#[cfg(unix)]
use uzers::get_current_gid;
#[cfg(unix)]
use uzers::get_current_uid;
#[cfg(unix)]
use uzers::get_group_by_gid;
#[cfg(unix)]
use uzers::get_user_by_uid;

/// Returns the username of the current process's effective user.
#[cfg(unix)]
pub fn current_username() -> String {
    get_user_by_uid(get_current_uid())
        .expect("current user must exist")
        .name()
        .to_string_lossy()
        .into_owned()
}

/// Returns the group name of the current process's primary group.
#[cfg(unix)]
pub fn current_groupname() -> String {
    get_group_by_gid(get_current_gid())
        .expect("current group must exist")
        .name()
        .to_string_lossy()
        .into_owned()
}

/// Returns the username of the current process's user.
#[cfg(windows)]
pub fn current_username() -> String {
    std::env::var("USERNAME").expect("USERNAME must be set")
}

/// Windows has no primary group for file ownership: thin-edge ignores the group.
#[cfg(windows)]
pub fn current_groupname() -> String {
    String::new()
}
