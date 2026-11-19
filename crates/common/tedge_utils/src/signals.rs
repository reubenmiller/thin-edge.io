use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

#[derive(Copy, Clone, Debug)]
pub enum Interruption {
    Timeout,
    Interrupted,
}

pub enum Signal {
    SIGTERM,
    SIGKILL,
}

struct TimeoutHandler {
    timeout: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl Future for TimeoutHandler {
    type Output = Option<()>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.timeout.as_mut() {
            Some(timeout) => timeout.as_mut().poll(cx).map(Some),
            None => Poll::Ready(None),
        }
    }
}

impl From<Option<tokio::time::Sleep>> for TimeoutHandler {
    fn from(timeout: Option<tokio::time::Sleep>) -> Self {
        TimeoutHandler {
            timeout: timeout.map(Box::pin),
        }
    }
}

#[cfg(unix)]
use tokio::signal::unix;

#[cfg(unix)]
pub struct TermSignals {
    sigint: SignalHandler,
    sigquit: SignalHandler,
    sigterm: SignalHandler,
    timeout: TimeoutHandler,
}

#[cfg(unix)]
impl TermSignals {
    pub fn new(timeout: Option<Duration>) -> TermSignals {
        let sigint = unix::signal(unix::SignalKind::interrupt())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGINT: {err}"))
            .ok()
            .into();
        let sigterm = unix::signal(unix::SignalKind::terminate())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGTERM: {err}"))
            .ok()
            .into();
        let sigquit = unix::signal(unix::SignalKind::quit())
            .map_err(|err| eprintln!("failed to set up signal handler for SIGQUIT: {err}"))
            .ok()
            .into();
        let timeout = timeout.map(tokio::time::sleep).into();

        TermSignals {
            sigint,
            sigquit,
            sigterm,
            timeout,
        }
    }

    pub async fn might_interrupt<F, O>(&mut self, future: F) -> Result<O, Interruption>
    where
        F: Future<Output = O>,
    {
        tokio::select! {
            Some(_) = &mut self.sigint => Err(Interruption::Interrupted),
            Some(_) = &mut self.sigterm => Err(Interruption::Interrupted),
            Some(_) = &mut self.sigquit => Err(Interruption::Interrupted),
            Some(_) = &mut self.timeout => Err(Interruption::Timeout),
            outcome = future => Ok(outcome),
        }
    }
}

#[cfg(unix)]
struct SignalHandler {
    signal: Option<unix::Signal>,
}

#[cfg(unix)]
impl Future for SignalHandler {
    type Output = Option<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.signal.as_mut() {
            Some(signal) => signal.poll_recv(cx),
            None => Poll::Ready(None),
        }
    }
}

#[cfg(unix)]
impl From<Option<unix::Signal>> for SignalHandler {
    fn from(signal: Option<unix::Signal>) -> Self {
        SignalHandler { signal }
    }
}

// On non-unix platforms (Windows), SIGTERM/SIGQUIT don't exist; ctrl_c is the
// only interactive termination signal.
#[cfg(not(unix))]
pub struct TermSignals {
    timeout: TimeoutHandler,
}

#[cfg(not(unix))]
impl TermSignals {
    pub fn new(timeout: Option<Duration>) -> TermSignals {
        let timeout = timeout.map(tokio::time::sleep).into();
        TermSignals { timeout }
    }

    pub async fn might_interrupt<F, O>(&mut self, future: F) -> Result<O, Interruption>
    where
        F: Future<Output = O>,
    {
        tokio::select! {
            _ = async { let _ = tokio::signal::ctrl_c().await; } => Err(Interruption::Interrupted),
            Some(_) = &mut self.timeout => Err(Interruption::Timeout),
            outcome = future => Ok(outcome),
        }
    }
}

/// Send a signal to all the processes of a process group.
///
/// On Windows, where there are no POSIX signals, the process tree rooted at `pgid` is terminated
/// with `taskkill /T`: `SIGTERM` asks the processes to close, `SIGKILL` forcefully terminates them (`taskkill /T /F`).
/// Only GUI applications can be asked to close: when `taskkill` reports that the processes
/// can only be terminated forcefully (e.g. console scripts), `SIGTERM` terminates them forcefully,
/// rather than leaving them running until the forceful timeout.
pub fn terminate_process_group(pgid: u32, signal_type: Signal) {
    #[cfg(unix)]
    {
        use nix::unistd::Pid;
        let pgid: Pid = Pid::from_raw(pgid as nix::libc::pid_t);
        let signal = match signal_type {
            Signal::SIGTERM => nix::sys::signal::SIGTERM,
            Signal::SIGKILL => nix::sys::signal::SIGKILL,
        };
        let _ = nix::sys::signal::killpg(pgid, signal);
    }
    #[cfg(windows)]
    {
        let taskkill = |force: bool| {
            let mut command = std::process::Command::new("taskkill");
            command.arg("/T");
            if force {
                command.arg("/F");
            }
            command
                .arg("/PID")
                .arg(pgid.to_string())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        };
        let force = matches!(signal_type, Signal::SIGKILL);
        if !taskkill(force) && !force {
            taskkill(true);
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pgid, signal_type);
    }
}

/// The exit code reported on Windows for a process terminated by thin-edge on timeout.
///
/// A process killed with `taskkill /F` exits with code 1, which cannot be told apart from a failure.
/// thin-edge reports such a process with this code instead: `STATUS_CONTROL_C_EXIT`,
/// the status of a process stopped by a termination request.
pub const WINDOWS_TERMINATED_EXIT_CODE: u32 = 0xC000_013A;

/// Returns the signal that terminated a process, if it has been killed.
///
/// On Unix, this is the POSIX signal.
/// On Windows, a process terminated by thin-edge on timeout (see [`WINDOWS_TERMINATED_EXIT_CODE`])
/// is reported as killed by `SIGKILL` (9).
pub fn killed_by_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        (status.code() == Some(WINDOWS_TERMINATED_EXIT_CODE as i32)).then_some(9)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn a_process_terminated_on_timeout_is_reported_as_killed() {
        use std::os::windows::process::ExitStatusExt;
        let terminated = std::process::ExitStatus::from_raw(WINDOWS_TERMINATED_EXIT_CODE);
        assert_eq!(killed_by_signal(&terminated), Some(9));
        assert_eq!(
            killed_by_signal(&std::process::ExitStatus::from_raw(1)),
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_process_killed_by_a_signal_is_reported_as_killed() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            killed_by_signal(&std::process::ExitStatus::from_raw(15)),
            Some(15)
        );
        assert_eq!(
            killed_by_signal(&std::process::ExitStatus::from_raw(1 << 8)),
            None
        );
    }
}
