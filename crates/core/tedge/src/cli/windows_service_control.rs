/// Register this process with the Windows Service Control Manager.
///
/// Must be called after `ensure_windows_data_dirs` so the log directory exists
/// for the diagnostic file written below.  Calling from a non-service context
/// (interactive terminal, `cargo run`, etc.) is safe: `register` returns
/// `ERROR_FAILED_SERVICE_CONTROLLER_CONNECT` and this function returns without
/// side effects (other than writing the diagnostic file).
///
/// When running as a Windows Service:
/// - Registers a control handler that accepts Stop and Shutdown events.
/// - Reports `ServiceState::Running` so the SCM transitions the service out
///   of "Starting" and enables the Stop / Restart controls in the Services UI.
/// - On Stop/Shutdown: reports `StopPending` and asks the supervisor to drain all the units,
///   so the component publishes its health status and disconnects cleanly.
///   If the component does not stop within [`STOP_TIMEOUT`], the process is exited.
///   Once the component has stopped, [`report_stopped`] reports `Stopped` to the SCM.
///
/// A small diagnostic log is written to `C:\ProgramData\tedge\log\<name>-scm.log`
/// (falling back to `C:\Windows\Temp`) so that SCM registration failures, which
/// would otherwise be invisible (no logger is set up yet), can be diagnosed.
pub fn register_with_scm(service_name: &str) {
    register(service_name)
}

/// How long a component is given to stop gracefully on a Stop request, before the process is exited.
const STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

static HANDLE: std::sync::OnceLock<windows_service::service_control_handler::ServiceStatusHandle> =
    std::sync::OnceLock::new();

/// Reports to the SCM that the service is stopped, with the given exit code.
///
/// Does nothing if the process is not running as a Windows Service.
pub fn report_stopped(exit_code: u32) {
    use windows_service::service::{
        ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
    };
    if let Some(handle) = HANDLE.get() {
        let _ = handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::Win32(exit_code),
            checkpoint: 0,
            wait_hint: std::time::Duration::default(),
            process_id: None,
        });
    }
}

fn register(service_name: &str) {
    use std::io::Write as _;
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    // Early diagnostic file — written before the tracing logger is initialised.
    let log_path = format!(r"C:\ProgramData\tedge\log\{service_name}-scm.log");
    let fallback_path = format!(r"C:\Windows\Temp\{service_name}-scm.log");
    let mut diag = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .or_else(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&fallback_path)
        })
        .ok();

    macro_rules! dlog {
        ($($arg:tt)*) => {
            if let Some(f) = diag.as_mut() {
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let _ = writeln!(f, "[{ts}] {}", format_args!($($arg)*));
            }
        };
    }

    dlog!("register_with_scm: starting for '{service_name}'");

    // Build the service control handler. The closure only accesses HANDLE (a
    // static) so it is trivially 'static + Send; we can create a fresh copy
    // for each registration attempt.
    let make_handler = || {
        |event: ServiceControl| match event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                if let Some(handle) = HANDLE.get() {
                    let _ = handle.set_service_status(ServiceStatus {
                        service_type: ServiceType::OWN_PROCESS,
                        current_state: ServiceState::StopPending,
                        controls_accepted: ServiceControlAccept::empty(),
                        exit_code: ServiceExitCode::Win32(0),
                        checkpoint: 0,
                        wait_hint: STOP_TIMEOUT,
                        process_id: None,
                    });
                }
                tedge_supervisor::request_shutdown();
                // Fallback, if the component doesn't stop in time
                std::thread::spawn(|| {
                    std::thread::sleep(STOP_TIMEOUT);
                    report_stopped(1);
                    std::process::exit(1);
                });
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // Attempt registration; retry once after 200 ms.  MSIX packaged services
    // may have a small race window where the SCM channel is not ready on the
    // very first attempt.
    let result = service_control_handler::register(service_name, make_handler()).or_else(|e| {
        dlog!("RegisterServiceCtrlHandlerExW attempt 1 failed: {e}");
        std::thread::sleep(Duration::from_millis(200));
        service_control_handler::register(service_name, make_handler())
    });

    match result {
        Ok(handle) => {
            dlog!("RegisterServiceCtrlHandlerExW succeeded");
            let _ = HANDLE.set(handle);
            match handle.set_service_status(ServiceStatus {
                service_type: ServiceType::OWN_PROCESS,
                current_state: ServiceState::Running,
                controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
                exit_code: ServiceExitCode::Win32(0),
                checkpoint: 0,
                wait_hint: Duration::default(),
                process_id: None,
            }) {
                Ok(()) => dlog!("SetServiceStatus(Running) succeeded"),
                Err(e) => dlog!("SetServiceStatus(Running) failed: {e}"),
            }
        }
        Err(e) => {
            // ERROR_FAILED_SERVICE_CONTROLLER_CONNECT means we are not running
            // as a Windows service (e.g. interactive terminal or cargo run).
            // Any other error is unexpected and worth noting in the diagnostic.
            dlog!("RegisterServiceCtrlHandlerExW failed after retry: {e}");
        }
    }
}
