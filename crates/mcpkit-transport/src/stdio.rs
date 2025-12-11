//! Standard I/O transport implementation.
//!
//! This module provides a transport that uses stdin/stdout for communication,
//! which is the most common transport for MCP servers that are launched
//! as subprocesses.
//!
//! # Wire Format
//!
//! Messages are newline-delimited JSON. Each JSON-RPC message is serialized
//! as a single line of JSON, followed by a newline character.
//!
//! # Runtime Support
//!
//! This transport is runtime-agnostic and works with:
//! - Tokio (`tokio-runtime` feature)
//! - async-std (`async-std-runtime` feature)
//! - smol (`smol-runtime` feature)
//!
//! # Example
//!
//! For subprocess communication, use [`SpawnedTransport`](crate::SpawnedTransport):
//!
//! ```no_run
//! use mcpkit_transport::SpawnedTransport;
//!
//! # async fn example() -> Result<(), mcpkit_transport::TransportError> {
//! // Spawn a server as a subprocess (uses stdio internally)
//! let transport = SpawnedTransport::spawn("my-mcp-server", &[] as &[&str]).await?;
//! # Ok(())
//! # }
//! ```
//!
//! For synchronous stdio, see [`SyncStdioTransport`].

use crate::error::TransportError;
use crate::runtime::{AsyncMutex, BufReader};
use crate::traits::{Transport, TransportMetadata};
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use mcpkit_core::protocol::Message;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum allowed message size (16 MB).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// A runtime-agnostic transport that uses stdin/stdout for communication.
///
/// This is typically used when the MCP server is launched as a subprocess
/// by the client. The server reads requests from stdin and writes responses
/// to stdout.
///
/// Logging and debug output should go to stderr to avoid interfering with
/// the protocol.
///
/// # Runtime Support
///
/// This transport works with any async runtime (Tokio, async-std, smol)
/// depending on which feature is enabled.
#[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime", feature = "smol-runtime"))]
pub struct StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    stdin: AsyncMutex<BufReader<R>>,
    stdout: AsyncMutex<W>,
    connected: AtomicBool,
    metadata: TransportMetadata,
}

#[cfg(feature = "tokio-runtime")]
impl StdioTransport<crate::runtime::TokioAsyncReadWrapper<tokio::io::Stdin>, crate::runtime::TokioAsyncWriteWrapper<tokio::io::Stdout>> {
    /// Create a new stdio transport using process stdin/stdout.
    #[must_use]
    pub fn new() -> Self {
        use crate::runtime::{TokioAsyncReadWrapper, TokioAsyncWriteWrapper};

        Self {
            stdin: AsyncMutex::new(BufReader::new(TokioAsyncReadWrapper(tokio::io::stdin()))),
            stdout: AsyncMutex::new(TokioAsyncWriteWrapper(tokio::io::stdout())),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("stdio")
                .remote_addr("stdin")
                .local_addr("stdout")
                .connected_now(),
        }
    }
}

#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
impl StdioTransport<async_std::io::Stdin, async_std::io::Stdout> {
    /// Create a new stdio transport using process stdin/stdout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdin: AsyncMutex::new(BufReader::new(async_std::io::stdin())),
            stdout: AsyncMutex::new(async_std::io::stdout()),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("stdio")
                .remote_addr("stdin")
                .local_addr("stdout")
                .connected_now(),
        }
    }
}

#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime"), not(feature = "async-std-runtime")))]
impl StdioTransport<smol::Unblock<std::io::Stdin>, smol::Unblock<std::io::Stdout>> {
    /// Create a new stdio transport using process stdin/stdout.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdin: AsyncMutex::new(BufReader::new(smol::Unblock::new(std::io::stdin()))),
            stdout: AsyncMutex::new(smol::Unblock::new(std::io::stdout())),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("stdio")
                .remote_addr("stdin")
                .local_addr("stdout")
                .connected_now(),
        }
    }
}

#[cfg(feature = "tokio-runtime")]
impl Default for StdioTransport<crate::runtime::TokioAsyncReadWrapper<tokio::io::Stdin>, crate::runtime::TokioAsyncWriteWrapper<tokio::io::Stdout>> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "async-std-runtime", not(feature = "tokio-runtime")))]
impl Default for StdioTransport<async_std::io::Stdin, async_std::io::Stdout> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(all(feature = "smol-runtime", not(feature = "tokio-runtime"), not(feature = "async-std-runtime")))]
impl Default for StdioTransport<smol::Unblock<std::io::Stdin>, smol::Unblock<std::io::Stdout>> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime", feature = "smol-runtime"))]
impl<R, W> StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send,
    W: AsyncWrite + Unpin + Send,
{
    /// Create a new stdio transport with custom readers/writers.
    ///
    /// This is useful for testing or when using non-standard I/O streams.
    #[must_use]
    pub fn with_streams(stdin: R, stdout: W) -> Self {
        Self {
            stdin: AsyncMutex::new(BufReader::new(stdin)),
            stdout: AsyncMutex::new(stdout),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("stdio")
                .remote_addr("custom")
                .local_addr("custom")
                .connected_now(),
        }
    }
}

#[cfg(any(feature = "tokio-runtime", feature = "async-std-runtime", feature = "smol-runtime"))]
impl<R, W> Transport for StdioTransport<R, W>
where
    R: AsyncRead + Unpin + Send + Sync,
    W: AsyncWrite + Unpin + Send + Sync,
{
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let json = serde_json::to_string(&msg)?;

        if json.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: json.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let mut stdout = self.stdout.lock().await;
        stdout.write_all(json.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;

        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let mut stdin = self.stdin.lock().await;

        loop {
            let mut line = String::new();
            let bytes_read = stdin.read_line(&mut line).await?;

            if bytes_read == 0 {
                // EOF - connection closed
                self.connected.store(false, Ordering::SeqCst);
                return Ok(None);
            }

            if line.len() > MAX_MESSAGE_SIZE {
                return Err(TransportError::MessageTooLarge {
                    size: line.len(),
                    max: MAX_MESSAGE_SIZE,
                });
            }

            // Trim the trailing newline
            let trimmed = line.trim();
            if trimmed.is_empty() {
                // Empty line, continue reading
                continue;
            }

            let msg: Message = serde_json::from_str(trimmed)?;
            return Ok(Some(msg));
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::SeqCst);
        // Flush any pending output
        let mut stdout = self.stdout.lock().await;
        stdout.flush().await?;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    fn metadata(&self) -> TransportMetadata {
        self.metadata.clone()
    }
}

// =============================================================================
// Synchronous Transport (no runtime required)
// =============================================================================

/// A synchronous stdio transport for blocking contexts.
///
/// This is useful when you don't want to use an async runtime.
/// It does not implement the `Transport` trait since that requires async.
pub struct SyncStdioTransport {
    stdin: std::sync::Mutex<std::io::BufReader<std::io::Stdin>>,
    stdout: std::sync::Mutex<std::io::Stdout>,
    connected: AtomicBool,
    metadata: TransportMetadata,
}

impl SyncStdioTransport {
    /// Create a new synchronous stdio transport.
    #[must_use]
    pub fn new() -> Self {
        Self {
            stdin: std::sync::Mutex::new(std::io::BufReader::new(std::io::stdin())),
            stdout: std::sync::Mutex::new(std::io::stdout()),
            connected: AtomicBool::new(true),
            metadata: TransportMetadata::new("stdio")
                .remote_addr("stdin")
                .local_addr("stdout")
                .connected_now(),
        }
    }

    /// Send a message synchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if the message could not be sent.
    pub fn send_sync(&self, msg: &Message) -> Result<(), TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let json = serde_json::to_string(msg)?;

        if json.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: json.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let mut stdout = self.stdout.lock().map_err(|_| {
            TransportError::Protocol {
                message: "stdout lock poisoned".to_string(),
            }
        })?;

        writeln!(stdout, "{json}")?;
        stdout.flush()?;

        Ok(())
    }

    /// Receive a message synchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if receiving failed.
    pub fn recv_sync(&self) -> Result<Option<Message>, TransportError> {
        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let mut line = String::new();
        let mut stdin = self.stdin.lock().map_err(|_| {
            TransportError::Protocol {
                message: "stdin lock poisoned".to_string(),
            }
        })?;

        let bytes_read = stdin.read_line(&mut line)?;

        if bytes_read == 0 {
            self.connected.store(false, Ordering::SeqCst);
            return Ok(None);
        }

        if line.len() > MAX_MESSAGE_SIZE {
            return Err(TransportError::MessageTooLarge {
                size: line.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            drop(stdin);
            return self.recv_sync();
        }

        let msg: Message = serde_json::from_str(trimmed)?;
        Ok(Some(msg))
    }

    /// Close the transport.
    pub fn close(&self) -> Result<(), TransportError> {
        self.connected.store(false, Ordering::SeqCst);
        if let Ok(mut stdout) = self.stdout.lock() {
            let _ = stdout.flush();
        }
        Ok(())
    }

    /// Check if the transport is connected.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Get transport metadata.
    #[must_use]
    pub fn metadata(&self) -> TransportMetadata {
        self.metadata.clone()
    }
}

impl Default for SyncStdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_transport_creation() {
        // Just verify we can create one - actual I/O would require mocking
        let transport = SyncStdioTransport::new();
        assert!(transport.is_connected());
        assert_eq!(transport.metadata().transport_type, "stdio");
    }

    #[test]
    fn test_max_message_size() {
        assert_eq!(MAX_MESSAGE_SIZE, 16 * 1024 * 1024);
    }
}
