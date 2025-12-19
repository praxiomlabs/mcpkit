//! Unix domain socket transport for MCP.
//!
//! This module provides Unix domain socket transport for local inter-process
//! communication. This is only available on Unix-like systems (Linux, macOS, BSDs).
//!
//! # Features
//!
//! - Low-latency local IPC
//! - File system-based addressing
//! - Abstract socket namespace support (Linux)
//! - Automatic cleanup of socket files
//! - Newline-delimited JSON message framing
//!
//! # Example
//!
//! ```rust
//! #[cfg(unix)]
//! fn example() {
//!     use mcpkit_transport::unix::UnixSocketConfig;
//!
//!     // Configure a Unix socket
//!     let config = UnixSocketConfig::new("/tmp/mcp.sock")
//!         .with_cleanup_on_close(true);
//!
//!     assert_eq!(config.path.to_str().unwrap(), "/tmp/mcp.sock");
//!     assert!(config.cleanup_on_close);
//! }
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportListener, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[cfg(feature = "tokio-runtime")]
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{UnixListener as TokioUnixListener, UnixStream},
};

/// Default maximum message size (16 MB).
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Configuration for Unix socket transport.
#[derive(Debug, Clone)]
pub struct UnixSocketConfig {
    /// Socket path.
    pub path: PathBuf,
    /// Whether to remove the socket file on close.
    pub cleanup_on_close: bool,
    /// Buffer size for reading.
    pub read_buffer_size: usize,
    /// Buffer size for writing.
    pub write_buffer_size: usize,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

impl UnixSocketConfig {
    /// Create a new Unix socket configuration.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            cleanup_on_close: true,
            read_buffer_size: 64 * 1024,  // 64 KB
            write_buffer_size: 64 * 1024, // 64 KB
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Set whether to cleanup the socket file on close.
    #[must_use]
    pub const fn with_cleanup_on_close(mut self, cleanup: bool) -> Self {
        self.cleanup_on_close = cleanup;
        self
    }

    /// Set the read buffer size.
    #[must_use]
    pub const fn with_read_buffer_size(mut self, size: usize) -> Self {
        self.read_buffer_size = size;
        self
    }

    /// Set the write buffer size.
    #[must_use]
    pub const fn with_write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }
}

/// Split Unix stream for reading.
#[cfg(feature = "tokio-runtime")]
type UnixReader = BufReader<tokio::net::unix::OwnedReadHalf>;

/// Split Unix stream for writing.
#[cfg(feature = "tokio-runtime")]
type UnixWriter = BufWriter<tokio::net::unix::OwnedWriteHalf>;

/// Internal state for Unix socket transport.
struct UnixTransportState {
    /// Reader half of the Unix stream.
    #[cfg(feature = "tokio-runtime")]
    reader: Option<UnixReader>,
    /// Writer half of the Unix stream.
    #[cfg(feature = "tokio-runtime")]
    writer: Option<UnixWriter>,
    /// Line buffer for reading complete messages.
    line_buffer: String,
}

/// Unix domain socket transport.
///
/// Provides low-latency local IPC using Unix domain sockets.
pub struct UnixTransport {
    config: UnixSocketConfig,
    state: AsyncMutex<UnixTransportState>,
    connected: AtomicBool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    is_server_side: bool,
}

impl UnixTransport {
    /// Create a new Unix socket transport from an existing stream.
    #[cfg(feature = "tokio-runtime")]
    fn from_stream(config: UnixSocketConfig, stream: UnixStream, is_server_side: bool) -> Self {
        let (read_half, write_half) = stream.into_split();
        let reader = BufReader::new(read_half);
        let writer = BufWriter::new(write_half);

        Self {
            state: AsyncMutex::new(UnixTransportState {
                reader: Some(reader),
                writer: Some(writer),
                line_buffer: String::with_capacity(4096),
            }),
            config,
            connected: AtomicBool::new(true),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            is_server_side,
        }
    }

    /// Create disconnected transport (for non-tokio runtimes or testing).
    #[cfg(not(feature = "tokio-runtime"))]
    fn new_disconnected(config: UnixSocketConfig, is_server_side: bool) -> Self {
        Self {
            state: AsyncMutex::new(UnixTransportState {
                line_buffer: String::with_capacity(4096),
            }),
            config,
            connected: AtomicBool::new(false),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            is_server_side,
        }
    }

    /// Connect to a Unix socket server.
    #[cfg(feature = "tokio-runtime")]
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let config = UnixSocketConfig::new(path);
        Self::connect_with_config(config).await
    }

    /// Connect to a Unix socket server (stub for non-tokio runtimes).
    #[cfg(not(feature = "tokio-runtime"))]
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Unix socket transport requires 'tokio-runtime' feature".to_string(),
        })
    }

    /// Connect with custom configuration.
    #[cfg(feature = "tokio-runtime")]
    pub async fn connect_with_config(config: UnixSocketConfig) -> Result<Self, TransportError> {
        let stream =
            UnixStream::connect(&config.path)
                .await
                .map_err(|e| TransportError::Connection {
                    message: format!(
                        "Failed to connect to Unix socket '{}': {}",
                        config.path.display(),
                        e
                    ),
                })?;

        tracing::debug!(path = %config.path.display(), "Connected to Unix socket");
        Ok(Self::from_stream(config, stream, false))
    }

    /// Connect with custom configuration (stub for non-tokio runtimes).
    #[cfg(not(feature = "tokio-runtime"))]
    pub async fn connect_with_config(config: UnixSocketConfig) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Unix socket transport requires 'tokio-runtime' feature".to_string(),
        })
    }

    /// Get the socket path.
    pub fn path(&self) -> &Path {
        &self.config.path
    }

    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }
}

impl Transport for UnixTransport {
    type Error = TransportError;

    #[cfg(feature = "tokio-runtime")]
    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if !self.connected.load(Ordering::Acquire) {
            return Err(TransportError::Connection {
                message: "Unix socket not connected".to_string(),
            });
        }

        // Serialize the message with newline delimiter
        let mut data = serde_json::to_vec(&msg).map_err(|e| TransportError::Serialization {
            message: format!("Failed to serialize message: {e}"),
        })?;

        // Check message size limit
        if data.len() > self.config.max_message_size {
            return Err(TransportError::MessageTooLarge {
                size: data.len(),
                max: self.config.max_message_size,
            });
        }

        data.push(b'\n');

        // Write to the socket
        let mut state = self.state.lock().await;
        if let Some(writer) = state.writer.as_mut() {
            writer
                .write_all(&data)
                .await
                .map_err(|e| TransportError::Io {
                    message: format!("Failed to write to Unix socket: {e}"),
                })?;
            writer.flush().await.map_err(|e| TransportError::Io {
                message: format!("Failed to flush Unix socket: {e}"),
            })?;
        } else {
            return Err(TransportError::Connection {
                message: "Unix socket writer not available".to_string(),
            });
        }

        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    #[cfg(not(feature = "tokio-runtime"))]
    async fn send(&self, _msg: Message) -> Result<(), Self::Error> {
        Err(TransportError::Connection {
            message: "Unix socket transport requires 'tokio-runtime' feature".to_string(),
        })
    }

    #[cfg(feature = "tokio-runtime")]
    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        if !self.connected.load(Ordering::Acquire) {
            return Ok(None);
        }

        let mut state = self.state.lock().await;

        // Take the reader temporarily to avoid borrowing issues
        let reader = match state.reader.take() {
            Some(r) => r,
            None => return Ok(None),
        };

        // Clear the buffer and read a line
        state.line_buffer.clear();

        // We need to read into a separate buffer to avoid borrow issues
        let (result, reader) = {
            let mut reader = reader;
            let result = reader.read_line(&mut state.line_buffer).await;
            (result, reader)
        };

        // Put the reader back
        state.reader = Some(reader);

        match result {
            Ok(0) => {
                // EOF - connection closed
                self.connected.store(false, Ordering::Release);
                Ok(None)
            }
            Ok(_) => {
                // Parse the message (trim the newline)
                let line = state.line_buffer.trim_end();
                if line.is_empty() {
                    return Ok(None);
                }

                // Check message size limit
                if line.len() > self.config.max_message_size {
                    return Err(TransportError::MessageTooLarge {
                        size: line.len(),
                        max: self.config.max_message_size,
                    });
                }

                let msg: Message =
                    serde_json::from_str(line).map_err(|e| TransportError::Deserialization {
                        message: format!("Failed to deserialize message: {e}"),
                    })?;

                self.messages_received.fetch_add(1, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Err(e) => {
                self.connected.store(false, Ordering::Release);
                Err(TransportError::Io {
                    message: format!("Failed to read from Unix socket: {e}"),
                })
            }
        }
    }

    #[cfg(not(feature = "tokio-runtime"))]
    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        Ok(None)
    }

    #[cfg(feature = "tokio-runtime")]
    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::Release);

        // Drop the stream parts
        let mut state = self.state.lock().await;
        state.reader = None;
        state.writer = None;

        // Cleanup socket file if this is server-side and cleanup is enabled
        if self.is_server_side && self.config.cleanup_on_close && self.config.path.exists() {
            let _ = std::fs::remove_file(&self.config.path);
        }

        Ok(())
    }

    #[cfg(not(feature = "tokio-runtime"))]
    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::Release);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("unix").remote_addr(self.config.path.display().to_string())
    }
}

/// Unix domain socket listener.
///
/// Listens for incoming connections on a Unix domain socket.
pub struct UnixListener {
    config: UnixSocketConfig,
    #[cfg(feature = "tokio-runtime")]
    listener: AsyncMutex<Option<TokioUnixListener>>,
    running: AtomicBool,
}

impl UnixListener {
    /// Bind to a Unix socket path.
    pub async fn bind(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let config = UnixSocketConfig::new(path);
        Self::bind_with_config(config).await
    }

    /// Bind with custom configuration.
    #[cfg(feature = "tokio-runtime")]
    pub async fn bind_with_config(config: UnixSocketConfig) -> Result<Self, TransportError> {
        // Remove existing socket file if it exists
        if config.path.exists() {
            std::fs::remove_file(&config.path).map_err(|e| TransportError::Io {
                message: format!("Failed to remove existing socket file: {e}"),
            })?;
        }

        // Bind the socket
        let listener =
            TokioUnixListener::bind(&config.path).map_err(|e| TransportError::Connection {
                message: format!(
                    "Failed to bind Unix socket '{}': {}",
                    config.path.display(),
                    e
                ),
            })?;

        tracing::info!(path = %config.path.display(), "Unix socket listener bound");

        Ok(Self {
            config,
            listener: AsyncMutex::new(Some(listener)),
            running: AtomicBool::new(true),
        })
    }

    /// Bind with custom configuration (stub for non-tokio runtimes).
    #[cfg(not(feature = "tokio-runtime"))]
    pub async fn bind_with_config(config: UnixSocketConfig) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Unix socket listener requires 'tokio-runtime' feature".to_string(),
        })
    }

    /// Get the socket path.
    pub fn path(&self) -> &Path {
        &self.config.path
    }

    /// Check if the listener is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Stop the listener.
    #[cfg(feature = "tokio-runtime")]
    pub async fn stop(&self) {
        self.running.store(false, Ordering::Release);
        // Drop the listener
        let mut guard = self.listener.lock().await;
        *guard = None;
    }

    /// Stop the listener (non-tokio version).
    #[cfg(not(feature = "tokio-runtime"))]
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

impl TransportListener for UnixListener {
    type Transport = UnixTransport;
    type Error = TransportError;

    #[cfg(feature = "tokio-runtime")]
    async fn accept(&self) -> Result<Self::Transport, Self::Error> {
        if !self.running.load(Ordering::Acquire) {
            return Err(TransportError::Connection {
                message: "Listener not running".to_string(),
            });
        }

        let mut guard = self.listener.lock().await;
        if let Some(listener) = guard.as_mut() {
            let (stream, addr) =
                listener
                    .accept()
                    .await
                    .map_err(|e| TransportError::Connection {
                        message: format!("Failed to accept connection: {e}"),
                    })?;

            tracing::debug!(addr = ?addr, "Accepted Unix socket connection");

            Ok(UnixTransport::from_stream(
                self.config.clone(),
                stream,
                true,
            ))
        } else {
            Err(TransportError::Connection {
                message: "Listener has been stopped".to_string(),
            })
        }
    }

    #[cfg(not(feature = "tokio-runtime"))]
    async fn accept(&self) -> Result<Self::Transport, Self::Error> {
        Err(TransportError::Connection {
            message: "Unix socket listener requires 'tokio-runtime' feature".to_string(),
        })
    }

    fn local_addr(&self) -> Option<String> {
        Some(self.config.path.display().to_string())
    }
}

impl Drop for UnixListener {
    fn drop(&mut self) {
        if self.config.cleanup_on_close && self.config.path.exists() {
            let _ = std::fs::remove_file(&self.config.path);
        }
    }
}

/// Abstract Unix socket address (Linux-only).
///
/// Abstract sockets don't create files in the filesystem and are
/// automatically cleaned up when all references are closed.
#[cfg(target_os = "linux")]
pub struct AbstractSocket {
    name: String,
}

#[cfg(target_os = "linux")]
impl AbstractSocket {
    /// Create a new abstract socket name.
    ///
    /// The name should not start with a null byte; this is added automatically.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// Get the socket name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Convert to a socket path for use with standard Unix socket APIs.
    ///
    /// Returns a path starting with a null byte to indicate an abstract socket.
    #[must_use]
    pub fn to_path(&self) -> Vec<u8> {
        let mut path = vec![0u8];
        path.extend_from_slice(self.name.as_bytes());
        path
    }
}

/// Builder for Unix socket transport.
pub struct UnixTransportBuilder {
    config: UnixSocketConfig,
}

impl UnixTransportBuilder {
    /// Create a new builder with the given socket path.
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            config: UnixSocketConfig::new(path),
        }
    }

    /// Set whether to cleanup the socket file on close.
    #[must_use]
    pub const fn cleanup_on_close(mut self, cleanup: bool) -> Self {
        self.config.cleanup_on_close = cleanup;
        self
    }

    /// Set the read buffer size.
    #[must_use]
    pub const fn read_buffer_size(mut self, size: usize) -> Self {
        self.config.read_buffer_size = size;
        self
    }

    /// Set the write buffer size.
    #[must_use]
    pub const fn write_buffer_size(mut self, size: usize) -> Self {
        self.config.write_buffer_size = size;
        self
    }

    /// Connect to the socket.
    pub async fn connect(self) -> Result<UnixTransport, TransportError> {
        UnixTransport::connect_with_config(self.config).await
    }

    /// Create a listener on the socket.
    pub async fn listen(self) -> Result<UnixListener, TransportError> {
        UnixListener::bind_with_config(self.config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = UnixSocketConfig::new("/tmp/test.sock")
            .with_cleanup_on_close(false)
            .with_read_buffer_size(128 * 1024);

        assert_eq!(config.path, PathBuf::from("/tmp/test.sock"));
        assert!(!config.cleanup_on_close);
        assert_eq!(config.read_buffer_size, 128 * 1024);
    }

    #[test]
    fn test_builder() {
        let builder = UnixTransportBuilder::new("/tmp/mcp.sock")
            .cleanup_on_close(true)
            .read_buffer_size(32 * 1024)
            .write_buffer_size(32 * 1024);

        assert_eq!(builder.config.read_buffer_size, 32 * 1024);
        assert_eq!(builder.config.write_buffer_size, 32 * 1024);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_abstract_socket() {
        let socket = AbstractSocket::new("mcp-test");
        assert_eq!(socket.name(), "mcp-test");

        let path = socket.to_path();
        assert_eq!(path[0], 0u8);
        assert_eq!(&path[1..], b"mcp-test");
    }

    /// Integration test: Test Unix socket client-server communication.
    #[cfg(feature = "tokio-runtime")]
    #[tokio::test]
    async fn test_unix_socket_communication() -> Result<(), Box<dyn std::error::Error>> {
        use mcpkit_core::protocol::Request;
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let socket_path = format!("/tmp/mcp-test-{}.sock", std::process::id());

        // Clean up any existing socket
        let _ = std::fs::remove_file(&socket_path);

        // Create server listener
        let listener = UnixListener::bind(&socket_path).await?;
        assert!(listener.is_running());

        // Use a barrier to synchronize
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        let socket_path_clone = socket_path.clone();

        // Server task
        let server_handle = tokio::spawn(async move {
            // Wait for client to be ready
            barrier_clone.wait().await;

            // Accept connection
            let transport = listener.accept().await.unwrap();
            assert!(transport.is_connected());

            // Receive message
            let msg = transport.recv().await.unwrap();
            assert!(msg.is_some());

            // Echo it back
            if let Some(m) = msg {
                transport.send(m).await.unwrap();
            }

            transport.close().await.unwrap();
        });

        // Client task
        let client_handle = tokio::spawn(async move {
            // Signal we're ready
            barrier.wait().await;

            // Give server time to start accepting
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            // Connect
            let transport = UnixTransport::connect(&socket_path_clone).await.unwrap();
            assert!(transport.is_connected());

            // Send a message
            let request = Request::new("test/echo", 1);
            let msg = Message::Request(request);
            transport.send(msg.clone()).await.unwrap();

            // Receive echo
            let response = transport.recv().await.unwrap();
            assert!(response.is_some());

            transport.close().await.unwrap();
        });

        // Wait for both tasks
        let (server_result, client_result) = tokio::join!(server_handle, client_handle);
        server_result?;
        client_result?;

        // Clean up socket file
        let _ = std::fs::remove_file(&socket_path);
        Ok(())
    }
}
