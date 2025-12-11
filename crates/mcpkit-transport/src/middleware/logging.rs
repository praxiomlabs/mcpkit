//! Logging middleware for MCP transports.
//!
//! This middleware logs all messages sent and received through the transport,
//! useful for debugging and monitoring.

use crate::middleware::TransportLayer;
use crate::traits::{Transport, TransportMetadata};
use mcpkit_core::protocol::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, trace, Level};

/// A layer that adds logging to a transport.
///
/// Logs all sent and received messages at the configured level.
#[derive(Debug, Clone)]
pub struct LoggingLayer {
    /// The log level to use.
    level: Level,
    /// Whether to log message contents.
    log_contents: bool,
}

impl LoggingLayer {
    /// Create a new logging layer with the specified log level.
    #[must_use]
    pub fn new(level: Level) -> Self {
        Self {
            level,
            log_contents: false,
        }
    }

    /// Configure whether to log full message contents.
    ///
    /// Warning: This may log sensitive data!
    #[must_use]
    pub fn with_contents(mut self, log_contents: bool) -> Self {
        self.log_contents = log_contents;
        self
    }
}

impl Default for LoggingLayer {
    fn default() -> Self {
        Self::new(Level::DEBUG)
    }
}

impl<T: Transport> TransportLayer<T> for LoggingLayer {
    type Transport = LoggingTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        LoggingTransport {
            inner,
            level: self.level,
            log_contents: self.log_contents,
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }
}

/// A transport wrapped with logging.
pub struct LoggingTransport<T> {
    inner: T,
    level: Level,
    log_contents: bool,
    messages_sent: AtomicU64,
    messages_received: AtomicU64,
}

impl<T> LoggingTransport<T> {
    /// Get the number of messages sent.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get the number of messages received.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }
}

impl<T: Transport> Transport for LoggingTransport<T> {
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let count = self.messages_sent.fetch_add(1, Ordering::Relaxed) + 1;

        if self.log_contents {
            match self.level {
                Level::TRACE => trace!(count, ?msg, "sending message"),
                Level::DEBUG => debug!(count, ?msg, "sending message"),
                _ => debug!(count, "sending message"),
            }
        } else {
            let method = msg.method().unwrap_or("<response>");
            match self.level {
                Level::TRACE => trace!(count, method, "sending message"),
                Level::DEBUG => debug!(count, method, "sending message"),
                _ => debug!(count, "sending message"),
            }
        }

        self.inner.send(msg).await
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        let result = self.inner.recv().await?;

        if let Some(ref msg) = result {
            let count = self.messages_received.fetch_add(1, Ordering::Relaxed) + 1;

            if self.log_contents {
                match self.level {
                    Level::TRACE => trace!(count, ?msg, "received message"),
                    Level::DEBUG => debug!(count, ?msg, "received message"),
                    _ => debug!(count, "received message"),
                }
            } else {
                let method = msg.method().unwrap_or("<response>");
                match self.level {
                    Level::TRACE => trace!(count, method, "received message"),
                    Level::DEBUG => debug!(count, method, "received message"),
                    _ => debug!(count, "received message"),
                }
            }
        }

        Ok(result)
    }

    async fn close(&self) -> Result<(), Self::Error> {
        debug!(
            sent = self.messages_sent(),
            received = self.messages_received(),
            "closing transport"
        );
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
    fn test_logging_layer_creation() {
        let layer = LoggingLayer::new(Level::DEBUG);
        assert!(!layer.log_contents);

        let layer = layer.with_contents(true);
        assert!(layer.log_contents);
    }
}
