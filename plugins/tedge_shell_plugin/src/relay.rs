//! Relay the output of a command from a pipe to a file, storing only the head of the output.
//!
//! The command writes to a pipe, never to the file directly,
//! so the output stored on disk is bounded whatever the command prints.
//! The pipe is read until the command exits, the output beyond the limit being discarded,
//! so the command runs to completion, unaware that its output is truncated.

use crate::OutputFile;
use camino::Utf8PathBuf;
use std::fs::File;
use std::io::ErrorKind;
use std::io::PipeReader;
use std::io::Write;
use std::time::Duration;
use std::time::Instant;

/// The size of the chunks read from the pipe
const CHUNK_SIZE: usize = 64 * 1024;

/// The most a pipe can hold, using the default maximum pipe size of Linux
///
/// Bounds what is read when the pipe is drained,
/// so a process still writing to the pipe cannot keep the relay draining forever.
const MAX_PIPE_CAPACITY: usize = 1024 * 1024;

/// How often the relay checks for a flush request
///
/// Bounds the file system checks made while relaying the output of a chatty command,
/// which is otherwise read chunk after chunk with no pause.
const FLUSH_CHECK_INTERVAL: Duration = Duration::from_millis(50);

pub(crate) struct OutputRelay {
    /// The pipe the command writes to, `None` once closed by all the writers
    pipe: Option<Pipe>,

    head: Head,

    /// A file created by a process asking for the output relayed so far to be persisted
    flush_request: Option<Utf8PathBuf>,

    /// When the flush request is next checked
    next_flush_check: Instant,
}

impl OutputRelay {
    pub fn new(pipe: PipeReader, output: OutputFile, max_output_size: u32) -> Self {
        OutputRelay {
            pipe: Some(Pipe::new(pipe)),
            head: Head {
                file: output.file,
                // One byte more than what is reported, telling the output has been truncated,
                // even when read by another process, as for an interrupted command
                remaining: u64::from(max_output_size) + 1,
            },
            flush_request: output.flush_request,
            next_flush_check: Instant::now(),
        }
    }

    /// Relay the output of the command, returning as soon as some output has been relayed,
    /// or after `timeout` if there is none
    pub fn relay(&mut self, timeout: Duration) -> std::io::Result<()> {
        let now = Instant::now();
        if now >= self.next_flush_check {
            self.next_flush_check = now + FLUSH_CHECK_INTERVAL;
            self.serve_flush_request()?;
        }
        self.read_available(timeout)?;
        Ok(())
    }

    /// Relay the output left in the pipe, and return the file the output has been stored into
    ///
    /// Called once the command has exited.
    /// The pipe is not read to its end, as a background process started by the command
    /// might keep it open: such a process gets `SIGPIPE` if it writes to its output afterwards.
    pub fn finish(mut self) -> std::io::Result<File> {
        self.serve_flush_request()?;
        self.drain()?;
        Ok(self.head.file)
    }

    /// Persist the output relayed so far, if requested, e.g. before a reboot
    ///
    /// The request file is removed to tell the requester it has been served.
    fn serve_flush_request(&mut self) -> std::io::Result<()> {
        match &self.flush_request {
            Some(request) if request.exists() => (),
            _ => return Ok(()),
        }
        self.drain()?;
        self.head.file.sync_data()?;
        if let Some(request) = &self.flush_request {
            match std::fs::remove_file(request) {
                Err(err) if err.kind() != ErrorKind::NotFound => return Err(err),
                _ => (),
            }
        }
        Ok(())
    }

    /// Relay what has already been written to the pipe
    fn drain(&mut self) -> std::io::Result<()> {
        let mut drained = 0;
        while drained < MAX_PIPE_CAPACITY {
            match self.read_available(Duration::ZERO)? {
                0 => break,
                n => drained += n,
            }
        }
        Ok(())
    }

    /// Relay one chunk of output, waiting at most `timeout` for some output
    ///
    /// Return the number of bytes read, 0 if none has been written in time or the pipe is closed.
    fn read_available(&mut self, timeout: Duration) -> std::io::Result<usize> {
        let Some(pipe) = &mut self.pipe else {
            std::thread::sleep(timeout);
            return Ok(0);
        };

        match pipe.read_chunk(timeout)? {
            Chunk::Closed => {
                self.pipe = None;
                Ok(0)
            }
            Chunk::None => Ok(0),
            Chunk::Data(data) => {
                self.head.store(data);
                Ok(data.len())
            }
        }
    }
}

/// What has been read from the pipe
enum Chunk<'a> {
    /// Nothing has been written in time
    None,
    /// The pipe has been closed by all the writers
    Closed,
    Data(&'a [u8]),
}

/// The read end of the pipe, waited on with `poll`
#[cfg(unix)]
struct Pipe {
    reader: PipeReader,
    buffer: Vec<u8>,
}

#[cfg(unix)]
impl Pipe {
    fn new(reader: PipeReader) -> Self {
        Pipe {
            reader,
            buffer: vec![0; CHUNK_SIZE],
        }
    }

    fn read_chunk(&mut self, timeout: Duration) -> std::io::Result<Chunk<'_>> {
        use nix::errno::Errno;
        use nix::poll::poll;
        use nix::poll::PollFd;
        use nix::poll::PollFlags;
        use nix::poll::PollTimeout;
        use std::io::Read;
        use std::os::fd::AsFd;

        let timeout = PollTimeout::try_from(timeout).unwrap_or(PollTimeout::MAX);
        match poll(
            &mut [PollFd::new(self.reader.as_fd(), PollFlags::POLLIN)],
            timeout,
        ) {
            Ok(0) | Err(Errno::EINTR) => return Ok(Chunk::None),
            Ok(_) => (),
            Err(err) => return Err(err.into()),
        }

        // The pipe being ready, this read does not block
        match self.reader.read(&mut self.buffer) {
            Ok(0) => Ok(Chunk::Closed),
            Ok(n) => Ok(Chunk::Data(&self.buffer[..n])),
            Err(err) if err.kind() == ErrorKind::Interrupted => Ok(Chunk::None),
            Err(err) => Err(err),
        }
    }
}

/// The read end of the pipe, read by a thread
///
/// Windows has no `poll` for anonymous pipes, so a thread blocks reading the pipe
/// and passes the chunks over a channel, which can be waited on with a timeout.
/// The thread ends when the pipe is closed by all the writers. A background process
/// started by the command can keep the pipe open: the thread then lives until this process exits.
#[cfg(windows)]
struct Pipe {
    chunks: std::sync::mpsc::Receiver<std::io::Result<Vec<u8>>>,
    current: Vec<u8>,
}

#[cfg(windows)]
impl Pipe {
    fn new(mut reader: PipeReader) -> Self {
        use std::io::Read;

        let (sender, chunks) = std::sync::mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut buffer = vec![0; CHUNK_SIZE];
            loop {
                let chunk = match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(n) => Ok(buffer[..n].to_vec()),
                    Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                    // A pipe closed by all the writers is reported as broken on Windows
                    Err(err) if err.kind() == ErrorKind::BrokenPipe => return,
                    Err(err) => Err(err),
                };
                if sender.send(chunk).is_err() {
                    return;
                }
            }
        });
        Pipe {
            chunks,
            current: Vec::new(),
        }
    }

    fn read_chunk(&mut self, timeout: Duration) -> std::io::Result<Chunk<'_>> {
        use std::sync::mpsc::RecvTimeoutError;
        use std::sync::mpsc::TryRecvError;

        let received = if timeout.is_zero() {
            self.chunks.try_recv().map_err(|err| match err {
                TryRecvError::Empty => RecvTimeoutError::Timeout,
                TryRecvError::Disconnected => RecvTimeoutError::Disconnected,
            })
        } else {
            self.chunks.recv_timeout(timeout)
        };
        match received {
            Ok(Ok(data)) => {
                self.current = data;
                Ok(Chunk::Data(&self.current))
            }
            Ok(Err(err)) => Err(err),
            Err(RecvTimeoutError::Timeout) => Ok(Chunk::None),
            Err(RecvTimeoutError::Disconnected) => Ok(Chunk::Closed),
        }
    }
}

/// Stores the head of the output, discarding the rest
struct Head {
    file: File,
    /// How many more bytes are stored
    remaining: u64,
}

impl Head {
    fn store(&mut self, chunk: &[u8]) {
        let len = chunk
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        if len == 0 {
            return;
        }
        match self.file.write_all(&chunk[..len]) {
            Ok(()) => self.remaining -= len as u64,
            Err(err) => {
                // Not failing the command, which still runs to completion, only with no more output stored
                tracing::warn!("Failed to store the command output, discarding the rest: {err}");
                self.remaining = 0;
            }
        }
    }
}
