/// Register this process with the Windows Service Control Manager.
///
/// Must be called after argument parsing but before the service's async
/// runtime starts.  Calling from a non-service context (interactive terminal,
/// `cargo run`, etc.) is safe: `register` returns
/// `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT` and this function returns without
/// side effects.
///
/// When running as a Windows Service:
/// - Registers a control handler that accepts Stop and Shutdown events.
/// - Reports `ServiceState::Running` so the SCM transitions the service out
///   of "Starting" and enables the Stop / Restart controls in the Services UI.
/// - On Stop/Shutdown: reports `StopPending` then calls `std::process::exit(0)`.
///   Reporting `StopPending` before exit ensures the SCM acknowledges the stop
///   request promptly; without it the SCM may time out waiting for a status
///   update, causing MSIX package install/update to fail with 0x8007041d.
pub fn register_with_scm(service_name: &str) {
    use std::sync::OnceLock;
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};

    // Shared storage so the stop handler can report StopPending before exit.
    // Set once after register() returns; read when Stop/Shutdown is received.
    static HANDLE: OnceLock<ServiceStatusHandle> = OnceLock::new();

    let result = service_control_handler::register(service_name, |event| match event {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            if let Some(handle) = HANDLE.get() {
                let _ = handle.set_service_status(ServiceStatus {
                    service_type: ServiceType::OWN_PROCESS,
                    current_state: ServiceState::StopPending,
                    controls_accepted: ServiceControlAccept::empty(),
                    exit_code: ServiceExitCode::Win32(0),
                    checkpoint: 0,
                    wait_hint: Duration::from_secs(5),
                    process_id: None,
                });
            }
            std::process::exit(0);
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    });

    match result {
        Ok(handle) => {
            let _ = HANDLE.set(handle);
            let _ = handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            });
        }
        // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT — not running as a service
        Err(_) => {}
    }
}
