//! Message batching middleware.
//!
//! This module provides a middleware layer for batching outgoing messages
//! to improve throughput and reduce per-message overhead.
//!
//! # Overview
//!
//! The batching layer collects outgoing messages and flushes them in batches
//! based on configurable thresholds:
//!
//! - **Batch size**: Flush when a certain number of messages are queued
//! - **Flush interval**: Flush after a time delay to prevent stale messages
//! - **Max bytes**: Flush when the total message size exceeds a threshold
//!
//! # Example
//!
//! ```rust,no_run
//! use mcpkit_transport::middleware::{BatchingLayer, BatchingConfig, LayerStack};
//! use mcpkit_transport::{MemoryTransport, Transport};
//! use std::time::Duration;
//!
//! # async fn example() {
//! let (client, server) = MemoryTransport::pair();
//!
//! let config = BatchingConfig::new()
//!     .max_batch_size(10)
//!     .flush_interval(Duration::from_millis(50));
//!
//! let batched = LayerStack::new(client)
//!     .with(BatchingLayer::new(config))
//!     .into_inner();
//! # }
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::error::TransportError;
use crate::runtime::AsyncMutex;
use crate::traits::{Transport, TransportMetadata};

use super::TransportLayer;
use mcpkit_core::protocol::Message;

/// Configuration for message batching.
#[derive(Debug, Clone)]
pub struct BatchingConfig {
    /// Maximum number of messages per batch.
    pub max_batch_size: usize,
    /// Maximum time to wait before flushing.
    pub flush_interval: Duration,
    /// Maximum total bytes before flushing.
    pub max_batch_bytes: usize,
    /// Whether to flush immediately on high-priority messages.
    ///
    /// Requests are considered high-priority by default.
    pub flush_on_request: bool,
}

impl Default for BatchingConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 10,
            flush_interval: Duration::from_millis(50),
            max_batch_bytes: 65536, // 64KB
            flush_on_request: true,
        }
    }
}

impl BatchingConfig {
    /// Create a new batching configuration with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the maximum batch size.
    #[must_use]
    pub const fn max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }

    /// Set the flush interval.
    #[must_use]
    pub const fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Set the maximum batch bytes.
    #[must_use]
    pub const fn max_batch_bytes(mut self, bytes: usize) -> Self {
        self.max_batch_bytes = bytes;
        self
    }

    /// Set whether to flush immediately on requests.
    #[must_use]
    pub const fn flush_on_request(mut self, flush: bool) -> Self {
        self.flush_on_request = flush;
        self
    }
}

/// Message batching layer.
///
/// Wraps a transport to batch outgoing messages for improved throughput.
#[derive(Debug, Clone)]
pub struct BatchingLayer {
    config: BatchingConfig,
}

impl BatchingLayer {
    /// Create a new batching layer with the given configuration.
    #[must_use]
    pub const fn new(config: BatchingConfig) -> Self {
        Self { config }
    }

    /// Create a batching layer with default configuration.
    #[must_use]
    pub fn default_config() -> Self {
        Self::new(BatchingConfig::default())
    }
}

impl<T: Transport + 'static> TransportLayer<T> for BatchingLayer
where
    T::Error: From<TransportError>,
{
    type Transport = BatchingTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        BatchingTransport::new(inner, self.config.clone())
    }
}

/// Internal batch buffer state.
struct BatchBuffer {
    /// Queued messages.
    messages: VecDeque<Message>,
    /// Estimated total size of queued messages.
    total_bytes: usize,
    /// When the first message was queued.
    first_queued: Option<Instant>,
}

impl BatchBuffer {
    fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            total_bytes: 0,
            first_queued: None,
        }
    }

    fn push(&mut self, msg: Message, estimated_size: usize) {
        if self.first_queued.is_none() {
            self.first_queued = Some(Instant::now());
        }
        self.messages.push_back(msg);
        self.total_bytes += estimated_size;
    }

    fn drain(&mut self) -> Vec<Message> {
        self.total_bytes = 0;
        self.first_queued = None;
        self.messages.drain(..).collect()
    }

    fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    fn should_flush(&self, config: &BatchingConfig) -> bool {
        if self.messages.len() >= config.max_batch_size {
            return true;
        }
        if self.total_bytes >= config.max_batch_bytes {
            return true;
        }
        if let Some(first) = self.first_queued {
            if first.elapsed() >= config.flush_interval {
                return true;
            }
        }
        false
    }
}

/// A transport that batches outgoing messages.
pub struct BatchingTransport<T: Transport> {
    inner: T,
    config: BatchingConfig,
    buffer: AsyncMutex<BatchBuffer>,
    closed: AtomicBool,
    /// Total messages batched.
    stats_messages_batched: AtomicUsize,
    /// Total batches sent.
    stats_batches_sent: AtomicUsize,
}

impl<T: Transport> BatchingTransport<T> {
    /// Create a new batching transport.
    pub fn new(inner: T, config: BatchingConfig) -> Self {
        Self {
            inner,
            config,
            buffer: AsyncMutex::new(BatchBuffer::new()),
            closed: AtomicBool::new(false),
            stats_messages_batched: AtomicUsize::new(0),
            stats_batches_sent: AtomicUsize::new(0),
        }
    }

    /// Get batching statistics.
    #[must_use]
    pub fn stats(&self) -> BatchingStats {
        BatchingStats {
            messages_batched: self.stats_messages_batched.load(Ordering::Relaxed),
            batches_sent: self.stats_batches_sent.load(Ordering::Relaxed),
        }
    }

    /// Flush any pending messages.
    ///
    /// # Errors
    ///
    /// Returns an error if sending fails.
    pub async fn flush(&self) -> Result<(), T::Error> {
        let messages = {
            let mut buffer = self.buffer.lock().await;
            if buffer.is_empty() {
                return Ok(());
            }
            buffer.drain()
        };

        let count = messages.len();
        for msg in messages {
            self.inner.send(msg).await?;
        }

        self.stats_batches_sent.fetch_add(1, Ordering::Relaxed);
        self.stats_messages_batched
            .fetch_add(count, Ordering::Relaxed);

        Ok(())
    }

    /// Check if a message is high-priority (a request).
    fn is_high_priority(msg: &Message) -> bool {
        matches!(msg, Message::Request(_))
    }

    /// Estimate the size of a message.
    fn estimate_size(msg: &Message) -> usize {
        // Simple estimate based on JSON serialization
        serde_json::to_string(msg).map(|s| s.len()).unwrap_or(100)
    }
}

impl<T: Transport> Transport for BatchingTransport<T>
where
    T::Error: From<TransportError>,
{
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::NotConnected.into());
        }

        let is_high_priority = Self::is_high_priority(&msg);
        let size = Self::estimate_size(&msg);

        let should_flush = {
            let mut buffer = self.buffer.lock().await;
            buffer.push(msg, size);

            // Check if we should flush
            if is_high_priority && self.config.flush_on_request {
                true
            } else {
                buffer.should_flush(&self.config)
            }
        };

        if should_flush {
            self.flush().await?;
        }

        Ok(())
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        self.inner.recv().await
    }

    async fn close(&self) -> Result<(), Self::Error> {
        self.closed.store(true, Ordering::Relaxed);
        // Flush any pending messages before closing
        let _ = self.flush().await;
        self.inner.close().await
    }

    fn is_connected(&self) -> bool {
        !self.closed.load(Ordering::Relaxed) && self.inner.is_connected()
    }

    fn metadata(&self) -> TransportMetadata {
        let mut meta = self.inner.metadata();
        meta.custom = Some(serde_json::json!({
            "batching": {
                "max_batch_size": self.config.max_batch_size,
                "flush_interval_ms": self.config.flush_interval.as_millis(),
                "stats": self.stats()
            }
        }));
        meta
    }
}

/// Statistics about batching operations.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct BatchingStats {
    /// Total messages that have been batched.
    pub messages_batched: usize,
    /// Total batches that have been sent.
    pub batches_sent: usize,
}

impl BatchingStats {
    /// Calculate the average batch size.
    #[must_use]
    pub fn average_batch_size(&self) -> f64 {
        if self.batches_sent == 0 {
            0.0
        } else {
            self.messages_batched as f64 / self.batches_sent as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = BatchingConfig::new()
            .max_batch_size(20)
            .flush_interval(Duration::from_millis(100))
            .max_batch_bytes(1024)
            .flush_on_request(false);

        assert_eq!(config.max_batch_size, 20);
        assert_eq!(config.flush_interval, Duration::from_millis(100));
        assert_eq!(config.max_batch_bytes, 1024);
        assert!(!config.flush_on_request);
    }

    #[test]
    fn test_batch_buffer() {
        let mut buffer = BatchBuffer::new();
        let config = BatchingConfig::new().max_batch_size(3);

        assert!(buffer.is_empty());

        buffer.push(
            Message::Request(mcpkit_core::protocol::Request::new("test", 1)),
            100,
        );
        assert!(!buffer.is_empty());
        assert!(!buffer.should_flush(&config));

        buffer.push(
            Message::Request(mcpkit_core::protocol::Request::new("test", 2)),
            100,
        );
        assert!(!buffer.should_flush(&config));

        buffer.push(
            Message::Request(mcpkit_core::protocol::Request::new("test", 3)),
            100,
        );
        assert!(buffer.should_flush(&config));

        let msgs = buffer.drain();
        assert_eq!(msgs.len(), 3);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_batching_stats() {
        let stats = BatchingStats {
            messages_batched: 30,
            batches_sent: 10,
        };

        assert!((stats.average_batch_size() - 3.0).abs() < f64::EPSILON);
    }
}
