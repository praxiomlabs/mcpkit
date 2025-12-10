//! Transport traits for the MCP protocol.
//!
//! This module defines the core transport abstractions that are runtime-agnostic.
//! Transports can be implemented for any async runtime (Tokio, async-std, smol).
//!
//! # Overview
//!
//! - [`Transport`]: Core trait for bidirectional message passing
//! - [`TransportListener`]: Trait for server-side transport listeners
//!
//! # Example
//!
//! ```ignore
//! use mcp_transport::{Transport, TransportMetadata};
//!
//! async fn send_message<T: Transport>(transport: &T) {
//!     let msg = Message::notification("ping", None);
//!     transport.send(msg).await.unwrap();
//! }
//! ```

use mcp_core::protocol::Message;
use std::future::Future;
use std::time::Instant;

/// Metadata about a transport connection.
#[derive(Debug, Clone, Default)]
pub struct TransportMetadata {
    /// Transport type identifier (e.g., "stdio", "http", "websocket").
    pub transport_type: String,
    /// Remote address, if applicable.
    pub remote_addr: Option<String>,
    /// Local address, if applicable.
    pub local_addr: Option<String>,
    /// When the connection was established.
    pub connected_at: Option<Instant>,
    /// Whether the transport supports bidirectional communication.
    pub bidirectional: bool,
    /// Custom metadata specific to the transport type.
    pub custom: Option<serde_json::Value>,
}

impl TransportMetadata {
    /// Create new metadata for a transport type.
    #[must_use]
    pub fn new(transport_type: impl Into<String>) -> Self {
        Self {
            transport_type: transport_type.into(),
            remote_addr: None,
            local_addr: None,
            connected_at: None,
            bidirectional: true,
            custom: None,
        }
    }

    /// Set the remote address.
    #[must_use]
    pub fn remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }

    /// Set the local address.
    #[must_use]
    pub fn local_addr(mut self, addr: impl Into<String>) -> Self {
        self.local_addr = Some(addr.into());
        self
    }

    /// Mark the connection time.
    #[must_use]
    pub fn connected_now(mut self) -> Self {
        self.connected_at = Some(Instant::now());
        self
    }

    /// Set bidirectional flag.
    #[must_use]
    pub fn bidirectional(mut self, bidirectional: bool) -> Self {
        self.bidirectional = bidirectional;
        self
    }
}

/// Core transport trait for MCP communication.
///
/// Transports provide bidirectional message passing between MCP clients
/// and servers. This trait is runtime-agnostic and uses `impl Future`
/// return types for flexibility.
///
/// # Implementing Transport
///
/// Implementations should be `Send + Sync` and handle concurrent access
/// safely. The send and receive operations should be independent and
/// can be called from different tasks.
///
/// # Example Implementation
///
/// ```ignore
/// struct MyTransport { /* ... */ }
///
/// impl Transport for MyTransport {
///     type Error = MyError;
///
///     fn send(&self, msg: Message) -> impl Future<Output = Result<(), Self::Error>> + Send {
///         async move {
///             // Send the message
///             Ok(())
///         }
///     }
///
///     fn recv(&self) -> impl Future<Output = Result<Option<Message>, Self::Error>> + Send {
///         async move {
///             // Receive a message, return None on EOF
///             Ok(None)
///         }
///     }
///
///     // ... other methods
/// }
/// ```
pub trait Transport: Send + Sync {
    /// The error type for transport operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send a message over the transport.
    ///
    /// # Errors
    ///
    /// Returns an error if the message could not be sent (e.g., connection closed,
    /// serialization failed, I/O error).
    fn send(&self, msg: Message) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Receive a message from the transport.
    ///
    /// Returns `Ok(None)` when the connection is cleanly closed.
    /// Returns `Err` on transport errors.
    ///
    /// # Errors
    ///
    /// Returns an error if receiving failed (e.g., connection reset,
    /// deserialization failed, I/O error).
    fn recv(&self) -> impl Future<Output = Result<Option<Message>, Self::Error>> + Send;

    /// Close the transport connection.
    ///
    /// This should perform a graceful shutdown if possible.
    ///
    /// # Errors
    ///
    /// Returns an error if the close operation failed.
    fn close(&self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Check if the transport is still connected.
    fn is_connected(&self) -> bool;

    /// Get metadata about the transport.
    fn metadata(&self) -> TransportMetadata;
}

/// Listener trait for server-side transports.
///
/// Transport listeners accept incoming connections and produce
/// transport instances for each connection.
///
/// # Example
///
/// ```ignore
/// let listener = TcpListener::bind("127.0.0.1:8080").await?;
///
/// loop {
///     let transport = listener.accept().await?;
///     tokio::spawn(async move {
///         handle_connection(transport).await;
///     });
/// }
/// ```
pub trait TransportListener: Send + Sync {
    /// The type of transport produced by this listener.
    type Transport: Transport;

    /// The error type for listener operations.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Accept an incoming connection.
    ///
    /// This method blocks until a new connection is available.
    ///
    /// # Errors
    ///
    /// Returns an error if accepting the connection failed.
    fn accept(&self) -> impl Future<Output = Result<Self::Transport, Self::Error>> + Send;

    /// Get the local address the listener is bound to, if available.
    fn local_addr(&self) -> Option<String>;
}

/// Extension trait for transports with buffered operations.
pub trait TransportExt: Transport {
    /// Send multiple messages in a batch.
    ///
    /// This can be more efficient than sending messages individually
    /// for transports that support batching.
    fn send_batch(
        &self,
        msgs: Vec<Message>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        async move {
            for msg in msgs {
                self.send(msg).await?;
            }
            Ok(())
        }
    }
}

impl<T: Transport> TransportExt for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_builder() {
        let meta = TransportMetadata::new("stdio")
            .remote_addr("stdin")
            .local_addr("stdout")
            .bidirectional(true)
            .connected_now();

        assert_eq!(meta.transport_type, "stdio");
        assert!(meta.remote_addr.is_some());
        assert!(meta.connected_at.is_some());
    }
}
