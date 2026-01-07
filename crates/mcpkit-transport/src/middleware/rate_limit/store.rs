//! Rate limit store abstraction for pluggable backends.
//!
//! This module provides the [`RateLimitStore`] trait that allows rate limiting
//! state to be stored in different backends (in-memory, Redis, etc.).
//!
//! # Built-in Stores
//!
//! - [`InMemoryStore`]: Default in-memory store using atomic operations
//!
//! # Implementing Custom Stores
//!
//! To implement a distributed rate limiting backend (e.g., Redis):
//!
//! ```rust,ignore
//! use mcpkit_transport::middleware::rate_limit::{
//!     RateLimitStore, RateLimitDecision, RateLimitStoreError, RateLimitConfig,
//! };
//! use async_trait::async_trait;
//!
//! struct RedisStore {
//!     client: redis::Client,
//! }
//!
//! #[async_trait]
//! impl RateLimitStore for RedisStore {
//!     async fn check_and_consume(
//!         &self,
//!         key: &str,
//!         config: &RateLimitConfig,
//!     ) -> Result<RateLimitDecision, RateLimitStoreError> {
//!         // Implement using Redis MULTI/EXEC or Lua scripts
//!         todo!()
//!     }
//!
//!     async fn get_stats(&self, key: &str) -> Result<StoreStats, RateLimitStoreError> {
//!         todo!()
//!     }
//!
//!     async fn reset(&self, key: &str) -> Result<(), RateLimitStoreError> {
//!         todo!()
//!     }
//! }
//! ```

use super::{RateLimitAlgorithm, RateLimitConfig};
use async_lock::Mutex;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Result of a rate limit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// Request is allowed.
    Allowed {
        /// Remaining tokens/requests in current window.
        remaining: u64,
    },
    /// Request is denied due to rate limiting.
    Denied {
        /// Suggested time to wait before retrying.
        retry_after: Duration,
    },
}

impl RateLimitDecision {
    /// Returns true if the request is allowed.
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Returns true if the request is denied.
    #[must_use]
    pub const fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }
}

/// Statistics from the rate limit store.
#[derive(Debug, Clone, Copy, Default)]
pub struct StoreStats {
    /// Current available tokens (for token bucket).
    pub current_tokens: u64,
    /// Total requests tracked.
    pub total_requests: u64,
    /// Total requests rejected.
    pub total_rejected: u64,
}

/// Errors that can occur in rate limit store operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RateLimitStoreError {
    /// The store backend is unavailable.
    #[error("store unavailable: {message}")]
    Unavailable {
        /// Error message.
        message: String,
    },

    /// An I/O or network error occurred.
    #[error("store I/O error: {message}")]
    IoError {
        /// Error message.
        message: String,
    },

    /// The key format is invalid.
    #[error("invalid key: {key}")]
    InvalidKey {
        /// The invalid key.
        key: String,
    },
}

/// Trait for rate limit state storage backends.
///
/// This trait abstracts the storage of rate limiting state, allowing
/// implementations for different backends (in-memory, Redis, DynamoDB, etc.).
///
/// All methods are async to support distributed backends that require
/// network I/O.
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to allow use across async tasks.
///
/// # Default Key
///
/// When `key` is empty, implementations should use a global/default bucket.
#[async_trait]
pub trait RateLimitStore: Send + Sync {
    /// Check if a request is allowed and consume quota atomically.
    ///
    /// This should be an atomic check-and-decrement operation:
    /// - If allowed: consume one unit of quota and return `Allowed`
    /// - If denied: return `Denied` with retry_after hint
    ///
    /// # Arguments
    ///
    /// * `key` - The rate limit key (e.g., client ID, IP address). Empty for global limit.
    /// * `config` - The rate limit configuration to apply.
    async fn check_and_consume(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<RateLimitDecision, RateLimitStoreError>;

    /// Get current statistics for a key.
    ///
    /// # Arguments
    ///
    /// * `key` - The rate limit key. Empty for global stats.
    async fn get_stats(&self, key: &str) -> Result<StoreStats, RateLimitStoreError>;

    /// Reset rate limit state for a key.
    ///
    /// This restores the key to its initial state (full token bucket, etc.).
    ///
    /// # Arguments
    ///
    /// * `key` - The rate limit key. Empty for global reset.
    async fn reset(&self, key: &str) -> Result<(), RateLimitStoreError>;
}

/// In-memory rate limit store using atomic operations.
///
/// This is the default store implementation, suitable for single-process
/// deployments. For distributed systems, implement a custom store using
/// Redis or similar.
///
/// # Thread Safety
///
/// Uses atomic operations and async mutexes for thread-safe access.
///
/// # Memory Usage
///
/// Currently uses a single global bucket. For per-client rate limiting,
/// consider using a distributed store or extending this implementation.
pub struct InMemoryStore {
    /// Token bucket: current token count (scaled by 1000 for precision).
    tokens: AtomicU64,
    /// Last refill time.
    last_refill: Mutex<Instant>,
    /// Sliding window: request timestamps.
    request_times: Mutex<Vec<Instant>>,
    /// Fixed window: request count in current window.
    window_count: AtomicU64,
    /// Fixed window: window start time.
    window_start: Mutex<Instant>,
    /// Total requests tracked (for metrics).
    total_requests: AtomicU64,
    /// Total rejected requests (for metrics).
    total_rejected: AtomicU64,
    /// Configuration snapshot for token bucket sizing.
    burst_size: u64,
}

impl InMemoryStore {
    /// Create a new in-memory store with the given configuration.
    #[must_use]
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            // Start with full bucket (scaled by 1000 for sub-token precision)
            tokens: AtomicU64::new(config.burst_size * 1000),
            last_refill: Mutex::new(Instant::now()),
            request_times: Mutex::new(Vec::with_capacity(config.max_requests as usize)),
            window_count: AtomicU64::new(0),
            window_start: Mutex::new(Instant::now()),
            total_requests: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
            burst_size: config.burst_size,
        }
    }

    /// Token bucket algorithm implementation.
    async fn check_token_bucket(&self, config: &RateLimitConfig) -> RateLimitDecision {
        let now = Instant::now();

        // Refill tokens based on elapsed time
        let mut last_refill = self.last_refill.lock().await;

        let elapsed = now.duration_since(*last_refill);
        let refill_rate = config.max_requests as f64 / config.window.as_millis() as f64;
        let tokens_to_add = (elapsed.as_millis() as f64 * refill_rate * 1000.0) as u64;

        if tokens_to_add > 0 {
            let current = self.tokens.load(Ordering::Relaxed);
            let max_tokens = config.burst_size * 1000;
            let new_tokens = (current + tokens_to_add).min(max_tokens);
            self.tokens.store(new_tokens, Ordering::Relaxed);
            *last_refill = now;
        }

        // Try to consume a token (1000 units = 1 token)
        let result = self
            .tokens
            .fetch_update(Ordering::SeqCst, Ordering::Relaxed, |current| {
                if current >= 1000 {
                    Some(current - 1000)
                } else {
                    None
                }
            });

        if let Ok(new_value) = result {
            RateLimitDecision::Allowed {
                remaining: new_value / 1000,
            }
        } else {
            // Calculate retry_after based on refill rate
            let wait_ms = (1000.0 / refill_rate).max(1.0) as u64;
            RateLimitDecision::Denied {
                retry_after: Duration::from_millis(wait_ms),
            }
        }
    }

    /// Sliding window algorithm implementation.
    async fn check_sliding_window(&self, config: &RateLimitConfig) -> RateLimitDecision {
        let now = Instant::now();
        let window_start = now.checked_sub(config.window);

        let mut times = self.request_times.lock().await;

        // Remove requests outside the window
        times.retain(|&t| window_start.is_none_or(|start| t > start));

        // Check if we're under the limit
        if times.len() < config.max_requests as usize {
            times.push(now);
            RateLimitDecision::Allowed {
                remaining: config.max_requests - times.len() as u64,
            }
        } else {
            // Estimate when the oldest request will expire
            let oldest = times.first().copied();
            let retry_after = oldest
                .and_then(|t| (t + config.window).checked_duration_since(now))
                .unwrap_or(config.window);
            RateLimitDecision::Denied { retry_after }
        }
    }

    /// Fixed window algorithm implementation.
    async fn check_fixed_window(&self, config: &RateLimitConfig) -> RateLimitDecision {
        let now = Instant::now();

        let mut window_start = self.window_start.lock().await;

        // Check if we need to start a new window
        if now.duration_since(*window_start) >= config.window {
            *window_start = now;
            self.window_count.store(1, Ordering::Relaxed);
            return RateLimitDecision::Allowed {
                remaining: config.max_requests - 1,
            };
        }

        // Increment count and check limit
        let count = self.window_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count <= config.max_requests {
            RateLimitDecision::Allowed {
                remaining: config.max_requests - count,
            }
        } else {
            // Calculate time until window resets
            let elapsed = now.duration_since(*window_start);
            let retry_after = config.window.saturating_sub(elapsed);
            RateLimitDecision::Denied { retry_after }
        }
    }
}

#[async_trait]
impl RateLimitStore for InMemoryStore {
    async fn check_and_consume(
        &self,
        _key: &str, // In-memory store uses global bucket
        config: &RateLimitConfig,
    ) -> Result<RateLimitDecision, RateLimitStoreError> {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        let decision = match config.algorithm {
            RateLimitAlgorithm::TokenBucket => self.check_token_bucket(config).await,
            RateLimitAlgorithm::SlidingWindow => self.check_sliding_window(config).await,
            RateLimitAlgorithm::FixedWindow => self.check_fixed_window(config).await,
        };

        if decision.is_denied() {
            self.total_rejected.fetch_add(1, Ordering::Relaxed);
        }

        Ok(decision)
    }

    async fn get_stats(&self, _key: &str) -> Result<StoreStats, RateLimitStoreError> {
        Ok(StoreStats {
            current_tokens: self.tokens.load(Ordering::Relaxed) / 1000,
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_rejected: self.total_rejected.load(Ordering::Relaxed),
        })
    }

    async fn reset(&self, _key: &str) -> Result<(), RateLimitStoreError> {
        self.tokens
            .store(self.burst_size * 1000, Ordering::Relaxed);
        self.window_count.store(0, Ordering::Relaxed);
        self.total_requests.store(0, Ordering::Relaxed);
        self.total_rejected.store(0, Ordering::Relaxed);

        *self.last_refill.lock().await = Instant::now();
        *self.window_start.lock().await = Instant::now();
        self.request_times.lock().await.clear();

        Ok(())
    }
}

/// Type alias for a boxed rate limit store.
pub type BoxedRateLimitStore = Arc<dyn RateLimitStore>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_store_allows_requests() {
        let config = RateLimitConfig::new(10, Duration::from_secs(1));
        let store = InMemoryStore::new(&config);

        // Should allow first 10 requests
        for i in 0..10 {
            let decision = store.check_and_consume("", &config).await.unwrap();
            assert!(
                decision.is_allowed(),
                "Request {} should be allowed",
                i + 1
            );
        }

        // 11th request should be denied
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_denied(), "11th request should be denied");
    }

    #[tokio::test]
    async fn test_in_memory_store_stats() {
        let config = RateLimitConfig::new(5, Duration::from_secs(1));
        let store = InMemoryStore::new(&config);

        // Make some requests
        for _ in 0..7 {
            let _ = store.check_and_consume("", &config).await;
        }

        let stats = store.get_stats("").await.unwrap();
        assert_eq!(stats.total_requests, 7);
        assert_eq!(stats.total_rejected, 2); // Last 2 were rejected
    }

    #[tokio::test]
    async fn test_in_memory_store_reset() {
        let config = RateLimitConfig::new(5, Duration::from_secs(1));
        let store = InMemoryStore::new(&config);

        // Exhaust all tokens
        for _ in 0..5 {
            let decision = store.check_and_consume("", &config).await.unwrap();
            assert!(decision.is_allowed());
        }

        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_denied());

        // Reset
        store.reset("").await.unwrap();

        // Should allow requests again
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_sliding_window_store() {
        let config = RateLimitConfig::new(5, Duration::from_millis(100))
            .with_algorithm(RateLimitAlgorithm::SlidingWindow);
        let store = InMemoryStore::new(&config);

        // Should allow first 5 requests
        for _ in 0..5 {
            let decision = store.check_and_consume("", &config).await.unwrap();
            assert!(decision.is_allowed());
        }

        // 6th should be denied
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_denied());

        // Wait for window to pass
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow requests again
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_fixed_window_store() {
        let config = RateLimitConfig::new(5, Duration::from_millis(100))
            .with_algorithm(RateLimitAlgorithm::FixedWindow);
        let store = InMemoryStore::new(&config);

        // Should allow first 5 requests
        for _ in 0..5 {
            let decision = store.check_and_consume("", &config).await.unwrap();
            assert!(decision.is_allowed());
        }

        // 6th should be denied
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_denied());

        // Wait for window to reset
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should allow requests again
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_allowed());
    }

    #[tokio::test]
    async fn test_decision_methods() {
        let allowed = RateLimitDecision::Allowed { remaining: 5 };
        assert!(allowed.is_allowed());
        assert!(!allowed.is_denied());

        let denied = RateLimitDecision::Denied {
            retry_after: Duration::from_secs(1),
        };
        assert!(!denied.is_allowed());
        assert!(denied.is_denied());
    }

    // Test that the store can be used as a trait object
    #[tokio::test]
    async fn test_store_trait_object() {
        let config = RateLimitConfig::new(10, Duration::from_secs(1));
        let store: Arc<dyn RateLimitStore> = Arc::new(InMemoryStore::new(&config));

        let decision = store.check_and_consume("test-key", &config).await.unwrap();
        assert!(decision.is_allowed());
    }
}
