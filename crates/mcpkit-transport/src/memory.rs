//! In-memory transport for testing.
//!
//! This module provides a transport that uses channels for in-process
//! communication. It's primarily useful for testing MCP servers and clients
//! without network overhead.
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
//! ```rust
//! use mcpkit_transport::{MemoryTransport, Transport};
//!
//! // Create a pair of connected transports
//! let (client_transport, server_transport) = MemoryTransport::pair();
//!
//! // Both transports are connected
//! assert!(client_transport.is_connected());
//! assert!(server_transport.is_connected());
//! ```

use crate::error::TransportError;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

// =============================================================================
// Runtime-agnostic implementation using futures channels
// =============================================================================

#[cfg(any(
    feature = "tokio-runtime",
    feature = "async-std-runtime",
    feature = "smol-runtime"
))]
use crate::runtime::AsyncMutex;

/// An in-memory transport using channels.
///
/// This is useful for testing MCP implementations without network I/O.
/// The transport is runtime-agnostic and works with any async runtime.
#[cfg(any(
    feature = "tokio-runtime",
    feature = "async-std-runtime",
    feature = "smol-runtime"
))]
pub struct MemoryTransport {
    sender: futures::channel::mpsc::Sender<Message>,
    receiver: AsyncMutex<futures::channel::mpsc::Receiver<Message>>,
    connected: Arc<AtomicBool>,
    metadata: TransportMetadata,
}

#[cfg(any(
    feature = "tokio-runtime",
    feature = "async-std-runtime",
    feature = "smol-runtime"
))]
impl MemoryTransport {
    /// Create a connected pair of memory transports.
    ///
    /// Messages sent on the first transport are received on the second,
    /// and vice versa.
    #[must_use]
    pub fn pair() -> (Self, Self) {
        Self::pair_with_capacity(32)
    }

    /// Create a connected pair with a specific buffer capacity.
    #[must_use]
    pub fn pair_with_capacity(capacity: usize) -> (Self, Self) {
        let (tx1, rx1) = futures::channel::mpsc::channel(capacity);
        let (tx2, rx2) = futures::channel::mpsc::channel(capacity);

        let connected1 = Arc::new(AtomicBool::new(true));
        let connected2 = Arc::clone(&connected1);

        let transport1 = Self {
            sender: tx2,
            receiver: AsyncMutex::new(rx1),
            connected: connected1,
            metadata: TransportMetadata::new("memory")
                .remote_addr("peer-1")
                .local_addr("peer-0")
                .connected_now(),
        };

        let transport2 = Self {
            sender: tx1,
            receiver: AsyncMutex::new(rx2),
            connected: connected2,
            metadata: TransportMetadata::new("memory")
                .remote_addr("peer-0")
                .local_addr("peer-1")
                .connected_now(),
        };

        (transport1, transport2)
    }
}

#[cfg(any(
    feature = "tokio-runtime",
    feature = "async-std-runtime",
    feature = "smol-runtime"
))]
impl Transport for MemoryTransport {
    type Error = TransportError;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        use futures::SinkExt;

        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        // Clone sender to get a mutable reference
        let mut sender = self.sender.clone();
        sender
            .send(msg)
            .await
            .map_err(|_| TransportError::ConnectionClosed)
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        use futures::StreamExt;

        if !self.is_connected() {
            return Err(TransportError::NotConnected);
        }

        let mut receiver = self.receiver.lock().await;
        if let Some(msg) = receiver.next().await {
            Ok(Some(msg))
        } else {
            self.connected.store(false, Ordering::SeqCst);
            Ok(None)
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.connected.store(false, Ordering::SeqCst);
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(
        feature = "tokio-runtime",
        feature = "async-std-runtime",
        feature = "smol-runtime"
    ))]
    mod async_tests {
        use super::*;
        use mcpkit_core::protocol::{Notification, Request, RequestId};

        #[cfg(feature = "tokio-runtime")]
        #[tokio::test]
        async fn test_memory_transport_pair() {
            let (client, server) = MemoryTransport::pair();

            assert!(client.is_connected());
            assert!(server.is_connected());
            assert_eq!(client.metadata().transport_type, "memory");
        }

        #[cfg(feature = "tokio-runtime")]
        #[tokio::test]
        async fn test_send_receive() {
            let (client, server) = MemoryTransport::pair();

            // Create a request message
            let request = Request::new("test/method", RequestId::Number(1));
            let msg = Message::Request(request);

            // Send from client
            client.send(msg.clone()).await.unwrap();

            // Receive on server
            let received = server.recv().await.unwrap().unwrap();

            match received {
                Message::Request(req) => {
                    assert_eq!(req.method.as_ref(), "test/method");
                }
                _ => panic!("Expected request"),
            }
        }

        #[cfg(feature = "tokio-runtime")]
        #[tokio::test]
        async fn test_bidirectional() {
            let (client, server) = MemoryTransport::pair();

            // Client -> Server
            let client_msg = Message::Notification(Notification::new("client/ping"));
            client.send(client_msg).await.unwrap();

            // Server -> Client
            let server_msg = Message::Notification(Notification::new("server/pong"));
            server.send(server_msg).await.unwrap();

            // Receive on both sides
            let from_client = server.recv().await.unwrap().unwrap();
            let from_server = client.recv().await.unwrap().unwrap();

            match from_client {
                Message::Notification(n) => assert_eq!(n.method.as_ref(), "client/ping"),
                _ => panic!("Expected notification"),
            }

            match from_server {
                Message::Notification(n) => assert_eq!(n.method.as_ref(), "server/pong"),
                _ => panic!("Expected notification"),
            }
        }

        #[cfg(feature = "tokio-runtime")]
        #[tokio::test]
        async fn test_close() {
            let (client, server) = MemoryTransport::pair();

            client.close().await.unwrap();
            assert!(!client.is_connected());
            // Server should also be disconnected since they share state
            assert!(!server.is_connected());
        }

        #[cfg(feature = "tokio-runtime")]
        #[tokio::test]
        async fn test_send_after_close() {
            let (client, _server) = MemoryTransport::pair();

            client.close().await.unwrap();

            let msg = Message::Notification(Notification::new("test"));
            let result = client.send(msg).await;

            assert!(matches!(result, Err(TransportError::NotConnected)));
        }
    }
}
