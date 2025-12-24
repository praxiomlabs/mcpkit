//! Windows named pipe transport for MCP.
//!
//! This module provides Windows named pipe transport for local inter-process
//! communication on Windows systems.
//!
//! # Features
//!
//! - Low-latency local IPC on Windows
//! - Named pipe addressing (e.g., `\\.\pipe\mcp-server`)
//! - Multiple client support
//! - Newline-delimited JSON message framing
//!
//! # Example
//!
//! ```rust
//! #[cfg(windows)]
//! fn example() {
//!     use mcpkit_transport::windows::NamedPipeConfig;
//!
//!     // Configure a named pipe
//!     let config = NamedPipeConfig::new(r"\\.\pipe\mcp-server")
//!         .with_max_instances(5);
//!
//!     assert!(config.name.contains("mcp-server"));
//!     assert_eq!(config.max_instances, 5);
//! }
//! ```
//!
//! # Pipe Naming Convention
//!
//! Windows named pipes follow the format `\\.\pipe\<name>`. The SDK
//! accepts pipe names with or without the `\\.\pipe\` prefix:
//!
//! ```rust
//! #[cfg(windows)]
//! fn example() {
//!     use mcpkit_transport::windows::NamedPipeConfig;
//!
//!     // Both of these create the same pipe
//!     let config1 = NamedPipeConfig::new("mcp-server");
//!     let config2 = NamedPipeConfig::new(r"\\.\pipe\mcp-server");
//!
//!     assert!(config1.name.contains("mcp-server"));
//!     assert!(config2.name.contains("mcp-server"));
//! }
//! ```

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportListener, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Default maximum message size (16 MB).
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Default maximum number of pipe instances.
pub const DEFAULT_MAX_INSTANCES: u32 = 16;

/// Default buffer size.
pub const DEFAULT_BUFFER_SIZE: usize = 64 * 1024;

/// Configuration for Windows named pipe transport.
#[derive(Debug, Clone)]
pub struct NamedPipeConfig {
    /// The pipe name (e.g., `\\.\pipe\mcp-server`).
    pub name: String,
    /// Maximum number of pipe instances for the server.
    pub max_instances: u32,
    /// Input buffer size.
    pub in_buffer_size: usize,
    /// Output buffer size.
    pub out_buffer_size: usize,
    /// Maximum message size in bytes.
    pub max_message_size: usize,
}

impl NamedPipeConfig {
    /// Create a new named pipe configuration.
    ///
    /// If the name doesn't start with `\\.\pipe\`, it will be prefixed.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let full_name = if name.starts_with(r"\\.\pipe\") {
            name
        } else {
            format!(r"\\.\pipe\{name}")
        };

        Self {
            name: full_name,
            max_instances: DEFAULT_MAX_INSTANCES,
            in_buffer_size: DEFAULT_BUFFER_SIZE,
            out_buffer_size: DEFAULT_BUFFER_SIZE,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    /// Set the maximum number of pipe instances.
    #[must_use]
    pub const fn with_max_instances(mut self, max: u32) -> Self {
        self.max_instances = max;
        self
    }

    /// Set the input buffer size.
    #[must_use]
    pub const fn with_in_buffer_size(mut self, size: usize) -> Self {
        self.in_buffer_size = size;
        self
    }

    /// Set the output buffer size.
    #[must_use]
    pub const fn with_out_buffer_size(mut self, size: usize) -> Self {
        self.out_buffer_size = size;
        self
    }

    /// Set the maximum message size.
    #[must_use]
    pub const fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    /// Get the short name (without the `\\.\pipe\` prefix).
    #[must_use]
    pub fn short_name(&self) -> &str {
        self.name.strip_prefix(r"\\.\pipe\").unwrap_or(&self.name)
    }
}

/// Internal state for reading/writing.
struct NamedPipeState {
    /// Line buffer for reading complete messages.
    line_buffer: String,
    /// Read buffer for accumulating bytes.
    read_buffer: Vec<u8>,
}

/// Windows named pipe transport.
///
/// Provides low-latency local IPC using Windows named pipes.
#[cfg(all(windows, feature = "tokio-runtime"))]
pub struct NamedPipeTransport {
    config: NamedPipeConfig,
    pipe: AsyncMutex<Option<tokio::net::windows::named_pipe::NamedPipeClient>>,
    server_pipe: AsyncMutex<Option<tokio::net::windows::named_pipe::NamedPipeServer>>,
    state: AsyncMutex<NamedPipeState>,
    connected: AtomicBool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
    is_server_side: bool,
}

#[cfg(all(windows, feature = "tokio-runtime"))]
impl NamedPipeTransport {
    /// Create a new client transport from a connected pipe.
    fn from_client(
        config: NamedPipeConfig,
        pipe: tokio::net::windows::named_pipe::NamedPipeClient,
    ) -> Self {
        Self {
            config,
            pipe: AsyncMutex::new(Some(pipe)),
            server_pipe: AsyncMutex::new(None),
            state: AsyncMutex::new(NamedPipeState {
                line_buffer: String::with_capacity(4096),
                read_buffer: Vec::with_capacity(8192),
            }),
            connected: AtomicBool::new(true),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            is_server_side: false,
        }
    }

    /// Create a new server transport from an accepted pipe.
    fn from_server(
        config: NamedPipeConfig,
        pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    ) -> Self {
        Self {
            config,
            pipe: AsyncMutex::new(None),
            server_pipe: AsyncMutex::new(Some(pipe)),
            state: AsyncMutex::new(NamedPipeState {
                line_buffer: String::with_capacity(4096),
                read_buffer: Vec::with_capacity(8192),
            }),
            connected: AtomicBool::new(true),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            is_server_side: true,
        }
    }

    /// Connect to a named pipe server.
    pub async fn connect(name: impl Into<String>) -> Result<Self, TransportError> {
        let config = NamedPipeConfig::new(name);
        Self::connect_with_config(config).await
    }

    /// Connect with custom configuration.
    pub async fn connect_with_config(config: NamedPipeConfig) -> Result<Self, TransportError> {
        use tokio::net::windows::named_pipe::ClientOptions;

        // Try to connect to the pipe
        let pipe =
            ClientOptions::new()
                .open(&config.name)
                .map_err(|e| TransportError::Connection {
                    message: format!("Failed to connect to named pipe '{}': {}", config.name, e),
                })?;

        tracing::debug!(name = %config.name, "Connected to named pipe");
        Ok(Self::from_client(config, pipe))
    }

    /// Get the pipe name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Internal send implementation.
    async fn send_impl(&self, msg: Message) -> Result<(), TransportError> {
        use tokio::io::AsyncWriteExt;

        if !self.connected.load(Ordering::Acquire) {
            return Err(TransportError::Connection {
                message: "Named pipe not connected".to_string(),
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

        // Write to the appropriate pipe
        if self.is_server_side {
            let mut guard = self.server_pipe.lock().await;
            if let Some(pipe) = guard.as_mut() {
                pipe.write_all(&data)
                    .await
                    .map_err(|e| TransportError::Io {
                        message: format!("Failed to write to named pipe: {e}"),
                    })?;
                pipe.flush().await.map_err(|e| TransportError::Io {
                    message: format!("Failed to flush named pipe: {e}"),
                })?;
            } else {
                return Err(TransportError::Connection {
                    message: "Named pipe not available".to_string(),
                });
            }
        } else {
            let mut guard = self.pipe.lock().await;
            if let Some(pipe) = guard.as_mut() {
                pipe.write_all(&data)
                    .await
                    .map_err(|e| TransportError::Io {
                        message: format!("Failed to write to named pipe: {e}"),
                    })?;
                pipe.flush().await.map_err(|e| TransportError::Io {
                    message: format!("Failed to flush named pipe: {e}"),
                })?;
            } else {
                return Err(TransportError::Connection {
                    message: "Named pipe not available".to_string(),
                });
            }
        }

        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Internal receive implementation.
    async fn recv_impl(&self) -> Result<Option<Message>, TransportError> {
        use tokio::io::AsyncReadExt;

        if !self.connected.load(Ordering::Acquire) {
            return Ok(None);
        }

        let mut state = self.state.lock().await;

        // Check if we have a complete line in the buffer
        if let Some(newline_pos) = state.line_buffer.find('\n') {
            let line = state.line_buffer[..newline_pos].to_string();
            state.line_buffer = state.line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                return Ok(None);
            }

            let msg: Message =
                serde_json::from_str(&line).map_err(|e| TransportError::Deserialization {
                    message: format!("Failed to deserialize message: {e}"),
                })?;

            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(msg));
        }

        // Need to read more data
        let mut temp_buf = [0u8; 4096];

        let bytes_read = if self.is_server_side {
            let mut guard = self.server_pipe.lock().await;
            if let Some(pipe) = guard.as_mut() {
                match pipe.read(&mut temp_buf).await {
                    Ok(0) => {
                        self.connected.store(false, Ordering::Release);
                        return Ok(None);
                    }
                    Ok(n) => n,
                    Err(e) => {
                        self.connected.store(false, Ordering::Release);
                        return Err(TransportError::Io {
                            message: format!("Failed to read from named pipe: {e}"),
                        });
                    }
                }
            } else {
                return Ok(None);
            }
        } else {
            let mut guard = self.pipe.lock().await;
            if let Some(pipe) = guard.as_mut() {
                match pipe.read(&mut temp_buf).await {
                    Ok(0) => {
                        self.connected.store(false, Ordering::Release);
                        return Ok(None);
                    }
                    Ok(n) => n,
                    Err(e) => {
                        self.connected.store(false, Ordering::Release);
                        return Err(TransportError::Io {
                            message: format!("Failed to read from named pipe: {e}"),
                        });
                    }
                }
            } else {
                return Ok(None);
            }
        };

        // Append to line buffer
        let chunk = std::str::from_utf8(&temp_buf[..bytes_read]).map_err(|e| {
            TransportError::Deserialization {
                message: format!("Invalid UTF-8 in message: {e}"),
            }
        })?;
        state.line_buffer.push_str(chunk);

        // Check if we now have a complete line
        if let Some(newline_pos) = state.line_buffer.find('\n') {
            let line = state.line_buffer[..newline_pos].to_string();
            state.line_buffer = state.line_buffer[newline_pos + 1..].to_string();

            if line.is_empty() {
                return Ok(None);
            }

            // Check message size
            if line.len() > self.config.max_message_size {
                return Err(TransportError::MessageTooLarge {
                    size: line.len(),
                    max: self.config.max_message_size,
                });
            }

            let msg: Message =
                serde_json::from_str(&line).map_err(|e| TransportError::Deserialization {
                    message: format!("Failed to deserialize message: {e}"),
                })?;

            self.messages_received.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(msg));
        }

        // No complete message yet, return None
        Ok(None)
    }
}

#[cfg(all(windows, feature = "tokio-runtime"))]
impl Transport for NamedPipeTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        self.send_impl(msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        self.recv_impl().await
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::Release);

        // Drop the pipe handles
        if self.is_server_side {
            let mut guard = self.server_pipe.lock().await;
            *guard = None;
        } else {
            let mut guard = self.pipe.lock().await;
            *guard = None;
        }

        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("named-pipe").remote_addr(self.config.name.clone())
    }
}

/// Windows named pipe server (listener).
///
/// Accepts incoming connections on a named pipe.
#[cfg(all(windows, feature = "tokio-runtime"))]
pub struct NamedPipeServer {
    config: NamedPipeConfig,
    running: AtomicBool,
}

#[cfg(all(windows, feature = "tokio-runtime"))]
impl NamedPipeServer {
    /// Create a new named pipe server.
    pub fn new(name: impl Into<String>) -> Result<Self, TransportError> {
        let config = NamedPipeConfig::new(name);
        Self::with_config(config)
    }

    /// Create a new named pipe server with custom configuration.
    pub fn with_config(config: NamedPipeConfig) -> Result<Self, TransportError> {
        tracing::info!(name = %config.name, "Named pipe server created");

        Ok(Self {
            config,
            running: AtomicBool::new(true),
        })
    }

    /// Get the pipe name.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Check if the server is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Stop the server.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

#[cfg(all(windows, feature = "tokio-runtime"))]
impl TransportListener for NamedPipeServer {
    type Transport = NamedPipeTransport;
    type Error = TransportError;

    async fn accept(&self) -> Result<Self::Transport, Self::Error> {
        use tokio::net::windows::named_pipe::ServerOptions;

        if !self.running.load(Ordering::Acquire) {
            return Err(TransportError::Connection {
                message: "Server not running".to_string(),
            });
        }

        // Create a new pipe instance for this connection
        let pipe = ServerOptions::new()
            .first_pipe_instance(false)
            .max_instances(self.config.max_instances)
            .in_buffer_size(self.config.in_buffer_size as u32)
            .out_buffer_size(self.config.out_buffer_size as u32)
            .create(&self.config.name)
            .map_err(|e| TransportError::Connection {
                message: format!("Failed to create named pipe '{}': {}", self.config.name, e),
            })?;

        // Wait for a client to connect
        pipe.connect()
            .await
            .map_err(|e| TransportError::Connection {
                message: format!(
                    "Failed to accept connection on '{}': {}",
                    self.config.name, e
                ),
            })?;

        tracing::debug!(name = %self.config.name, "Accepted named pipe connection");

        Ok(NamedPipeTransport::from_server(self.config.clone(), pipe))
    }

    fn local_addr(&self) -> Option<String> {
        Some(self.config.name.clone())
    }
}

/// Builder for Windows named pipe transport.
pub struct NamedPipeBuilder {
    config: NamedPipeConfig,
}

impl NamedPipeBuilder {
    /// Create a new builder with the given pipe name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: NamedPipeConfig::new(name),
        }
    }

    /// Set the maximum number of pipe instances.
    #[must_use]
    pub const fn max_instances(mut self, max: u32) -> Self {
        self.config.max_instances = max;
        self
    }

    /// Set the input buffer size.
    #[must_use]
    pub const fn in_buffer_size(mut self, size: usize) -> Self {
        self.config.in_buffer_size = size;
        self
    }

    /// Set the output buffer size.
    #[must_use]
    pub const fn out_buffer_size(mut self, size: usize) -> Self {
        self.config.out_buffer_size = size;
        self
    }

    /// Connect to the named pipe server.
    #[cfg(all(windows, feature = "tokio-runtime"))]
    pub async fn connect(self) -> Result<NamedPipeTransport, TransportError> {
        NamedPipeTransport::connect_with_config(self.config).await
    }

    /// Connect to the named pipe server (stub for non-Windows).
    #[cfg(not(all(windows, feature = "tokio-runtime")))]
    pub async fn connect(self) -> Result<NamedPipeTransport, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    /// Create a server on the named pipe.
    #[cfg(all(windows, feature = "tokio-runtime"))]
    pub fn server(self) -> Result<NamedPipeServer, TransportError> {
        NamedPipeServer::with_config(self.config)
    }

    /// Create a server on the named pipe (stub for non-Windows).
    #[cfg(not(all(windows, feature = "tokio-runtime")))]
    pub fn server(self) -> Result<NamedPipeServer, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }
}

// Stub types for non-Windows platforms
#[cfg(not(all(windows, feature = "tokio-runtime")))]
pub struct NamedPipeTransport {
    _private: (),
}

#[cfg(not(all(windows, feature = "tokio-runtime")))]
impl NamedPipeTransport {
    /// Connect to a named pipe server.
    pub async fn connect(_name: impl Into<String>) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    /// Connect with custom configuration.
    pub async fn connect_with_config(_config: NamedPipeConfig) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }
}

#[cfg(not(all(windows, feature = "tokio-runtime")))]
impl Transport for NamedPipeTransport {
    type Error = TransportError;

    async fn send(&self, _msg: Message) -> Result<(), Self::Error> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    async fn close(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        false
    }

    fn metadata(&self) -> TransportMetadata {
        TransportMetadata::new("named-pipe")
    }
}

#[cfg(not(all(windows, feature = "tokio-runtime")))]
pub struct NamedPipeServer {
    _private: (),
}

#[cfg(not(all(windows, feature = "tokio-runtime")))]
impl NamedPipeServer {
    /// Create a new named pipe server.
    pub fn new(_name: impl Into<String>) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    /// Create a new named pipe server with custom configuration.
    pub fn with_config(_config: NamedPipeConfig) -> Result<Self, TransportError> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }
}

#[cfg(not(all(windows, feature = "tokio-runtime")))]
impl TransportListener for NamedPipeServer {
    type Transport = NamedPipeTransport;
    type Error = TransportError;

    async fn accept(&self) -> Result<Self::Transport, Self::Error> {
        Err(TransportError::Connection {
            message: "Named pipe transport is only available on Windows".to_string(),
        })
    }

    fn local_addr(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_creation() {
        let config = NamedPipeConfig::new("mcp-server");
        assert_eq!(config.name, r"\\.\pipe\mcp-server");
        assert_eq!(config.short_name(), "mcp-server");
    }

    #[test]
    fn test_config_with_full_path() {
        let config = NamedPipeConfig::new(r"\\.\pipe\custom-pipe");
        assert_eq!(config.name, r"\\.\pipe\custom-pipe");
        assert_eq!(config.short_name(), "custom-pipe");
    }

    #[test]
    fn test_config_builder() {
        let config = NamedPipeConfig::new("test")
            .with_max_instances(10)
            .with_in_buffer_size(32 * 1024)
            .with_out_buffer_size(32 * 1024);

        assert_eq!(config.max_instances, 10);
        assert_eq!(config.in_buffer_size, 32 * 1024);
        assert_eq!(config.out_buffer_size, 32 * 1024);
    }

    #[test]
    fn test_builder() {
        let builder = NamedPipeBuilder::new("mcp-test")
            .max_instances(5)
            .in_buffer_size(128 * 1024)
            .out_buffer_size(128 * 1024);

        assert_eq!(builder.config.max_instances, 5);
        assert_eq!(builder.config.in_buffer_size, 128 * 1024);
    }
}
