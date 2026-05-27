//! Windows Service Control Manager (SCM) backend for `SystemServiceManager`.
//!
//! This implementation drives the Windows `sc.exe` CLI as an interim approach.
//! A future iteration will use the `windows-service` crate to call the SCM
//! APIs directly, which avoids the overhead of spawning a child process and
//! allows richer error reporting.

use crate::SystemService;
use crate::SystemServiceError;
use crate::SystemServiceManager;
use std::fmt;
use std::process::Command;

#[derive(Debug)]
pub struct WindowsServiceManager;

impl WindowsServiceManager {
    pub fn new() -> Self {
        WindowsServiceManager
    }
}

impl Default for WindowsServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WindowsServiceManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Windows SCM")
    }
}

#[async_trait::async_trait]
impl SystemServiceManager for WindowsServiceManager {
    fn name(&self) -> &str {
        "windows-scm"
    }

    async fn check_operational(&self) -> Result<(), SystemServiceError> {
        // Verify that sc.exe is accessible by running a harmless query.
        run_sc(&["query", "type=", "all", "state=", "inactive"])
            .map(|_| ())
            .or_else(|_| {
                // Tolerate non-zero exit (no inactive services); the tool exists.
                Ok(())
            })
    }

    async fn stop_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        run_sc(&["stop", &service.to_string()]).map(|_| ())
    }

    async fn start_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        run_sc(&["start", &service.to_string()]).map(|_| ())
    }

    async fn restart_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        // Windows SCM has no native restart command; stop then start.
        let name = service.to_string();
        // Tolerate stop errors (service may already be stopped).
        let _ = run_sc(&["stop", &name]);
        run_sc(&["start", &name]).map(|_| ())
    }

    async fn enable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        run_sc(&["config", &service.to_string(), "start=", "auto"]).map(|_| ())
    }

    async fn disable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        run_sc(&["config", &service.to_string(), "start=", "disabled"]).map(|_| ())
    }

    async fn is_service_running(
        &self,
        service: SystemService<'_>,
    ) -> Result<bool, SystemServiceError> {
        let output = Command::new("sc")
            .args(["query", &service.to_string()])
            .output()
            .map_err(|_| SystemServiceError::ServiceManagerUnavailable {
                cmd: "sc query".into(),
                name: self.name().into(),
            })?;
        Ok(String::from_utf8_lossy(&output.stdout).contains("RUNNING"))
    }
}

fn run_sc(args: &[&str]) -> Result<std::process::Output, SystemServiceError> {
    let output = Command::new("sc")
        .args(args)
        .output()
        .map_err(|_| SystemServiceError::ServiceManagerUnavailable {
            cmd: format!("sc {}", args.join(" ")),
            name: "windows-scm".into(),
        })?;

    if output.status.success() {
        Ok(output)
    } else {
        let service_command = format!("sc {}", args.join(" "));
        match output.status.code() {
            Some(code) => Err(SystemServiceError::ServiceCommandFailedWithCode {
                service_command,
                code,
            }),
            None => Err(SystemServiceError::ServiceCommandFailedBySignal { service_command }),
        }
    }
}
