//! thin-edge.io PKCS#11 server.
//!
//! The purpose of this crate is to allow thin-edge services that possibly run in containers to access PKCS#11 tokens in
//! all of our supported architectures.
//!
//! This server uses Unix domain sockets and is therefore only available on Unix platforms.
//! Named-pipe support for Windows is a future enhancement.

#[cfg(unix)]
mod unix;

#[cfg(unix)]
fn main() -> anyhow::Result<()> {
    unix::main()
}

#[cfg(not(unix))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!(
        "tedge-p11-server is not supported on this platform: it relies on Unix domain sockets"
    )
}
