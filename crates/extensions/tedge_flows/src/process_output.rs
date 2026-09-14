use crate::flow::Message;
use crate::flow::ProcessOutput;
use crate::flow::ProcessOutputFormat;
use camino::Utf8Path;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::Command;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::warn;

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

    #[error("cannot write to {command:?}: {error}")]
    CannotWrite {
        command: String,
        error: std::io::Error,
    },

    #[error("{command:?} exited with {status}")]
    Exited { command: String, status: ExitStatus },

    #[error("cannot write a payload containing a newline to {command:?} using the lines format")]
    NewlineInPayload { command: String },
}

impl ProcessOutputError {
    /// Whether a streaming process has to be restarted after this error
    pub fn requires_restart(&self) -> bool {
        matches!(
            self,
            ProcessOutputError::Timeout { .. }
                | ProcessOutputError::CannotWrite { .. }
                | ProcessOutputError::Exited { .. }
        )
    }
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

/// A long-running command to which the output messages of a flow are streamed
pub struct StreamingProcess {
    process: ProcessOutput,
    child: Child,
    stdin: ChildStdin,
}

impl StreamingProcess {
    /// Start the command, its standard output and error being logged
    pub fn spawn(flow: &Utf8Path, process: &ProcessOutput) -> Result<Self, ProcessOutputError> {
        let mut child = command(process)?
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| ProcessOutputError::CannotExecute {
                command: process.command.clone(),
                error,
            })?;
        let Some(stdin) = child.stdin.take() else {
            unreachable!("the stdin of the command is piped")
        };
        if let Some(stdout) = child.stdout.take() {
            log_output(flow, &process.command, stdout, false);
        }
        if let Some(stderr) = child.stderr.take() {
            log_output(flow, &process.command, stderr, true);
        }
        Ok(StreamingProcess {
            process: process.clone(),
            child,
            stdin,
        })
    }

    pub fn process(&self) -> &ProcessOutput {
        &self.process
    }

    /// Return the exit status of the command, if it has exited
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Write a message to the standard input of the command
    pub async fn write(&mut self, message: &Message) -> Result<(), ProcessOutputError> {
        let StreamingProcess {
            process,
            child,
            stdin,
        } = self;
        let format = process.format;
        if format == ProcessOutputFormat::Lines && message.payload.contains(&b'\n') {
            return Err(ProcessOutputError::NewlineInPayload {
                command: process.command.clone(),
            });
        }

        let write = async {
            stdin.write_all(&message.payload).await?;
            if format == ProcessOutputFormat::Lines {
                stdin.write_all(b"\n").await?;
            }
            stdin.flush().await
        };

        match tokio::time::timeout(process.timeout, write).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => match child.try_wait() {
                Ok(Some(status)) => Err(ProcessOutputError::Exited {
                    command: process.command.clone(),
                    status,
                }),
                _ => Err(ProcessOutputError::CannotWrite {
                    command: process.command.clone(),
                    error,
                }),
            },
            Err(_) => Err(ProcessOutputError::Timeout {
                command: process.command.clone(),
                timeout: process.timeout,
            }),
        }
    }

    /// Close the standard input of the command, letting it complete its work,
    /// and kill the command if it doesn't exit within the grace period
    pub fn close(self, grace_period: Duration) -> JoinHandle<()> {
        let StreamingProcess {
            process,
            mut child,
            stdin,
        } = self;
        drop(stdin);
        tokio::spawn(async move {
            let command = process.command;
            match tokio::time::timeout(grace_period, child.wait()).await {
                Ok(Ok(status)) if status.success() => {}
                Ok(Ok(status)) => warn!(target: "flows", "{command:?} exited with {status}"),
                Ok(Err(err)) => warn!(target: "flows", "cannot wait for {command:?}: {err}"),
                Err(_) => {
                    warn!(target: "flows", "{command:?} did not exit within {grace_period:?} after its input was closed, killing it");
                    let _ = child.kill().await;
                }
            }
        })
    }
}

/// Log the output of a streaming command, line by line
///
/// The output is read as bytes, so the command is never blocked by a non UTF-8 output
fn log_output(
    flow: &Utf8Path,
    command: &str,
    output: impl AsyncRead + Unpin + Send + 'static,
    is_stderr: bool,
) {
    let flow = flow.to_string();
    let command = command.to_string();
    tokio::spawn(async move {
        let mut reader = BufReader::new(output);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&line);
                    let text = text.trim_end();
                    if is_stderr {
                        warn!(target: "flows", "{flow}: {command:?}: {text}");
                    } else {
                        debug!(target: "flows", "{flow}: {command:?}: {text}");
                    }
                }
            }
        }
    });
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
    use crate::flow::ProcessOutputMode;
    use tempfile::TempDir;

    fn process_output(dir: &TempDir, command: &str, timeout: Duration) -> ProcessOutput {
        ProcessOutput {
            command: command.to_string(),
            cwd: Utf8Path::from_path(dir.path()).unwrap().to_path_buf(),
            timeout,
            mode: ProcessOutputMode::Oneshot,
            format: ProcessOutputFormat::Lines,
        }
    }

    fn streaming_output(
        dir: &TempDir,
        command: &str,
        format: ProcessOutputFormat,
        timeout: Duration,
    ) -> ProcessOutput {
        ProcessOutput {
            mode: ProcessOutputMode::Stream,
            format,
            ..process_output(dir, command, timeout)
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

    #[tokio::test]
    async fn streams_lines_to_a_single_command() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sh -c 'cat > stream.txt'",
            ProcessOutputFormat::Lines,
            Duration::from_secs(5),
        );
        let flow = Utf8Path::new("test.toml");

        let mut streaming = StreamingProcess::spawn(flow, &process).unwrap();
        streaming.write(&Message::new("test", "one")).await.unwrap();
        streaming.write(&Message::new("test", "two")).await.unwrap();
        streaming.close(Duration::from_secs(5)).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("stream.txt")).unwrap(),
            "one\ntwo\n"
        );
    }

    #[tokio::test]
    async fn streams_raw_payloads_to_a_single_command() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sh -c 'cat > stream.bin'",
            ProcessOutputFormat::Raw,
            Duration::from_secs(5),
        );
        let flow = Utf8Path::new("test.toml");

        let mut streaming = StreamingProcess::spawn(flow, &process).unwrap();
        streaming
            .write(&Message::new("test", vec![0x00, 0x0a]))
            .await
            .unwrap();
        streaming
            .write(&Message::new("test", vec![0xff]))
            .await
            .unwrap();
        streaming.close(Duration::from_secs(5)).await.unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("stream.bin")).unwrap(),
            vec![0x00, 0x0a, 0xff]
        );
    }

    #[tokio::test]
    async fn rejects_payloads_containing_a_newline_with_the_lines_format() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sh -c 'cat > stream.txt'",
            ProcessOutputFormat::Lines,
            Duration::from_secs(5),
        );
        let flow = Utf8Path::new("test.toml");

        let mut streaming = StreamingProcess::spawn(flow, &process).unwrap();
        let result = streaming.write(&Message::new("test", "one\ntwo")).await;
        let Err(error) = result else {
            panic!("a payload with a newline should be rejected");
        };
        assert!(!error.requires_restart());

        streaming
            .write(&Message::new("test", "three"))
            .await
            .unwrap();
        streaming.close(Duration::from_secs(5)).await.unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("stream.txt")).unwrap(),
            "three\n"
        );
    }

    #[tokio::test]
    async fn detects_a_streaming_command_which_has_exited() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sh -c 'exit 2'",
            ProcessOutputFormat::Lines,
            Duration::from_secs(5),
        );
        let flow = Utf8Path::new("test.toml");

        let mut streaming = StreamingProcess::spawn(flow, &process).unwrap();
        let mut status = None;
        for _ in 0..100 {
            status = streaming.exit_status();
            if status.is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(status.and_then(|status| status.code()), Some(2));

        let result = streaming.write(&Message::new("test", "hello")).await;
        assert!(matches!(result, Err(error) if error.requires_restart()));
    }

    #[tokio::test]
    async fn times_out_when_a_streaming_command_does_not_read_its_input() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sleep 10",
            ProcessOutputFormat::Raw,
            Duration::from_millis(200),
        );
        let flow = Utf8Path::new("test.toml");

        let mut streaming = StreamingProcess::spawn(flow, &process).unwrap();
        // Larger than a pipe buffer, so the write blocks
        let message = Message::new("test", vec![b'x'; 1024 * 1024]);
        let result = streaming.write(&message).await;

        assert!(matches!(result, Err(ProcessOutputError::Timeout { .. })));
    }

    #[tokio::test]
    async fn kills_a_streaming_command_which_does_not_exit_after_its_input_is_closed() {
        let dir = tempfile::tempdir().unwrap();
        let process = streaming_output(
            &dir,
            "sleep 10",
            ProcessOutputFormat::Lines,
            Duration::from_secs(5),
        );
        let flow = Utf8Path::new("test.toml");

        let streaming = StreamingProcess::spawn(flow, &process).unwrap();
        let start = std::time::Instant::now();
        streaming.close(Duration::from_millis(100)).await.unwrap();

        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn keeps_the_end_of_long_texts() {
        assert_eq!(tail("hello", 10), "hello");
        assert_eq!(tail("hello world", 5), "world");
        assert_eq!(tail("héllo", 4), "llo");
    }
}
