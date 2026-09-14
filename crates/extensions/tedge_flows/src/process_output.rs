use crate::flow::Message;
use crate::flow::ProcessOutput;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Maximum number of bytes of stderr included in an error
const MAX_STDERR_LEN: usize = 1024;

#[derive(thiserror::Error, Debug)]
pub enum ProcessOutputError {
    #[error("invalid command {command:?}: {reason}")]
    InvalidCommand { command: String, reason: String },

    #[error("cannot execute {command:?}: {error}")]
    CannotExecute {
        command: String,
        error: std::io::Error,
    },

    #[error("{command:?} did not complete within {timeout:?}")]
    Timeout { command: String, timeout: Duration },

    #[error("{command:?} failed with {status}: {stderr}")]
    Failed {
        command: String,
        status: ExitStatus,
        stderr: String,
    },
}

/// Build the command of a process output, without using a shell
fn command(process: &ProcessOutput) -> Result<Command, ProcessOutputError> {
    let invalid = |reason: String| ProcessOutputError::InvalidCommand {
        command: process.command.clone(),
        reason,
    };
    let args = shell_words::split(&process.command).map_err(|err| invalid(err.to_string()))?;
    let Some((program, args)) = args.split_first() else {
        return Err(invalid("empty command".to_string()));
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&process.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    Ok(command)
}

/// Execute the command of a process output for a single message
///
/// The message payload is written to the stdin of the command, and the message topic
/// is available in the `TEDGE_FLOW_TOPIC` environment variable.
/// The command is killed if it doesn't complete within the configured timeout.
pub async fn execute_process_output(
    process: &ProcessOutput,
    message: &Message,
) -> Result<(), ProcessOutputError> {
    let mut child = command(process)?
        .env("TEDGE_FLOW_TOPIC", &message.topic)
        .spawn()
        .map_err(|error| ProcessOutputError::CannotExecute {
            command: process.command.clone(),
            error,
        })?;

    let stdin = child.stdin.take();
    let write_payload = async move {
        if let Some(mut stdin) = stdin {
            // A command can exit without reading its input, which is not considered as an error
            let _ = stdin.write_all(&message.payload).await;
            // stdin is closed when dropped, notifying the end of the input to the command
        }
    };
    let run = async {
        let ((), output) = tokio::join!(write_payload, child.wait_with_output());
        output
    };

    // On timeout, the child process is dropped and killed
    let output = match tokio::time::timeout(process.timeout, run).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(ProcessOutputError::CannotExecute {
                command: process.command.clone(),
                error,
            })
        }
        Err(_) => {
            return Err(ProcessOutputError::Timeout {
                command: process.command.clone(),
                timeout: process.timeout,
            })
        }
    };

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(ProcessOutputError::Failed {
            command: process.command.clone(),
            status: output.status,
            stderr: tail(stderr.trim(), MAX_STDERR_LEN).to_string(),
        })
    }
}

/// Return the last bytes of a text, at a char boundary
fn tail(text: &str, max_len: usize) -> &str {
    if text.len() <= max_len {
        return text;
    }
    let mut start = text.len() - max_len;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use camino::Utf8Path;
    use tempfile::TempDir;

    fn process_output(dir: &TempDir, command: &str, timeout: Duration) -> ProcessOutput {
        ProcessOutput {
            command: command.to_string(),
            cwd: Utf8Path::from_path(dir.path()).unwrap().to_path_buf(),
            timeout,
        }
    }

    #[tokio::test]
    async fn passes_the_message_to_the_command() {
        let dir = tempfile::tempdir().unwrap();
        let process = process_output(
            &dir,
            r#"sh -c 'cat > payload.bin; printf %s "$TEDGE_FLOW_TOPIC" > topic.txt'"#,
            Duration::from_secs(5),
        );
        let message = Message::new("test/output", vec![0x00, 0xff, 0x0a]);

        execute_process_output(&process, &message).await.unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("payload.bin")).unwrap(),
            vec![0x00, 0xff, 0x0a]
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("topic.txt")).unwrap(),
            "test/output"
        );
    }

    #[tokio::test]
    async fn reports_the_exit_status_and_stderr_of_a_failing_command() {
        let dir = tempfile::tempdir().unwrap();
        let process = process_output(
            &dir,
            "sh -c 'echo something went wrong >&2; exit 3'",
            Duration::from_secs(5),
        );

        let result = execute_process_output(&process, &Message::new("test", "")).await;

        let Err(ProcessOutputError::Failed { status, stderr, .. }) = result else {
            panic!("expected a failure, got {result:?}");
        };
        assert_eq!(status.code(), Some(3));
        assert_eq!(stderr, "something went wrong");
    }

    #[tokio::test]
    async fn kills_a_command_which_does_not_complete_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let process = process_output(&dir, "sleep 10", Duration::from_millis(100));

        let start = std::time::Instant::now();
        let result = execute_process_output(&process, &Message::new("test", "")).await;

        assert!(matches!(result, Err(ProcessOutputError::Timeout { .. })));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_command_does_not_have_to_read_its_input() {
        let dir = tempfile::tempdir().unwrap();
        let process = process_output(&dir, "true", Duration::from_secs(5));
        let message = Message::new("test", vec![b'x'; 1024 * 1024]);

        execute_process_output(&process, &message).await.unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_commands() {
        let dir = tempfile::tempdir().unwrap();
        let message = Message::new("test", "");

        for command in ["", "unterminated 'quote"] {
            let process = process_output(&dir, command, Duration::from_secs(5));
            let result = execute_process_output(&process, &message).await;
            assert!(
                matches!(result, Err(ProcessOutputError::InvalidCommand { .. })),
                "{command:?} should be rejected"
            );
        }

        let process = process_output(&dir, "/does/not/exist", Duration::from_secs(5));
        let result = execute_process_output(&process, &message).await;
        assert!(matches!(
            result,
            Err(ProcessOutputError::CannotExecute { .. })
        ));
    }

    #[test]
    fn keeps_the_end_of_long_texts() {
        assert_eq!(tail("hello", 10), "hello");
        assert_eq!(tail("hello world", 5), "world");
        assert_eq!(tail("héllo", 4), "llo");
    }
}
