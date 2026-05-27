mod general_manager;
#[cfg(windows)]
mod windows_manager;

pub use self::general_manager::*;
#[cfg(windows)]
pub use self::windows_manager::WindowsServiceManager;
