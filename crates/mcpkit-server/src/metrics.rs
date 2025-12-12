//! Server-level metrics for MCP servers.
//!
//! This module provides request-level metrics tracking for MCP servers,
//! complementing the transport-level telemetry in `mcpkit_transport::telemetry`.
//!
//! # Example
//!
//! ```rust
//! use mcpkit_server::metrics::ServerMetrics;
//!
//! let metrics = ServerMetrics::new();
//!
//! // Record a request
//! metrics.record_request("tools/call", std::time::Duration::from_millis(50), true);
//!
//! // Get statistics
//! let stats = metrics.snapshot();
//! println!("Total requests: {}", stats.total_requests);
//! println!("Error rate: {:.2}%", stats.error_rate() * 100.0);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Duration;

/// Server metrics collector.
///
/// Tracks request counts, latencies, and errors at the MCP method level.
/// All operations are thread-safe and lock-free where possible.
#[derive(Debug, Default)]
pub struct ServerMetrics {
    /// Total requests received.
    total_requests: AtomicU64,
    /// Total successful requests.
    successful_requests: AtomicU64,
    /// Total failed requests.
    failed_requests: AtomicU64,
    /// Total latency in microseconds (for average calculation).
    total_latency_us: AtomicU64,
    /// Per-method request counts.
    method_counts: RwLock<HashMap<String, AtomicU64>>,
    /// Per-method error counts.
    method_errors: RwLock<HashMap<String, AtomicU64>>,
    /// Per-method total latency in microseconds.
    method_latency_us: RwLock<HashMap<String, AtomicU64>>,
}

impl ServerMetrics {
    /// Create a new metrics collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a request.
    ///
    /// # Arguments
    ///
    /// * `method` - The MCP method name (e.g., "tools/call", "resources/read")
    /// * `duration` - How long the request took
    /// * `success` - Whether the request succeeded
    pub fn record_request(&self, method: &str, duration: Duration, success: bool) {
        let latency_us = duration.as_micros() as u64;

        // Update global counters
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us.fetch_add(latency_us, Ordering::Relaxed);

        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        // Update per-method counters
        self.increment_method_counter(&self.method_counts, method);
        self.add_method_latency(method, latency_us);

        if !success {
            self.increment_method_counter(&self.method_errors, method);
        }
    }

    /// Record a successful request (convenience method).
    pub fn record_success(&self, method: &str, duration: Duration) {
        self.record_request(method, duration, true);
    }

    /// Record a failed request (convenience method).
    pub fn record_failure(&self, method: &str, duration: Duration) {
        self.record_request(method, duration, false);
    }

    /// Get a snapshot of current metrics.
    #[must_use]
    pub fn snapshot(&self) -> MetricsSnapshot {
        let method_counts = self.method_counts.read().unwrap_or_else(|e| e.into_inner());
        let method_errors = self.method_errors.read().unwrap_or_else(|e| e.into_inner());
        let method_latency = self.method_latency_us.read().unwrap_or_else(|e| e.into_inner());

        let per_method: HashMap<String, MethodStats> = method_counts
            .iter()
            .map(|(method, count)| {
                let requests = count.load(Ordering::Relaxed);
                let errors = method_errors
                    .get(method)
                    .map(|c| c.load(Ordering::Relaxed))
                    .unwrap_or(0);
                let latency_us = method_latency
                    .get(method)
                    .map(|c| c.load(Ordering::Relaxed))
                    .unwrap_or(0);

                (
                    method.clone(),
                    MethodStats {
                        requests,
                        errors,
                        avg_latency_ms: if requests > 0 {
                            (latency_us as f64 / requests as f64) / 1000.0
                        } else {
                            0.0
                        },
                    },
                )
            })
            .collect();

        let total = self.total_requests.load(Ordering::Relaxed);
        let total_latency = self.total_latency_us.load(Ordering::Relaxed);

        MetricsSnapshot {
            total_requests: total,
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            failed_requests: self.failed_requests.load(Ordering::Relaxed),
            avg_latency_ms: if total > 0 {
                (total_latency as f64 / total as f64) / 1000.0
            } else {
                0.0
            },
            per_method,
        }
    }

    /// Reset all metrics to zero.
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.successful_requests.store(0, Ordering::Relaxed);
        self.failed_requests.store(0, Ordering::Relaxed);
        self.total_latency_us.store(0, Ordering::Relaxed);

        if let Ok(mut counts) = self.method_counts.write() {
            counts.clear();
        }
        if let Ok(mut errors) = self.method_errors.write() {
            errors.clear();
        }
        if let Ok(mut latency) = self.method_latency_us.write() {
            latency.clear();
        }
    }

    fn increment_method_counter(
        &self,
        map: &RwLock<HashMap<String, AtomicU64>>,
        method: &str,
    ) {
        // Try to increment existing counter
        if let Ok(counts) = map.read() {
            if let Some(counter) = counts.get(method) {
                counter.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        // Counter doesn't exist, need to create it
        if let Ok(mut counts) = map.write() {
            counts
                .entry(method.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    fn add_method_latency(&self, method: &str, latency_us: u64) {
        // Try to add to existing counter
        if let Ok(latencies) = self.method_latency_us.read() {
            if let Some(counter) = latencies.get(method) {
                counter.fetch_add(latency_us, Ordering::Relaxed);
                return;
            }
        }

        // Counter doesn't exist, need to create it
        if let Ok(mut latencies) = self.method_latency_us.write() {
            latencies
                .entry(method.to_string())
                .or_insert_with(|| AtomicU64::new(0))
                .fetch_add(latency_us, Ordering::Relaxed);
        }
    }
}

/// A point-in-time snapshot of server metrics.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    /// Total requests received.
    pub total_requests: u64,
    /// Total successful requests.
    pub successful_requests: u64,
    /// Total failed requests.
    pub failed_requests: u64,
    /// Average request latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Per-method statistics.
    pub per_method: HashMap<String, MethodStats>,
}

impl MetricsSnapshot {
    /// Calculate the error rate (0.0 to 1.0).
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.failed_requests as f64 / self.total_requests as f64
        }
    }

    /// Calculate the success rate (0.0 to 1.0).
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        1.0 - self.error_rate()
    }

    /// Get statistics for a specific method.
    #[must_use]
    pub fn method(&self, name: &str) -> Option<&MethodStats> {
        self.per_method.get(name)
    }

    /// Get the most called methods, sorted by request count.
    #[must_use]
    pub fn top_methods(&self, limit: usize) -> Vec<(&String, &MethodStats)> {
        let mut methods: Vec<_> = self.per_method.iter().collect();
        methods.sort_by(|a, b| b.1.requests.cmp(&a.1.requests));
        methods.into_iter().take(limit).collect()
    }

    /// Get methods with highest error rates.
    #[must_use]
    pub fn most_errors(&self, limit: usize) -> Vec<(&String, &MethodStats)> {
        let mut methods: Vec<_> = self.per_method.iter().filter(|(_, s)| s.errors > 0).collect();
        methods.sort_by(|a, b| b.1.error_rate().partial_cmp(&a.1.error_rate()).unwrap());
        methods.into_iter().take(limit).collect()
    }

    /// Get methods with highest average latency.
    #[must_use]
    pub fn slowest_methods(&self, limit: usize) -> Vec<(&String, &MethodStats)> {
        let mut methods: Vec<_> = self.per_method.iter().collect();
        methods.sort_by(|a, b| b.1.avg_latency_ms.partial_cmp(&a.1.avg_latency_ms).unwrap());
        methods.into_iter().take(limit).collect()
    }
}

/// Statistics for a single MCP method.
#[derive(Debug, Clone)]
pub struct MethodStats {
    /// Total requests for this method.
    pub requests: u64,
    /// Total errors for this method.
    pub errors: u64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
}

impl MethodStats {
    /// Calculate the error rate for this method.
    #[must_use]
    pub fn error_rate(&self) -> f64 {
        if self.requests == 0 {
            0.0
        } else {
            self.errors as f64 / self.requests as f64
        }
    }

    /// Calculate the success rate for this method.
    #[must_use]
    pub fn success_rate(&self) -> f64 {
        1.0 - self.error_rate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_metrics() {
        let metrics = ServerMetrics::new();

        metrics.record_success("tools/call", Duration::from_millis(50));
        metrics.record_success("tools/call", Duration::from_millis(100));
        metrics.record_failure("tools/call", Duration::from_millis(25));
        metrics.record_success("resources/read", Duration::from_millis(10));

        let snapshot = metrics.snapshot();

        assert_eq!(snapshot.total_requests, 4);
        assert_eq!(snapshot.successful_requests, 3);
        assert_eq!(snapshot.failed_requests, 1);
        assert!((snapshot.error_rate() - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_per_method_stats() {
        let metrics = ServerMetrics::new();

        metrics.record_success("tools/call", Duration::from_millis(100));
        metrics.record_success("tools/call", Duration::from_millis(100));
        metrics.record_failure("tools/call", Duration::from_millis(100));

        let snapshot = metrics.snapshot();
        let tools_stats = snapshot.method("tools/call").unwrap();

        assert_eq!(tools_stats.requests, 3);
        assert_eq!(tools_stats.errors, 1);
        assert!((tools_stats.avg_latency_ms - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_reset() {
        let metrics = ServerMetrics::new();

        metrics.record_success("test", Duration::from_millis(50));
        assert_eq!(metrics.snapshot().total_requests, 1);

        metrics.reset();
        assert_eq!(metrics.snapshot().total_requests, 0);
    }

    #[test]
    fn test_top_methods() {
        let metrics = ServerMetrics::new();

        for _ in 0..10 {
            metrics.record_success("tools/call", Duration::from_millis(10));
        }
        for _ in 0..5 {
            metrics.record_success("resources/read", Duration::from_millis(10));
        }
        for _ in 0..3 {
            metrics.record_success("prompts/get", Duration::from_millis(10));
        }

        let snapshot = metrics.snapshot();
        let top = snapshot.top_methods(2);

        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "tools/call");
        assert_eq!(top[1].0, "resources/read");
    }
}
