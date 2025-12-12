//! Timeout middleware for MCP transports.
//!
//! This middleware adds configurable timeouts to send and receive operations.

use crate::error::TransportError;
use crate::middleware::TransportLayer;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::time::Duration;

/// A layer that adds timeouts to transport operations.
#[derive(Debug, Clone)]
pub struct TimeoutLayer {
    /// Timeout for send operations.
    send_timeout: Option<Duration>,
    /// Timeout for receive operations.
    recv_timeout: Option<Duration>,
}

impl TimeoutLayer {
    /// Create a new timeout layer with equal timeouts for send and receive.
    #[must_use]
    pub const fn new(timeout: Duration) -> Self {
        Self {
            send_timeout: Some(timeout),
            recv_timeout: Some(timeout),
        }
    }

    /// Create a timeout layer with separate send and receive timeouts.
    #[must_use]
    pub const fn with_timeouts(send: Duration, recv: Duration) -> Self {
        Self {
            send_timeout: Some(send),
            recv_timeout: Some(recv),
        }
    }

    /// Set the send timeout.
    #[must_use]
    pub const fn send_timeout(mut self, timeout: Duration) -> Self {
        self.send_timeout = Some(timeout);
        self
    }

    /// Set the receive timeout.
    #[must_use]
    pub const fn recv_timeout(mut self, timeout: Duration) -> Self {
        self.recv_timeout = Some(timeout);
        self
    }

    /// Disable the send timeout.
    #[must_use]
    pub const fn no_send_timeout(mut self) -> Self {
        self.send_timeout = None;
        self
    }

    /// Disable the receive timeout.
    #[must_use]
    pub const fn no_recv_timeout(mut self) -> Self {
        self.recv_timeout = None;
        self
    }
}

impl Default for TimeoutLayer {
    fn default() -> Self {
        Self {
            send_timeout: Some(Duration::from_secs(30)),
            recv_timeout: Some(Duration::from_secs(60)),
        }
    }
}

impl<T: Transport> TransportLayer<T> for TimeoutLayer
where
    T::Error: From<TransportError>,
{
    type Transport = TimeoutTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        TimeoutTransport {
            inner,
            send_timeout: self.send_timeout,
            recv_timeout: self.recv_timeout,
        }
    }
}

/// A transport wrapped with timeout handling.
pub struct TimeoutTransport<T> {
    inner: T,
    send_timeout: Option<Duration>,
    recv_timeout: Option<Duration>,
}

impl<T: Transport> TimeoutTransport<T> {
    /// Get the configured send timeout.
    pub const fn send_timeout(&self) -> Option<Duration> {
        self.send_timeout
    }

    /// Get the configured receive timeout.
    pub const fn recv_timeout(&self) -> Option<Duration> {
        self.recv_timeout
    }
}

impl<T: Transport> Transport for TimeoutTransport<T>
where
    T::Error: From<TransportError>,
{
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        match self.send_timeout {
            Some(timeout) => match crate::runtime::timeout(timeout, self.inner.send(msg)).await {
                Ok(result) => result,
                Err(_) => Err(TransportError::Timeout {
                    operation: "send".to_string(),
                    duration: timeout,
                }
                .into()),
            },
            None => self.inner.send(msg).await,
        }
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        match self.recv_timeout {
            Some(timeout) => match crate::runtime::timeout(timeout, self.inner.recv()).await {
                Ok(result) => result,
                Err(_) => Err(TransportError::Timeout {
                    operation: "recv".to_string(),
                    duration: timeout,
                }
                .into()),
            },
            None => self.inner.recv().await,
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
        // Close should not timeout - we want graceful shutdown
        self.inner.close().await
    }

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    fn metadata(&self) -> TransportMetadata {
        self.inner.metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_layer_creation() {
        let layer = TimeoutLayer::new(Duration::from_secs(10));
        assert_eq!(layer.send_timeout, Some(Duration::from_secs(10)));
        assert_eq!(layer.recv_timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_timeout_layer_builder() {
        let layer = TimeoutLayer::default()
            .send_timeout(Duration::from_secs(5))
            .recv_timeout(Duration::from_secs(120));

        assert_eq!(layer.send_timeout, Some(Duration::from_secs(5)));
        assert_eq!(layer.recv_timeout, Some(Duration::from_secs(120)));
    }

    #[test]
    fn test_timeout_layer_disable() {
        let layer = TimeoutLayer::default().no_send_timeout().no_recv_timeout();

        assert!(layer.send_timeout.is_none());
        assert!(layer.recv_timeout.is_none());
    }
}
