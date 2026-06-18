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
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// Default maximum number of distinct keys tracked before the
/// least-recently-used bucket is evicted, bounding memory under many clients.
const DEFAULT_MAX_KEYS: usize = 10_000;

/// Per-key rate-limit state.
struct Bucket {
    /// Token bucket: current token count (scaled by 1000 for precision).
    tokens: u64,
    /// Token bucket: last refill time.
    last_refill: Instant,
    /// Sliding window: request timestamps.
    request_times: Vec<Instant>,
    /// Fixed window: request count in the current window.
    window_count: u64,
    /// Fixed window: window start time.
    window_start: Instant,
    /// When this bucket was last accessed (for LRU eviction).
    last_seen: Instant,
}

impl Bucket {
    fn new(config: &RateLimitConfig, now: Instant) -> Self {
        Self {
            // Start with a full bucket (scaled by 1000 for sub-token precision).
            tokens: config.burst_size * 1000,
            last_refill: now,
            request_times: Vec::new(),
            window_count: 0,
            window_start: now,
            last_seen: now,
        }
    }
}

/// In-memory rate limit store with independent per-key buckets.
///
/// This is the default store implementation, suitable for single-process
/// deployments. Each key (e.g. client IP) gets its own bucket, so one client
/// cannot exhaust another's budget. For distributed systems, implement a custom
/// store using Redis or similar.
///
/// # Memory Usage
///
/// Buckets are tracked in a map bounded to `max_keys` (default 10,000); once
/// full, the least-recently-used bucket is evicted when a new key arrives, so
/// the map cannot grow without bound.
pub struct InMemoryStore {
    /// Per-key buckets, guarded by a single async mutex.
    buckets: Mutex<HashMap<String, Bucket>>,
    /// Maximum number of keys retained before LRU eviction.
    max_keys: usize,
    /// Burst size, used to report stats for keys without a live bucket.
    burst_size: u64,
    /// Total requests tracked across all keys (for metrics).
    total_requests: AtomicU64,
    /// Total rejected requests across all keys (for metrics).
    total_rejected: AtomicU64,
}

impl InMemoryStore {
    /// Create a new in-memory store with the given configuration.
    #[must_use]
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            max_keys: DEFAULT_MAX_KEYS,
            burst_size: config.burst_size,
            total_requests: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
        }
    }

    /// Evict the least-recently-used bucket to stay within `max_keys`.
    fn evict_lru(buckets: &mut HashMap<String, Bucket>) {
        if let Some(key) = buckets
            .iter()
            .min_by_key(|(_, b)| b.last_seen)
            .map(|(k, _)| k.clone())
        {
            buckets.remove(&key);
        }
    }

    /// Token bucket algorithm, operating on a single key's bucket.
    fn check_token_bucket(
        bucket: &mut Bucket,
        config: &RateLimitConfig,
        now: Instant,
    ) -> RateLimitDecision {
        // Refill tokens based on elapsed time.
        let elapsed = now.duration_since(bucket.last_refill);
        let refill_rate = config.max_requests as f64 / config.window.as_millis() as f64;
        let tokens_to_add = (elapsed.as_millis() as f64 * refill_rate * 1000.0) as u64;

        if tokens_to_add > 0 {
            let max_tokens = config.burst_size * 1000;
            bucket.tokens = (bucket.tokens + tokens_to_add).min(max_tokens);
            bucket.last_refill = now;
        }

        // Try to consume a token (1000 units = 1 token).
        if bucket.tokens >= 1000 {
            bucket.tokens -= 1000;
            RateLimitDecision::Allowed {
                remaining: bucket.tokens / 1000,
            }
        } else {
            let wait_ms = (1000.0 / refill_rate).max(1.0) as u64;
            RateLimitDecision::Denied {
                retry_after: Duration::from_millis(wait_ms),
            }
        }
    }

    /// Sliding window algorithm, operating on a single key's bucket.
    fn check_sliding_window(
        bucket: &mut Bucket,
        config: &RateLimitConfig,
        now: Instant,
    ) -> RateLimitDecision {
        let window_start = now.checked_sub(config.window);

        // Remove requests outside the window.
        bucket
            .request_times
            .retain(|&t| window_start.is_none_or(|start| t > start));

        if bucket.request_times.len() < config.max_requests as usize {
            bucket.request_times.push(now);
            RateLimitDecision::Allowed {
                remaining: config.max_requests - bucket.request_times.len() as u64,
            }
        } else {
            let oldest = bucket.request_times.first().copied();
            let retry_after = oldest
                .and_then(|t| (t + config.window).checked_duration_since(now))
                .unwrap_or(config.window);
            RateLimitDecision::Denied { retry_after }
        }
    }

    /// Fixed window algorithm, operating on a single key's bucket.
    fn check_fixed_window(
        bucket: &mut Bucket,
        config: &RateLimitConfig,
        now: Instant,
    ) -> RateLimitDecision {
        // Start a new window if the current one has elapsed.
        if now.duration_since(bucket.window_start) >= config.window {
            bucket.window_start = now;
            bucket.window_count = 1;
            return RateLimitDecision::Allowed {
                remaining: config.max_requests - 1,
            };
        }

        bucket.window_count += 1;
        let count = bucket.window_count;
        if count <= config.max_requests {
            RateLimitDecision::Allowed {
                remaining: config.max_requests - count,
            }
        } else {
            let elapsed = now.duration_since(bucket.window_start);
            let retry_after = config.window.saturating_sub(elapsed);
            RateLimitDecision::Denied { retry_after }
        }
    }
}

#[async_trait]
impl RateLimitStore for InMemoryStore {
    async fn check_and_consume(
        &self,
        key: &str,
        config: &RateLimitConfig,
    ) -> Result<RateLimitDecision, RateLimitStoreError> {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();

        let mut buckets = self.buckets.lock().await;

        // Bound the key map: evict the LRU bucket before adding a new key.
        if buckets.len() >= self.max_keys && !buckets.contains_key(key) {
            Self::evict_lru(&mut buckets);
        }

        let bucket = buckets
            .entry(key.to_string())
            .or_insert_with(|| Bucket::new(config, now));
        bucket.last_seen = now;

        let decision = match config.algorithm {
            RateLimitAlgorithm::TokenBucket => Self::check_token_bucket(bucket, config, now),
            RateLimitAlgorithm::SlidingWindow => Self::check_sliding_window(bucket, config, now),
            RateLimitAlgorithm::FixedWindow => Self::check_fixed_window(bucket, config, now),
        };
        drop(buckets);

        if decision.is_denied() {
            self.total_rejected.fetch_add(1, Ordering::Relaxed);
        }

        Ok(decision)
    }

    async fn get_stats(&self, key: &str) -> Result<StoreStats, RateLimitStoreError> {
        let buckets = self.buckets.lock().await;
        // A key with no live bucket behaves as a full bucket.
        let current_tokens = buckets
            .get(key)
            .map_or(self.burst_size, |b| b.tokens / 1000);
        Ok(StoreStats {
            current_tokens,
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_rejected: self.total_rejected.load(Ordering::Relaxed),
        })
    }

    async fn reset(&self, key: &str) -> Result<(), RateLimitStoreError> {
        // Drop the key's bucket; the next request recreates a full one.
        self.buckets.lock().await.remove(key);
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
            assert!(decision.is_allowed(), "Request {} should be allowed", i + 1);
        }

        // 11th request should be denied
        let decision = store.check_and_consume("", &config).await.unwrap();
        assert!(decision.is_denied(), "11th request should be denied");
    }

    /// Regression test for #11: distinct keys must have independent buckets, so
    /// one client exhausting its budget does not throttle another.
    #[tokio::test]
    async fn check_and_consume_is_per_key() {
        let config = RateLimitConfig::new(2, Duration::from_secs(60));
        let store = InMemoryStore::new(&config);

        // Exhaust "alice".
        assert!(
            store
                .check_and_consume("alice", &config)
                .await
                .unwrap()
                .is_allowed()
        );
        assert!(
            store
                .check_and_consume("alice", &config)
                .await
                .unwrap()
                .is_allowed()
        );
        assert!(
            store
                .check_and_consume("alice", &config)
                .await
                .unwrap()
                .is_denied()
        );

        // "bob" has its own full bucket and is unaffected by "alice".
        assert!(
            store
                .check_and_consume("bob", &config)
                .await
                .unwrap()
                .is_allowed()
        );
        assert!(
            store
                .check_and_consume("bob", &config)
                .await
                .unwrap()
                .is_allowed()
        );
        assert!(
            store
                .check_and_consume("bob", &config)
                .await
                .unwrap()
                .is_denied()
        );
    }

    /// The key map is bounded: once at capacity, the least-recently-used bucket
    /// is evicted when a new key arrives.
    #[tokio::test]
    async fn evicts_least_recently_used_key_when_over_capacity() {
        let config = RateLimitConfig::new(5, Duration::from_secs(60));
        let mut store = InMemoryStore::new(&config);
        store.max_keys = 2;

        store.check_and_consume("alice", &config).await.unwrap();
        store.check_and_consume("bob", &config).await.unwrap();
        // "carol" pushes past max_keys=2, evicting the LRU bucket ("alice").
        store.check_and_consume("carol", &config).await.unwrap();

        let buckets = store.buckets.lock().await;
        assert_eq!(buckets.len(), 2);
        assert!(
            !buckets.contains_key("alice"),
            "alice (least recently used) should have been evicted"
        );
        assert!(buckets.contains_key("bob"));
        assert!(buckets.contains_key("carol"));
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
