//! Metrics middleware for MCP transports.
//!
//! This middleware collects metrics about transport operations:
//! - Message counts (sent/received)
//! - Latency histograms
//! - Error counts
//! - Connection duration

use crate::middleware::TransportLayer;
use crate::traits::{Transport, TransportMetadata};
use mcp_core::protocol::Message;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Metrics collected by the metrics layer.
#[derive(Debug, Default)]
pub struct Metrics {
    /// Number of messages sent.
    pub messages_sent: AtomicU64,
    /// Number of messages received.
    pub messages_received: AtomicU64,
    /// Number of send errors.
    pub send_errors: AtomicU64,
    /// Number of receive errors.
    pub recv_errors: AtomicU64,
    /// Total bytes sent (approximate, JSON serialized).
    pub bytes_sent: AtomicU64,
    /// Total bytes received (approximate, JSON serialized).
    pub bytes_received: AtomicU64,
    /// Timestamp when metrics collection started.
    started_at: Option<Instant>,
}

impl Metrics {
    /// Create new metrics with the current time as start.
    pub fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            send_errors: AtomicU64::new(0),
            recv_errors: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            started_at: Some(Instant::now()),
        }
    }

    /// Get the duration since metrics collection started.
    pub fn duration(&self) -> Option<Duration> {
        self.started_at.map(|t| t.elapsed())
    }

    /// Get messages sent count.
    pub fn messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Get messages received count.
    pub fn messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Get send error count.
    pub fn send_errors(&self) -> u64 {
        self.send_errors.load(Ordering::Relaxed)
    }

    /// Get receive error count.
    pub fn recv_errors(&self) -> u64 {
        self.recv_errors.load(Ordering::Relaxed)
    }

    /// Get bytes sent.
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Get bytes received.
    pub fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Calculate send rate (messages per second).
    pub fn send_rate(&self) -> f64 {
        match self.duration() {
            Some(d) if !d.is_zero() => self.messages_sent() as f64 / d.as_secs_f64(),
            _ => 0.0,
        }
    }

    /// Calculate receive rate (messages per second).
    pub fn recv_rate(&self) -> f64 {
        match self.duration() {
            Some(d) if !d.is_zero() => self.messages_received() as f64 / d.as_secs_f64(),
            _ => 0.0,
        }
    }

    /// Reset all counters.
    pub fn reset(&self) {
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_received.store(0, Ordering::Relaxed);
        self.send_errors.store(0, Ordering::Relaxed);
        self.recv_errors.store(0, Ordering::Relaxed);
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.bytes_received.store(0, Ordering::Relaxed);
    }
}

/// A handle to access metrics from outside the transport.
pub type MetricsHandle = Arc<Metrics>;

/// A layer that collects metrics about transport operations.
#[derive(Debug, Clone)]
pub struct MetricsLayer {
    /// The metrics handle to populate.
    metrics: MetricsHandle,
}

impl MetricsLayer {
    /// Create a new metrics layer with the provided metrics handle.
    #[must_use]
    pub fn new(metrics: MetricsHandle) -> Self {
        Self { metrics }
    }

    /// Create a new metrics layer and return both the layer and handle.
    #[must_use]
    pub fn new_with_handle() -> (Self, MetricsHandle) {
        let metrics = Arc::new(Metrics::new());
        let layer = Self { metrics: Arc::clone(&metrics) };
        (layer, metrics)
    }

    /// Get a reference to the metrics handle.
    pub fn handle(&self) -> &MetricsHandle {
        &self.metrics
    }
}

impl Default for MetricsLayer {
    fn default() -> Self {
        Self::new(Arc::new(Metrics::new()))
    }
}

impl<T: Transport> TransportLayer<T> for MetricsLayer {
    type Transport = MetricsTransport<T>;

    fn layer(&self, inner: T) -> Self::Transport {
        MetricsTransport {
            inner,
            metrics: Arc::clone(&self.metrics),
        }
    }
}

/// A transport wrapped with metrics collection.
pub struct MetricsTransport<T> {
    inner: T,
    metrics: MetricsHandle,
}

impl<T> MetricsTransport<T> {
    /// Get a reference to the metrics.
    pub fn metrics(&self) -> &MetricsHandle {
        &self.metrics
    }
}

impl<T: Transport> Transport for MetricsTransport<T> {
    type Error = T::Error;

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        // Estimate bytes (this is approximate)
        if let Ok(json) = serde_json::to_string(&msg) {
            self.metrics
                .bytes_sent
                .fetch_add(json.len() as u64, Ordering::Relaxed);
        }

        match self.inner.send(msg).await {
            Ok(()) => {
                self.metrics.messages_sent.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(e) => {
                self.metrics.send_errors.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    async fn recv(&self) -> Result<Option<Message>, Self::Error> {
        match self.inner.recv().await {
            Ok(Some(msg)) => {
                self.metrics.messages_received.fetch_add(1, Ordering::Relaxed);
                // Estimate bytes
                if let Ok(json) = serde_json::to_string(&msg) {
                    self.metrics
                        .bytes_received
                        .fetch_add(json.len() as u64, Ordering::Relaxed);
                }
                Ok(Some(msg))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.metrics.recv_errors.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    async fn close(&self) -> Result<(), Self::Error> {
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
    fn test_metrics_creation() {
        let metrics = Metrics::new();
        assert_eq!(metrics.messages_sent(), 0);
        assert_eq!(metrics.messages_received(), 0);
        assert!(metrics.duration().is_some());
    }

    #[test]
    fn test_metrics_layer_handle() {
        let (layer, handle) = MetricsLayer::new_with_handle();

        // Increment through handle
        handle.messages_sent.fetch_add(5, Ordering::Relaxed);

        // Check through layer's handle
        assert_eq!(layer.handle().messages_sent(), 5);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = Metrics::new();
        metrics.messages_sent.store(100, Ordering::Relaxed);
        metrics.messages_received.store(50, Ordering::Relaxed);

        metrics.reset();

        assert_eq!(metrics.messages_sent(), 0);
        assert_eq!(metrics.messages_received(), 0);
    }

    #[test]
    fn test_metrics_rates() {
        let metrics = Metrics::new();

        // No messages, rate should be 0
        assert_eq!(metrics.send_rate(), 0.0);
        assert_eq!(metrics.recv_rate(), 0.0);
    }
}
